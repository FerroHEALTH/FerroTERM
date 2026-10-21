// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A versioned binary layout for the graph, for the offline build to store.
//!
//! No spec governs this: our own design. Little-endian, a magic and version
//! prefix, then the is-a adjacency (offsets and targets), then the two
//! closure bitmap lists in roaring's portable serialization. The store crate
//! places these bytes in its artifact; the server reads them back at startup.

use std::io::{self, Read, Write};

use roaring::RoaringBitmap;

use crate::closure::Closure;
use crate::csr::{Csr, CsrError};
use crate::ordinal::to_usize;

const MAGIC: &[u8; 8] = b"FTGRAPH\0";
const VERSION: u32 = 1;

/// A failure while reading or writing the layout.
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// An I/O failure.
    #[error("graph I/O failed")]
    Io(#[from] io::Error),
    /// The bytes do not start with the graph magic.
    #[error("not a graph artifact")]
    Magic,
    /// The layout version is not the one this build reads.
    #[error("graph layout version {found}, expected {expected}")]
    Version {
        /// The version found.
        found: u32,
        /// The version this build reads.
        expected: u32,
    },
    /// The adjacency arrays are inconsistent.
    #[error(transparent)]
    Csr(#[from] CsrError),
    /// The bitmap lists do not match the node count.
    #[error("{found} closure sets for {nodes} nodes")]
    Count {
        /// The number of sets found.
        found: usize,
        /// The node count.
        nodes: u32,
    },
}

/// The is-a adjacency and its closure, as one artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hierarchy {
    /// Child-to-parent adjacency.
    pub is_a: Csr,
    /// The transitive closure of `is_a`.
    pub closure: Closure,
}

impl Hierarchy {
    /// The heap bytes the adjacency and the closure hold
    /// ([`crate::footprint`]).
    #[must_use]
    pub fn size_in_bytes(&self) -> usize {
        self.is_a
            .size_in_bytes()
            .saturating_add(self.closure.size_in_bytes())
    }

    /// Writes the layout.
    ///
    /// # Errors
    ///
    /// Returns [`PersistError::Io`] when writing fails.
    pub fn write_to(&self, out: &mut impl Write) -> Result<(), PersistError> {
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        write_u32s(out, self.is_a.offsets())?;
        write_u32s(out, self.is_a.targets())?;
        write_bitmaps(out, self.closure.ancestor_sets())?;
        write_bitmaps(out, self.closure.descendant_sets())?;
        Ok(())
    }

    /// Reads the layout.
    ///
    /// # Errors
    ///
    /// Returns [`PersistError`] for a truncated, foreign, or inconsistent artifact.
    pub fn read_from(input: &mut impl Read) -> Result<Self, PersistError> {
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
        let offsets = read_u32s(input)?;
        let targets = read_u32s(input)?;
        let is_a = Csr::from_parts(offsets, targets)?;
        let nodes = is_a.nodes();
        let ancestors = read_bitmaps(input)?;
        let descendants = read_bitmaps(input)?;
        for list in [&ancestors, &descendants] {
            if list.len() != to_usize(nodes) {
                return Err(PersistError::Count {
                    found: list.len(),
                    nodes,
                });
            }
        }
        Ok(Self {
            is_a,
            closure: Closure::from_parts(ancestors, descendants),
        })
    }
}

/// A length or size past `u32::MAX`, which the artifact layout cannot store.
#[derive(Debug, thiserror::Error)]
#[error("{what} exceeds the u32 the artifact layout stores")]
pub(crate) struct TooLong {
    what: &'static str,
    #[source]
    source: std::num::TryFromIntError,
}

/// `len` as the `u32` the layout stores, or the I/O error naming `what` overflowed.
pub(crate) fn u32_len(len: usize, what: &'static str) -> io::Result<u32> {
    u32::try_from(len).map_err(|source| io::Error::other(TooLong { what, source }))
}

fn write_u32s(out: &mut impl Write, values: &[u32]) -> io::Result<()> {
    let len = u32_len(values.len(), "an array length")?;
    out.write_all(&len.to_le_bytes())?;
    for value in values {
        out.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}

fn read_u32(input: &mut impl Read) -> io::Result<u32> {
    let mut buffer = [0_u8; 4];
    input.read_exact(&mut buffer)?;
    Ok(u32::from_le_bytes(buffer))
}

fn read_u32s(input: &mut impl Read) -> io::Result<Vec<u32>> {
    let len = read_u32(input)?;
    let mut values = Vec::with_capacity(to_usize(len));
    for _ in 0..len {
        values.push(read_u32(input)?);
    }
    Ok(values)
}

fn write_bitmaps(out: &mut impl Write, sets: &[RoaringBitmap]) -> io::Result<()> {
    let len = u32_len(sets.len(), "the set count")?;
    out.write_all(&len.to_le_bytes())?;
    for set in sets {
        let size = u32_len(set.serialized_size(), "a set size")?;
        out.write_all(&size.to_le_bytes())?;
        set.serialize_into(&mut *out)?;
    }
    Ok(())
}

fn read_bitmaps(input: &mut impl Read) -> io::Result<Vec<RoaringBitmap>> {
    let len = read_u32(input)?;
    let mut sets = Vec::with_capacity(to_usize(len));
    for _ in 0..len {
        let size = read_u32(input)?;
        let mut bytes = vec![0_u8; to_usize(size)];
        input.read_exact(&mut bytes)?;
        sets.push(RoaringBitmap::deserialize_from(bytes.as_slice())?);
    }
    Ok(sets)
}

#[cfg(test)]
mod tests {
    use super::{Hierarchy, PersistError};
    use crate::closure::Closure;
    use crate::csr::Csr;
    use crate::ordinal::Ordinal;

    #[test]
    fn the_layout_round_trips_and_rejects_foreign_bytes() {
        let o = Ordinal::new;
        let is_a = Csr::build(4, [(o(1), o(0)), (o(2), o(0)), (o(3), o(1)), (o(3), o(2))])
            .expect("builds");
        let closure = Closure::compute(&is_a).expect("acyclic");
        let hierarchy = Hierarchy { is_a, closure };
        let mut bytes = Vec::new();
        hierarchy.write_to(&mut bytes).expect("writes");
        let back = Hierarchy::read_from(&mut bytes.as_slice()).expect("reads");
        assert_eq!(back, hierarchy);
        assert!(matches!(
            Hierarchy::read_from(&mut b"nope".as_slice()),
            Err(PersistError::Io(_))
        ));
        assert!(matches!(
            Hierarchy::read_from(&mut b"XXXXXXXX\0\0\0\0".as_slice()),
            Err(PersistError::Magic)
        ));
    }
}
