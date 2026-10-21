// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reference set memberships: for every reference set of an edition, the
//! bitmap of the concepts that are its active members.
//!
//! `?fhir_vs=refset/[sctid]` and the `concept in [sctid]` filter of the FHIR
//! SNOMED CT page read one bitmap; `?fhir_vs=refset` reads the keys. No spec
//! governs the layout: our own design. Little-endian, a magic and version
//! prefix, a count, then per reference set its SCTID as `u64`, the bitmap's
//! serialized length, and the bitmap in roaring's portable serialization.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};

use roaring::RoaringBitmap;

use crate::ordinal::{Ordinal, to_usize};

const MAGIC: &[u8; 8] = b"FTRSETS\0";
const VERSION: u32 = 1;

/// A failure while reading or writing the memberships.
#[derive(Debug, thiserror::Error)]
pub enum MembersError {
    /// An I/O failure.
    #[error("memberships I/O failed")]
    Io(#[from] io::Error),
    /// The bytes do not start with the memberships magic.
    #[error("not a memberships artifact")]
    Magic,
    /// The layout version is not the one this build reads.
    #[error("memberships layout version {found}, expected {expected}")]
    Version {
        /// The version found.
        found: u32,
        /// The version this build reads.
        expected: u32,
    },
}

/// The members of every reference set, by reference set SCTID.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Memberships {
    sets: BTreeMap<u64, RoaringBitmap>,
}

impl Memberships {
    /// No reference sets.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `concept` to the reference set `refset`.
    pub fn insert(&mut self, refset: u64, concept: Ordinal) {
        self.sets.entry(refset).or_default().insert(concept.index());
    }

    /// The heap bytes the membership bitmaps hold ([`crate::footprint`]).
    #[must_use]
    pub fn size_in_bytes(&self) -> usize {
        self.sets.values().fold(
            crate::footprint::entries::<u64, RoaringBitmap>(self.sets.len()),
            |total, set| total.saturating_add(crate::footprint::bitmap(set)),
        )
    }

    /// The members of `refset`, when the edition has it.
    #[must_use]
    pub fn members(&self, refset: u64) -> Option<&RoaringBitmap> {
        self.sets.get(&refset)
    }

    /// The reference set SCTIDs, ascending.
    pub fn refsets(&self) -> impl Iterator<Item = u64> + '_ {
        self.sets.keys().copied()
    }

    /// The number of reference sets.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sets.len()
    }

    /// Whether there are no reference sets.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    /// The number of memberships over every reference set.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.sets.values().map(RoaringBitmap::len).sum()
    }

    /// Writes the layout.
    ///
    /// # Errors
    ///
    /// Returns [`MembersError::Io`] when writing fails.
    pub fn write_to(&self, out: &mut impl Write) -> Result<(), MembersError> {
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        let count = crate::persist::u32_len(self.sets.len(), "the refset count")?;
        out.write_all(&count.to_le_bytes())?;
        for (refset, set) in &self.sets {
            out.write_all(&refset.to_le_bytes())?;
            let size = crate::persist::u32_len(set.serialized_size(), "a set size")?;
            out.write_all(&size.to_le_bytes())?;
            set.serialize_into(&mut *out)?;
        }
        Ok(())
    }

    /// Reads the layout.
    ///
    /// # Errors
    ///
    /// Returns [`MembersError`] for a truncated or foreign artifact.
    pub fn read_from(input: &mut impl Read) -> Result<Self, MembersError> {
        let mut magic = [0_u8; 8];
        input.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(MembersError::Magic);
        }
        let version = read_u32(input)?;
        if version != VERSION {
            return Err(MembersError::Version {
                found: version,
                expected: VERSION,
            });
        }
        let count = read_u32(input)?;
        let mut sets = BTreeMap::new();
        for _ in 0..count {
            let mut long = [0_u8; 8];
            input.read_exact(&mut long)?;
            let size = read_u32(input)?;
            let mut bytes = vec![0_u8; to_usize(size)];
            input.read_exact(&mut bytes)?;
            sets.insert(
                u64::from_le_bytes(long),
                RoaringBitmap::deserialize_from(bytes.as_slice())?,
            );
        }
        Ok(Self { sets })
    }
}

fn read_u32(input: &mut impl Read) -> io::Result<u32> {
    let mut buffer = [0_u8; 4];
    input.read_exact(&mut buffer)?;
    Ok(u32::from_le_bytes(buffer))
}

#[cfg(test)]
mod tests {
    use roaring::RoaringBitmap;

    use super::{MembersError, Memberships};
    use crate::ordinal::Ordinal;

    #[test]
    fn memberships_round_trip_and_reject_foreign_bytes() {
        let mut members = Memberships::new();
        members.insert(31_000_147_101, Ordinal::new(2));
        members.insert(31_000_147_101, Ordinal::new(3));
        members.insert(900_000_000_000_497_000, Ordinal::new(3));
        assert_eq!(members.len(), 2);
        assert_eq!(members.total(), 3);
        assert_eq!(
            members.members(31_000_147_101).map(RoaringBitmap::len),
            Some(2)
        );
        assert!(members.members(1).is_none());
        assert_eq!(members.refsets().next(), Some(31_000_147_101));
        let mut bytes = Vec::new();
        members.write_to(&mut bytes).expect("writes");
        assert_eq!(
            Memberships::read_from(&mut bytes.as_slice()).expect("reads"),
            members
        );
        assert!(matches!(
            Memberships::read_from(&mut b"XXXXXXXX\0\0\0\0".as_slice()),
            Err(MembersError::Magic)
        ));
    }
}
