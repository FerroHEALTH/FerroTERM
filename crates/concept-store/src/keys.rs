// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A sorted `u64` key to `u32` ordinal table beside an artifact.
//!
//! A code system whose concepts have identifiers beyond their codes (the atom
//! identifiers of `RxNorm`, the entity identifiers of ICD-11) keeps them here,
//! each next to its concept ordinal. No spec governs the layout: our own
//! design, little-endian, a magic and version, a count, then sorted
//! `(key, ordinal)` pairs.

use std::io::{self, Read, Write};

const MAGIC: &[u8; 8] = b"FTKEYS\0\0";
const VERSION: u32 = 1;

/// A count past the integer the key table layout stores.
#[derive(Debug, thiserror::Error)]
#[error("{what} exceeds the integer the key table layout stores")]
struct Overflow {
    what: &'static str,
    #[source]
    source: std::num::TryFromIntError,
}

/// A failure while reading or writing the table.
#[derive(Debug, thiserror::Error)]
pub enum KeyTableError {
    /// An I/O failure.
    #[error("key table I/O failed")]
    Io(#[from] io::Error),
    /// The bytes do not start with the table magic.
    #[error("not a key table")]
    Magic,
    /// The layout version is not the one this build reads.
    #[error("key table version {found}, expected {expected}")]
    Version {
        /// The version found.
        found: u32,
        /// The version this build reads.
        expected: u32,
    },
}

/// The sorted table.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeyTable {
    pairs: Vec<(u64, u32)>,
}

impl KeyTable {
    /// Builds the table from `(key, ordinal)` pairs, in any order.
    #[must_use]
    pub fn new(mut pairs: Vec<(u64, u32)>) -> Self {
        pairs.sort_unstable();
        pairs.dedup();
        Self { pairs }
    }

    /// The ordinal stored under `key`.
    #[must_use]
    pub fn get(&self, key: u64) -> Option<u32> {
        self.pairs
            .binary_search_by_key(&key, |(k, _)| *k)
            .ok()
            .and_then(|i| self.pairs.get(i))
            .map(|(_, o)| *o)
    }

    /// The entry count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Writes the layout.
    ///
    /// # Errors
    ///
    /// Returns [`KeyTableError::Io`] when writing fails.
    pub fn write_to(&self, out: &mut impl Write) -> Result<(), KeyTableError> {
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        let len = u64::try_from(self.pairs.len()).map_err(|source| {
            io::Error::other(Overflow {
                what: "the key count",
                source,
            })
        })?;
        out.write_all(&len.to_le_bytes())?;
        for (key, ordinal) in &self.pairs {
            out.write_all(&key.to_le_bytes())?;
            out.write_all(&ordinal.to_le_bytes())?;
        }
        Ok(())
    }

    /// Reads the layout.
    ///
    /// # Errors
    ///
    /// Returns [`KeyTableError`] for a truncated or foreign table.
    pub fn read_from(input: &mut impl Read) -> Result<Self, KeyTableError> {
        let mut magic = [0_u8; 8];
        input.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(KeyTableError::Magic);
        }
        let mut word = [0_u8; 4];
        input.read_exact(&mut word)?;
        let version = u32::from_le_bytes(word);
        if version != VERSION {
            return Err(KeyTableError::Version {
                found: version,
                expected: VERSION,
            });
        }
        let mut long = [0_u8; 8];
        input.read_exact(&mut long)?;
        let len = usize::try_from(u64::from_le_bytes(long)).map_err(|source| {
            io::Error::other(Overflow {
                what: "the key count",
                source,
            })
        })?;
        let mut pairs = Vec::with_capacity(len);
        for _ in 0..len {
            input.read_exact(&mut long)?;
            input.read_exact(&mut word)?;
            pairs.push((u64::from_le_bytes(long), u32::from_le_bytes(word)));
        }
        Ok(Self { pairs })
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyTable, KeyTableError};

    #[test]
    fn the_table_answers_by_key_and_round_trips() {
        let keys = KeyTable::new(vec![(829, 0), (12_251_526, 1), (2_798_745, 1), (829, 0)]);
        assert_eq!(keys.len(), 3);
        assert_eq!(keys.get(2_798_745), Some(1));
        assert_eq!(keys.get(1), None);
        let mut bytes = Vec::new();
        keys.write_to(&mut bytes).expect("writes");
        assert_eq!(
            KeyTable::read_from(&mut bytes.as_slice()).expect("reads"),
            keys
        );
        assert!(matches!(
            KeyTable::read_from(&mut b"XXXXXXXX\0\0\0\0".as_slice()),
            Err(KeyTableError::Magic)
        ));
    }
}
