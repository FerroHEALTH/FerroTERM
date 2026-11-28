// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The alternate identifiers of an edition: a code of another scheme
//! (`LOINC` `54486-6`) for a concept, from the RF2 identifier file, so an
//! ECL `scheme#code` focus resolves to the concept.
//!
//! No spec governs the layout: our own design. Little-endian, a magic and
//! version prefix, a count, then per identifier the scheme concept SCTID, the
//! code, and the concept ordinal, sorted by scheme then code.

use std::io::{self, Read, Write};

use crate::ordinal::{Ordinal, to_usize};

const MAGIC: &[u8; 8] = b"FTIDENT\0";
const VERSION: u32 = 1;

/// A failure while reading or writing the identifiers.
#[derive(Debug, thiserror::Error)]
pub enum IdentifiersError {
    /// An I/O failure.
    #[error("identifiers I/O failed")]
    Io(#[from] io::Error),
    /// The bytes do not start with the identifiers magic.
    #[error("not an identifiers artifact")]
    Magic,
    /// The layout version is not the one this build reads.
    #[error("identifiers layout version {found}, expected {expected}")]
    Version {
        /// The version found.
        found: u32,
        /// The version this build reads.
        expected: u32,
    },
    /// More identifiers than the `u32` count addresses.
    #[error("too many identifiers")]
    TooMany(#[source] std::num::TryFromIntError),
    /// A code is not UTF-8.
    #[error("an alternate identifier is not UTF-8")]
    Text(#[from] std::string::FromUtf8Error),
}

/// The alternate identifiers, sorted by scheme then code, with an index by
/// concept.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Identifiers {
    entries: Vec<(u64, String, u32)>,
    /// The positions in `entries`, ordered by concept, then by position.
    by_concept: Vec<u32>,
}

/// The positions of `entries` ordered by their concept.
fn by_concept(entries: &[(u64, String, u32)]) -> Vec<u32> {
    let mut positions: Vec<u32> = (0..entries.len())
        .filter_map(|position| u32::try_from(position).ok())
        .collect();
    positions.sort_by_key(|position| {
        (
            entries
                .get(to_usize(*position))
                .map(|(_, _, concept)| *concept),
            *position,
        )
    });
    positions
}

impl Identifiers {
    /// Builds the table from `(scheme SCTID, code, concept)` entries.
    #[must_use]
    pub fn new(mut entries: Vec<(u64, String, Ordinal)>) -> Self {
        entries.sort();
        entries.dedup();
        let entries: Vec<(u64, String, u32)> = entries
            .into_iter()
            .map(|(scheme, code, concept)| (scheme, code, concept.index()))
            .collect();
        Self {
            by_concept: by_concept(&entries),
            entries,
        }
    }

    /// The heap bytes the table holds, its codes included
    /// ([`crate::footprint`]).
    #[must_use]
    pub fn size_in_bytes(&self) -> usize {
        self.entries.iter().fold(
            crate::footprint::vector(&self.entries)
                .saturating_add(crate::footprint::vector(&self.by_concept)),
            |total, entry| total.saturating_add(entry.1.capacity()),
        )
    }

    /// The concept identified by `code` in `scheme`.
    #[must_use]
    pub fn lookup(&self, scheme: u64, code: &str) -> Option<Ordinal> {
        self.entries
            .binary_search_by(|(s, c, _)| (*s, c.as_str()).cmp(&(scheme, code)))
            .ok()
            .and_then(|i| self.entries.get(i))
            .map(|(_, _, concept)| Ordinal::new(*concept))
    }

    /// The `(scheme, code)` identifiers of `concept`, by scheme then code.
    pub fn of_concept(&self, concept: Ordinal) -> impl Iterator<Item = (u64, &str)> + '_ {
        let held = |position: &u32| {
            self.entries
                .get(to_usize(*position))
                .map(|(_, _, held)| *held)
        };
        let start = self
            .by_concept
            .partition_point(|position| held(position) < Some(concept.index()));
        self.by_concept
            .get(start..)
            .unwrap_or_default()
            .iter()
            .take_while(move |position| held(position) == Some(concept.index()))
            .filter_map(|position| self.entries.get(to_usize(*position)))
            .map(|(scheme, code, _)| (*scheme, code.as_str()))
    }

    /// The concepts that have an identifier in `scheme`, ascending by code.
    pub fn concepts(&self, scheme: u64) -> impl Iterator<Item = Ordinal> + '_ {
        let start = self.entries.partition_point(|(s, _, _)| *s < scheme);
        self.entries
            .get(start..)
            .unwrap_or_default()
            .iter()
            .take_while(move |(s, _, _)| *s == scheme)
            .map(|(_, _, concept)| Ordinal::new(*concept))
    }

    /// The identifier schemes present, ascending.
    #[must_use]
    pub fn schemes(&self) -> Vec<u64> {
        let mut schemes: Vec<u64> = self.entries.iter().map(|(s, _, _)| *s).collect();
        schemes.dedup();
        schemes
    }

    /// The number of identifiers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are no identifiers.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Writes the layout.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifiersError::Io`] when writing fails.
    pub fn write_to(&self, out: &mut impl Write) -> Result<(), IdentifiersError> {
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        let count = u32::try_from(self.entries.len()).map_err(IdentifiersError::TooMany)?;
        out.write_all(&count.to_le_bytes())?;
        for (scheme, code, concept) in &self.entries {
            out.write_all(&scheme.to_le_bytes())?;
            let len = u32::try_from(code.len()).map_err(IdentifiersError::TooMany)?;
            out.write_all(&len.to_le_bytes())?;
            out.write_all(code.as_bytes())?;
            out.write_all(&concept.to_le_bytes())?;
        }
        Ok(())
    }

    /// Reads the layout.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifiersError`] for a truncated or foreign artifact.
    pub fn read_from(input: &mut impl Read) -> Result<Self, IdentifiersError> {
        let mut magic = [0_u8; 8];
        input.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(IdentifiersError::Magic);
        }
        let version = read_u32(input)?;
        if version != VERSION {
            return Err(IdentifiersError::Version {
                found: version,
                expected: VERSION,
            });
        }
        let count = read_u32(input)?;
        let mut entries = Vec::with_capacity(to_usize(count));
        for _ in 0..count {
            let mut long = [0_u8; 8];
            input.read_exact(&mut long)?;
            let len = to_usize(read_u32(input)?);
            let mut bytes = vec![0_u8; len];
            input.read_exact(&mut bytes)?;
            let concept = read_u32(input)?;
            entries.push((u64::from_le_bytes(long), String::from_utf8(bytes)?, concept));
        }
        entries.sort();
        Ok(Self {
            by_concept: by_concept(&entries),
            entries,
        })
    }
}

fn read_u32(input: &mut impl Read) -> Result<u32, IdentifiersError> {
    let mut bytes = [0_u8; 4];
    input.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::{Identifiers, IdentifiersError};
    use crate::ordinal::Ordinal;

    #[test]
    fn codes_resolve_per_scheme_and_the_layout_round_trips() {
        let identifiers = Identifiers::new(vec![
            (705_114_005, String::from("54486-6"), Ordinal::new(3)),
            (705_114_005, String::from("1234-5"), Ordinal::new(4)),
            (900, String::from("54486-6"), Ordinal::new(5)),
        ]);
        assert_eq!(
            identifiers.lookup(705_114_005, "54486-6"),
            Some(Ordinal::new(3))
        );
        assert_eq!(identifiers.lookup(900, "54486-6"), Some(Ordinal::new(5)));
        assert_eq!(identifiers.lookup(705_114_005, "9999"), None);
        assert_eq!(identifiers.schemes(), [900, 705_114_005]);
        let of = |concept| {
            identifiers
                .of_concept(Ordinal::new(concept))
                .collect::<Vec<_>>()
        };
        assert_eq!(of(3), [(705_114_005, "54486-6")]);
        assert_eq!(of(5), [(900, "54486-6")]);
        assert_eq!(of(6), []);
        let schemed: Vec<Ordinal> = identifiers.concepts(705_114_005).collect();
        assert_eq!(schemed, [Ordinal::new(4), Ordinal::new(3)]);
        assert_eq!(identifiers.concepts(1).count(), 0);
        let mut bytes = Vec::new();
        identifiers.write_to(&mut bytes).expect("writes");
        assert_eq!(
            Identifiers::read_from(&mut bytes.as_slice()).expect("reads"),
            identifiers
        );
        assert!(matches!(
            Identifiers::read_from(&mut b"XXXXXXXX\0\0\0\0".as_slice()),
            Err(IdentifiersError::Magic)
        ));
    }
}
