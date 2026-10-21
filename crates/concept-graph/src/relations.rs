// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Typed adjacency: every edge carries a relationship type, and both
//! directions are materialized.
//!
//! The is-a hierarchy is [`crate::persist::Hierarchy`]; this is for the other
//! relationships a code system states between its concepts (`RxNorm`'s `REL`
//! and `RELA`, SNOMED CT's attributes when they are served as a graph). No
//! spec governs the layout: our own design. Little-endian, a magic and
//! version prefix, the type names, then the outgoing and the incoming
//! adjacency as offsets with parallel type and node arrays, each node's edges
//! sorted by type then node.

use std::io::{self, Read, Write};

use crate::ordinal::{Ordinal, to_usize};

const MAGIC: &[u8; 8] = b"FTRELS\0\0";
const VERSION: u32 = 1;

/// A failure while building, reading, or writing the relations.
#[derive(Debug, thiserror::Error)]
pub enum RelationsError {
    /// An edge names a node or a type beyond the declared counts.
    #[error("edge ({from}, {kind}, {target}) is out of range for {nodes} nodes and {types} types")]
    OutOfRange {
        /// The source node.
        from: u32,
        /// The type index.
        kind: u32,
        /// The target node.
        target: u32,
        /// The node count.
        nodes: u32,
        /// The type count.
        types: u32,
    },
    /// An I/O failure.
    #[error("relations I/O failed")]
    Io(#[from] io::Error),
    /// The bytes do not start with the relations magic.
    #[error("not a relations artifact")]
    Magic,
    /// The layout version is not the one this build reads.
    #[error("relations layout version {found}, expected {expected}")]
    Version {
        /// The version found.
        found: u32,
        /// The version this build reads.
        expected: u32,
    },
    /// The arrays are inconsistent.
    #[error("the relations arrays are inconsistent")]
    Inconsistent,
    /// A type name is not UTF-8.
    #[error("a relationship type name is not UTF-8")]
    Name(#[from] std::string::FromUtf8Error),
}

/// One direction of the adjacency.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Adjacency {
    /// `nodes + 1` offsets into `kinds` and `ends`.
    offsets: Vec<u32>,
    kinds: Vec<u32>,
    ends: Vec<u32>,
}

impl Adjacency {
    fn build(nodes: u32, mut edges: Vec<(u32, u32, u32)>) -> Self {
        edges.sort_unstable();
        edges.dedup();
        let mut offsets = Vec::with_capacity(to_usize(nodes).saturating_add(1));
        let mut kinds = Vec::with_capacity(edges.len());
        let mut ends = Vec::with_capacity(edges.len());
        let mut cursor = 0usize;
        for node in 0..nodes {
            offsets.push(u32::try_from(kinds.len()).unwrap_or(u32::MAX));
            while let Some(&(from, kind, to)) = edges.get(cursor) {
                if from != node {
                    break;
                }
                kinds.push(kind);
                ends.push(to);
                cursor = cursor.saturating_add(1);
            }
        }
        offsets.push(u32::try_from(kinds.len()).unwrap_or(u32::MAX));
        Self {
            offsets,
            kinds,
            ends,
        }
    }

    fn edges(&self, node: Ordinal) -> impl Iterator<Item = (u32, u32)> + '_ {
        let index = to_usize(node.index());
        let (start, end) = match (
            self.offsets.get(index),
            self.offsets.get(index.saturating_add(1)),
        ) {
            (Some(&s), Some(&e)) => (to_usize(s), to_usize(e)),
            _ => (0, 0),
        };
        let kinds = self.kinds.get(start..end).unwrap_or_default();
        let ends = self.ends.get(start..end).unwrap_or_default();
        kinds.iter().copied().zip(ends.iter().copied())
    }

    fn check(&self, nodes: u32) -> Result<(), RelationsError> {
        let consistent = self.offsets.len() == to_usize(nodes).saturating_add(1)
            && self.kinds.len() == self.ends.len()
            && self
                .offsets
                .last()
                .is_some_and(|&l| to_usize(l) == self.kinds.len())
            && self.offsets.windows(2).all(|w| w.first() <= w.get(1));
        consistent.then_some(()).ok_or(RelationsError::Inconsistent)
    }
}

/// The typed edges of a code system, both ways.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Relations {
    /// The relationship type names; an edge's type is an index into this list.
    types: Vec<String>,
    outgoing: Adjacency,
    incoming: Adjacency,
}

impl Relations {
    /// Builds the relations of `nodes` nodes from `(source, type, target)` edges.
    ///
    /// # Errors
    ///
    /// Returns [`RelationsError::OutOfRange`] for an edge beyond `nodes` or
    /// beyond the type list.
    pub fn build(
        nodes: u32,
        types: Vec<String>,
        edges: Vec<(Ordinal, u32, Ordinal)>,
    ) -> Result<Self, RelationsError> {
        let type_count = u32::try_from(types.len()).unwrap_or(u32::MAX);
        let mut forward = Vec::with_capacity(edges.len());
        let mut backward = Vec::with_capacity(edges.len());
        for (source, kind, target) in edges {
            let (s, t) = (source.index(), target.index());
            if s >= nodes || t >= nodes || kind >= type_count {
                return Err(RelationsError::OutOfRange {
                    from: s,
                    kind,
                    target: t,
                    nodes,
                    types: type_count,
                });
            }
            forward.push((s, kind, t));
            backward.push((t, kind, s));
        }
        Ok(Self {
            types,
            outgoing: Adjacency::build(nodes, forward),
            incoming: Adjacency::build(nodes, backward),
        })
    }

    /// The relationship type names.
    #[must_use]
    pub fn types(&self) -> &[String] {
        &self.types
    }

    /// The index of the type `name`.
    #[must_use]
    pub fn kind(&self, name: &str) -> Option<u32> {
        self.types
            .iter()
            .position(|t| t == name)
            .and_then(|i| u32::try_from(i).ok())
    }

    /// The node count.
    #[must_use]
    pub fn nodes(&self) -> u32 {
        u32::try_from(self.outgoing.offsets.len().saturating_sub(1)).unwrap_or(u32::MAX)
    }

    /// The edge count.
    #[must_use]
    pub fn edges(&self) -> usize {
        self.outgoing.ends.len()
    }

    /// The `(type, target)` pairs leaving `node`, by type then target.
    pub fn outgoing(&self, node: Ordinal) -> impl Iterator<Item = (u32, Ordinal)> + '_ {
        self.outgoing.edges(node).map(|(k, n)| (k, Ordinal::new(n)))
    }

    /// The `(type, source)` pairs arriving at `node`, by type then source.
    pub fn incoming(&self, node: Ordinal) -> impl Iterator<Item = (u32, Ordinal)> + '_ {
        self.incoming.edges(node).map(|(k, n)| (k, Ordinal::new(n)))
    }

    /// The sources of edges of type `kind` arriving at `node`.
    pub fn sources(&self, node: Ordinal, kind: u32) -> impl Iterator<Item = Ordinal> + '_ {
        self.incoming(node)
            .filter(move |(k, _)| *k == kind)
            .map(|(_, n)| n)
    }

    /// The targets of edges of type `kind` leaving `node`.
    pub fn targets(&self, node: Ordinal, kind: u32) -> impl Iterator<Item = Ordinal> + '_ {
        self.outgoing(node)
            .filter(move |(k, _)| *k == kind)
            .map(|(_, n)| n)
    }

    /// Writes the layout.
    ///
    /// # Errors
    ///
    /// Returns [`RelationsError::Io`] when writing fails.
    pub fn write_to(&self, out: &mut impl Write) -> Result<(), RelationsError> {
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        let count = crate::persist::u32_len(self.types.len(), "the type count")?;
        out.write_all(&count.to_le_bytes())?;
        for name in &self.types {
            let len = crate::persist::u32_len(name.len(), "a type name length")?;
            out.write_all(&len.to_le_bytes())?;
            out.write_all(name.as_bytes())?;
        }
        for side in [&self.outgoing, &self.incoming] {
            write_u32s(out, &side.offsets)?;
            write_u32s(out, &side.kinds)?;
            write_u32s(out, &side.ends)?;
        }
        Ok(())
    }

    /// Reads the layout.
    ///
    /// # Errors
    ///
    /// Returns [`RelationsError`] for a truncated, foreign, or inconsistent artifact.
    pub fn read_from(input: &mut impl Read) -> Result<Self, RelationsError> {
        let mut magic = [0_u8; 8];
        input.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(RelationsError::Magic);
        }
        let version = read_u32(input)?;
        if version != VERSION {
            return Err(RelationsError::Version {
                found: version,
                expected: VERSION,
            });
        }
        let count = read_u32(input)?;
        let mut types = Vec::with_capacity(to_usize(count));
        for _ in 0..count {
            let len = read_u32(input)?;
            let mut bytes = vec![0_u8; to_usize(len)];
            input.read_exact(&mut bytes)?;
            types.push(String::from_utf8(bytes)?);
        }
        let mut sides = Vec::with_capacity(2);
        for _ in 0..2 {
            sides.push(Adjacency {
                offsets: read_u32s(input)?,
                kinds: read_u32s(input)?,
                ends: read_u32s(input)?,
            });
        }
        let incoming = sides.pop().ok_or(RelationsError::Inconsistent)?;
        let outgoing = sides.pop().ok_or(RelationsError::Inconsistent)?;
        // A node count past u32::MAX is an inconsistent artifact; the conversion
        // error adds nothing to that.
        let Ok(nodes) = u32::try_from(outgoing.offsets.len().saturating_sub(1)) else {
            return Err(RelationsError::Inconsistent);
        };
        outgoing.check(nodes)?;
        incoming.check(nodes)?;
        Ok(Self {
            types,
            outgoing,
            incoming,
        })
    }
}

fn write_u32s(out: &mut impl Write, values: &[u32]) -> io::Result<()> {
    let len = crate::persist::u32_len(values.len(), "an array length")?;
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

#[cfg(test)]
mod tests {
    use super::{Relations, RelationsError};
    use crate::ordinal::Ordinal;

    #[test]
    fn edges_are_answered_both_ways_and_round_trip() {
        let o = Ordinal::new;
        let types = vec![String::from("has_ingredient"), String::from("isa")];
        let relations = Relations::build(
            4,
            types,
            vec![
                (o(2), 0, o(0)),
                (o(3), 0, o(0)),
                (o(3), 1, o(2)),
                (o(2), 0, o(0)),
            ],
        )
        .expect("builds");
        assert_eq!(relations.edges(), 3, "duplicates collapse");
        assert_eq!(relations.kind("isa"), Some(1));
        assert_eq!(relations.kind("part_of"), None);
        let sources: Vec<u32> = relations.sources(o(0), 0).map(Ordinal::index).collect();
        assert_eq!(sources, [2, 3]);
        let targets: Vec<u32> = relations.targets(o(3), 1).map(Ordinal::index).collect();
        assert_eq!(targets, [2]);
        assert_eq!(relations.outgoing(o(1)).count(), 0);
        let mut bytes = Vec::new();
        relations.write_to(&mut bytes).expect("writes");
        let back = Relations::read_from(&mut bytes.as_slice()).expect("reads");
        assert_eq!(back, relations);
        assert!(matches!(
            Relations::read_from(&mut b"XXXXXXXX\0\0\0\0".as_slice()),
            Err(RelationsError::Magic)
        ));
        assert!(matches!(
            Relations::build(2, Vec::new(), vec![(o(0), 0, o(1))]),
            Err(RelationsError::OutOfRange { .. })
        ));
    }
}
