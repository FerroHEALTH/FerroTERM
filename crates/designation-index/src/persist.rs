// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A versioned binary layout for the index, for the offline build to store.
//!
//! No spec governs this: our own design. Little-endian, a magic and version
//! prefix, the fst bytes, the posting bitmaps, the entries, and the filter
//! bitmaps keyed by language, use, and reference set.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};

use roaring::RoaringBitmap;

use concept_graph::ordinal::Ordinal;

use crate::index::{BuildError, Entry, OwnedParts, TextIndex};

const MAGIC: &[u8; 8] = b"FTTEXT\0\0";
const VERSION: u32 = 1;

/// A length or size past `u32::MAX`, which the artifact layout cannot store.
#[derive(Debug, thiserror::Error)]
#[error("{what} exceeds the u32 the artifact layout stores")]
struct TooLong {
    what: &'static str,
    #[source]
    source: std::num::TryFromIntError,
}

/// `len` as the `u32` the layout stores, or the I/O error naming `what` overflowed.
fn u32_len(len: usize, what: &'static str) -> io::Result<u32> {
    u32::try_from(len).map_err(|source| io::Error::other(TooLong { what, source }))
}

/// A failure while reading or writing the layout.
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// An I/O failure.
    #[error("index I/O failed")]
    Io(#[from] io::Error),
    /// The bytes do not start with the index magic.
    #[error("not a text index artifact")]
    Magic,
    /// The layout version is not the one this build reads.
    #[error("text index layout version {found}, expected {expected}")]
    Version {
        /// The version found.
        found: u32,
        /// The version this build reads.
        expected: u32,
    },
    /// The dictionary bytes are not an fst map.
    #[error(transparent)]
    Build(#[from] BuildError),
    /// A string is not UTF-8.
    #[error("a language tag is not UTF-8")]
    Utf8(#[source] std::string::FromUtf8Error),
}

/// Writes `index`.
///
/// # Errors
///
/// Returns [`PersistError::Io`] when writing fails.
pub fn write_to(index: &TextIndex, out: &mut impl Write) -> Result<(), PersistError> {
    let parts = index.parts();
    out.write_all(MAGIC)?;
    out.write_all(&VERSION.to_le_bytes())?;
    write_bytes(out, parts.dictionary)?;
    write_bitmaps(out, parts.postings)?;
    write_u32(out, u32_len(parts.entries.len(), "the entry count")?)?;
    for entry in parts.entries {
        write_u32(out, entry.concept.index())?;
        write_u32(out, entry.index)?;
        out.write_all(&entry.term_length.to_le_bytes())?;
    }
    write_u32(out, u32_len(parts.languages.len(), "the language count")?)?;
    for (language, bitmap) in parts.languages {
        write_bytes(out, language.as_bytes())?;
        write_bitmap(out, bitmap)?;
    }
    write_keyed(out, parts.uses)?;
    write_keyed(out, parts.refsets)?;
    write_bitmap(out, parts.active)?;
    write_u32(out, u32_len(parts.lengths.len(), "the length count")?)?;
    for (length, bitmap) in parts.lengths {
        out.write_all(&length.to_le_bytes())?;
        write_bitmap(out, bitmap)?;
    }
    Ok(())
}

/// Reads an index.
///
/// # Errors
///
/// Returns [`PersistError`] for a truncated, foreign, or inconsistent artifact.
pub fn read_from(input: &mut impl Read) -> Result<TextIndex, PersistError> {
    let mut magic = [0_u8; 8];
    input.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(PersistError::Magic);
    }
    let version = read_u32(input)?;
    if version != VERSION {
        return Err(PersistError::Version {
            found: version,
            expected: VERSION,
        });
    }
    let dictionary = read_bytes(input)?;
    let postings = read_bitmaps(input)?;
    let count = read_u32(input)?;
    let mut entries = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for _ in 0..count {
        let concept = Ordinal::new(read_u32(input)?);
        let index = read_u32(input)?;
        let mut length = [0_u8; 2];
        input.read_exact(&mut length)?;
        entries.push(Entry {
            concept,
            index,
            term_length: u16::from_le_bytes(length),
        });
    }
    let language_count = read_u32(input)?;
    let mut languages = BTreeMap::new();
    for _ in 0..language_count {
        let tag = String::from_utf8(read_bytes(input)?).map_err(PersistError::Utf8)?;
        languages.insert(tag, read_bitmap(input)?);
    }
    let uses = read_keyed(input)?;
    let refsets = read_keyed(input)?;
    let active = read_bitmap(input)?;
    let length_count = read_u32(input)?;
    let mut lengths = BTreeMap::new();
    for _ in 0..length_count {
        let mut length = [0_u8; 2];
        input.read_exact(&mut length)?;
        lengths.insert(u16::from_le_bytes(length), read_bitmap(input)?);
    }
    Ok(TextIndex::from_parts(OwnedParts {
        dictionary,
        postings,
        entries,
        languages,
        uses,
        refsets,
        active,
        lengths,
    })?)
}

fn write_u32(out: &mut impl Write, value: u32) -> io::Result<()> {
    out.write_all(&value.to_le_bytes())
}

fn read_u32(input: &mut impl Read) -> io::Result<u32> {
    let mut buffer = [0_u8; 4];
    input.read_exact(&mut buffer)?;
    Ok(u32::from_le_bytes(buffer))
}

fn write_bytes(out: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    write_u32(out, u32_len(bytes.len(), "a blob size")?)?;
    out.write_all(bytes)
}

fn read_bytes(input: &mut impl Read) -> io::Result<Vec<u8>> {
    let len = read_u32(input)?;
    let mut bytes = vec![0_u8; usize::try_from(len).unwrap_or(0)];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn write_bitmap(out: &mut impl Write, bitmap: &RoaringBitmap) -> io::Result<()> {
    write_u32(out, u32_len(bitmap.serialized_size(), "a bitmap size")?)?;
    bitmap.serialize_into(out)
}

fn read_bitmap(input: &mut impl Read) -> io::Result<RoaringBitmap> {
    let bytes = read_bytes(input)?;
    RoaringBitmap::deserialize_from(bytes.as_slice())
}

fn write_bitmaps(out: &mut impl Write, bitmaps: &[RoaringBitmap]) -> io::Result<()> {
    write_u32(out, u32_len(bitmaps.len(), "the bitmap count")?)?;
    for bitmap in bitmaps {
        write_bitmap(out, bitmap)?;
    }
    Ok(())
}

fn read_bitmaps(input: &mut impl Read) -> io::Result<Vec<RoaringBitmap>> {
    let count = read_u32(input)?;
    let mut bitmaps = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for _ in 0..count {
        bitmaps.push(read_bitmap(input)?);
    }
    Ok(bitmaps)
}

fn write_keyed(out: &mut impl Write, map: &BTreeMap<u32, RoaringBitmap>) -> io::Result<()> {
    write_u32(out, u32_len(map.len(), "the key count")?)?;
    for (key, bitmap) in map {
        write_u32(out, *key)?;
        write_bitmap(out, bitmap)?;
    }
    Ok(())
}

fn read_keyed(input: &mut impl Read) -> io::Result<BTreeMap<u32, RoaringBitmap>> {
    let count = read_u32(input)?;
    let mut map = BTreeMap::new();
    for _ in 0..count {
        let key = read_u32(input)?;
        map.insert(key, read_bitmap(input)?);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::{PersistError, read_from, write_to};
    use crate::index::{IndexBuilder, Input, Query};
    use concept_graph::ordinal::Ordinal;

    #[test]
    fn the_layout_round_trips_and_rejects_foreign_bytes() {
        let mut builder = IndexBuilder::new();
        builder
            .add(&Input {
                concept: Ordinal::new(4),
                index: 1,
                term: "Ménière's disease",
                language: "en",
                use_ordinal: 1,
                active: true,
                refsets: &[7],
            })
            .expect("adds");
        let index = builder.build().expect("builds");
        let mut bytes = Vec::new();
        write_to(&index, &mut bytes).expect("writes");
        let back = read_from(&mut bytes.as_slice()).expect("reads");
        let query = Query {
            text: "menier".to_owned(),
            refset: Some(7),
            ..Query::default()
        };
        assert_eq!(back.search(&query, 0, 10), index.search(&query, 0, 10));
        assert_eq!(back.entry(0), index.entry(0));
        assert!(matches!(
            read_from(&mut b"nope".as_slice()),
            Err(PersistError::Io(_))
        ));
        assert!(matches!(
            read_from(&mut b"XXXXXXXX\0\0\0\0".as_slice()),
            Err(PersistError::Magic)
        ));
    }
}
