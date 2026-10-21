// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A dense column: one record per ordinal, addressed by position.
//!
//! An ordinal is a position, so the record at that position is found by
//! reading two offsets and slicing, with no search and no per-read
//! transaction. No spec governs the layout: our own design, the same shape
//! `concept-graph` stores its adjacency in. Little-endian throughout: a
//! record count, then one offset per record plus a terminator, then the
//! records themselves end to end.
//!
//! A record the build never wrote has an empty range. Every encoding in
//! [`crate::record`] is longer than zero bytes, so an empty range means
//! absent and never an empty record.
//!
//! A column is its own file beside the database, written once by the build and
//! read once when the store opens. Held in the database it arrived twice, as
//! the value `redb` materialized and as the copy the store serves from, and
//! the pages of the first stayed with the process for its life (#641).

use std::fmt;
use std::path::{Path, PathBuf};

use concept_graph::ordinal::{Ordinal, to_usize};

/// The width of the count and of each offset.
const WIDTH: usize = 4;

/// How many offsets a reader converts per read.
const CHUNK: usize = 1024;

/// The extension of a column's side file.
const EXTENSION: &str = "col";

/// A column whose bytes are not the layout this build reads.
#[derive(Debug, thiserror::Error)]
pub enum ColumnError {
    /// The bytes end before the named part is complete.
    #[error("the column ends inside its {part}")]
    Truncated {
        /// Which part the bytes end inside.
        part: &'static str,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// An offset lies behind the one before it, so its record has no range.
    #[error("offset {at} is {offset}, behind its predecessor {previous}")]
    Offset {
        /// The index of the offending offset.
        at: usize,
        /// Its value.
        offset: usize,
        /// The offset before it.
        previous: usize,
    },
    /// The column could not be read.
    #[error("cannot read the column")]
    Io(#[source] std::io::Error),
}

/// Classifies a read failure: an early end names the part it ended inside.
fn read_failed(part: &'static str) -> impl FnOnce(std::io::Error) -> ColumnError {
    move |source| match source.kind() {
        std::io::ErrorKind::UnexpectedEof => ColumnError::Truncated { part, source },
        _ => ColumnError::Io(source),
    }
}

/// Appends one offset, refusing one that lies behind the one before it.
fn push_offset(
    offsets: &mut Vec<u32>,
    previous: &mut u32,
    word: [u8; WIDTH],
) -> Result<(), ColumnError> {
    let offset = u32::from_le_bytes(word);
    if offset < *previous {
        return Err(ColumnError::Offset {
            at: offsets.len(),
            offset: to_usize(offset),
            previous: to_usize(*previous),
        });
    }
    *previous = offset;
    offsets.push(offset);
    Ok(())
}

/// A read-only dense column.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Column {
    /// `count + 1` offsets into `records`, ascending.
    offsets: Vec<u32>,
    records: Vec<u8>,
}

impl fmt::Debug for Column {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Column")
            .field("len", &self.len())
            .field("bytes", &self.records.len())
            .finish_non_exhaustive()
    }
}

impl Column {
    /// The side file the `name` column of the store at `store` is written to.
    ///
    /// The name carries the store's own stem, so two stores in one directory
    /// cannot write each other's columns.
    #[must_use]
    pub fn file(store: &Path, name: &str) -> PathBuf {
        let stem = store.file_stem().unwrap_or_default().to_string_lossy();
        store.with_file_name(format!("{stem}.{name}.{EXTENSION}"))
    }

    /// Writes the column `records` pack, in ordinal order.
    ///
    /// A record is placed at its ordinal, and an ordinal no record names gets
    /// an empty range. The records are borrowed rather than packed into one
    /// buffer first, so a build writes an edition's column without holding a
    /// second copy of it.
    ///
    /// # Errors
    ///
    /// Returns the I/O error from writing.
    pub fn write_to<'a>(
        count: u32,
        records: impl IntoIterator<Item = (Ordinal, &'a [u8])>,
        out: &mut impl std::io::Write,
    ) -> std::io::Result<()> {
        let count = to_usize(count);
        let mut placed: Vec<&[u8]> = vec![&[]; count];
        for (ordinal, bytes) in records {
            let at = to_usize(ordinal.index());
            if let Some(slot) = placed.get_mut(at) {
                *slot = bytes;
            }
        }
        out.write_all(&u32::try_from(count).unwrap_or(u32::MAX).to_le_bytes())?;
        let mut at = 0_u32;
        for record in &placed {
            out.write_all(&at.to_le_bytes())?;
            at = at.saturating_add(u32::try_from(record.len()).unwrap_or(u32::MAX));
        }
        out.write_all(&at.to_le_bytes())?;
        for record in placed {
            out.write_all(record)?;
        }
        Ok(())
    }

    /// Reads a column, checking its offsets once so a read never can.
    ///
    /// The records land in one allocation of their final size, filled from the
    /// reader, so the column arrives at the address it is served from.
    ///
    /// # Errors
    ///
    /// Returns [`ColumnError`] when the bytes end early, an offset lies behind
    /// the one before it, or the reader fails.
    pub fn read_from(reader: &mut impl std::io::Read) -> Result<Self, ColumnError> {
        let mut word = [0_u8; WIDTH];
        reader
            .read_exact(&mut word)
            .map_err(read_failed("header"))?;
        let count = to_usize(u32::from_le_bytes(word));
        let mut offsets: Vec<u32> = Vec::with_capacity(count.saturating_add(1));
        let mut buffer = [0_u8; WIDTH * CHUNK];
        let mut previous = 0_u32;
        let mut remaining = count.saturating_add(1);
        while remaining >= CHUNK {
            reader
                .read_exact(&mut buffer)
                .map_err(read_failed("offsets"))?;
            for chunk in buffer.as_chunks::<WIDTH>().0 {
                push_offset(&mut offsets, &mut previous, *chunk)?;
            }
            remaining = remaining.saturating_sub(CHUNK);
        }
        for _ in 0..remaining {
            reader
                .read_exact(&mut word)
                .map_err(read_failed("offsets"))?;
            push_offset(&mut offsets, &mut previous, word)?;
        }
        let mut records = vec![0_u8; to_usize(previous)];
        reader
            .read_exact(&mut records)
            .map_err(read_failed("records"))?;
        Ok(Self { offsets, records })
    }

    /// Reads a column from bytes already in memory.
    ///
    /// # Errors
    ///
    /// Returns [`ColumnError`] as [`Self::read_from`] does.
    pub fn read(bytes: &[u8]) -> Result<Self, ColumnError> {
        Self::read_from(&mut std::io::Cursor::new(bytes))
    }

    /// The heap bytes this column holds
    /// (`concept_graph::footprint`).
    #[must_use]
    pub fn size_in_bytes(&self) -> usize {
        concept_graph::footprint::vector(&self.offsets)
            .saturating_add(concept_graph::footprint::vector(&self.records))
    }

    /// How many ordinals the column holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    /// Whether the column holds no ordinals.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The record at `ordinal`, `None` beyond the column or where the build
    /// wrote none.
    ///
    /// The offsets were checked when the column was read, so this slices
    /// without failing.
    #[must_use]
    pub fn get(&self, ordinal: Ordinal) -> Option<&[u8]> {
        let at = to_usize(ordinal.index());
        let start = to_usize(*self.offsets.get(at)?);
        let end = to_usize(*self.offsets.get(at.checked_add(1)?)?);
        if start == end {
            return None;
        }
        self.records.get(start..end)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use concept_graph::ordinal::Ordinal;

    use super::{Column, ColumnError};

    fn pack<'a>(count: u32, records: impl IntoIterator<Item = (Ordinal, &'a [u8])>) -> Vec<u8> {
        let mut bytes = Vec::new();
        Column::write_to(count, records, &mut bytes).expect("a vector takes every write");
        bytes
    }

    #[test]
    fn a_packed_column_reads_back_what_was_placed() {
        let packed = pack(
            4,
            [
                (Ordinal::new(0), b"first".as_slice()),
                (Ordinal::new(3), b"fourth".as_slice()),
            ],
        );
        let column = Column::read(&packed).expect("reads");
        assert_eq!(column.len(), 4);
        assert!(!column.is_empty());
        assert_eq!(column.get(Ordinal::new(0)), Some(b"first".as_slice()));
        assert_eq!(column.get(Ordinal::new(1)), None, "no record was placed");
        assert_eq!(column.get(Ordinal::new(3)), Some(b"fourth".as_slice()));
        assert_eq!(column.get(Ordinal::new(4)), None, "beyond the column");
    }

    #[test]
    fn a_column_reads_the_same_from_a_reader_as_from_bytes() {
        // The store reads a file and the tests read bytes; one reader answers
        // both, so the two cannot drift apart.
        let packed = pack(
            2048,
            (0..2048).map(|at| (Ordinal::new(at), b"x".as_slice())),
        );
        let from_bytes = Column::read(&packed).expect("reads");
        let from_reader =
            Column::read_from(&mut std::io::Cursor::new(packed.clone())).expect("reads");
        assert_eq!(
            from_bytes, from_reader,
            "more offsets than one read converts"
        );
    }

    #[test]
    fn an_empty_column_holds_nothing() {
        let column = Column::read(&pack(0, [])).expect("reads");
        assert!(column.is_empty());
        assert_eq!(column.get(Ordinal::new(0)), None);
    }

    #[test]
    fn a_short_or_crossed_column_is_refused() {
        assert!(matches!(
            Column::read(&[0, 0]),
            Err(ColumnError::Truncated { part: "header", .. })
        ));
        let packed = pack(2, [(Ordinal::new(0), b"ab".as_slice())]);
        assert!(
            matches!(
                Column::read(packed.get(..packed.len() - 1).expect("shortened")),
                Err(ColumnError::Truncated {
                    part: "records",
                    ..
                })
            ),
            "the records end before the offsets say they do"
        );
        // One record of two bytes, then a terminator that goes backwards.
        let crossed = [1_u32, 2, 0]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .chain(*b"ab")
            .collect::<Vec<u8>>();
        assert!(matches!(
            Column::read(&crossed),
            Err(ColumnError::Offset {
                at: 1,
                offset: 0,
                previous: 2
            })
        ));
    }

    #[test]
    fn a_column_file_carries_the_store_it_belongs_to() {
        assert_eq!(
            Column::file(Path::new("/srv/nl/store.redb"), "concepts"),
            Path::new("/srv/nl/store.concepts.col")
        );
        assert_eq!(
            Column::file(Path::new("other.redb"), "displays"),
            Path::new("other.displays.col"),
            "two stores in one directory keep their columns apart"
        );
    }
}
