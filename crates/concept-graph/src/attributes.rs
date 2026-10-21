//! The attribute relationships of an edition.
//!
//! Per source concept, every active relationship that is not is-a, with its
//! role group, its type, and its value (a concept, a number, or a string),
//! plus the inverted index the ECL evaluator reads (`type = value` as the
//! sources of a type with a value in a set).
//!
//! No spec governs the layout: our own design. Little-endian, a magic and
//! version prefix, the type SCTIDs, the node count, `nodes + 1` row offsets,
//! then the rows as parallel arrays (group, type index, value tag, payload)
//! and the interned strings. The inverted index is derived on read.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};

use roaring::RoaringBitmap;

use crate::ordinal::{Ordinal, to_usize};

const MAGIC: &[u8; 8] = b"FTATTR\0\0";
const VERSION: u32 = 1;
const TAG_CONCEPT: u8 = 0;
const TAG_NUMBER: u8 = 1;
const TAG_STRING: u8 = 2;

/// A failure while building, reading, or writing the attributes.
#[derive(Debug, thiserror::Error)]
pub enum AttributesError {
    /// A row names a node or a type beyond the declared counts.
    #[error("attribute row ({from}, {kind}) is out of range for {nodes} nodes and {types} types")]
    OutOfRange {
        /// The source node.
        from: u32,
        /// The type index.
        kind: u32,
        /// The node count.
        nodes: u32,
        /// The type count.
        types: u32,
    },
    /// More rows or strings than the `u32` offsets address.
    #[error("too many attribute rows")]
    TooMany(#[source] std::num::TryFromIntError),
    /// An I/O failure.
    #[error("attributes I/O failed")]
    Io(#[from] io::Error),
    /// The bytes do not start with the attributes magic.
    #[error("not an attributes artifact")]
    Magic,
    /// The layout version is not the one this build reads.
    #[error("attributes layout version {found}, expected {expected}")]
    Version {
        /// The version found.
        found: u32,
        /// The version this build reads.
        expected: u32,
    },
    /// The arrays are inconsistent.
    #[error("the attribute arrays are inconsistent")]
    Inconsistent,
    /// An interned string is not UTF-8.
    #[error("an attribute value is not UTF-8")]
    Text(#[from] std::string::FromUtf8Error),
}

/// An attribute value as built.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    /// The destination concept.
    Concept(Ordinal),
    /// A concrete number, as the release spells it (`500`, `0.25`, `-1`).
    Number(String),
    /// A concrete string.
    String(String),
}

/// One attribute relationship as built.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge {
    /// The source concept.
    pub source: Ordinal,
    /// The role group; `0` is ungrouped.
    pub group: u32,
    /// The attribute type, an index into the type list.
    pub kind: u32,
    /// The value.
    pub value: Value,
}

/// An attribute value as read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueRef<'a> {
    /// The destination concept.
    Concept(Ordinal),
    /// A concrete number, as the release spells it.
    Number(&'a str),
    /// A concrete string.
    String(&'a str),
}

/// One attribute relationship as read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row<'a> {
    /// The role group; `0` is ungrouped.
    pub group: u32,
    /// The attribute type, an index into the type list.
    pub kind: u32,
    /// The value.
    pub value: ValueRef<'a>,
}

/// The sources of one type by destination concept, and all of them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Inverted {
    /// The destination concepts, sorted.
    targets: Vec<u32>,
    /// `targets.len() + 1` offsets into `sources`.
    offsets: Vec<u32>,
    /// The sources per destination, each run sorted.
    sources: Vec<u32>,
    /// Every source with a relationship of the type, whatever its value.
    all_sources: RoaringBitmap,
}

impl Inverted {
    fn build(mut pairs: Vec<(u32, u32)>, all_sources: RoaringBitmap) -> Self {
        pairs.sort_unstable();
        pairs.dedup();
        let mut targets = Vec::new();
        let mut offsets = Vec::new();
        let mut sources = Vec::with_capacity(pairs.len());
        for (target, source) in pairs {
            if targets.last() != Some(&target) {
                targets.push(target);
                offsets.push(u32::try_from(sources.len()).unwrap_or(u32::MAX));
            }
            sources.push(source);
        }
        offsets.push(u32::try_from(sources.len()).unwrap_or(u32::MAX));
        Self {
            targets,
            offsets,
            sources,
            all_sources,
        }
    }

    fn size_in_bytes(&self) -> usize {
        crate::footprint::vector(&self.targets)
            .saturating_add(crate::footprint::vector(&self.offsets))
            .saturating_add(crate::footprint::vector(&self.sources))
            .saturating_add(crate::footprint::bitmap(&self.all_sources))
    }

    fn sources(&self, target: u32) -> &[u32] {
        let Ok(index) = self.targets.binary_search(&target) else {
            return &[];
        };
        match (
            self.offsets.get(index),
            self.offsets.get(index.saturating_add(1)),
        ) {
            (Some(&start), Some(&end)) => self
                .sources
                .get(to_usize(start)..to_usize(end))
                .unwrap_or_default(),
            _ => &[],
        }
    }
}

/// The attribute relationships of every concept.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Attributes {
    /// The attribute type SCTIDs; a row's type is an index into this list.
    types: Vec<u64>,
    /// `nodes + 1` offsets into the row arrays.
    offsets: Vec<u32>,
    groups: Vec<u32>,
    kinds: Vec<u32>,
    tags: Vec<u8>,
    payloads: Vec<u32>,
    strings: Vec<String>,
    /// Per type, the inverted index; derived, not persisted.
    inverted: Vec<Inverted>,
}

impl Attributes {
    /// The heap bytes the rows hold ([`crate::footprint`]).
    #[must_use]
    pub fn rows_size_in_bytes(&self) -> usize {
        crate::footprint::vector(&self.types)
            .saturating_add(crate::footprint::vector(&self.offsets))
            .saturating_add(crate::footprint::vector(&self.groups))
            .saturating_add(crate::footprint::vector(&self.kinds))
            .saturating_add(crate::footprint::vector(&self.tags))
            .saturating_add(crate::footprint::vector(&self.payloads))
            .saturating_add(crate::footprint::strings(&self.strings))
    }

    /// The heap bytes the inverted index holds.
    ///
    /// It is derived when the artifact opens rather than read from it, so it
    /// is reported apart from the rows it was derived from.
    #[must_use]
    pub fn inverted_size_in_bytes(&self) -> usize {
        self.inverted.iter().fold(
            crate::footprint::vector(&self.inverted),
            |total, inverted| total.saturating_add(inverted.size_in_bytes()),
        )
    }

    /// The heap bytes the rows and the inverted index hold together.
    #[must_use]
    pub fn size_in_bytes(&self) -> usize {
        self.rows_size_in_bytes()
            .saturating_add(self.inverted_size_in_bytes())
    }

    /// Builds the attributes of `nodes` concepts from `edges`, whose types
    /// index `types`.
    ///
    /// # Errors
    ///
    /// Returns [`AttributesError::OutOfRange`] for a row beyond `nodes` or
    /// beyond the type list, and [`AttributesError::TooMany`] past `u32`.
    pub fn build(
        nodes: u32,
        types: Vec<u64>,
        mut edges: Vec<Edge>,
    ) -> Result<Self, AttributesError> {
        let type_count = u32::try_from(types.len()).map_err(AttributesError::TooMany)?;
        Self::check_range(nodes, type_count, &edges)?;
        edges.sort_unstable();
        edges.dedup();
        u32::try_from(edges.len()).map_err(AttributesError::TooMany)?;
        let mut attributes = Self::pack(nodes, types, &edges)?;
        attributes.derive();
        Ok(attributes)
    }

    /// Refuses an edge whose source, type, or concept value lies outside the
    /// `nodes` concepts and `type_count` attribute types.
    fn check_range(nodes: u32, type_count: u32, edges: &[Edge]) -> Result<(), AttributesError> {
        for edge in edges {
            let target_ok = match edge.value {
                Value::Concept(target) => target.index() < nodes,
                Value::Number(_) | Value::String(_) => true,
            };
            if edge.source.index() >= nodes || edge.kind >= type_count || !target_ok {
                return Err(AttributesError::OutOfRange {
                    from: edge.source.index(),
                    kind: edge.kind,
                    nodes,
                    types: type_count,
                });
            }
        }
        Ok(())
    }

    /// The row arrays of `nodes` concepts read off the sorted `edges`, with the
    /// number and string values interned; the inverted index is not derived yet.
    fn pack(nodes: u32, types: Vec<u64>, edges: &[Edge]) -> Result<Self, AttributesError> {
        let mut interned: BTreeMap<String, u32> = BTreeMap::new();
        let mut strings = Vec::new();
        let mut intern = |text: &str| -> Result<u32, AttributesError> {
            if let Some(&index) = interned.get(text) {
                return Ok(index);
            }
            let index = u32::try_from(strings.len()).map_err(AttributesError::TooMany)?;
            strings.push(text.to_owned());
            interned.insert(text.to_owned(), index);
            Ok(index)
        };
        let mut offsets = Vec::with_capacity(to_usize(nodes).saturating_add(1));
        let (mut groups, mut kinds, mut tags, mut payloads) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut cursor = 0_usize;
        for node in 0..nodes {
            offsets.push(u32::try_from(groups.len()).unwrap_or(u32::MAX));
            while let Some(edge) = edges.get(cursor) {
                if edge.source.index() != node {
                    break;
                }
                groups.push(edge.group);
                kinds.push(edge.kind);
                let (tag, payload) = match &edge.value {
                    Value::Concept(target) => (TAG_CONCEPT, target.index()),
                    Value::Number(text) => (TAG_NUMBER, intern(text)?),
                    Value::String(text) => (TAG_STRING, intern(text)?),
                };
                tags.push(tag);
                payloads.push(payload);
                cursor = cursor.saturating_add(1);
            }
        }
        offsets.push(u32::try_from(groups.len()).unwrap_or(u32::MAX));
        Ok(Self {
            types,
            offsets,
            groups,
            kinds,
            tags,
            payloads,
            strings,
            inverted: Vec::new(),
        })
    }

    /// Builds the inverted index from the rows.
    fn derive(&mut self) {
        let mut pairs: Vec<Vec<(u32, u32)>> = vec![Vec::new(); self.types.len()];
        let mut all: Vec<RoaringBitmap> = vec![RoaringBitmap::new(); self.types.len()];
        for node in 0..self.nodes() {
            for row in self.rows(Ordinal::new(node)) {
                let kind = to_usize(row.kind);
                if let Some(set) = all.get_mut(kind) {
                    set.insert(node);
                }
                if let (ValueRef::Concept(target), Some(list)) = (row.value, pairs.get_mut(kind)) {
                    list.push((target.index(), node));
                }
            }
        }
        self.inverted = pairs
            .into_iter()
            .zip(all)
            .map(|(pairs, all_sources)| Inverted::build(pairs, all_sources))
            .collect();
    }

    /// The attribute type SCTIDs, in type-index order.
    #[must_use]
    pub fn types(&self) -> &[u64] {
        &self.types
    }

    /// The type index of `sctid`.
    #[must_use]
    pub fn kind(&self, sctid: u64) -> Option<u32> {
        self.types
            .iter()
            .position(|&t| t == sctid)
            .and_then(|i| u32::try_from(i).ok())
    }

    /// The number of concepts.
    #[must_use]
    pub fn nodes(&self) -> u32 {
        u32::try_from(self.offsets.len().saturating_sub(1)).unwrap_or(u32::MAX)
    }

    /// The number of rows.
    #[must_use]
    pub fn edges(&self) -> usize {
        self.groups.len()
    }

    /// The rows of `source`, sorted by group, then type, then value.
    pub fn rows(&self, source: Ordinal) -> impl Iterator<Item = Row<'_>> + '_ {
        let index = to_usize(source.index());
        let (start, end) = match (
            self.offsets.get(index),
            self.offsets.get(index.saturating_add(1)),
        ) {
            (Some(&s), Some(&e)) => (to_usize(s), to_usize(e)),
            _ => (0, 0),
        };
        (start..end).filter_map(move |i| {
            Some(Row {
                group: *self.groups.get(i)?,
                kind: *self.kinds.get(i)?,
                value: match (*self.tags.get(i)?, *self.payloads.get(i)?) {
                    (TAG_CONCEPT, target) => ValueRef::Concept(Ordinal::new(target)),
                    (TAG_NUMBER, text) => ValueRef::Number(self.strings.get(to_usize(text))?),
                    (_, text) => ValueRef::String(self.strings.get(to_usize(text))?),
                },
            })
        })
    }

    /// The sources with a relationship of type `kind` to `target`, sorted.
    #[must_use]
    pub fn sources(&self, kind: u32, target: Ordinal) -> &[u32] {
        self.inverted
            .get(to_usize(kind))
            .map_or(&[], |inverted| inverted.sources(target.index()))
    }

    /// Every source with a relationship of type `kind`, whatever its value.
    #[must_use]
    pub fn sources_of_kind(&self, kind: u32) -> Option<&RoaringBitmap> {
        self.inverted.get(to_usize(kind)).map(|i| &i.all_sources)
    }

    /// The destination concepts of type `kind`, sorted.
    #[must_use]
    pub fn targets_of_kind(&self, kind: u32) -> &[u32] {
        self.inverted
            .get(to_usize(kind))
            .map_or(&[], |i| i.targets.as_slice())
    }

    /// Writes the layout.
    ///
    /// # Errors
    ///
    /// Returns [`AttributesError::Io`] when writing fails.
    pub fn write_to(&self, out: &mut impl Write) -> Result<(), AttributesError> {
        let count = |len: usize| u32::try_from(len).map_err(AttributesError::TooMany);
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        out.write_all(&count(self.types.len())?.to_le_bytes())?;
        for sctid in &self.types {
            out.write_all(&sctid.to_le_bytes())?;
        }
        out.write_all(&self.nodes().to_le_bytes())?;
        write_u32s(out, &self.offsets)?;
        out.write_all(&count(self.groups.len())?.to_le_bytes())?;
        write_u32s(out, &self.groups)?;
        write_u32s(out, &self.kinds)?;
        out.write_all(&self.tags)?;
        write_u32s(out, &self.payloads)?;
        out.write_all(&count(self.strings.len())?.to_le_bytes())?;
        for text in &self.strings {
            out.write_all(&count(text.len())?.to_le_bytes())?;
            out.write_all(text.as_bytes())?;
        }
        Ok(())
    }

    /// Reads the layout and derives the inverted index.
    ///
    /// # Errors
    ///
    /// Returns [`AttributesError`] for a truncated, inconsistent, or foreign
    /// artifact.
    pub fn read_from(input: &mut impl Read) -> Result<Self, AttributesError> {
        let mut magic = [0_u8; 8];
        input.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(AttributesError::Magic);
        }
        let version = read_u32(input)?;
        if version != VERSION {
            return Err(AttributesError::Version {
                found: version,
                expected: VERSION,
            });
        }
        let type_count = read_u32(input)?;
        let mut types = Vec::with_capacity(to_usize(type_count));
        for _ in 0..type_count {
            let mut long = [0_u8; 8];
            input.read_exact(&mut long)?;
            types.push(u64::from_le_bytes(long));
        }
        let nodes = read_u32(input)?;
        let offsets = read_u32s(input, to_usize(nodes).saturating_add(1))?;
        let rows = to_usize(read_u32(input)?);
        let groups = read_u32s(input, rows)?;
        let kinds = read_u32s(input, rows)?;
        let mut tags = vec![0_u8; rows];
        input.read_exact(&mut tags)?;
        let payloads = read_u32s(input, rows)?;
        let string_count = read_u32(input)?;
        let mut strings = Vec::with_capacity(to_usize(string_count));
        for _ in 0..string_count {
            let len = to_usize(read_u32(input)?);
            let mut bytes = vec![0_u8; len];
            input.read_exact(&mut bytes)?;
            strings.push(String::from_utf8(bytes)?);
        }
        let consistent = offsets.last().is_some_and(|&l| to_usize(l) == rows)
            && offsets.windows(2).all(|w| w.first() <= w.get(1))
            && kinds.iter().all(|&k| to_usize(k) < types.len())
            && tags
                .iter()
                .zip(&payloads)
                .all(|(&tag, &payload)| match tag {
                    TAG_CONCEPT => payload < nodes,
                    TAG_NUMBER | TAG_STRING => to_usize(payload) < strings.len(),
                    _ => false,
                });
        if !consistent {
            return Err(AttributesError::Inconsistent);
        }
        let mut attributes = Self {
            types,
            offsets,
            groups,
            kinds,
            tags,
            payloads,
            strings,
            inverted: Vec::new(),
        };
        attributes.derive();
        Ok(attributes)
    }
}

fn write_u32s(out: &mut impl Write, values: &[u32]) -> Result<(), AttributesError> {
    for value in values {
        out.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}

fn read_u32(input: &mut impl Read) -> Result<u32, AttributesError> {
    let mut bytes = [0_u8; 4];
    input.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u32s(input: &mut impl Read, count: usize) -> Result<Vec<u32>, AttributesError> {
    let mut bytes = vec![0_u8; count.saturating_mul(4)];
    input.read_exact(&mut bytes)?;
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| u32::from_le_bytes(*chunk))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{Attributes, AttributesError, Edge, Value, ValueRef};
    use crate::ordinal::Ordinal;

    fn sample() -> Attributes {
        let edge = |source: u32, group: u32, kind: u32, value: Value| Edge {
            source: Ordinal::new(source),
            group,
            kind,
            value,
        };
        Attributes::build(
            5,
            vec![100, 200],
            vec![
                edge(1, 1, 0, Value::Concept(Ordinal::new(3))),
                edge(1, 1, 1, Value::Number(String::from("4"))),
                edge(2, 0, 0, Value::Concept(Ordinal::new(3))),
                edge(2, 2, 1, Value::Number(String::from("4"))),
                edge(2, 0, 0, Value::Concept(Ordinal::new(3))),
                edge(4, 0, 1, Value::String(String::from("blue"))),
            ],
        )
        .expect("builds")
    }

    #[test]
    fn rows_are_grouped_and_the_inverted_index_answers_by_type_and_target() {
        let attributes = sample();
        assert_eq!(attributes.edges(), 5, "the duplicate row is dropped");
        let cat: Vec<_> = attributes.rows(Ordinal::new(1)).collect();
        assert_eq!(cat.len(), 2);
        assert_eq!(cat[0].group, 1);
        assert_eq!(cat[0].value, ValueRef::Concept(Ordinal::new(3)));
        assert_eq!(cat[1].value, ValueRef::Number("4"));
        assert_eq!(attributes.sources(0, Ordinal::new(3)), [1, 2]);
        assert_eq!(attributes.sources(1, Ordinal::new(3)), []);
        assert_eq!(attributes.targets_of_kind(0), [3]);
        assert_eq!(
            attributes
                .sources_of_kind(1)
                .expect("kind")
                .iter()
                .collect::<Vec<_>>(),
            [1, 2, 4]
        );
        assert_eq!(attributes.kind(200), Some(1));
        assert_eq!(attributes.kind(300), None);
        assert_eq!(
            attributes.rows(Ordinal::new(4)).next().expect("row").value,
            ValueRef::String("blue")
        );
        assert!(attributes.rows(Ordinal::new(0)).next().is_none());
    }

    #[test]
    fn the_layout_round_trips_and_refuses_bad_input() {
        let attributes = sample();
        let mut bytes = Vec::new();
        attributes.write_to(&mut bytes).expect("writes");
        let again = Attributes::read_from(&mut bytes.as_slice()).expect("reads");
        assert_eq!(again, attributes);
        assert!(matches!(
            Attributes::read_from(&mut b"nope".as_slice()),
            Err(AttributesError::Io(_))
        ));
        assert!(matches!(
            Attributes::build(
                2,
                vec![1],
                vec![Edge {
                    source: Ordinal::new(1),
                    group: 0,
                    kind: 1,
                    value: Value::Concept(Ordinal::new(0)),
                }]
            ),
            Err(AttributesError::OutOfRange { kind: 1, .. })
        ));
        let mut wrong = bytes.clone();
        wrong[0] = b'X';
        assert!(matches!(
            Attributes::read_from(&mut wrong.as_slice()),
            Err(AttributesError::Magic)
        ));
    }
}
