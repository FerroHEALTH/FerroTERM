// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The active members of every concept-referencing reference set, with their
//! fields.
//!
//! [`crate::members::Memberships`] answers "is this concept a member" from a
//! bitmap; this holds the rows behind it (the effective time, the module, and
//! every additional field the reference set declares), so the ECL member
//! filters, the reference set field selection, and the history supplements
//! read the values. No spec governs the layout: our own design. Little-endian,
//! a magic and version prefix, then per reference set its SCTID, its field
//! names and kinds, the rows as parallel arrays, and the interned longs and
//! strings.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};

use roaring::RoaringBitmap;

use crate::ordinal::{Ordinal, to_usize};

const MAGIC: &[u8; 8] = b"FTMBRS\0\0";
const VERSION: u32 = 1;
const TAG_CONCEPT: u8 = 0;
const TAG_COMPONENT: u8 = 1;
const TAG_INTEGER: u8 = 2;
const TAG_STRING: u8 = 3;

/// A failure while building, reading, or writing the members.
#[derive(Debug, thiserror::Error)]
pub enum RefsetsError {
    /// A row has a different number of values than the table has fields.
    #[error("reference set {refset} row has {values} values for {fields} fields")]
    Arity {
        /// The reference set.
        refset: u64,
        /// The values in the row.
        values: usize,
        /// The fields declared.
        fields: usize,
    },
    /// More rows or interned values than the `u32` offsets address.
    #[error("too many reference set rows")]
    TooMany(#[source] std::num::TryFromIntError),
    /// An I/O failure.
    #[error("reference set members I/O failed")]
    Io(#[from] io::Error),
    /// The bytes do not start with the members magic.
    #[error("not a reference set members artifact")]
    Magic,
    /// The layout version is not the one this build reads.
    #[error("reference set members layout version {found}, expected {expected}")]
    Version {
        /// The version found.
        found: u32,
        /// The version this build reads.
        expected: u32,
    },
    /// The arrays are inconsistent.
    #[error("the reference set member arrays are inconsistent")]
    Inconsistent,
    /// A name or value is not UTF-8.
    #[error("a reference set field is not UTF-8")]
    Text(#[from] std::string::FromUtf8Error),
}

/// The kind of a reference set field, as its file name declares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// A component identifier (`c`).
    Component,
    /// An integer (`i`).
    Integer,
    /// A string (`s`).
    String,
}

/// A field value as built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    /// A concept of the edition.
    Concept(Ordinal),
    /// A component identifier that is not a concept of the edition.
    Component(u64),
    /// An integer.
    Integer(i64),
    /// A string.
    String(String),
}

/// A field value as read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueRef<'a> {
    /// A concept of the edition.
    Concept(Ordinal),
    /// A component identifier that is not a concept of the edition.
    Component(u64),
    /// An integer.
    Integer(i64),
    /// A string.
    String(&'a str),
}

/// One active member as built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberRow {
    /// The referenced concept.
    pub concept: Ordinal,
    /// The effective time as `YYYYMMDD`.
    pub effective_time: u32,
    /// The module SCTID.
    pub module: u64,
    /// The additional fields, in the table's field order.
    pub values: Vec<FieldValue>,
}

/// The members of one reference set.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Table {
    fields: Vec<String>,
    kinds: Vec<FieldKind>,
    concepts: Vec<u32>,
    times: Vec<u32>,
    modules: Vec<u64>,
    tags: Vec<u8>,
    payloads: Vec<u32>,
    longs: Vec<u64>,
    strings: Vec<String>,
    /// The member concepts; derived.
    members: RoaringBitmap,
}

impl Table {
    /// The heap bytes this table holds ([`crate::footprint`]).
    #[must_use]
    pub fn size_in_bytes(&self) -> usize {
        crate::footprint::strings(&self.fields)
            .saturating_add(crate::footprint::vector(&self.kinds))
            .saturating_add(crate::footprint::vector(&self.concepts))
            .saturating_add(crate::footprint::vector(&self.times))
            .saturating_add(crate::footprint::vector(&self.modules))
            .saturating_add(crate::footprint::vector(&self.tags))
            .saturating_add(crate::footprint::vector(&self.payloads))
            .saturating_add(crate::footprint::vector(&self.longs))
            .saturating_add(crate::footprint::strings(&self.strings))
            .saturating_add(crate::footprint::bitmap(&self.members))
    }

    /// The additional field names, in column order.
    #[must_use]
    pub fn fields(&self) -> &[String] {
        &self.fields
    }

    /// The additional field kinds, in column order.
    #[must_use]
    pub fn kinds(&self) -> &[FieldKind] {
        &self.kinds
    }

    /// The column of the field named `name`, case-insensitively.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<usize> {
        self.fields
            .iter()
            .position(|f| f.eq_ignore_ascii_case(name))
    }

    /// The number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.concepts.len()
    }

    /// Whether the table has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.concepts.is_empty()
    }

    /// The member concepts.
    #[must_use]
    pub fn members(&self) -> &RoaringBitmap {
        &self.members
    }

    /// The referenced concept of row `row`.
    #[must_use]
    pub fn concept(&self, row: usize) -> Option<Ordinal> {
        self.concepts.get(row).map(|&c| Ordinal::new(c))
    }

    /// The effective time of row `row`, as `YYYYMMDD`.
    #[must_use]
    pub fn effective_time(&self, row: usize) -> Option<u32> {
        self.times.get(row).copied()
    }

    /// The module SCTID of row `row`.
    #[must_use]
    pub fn module(&self, row: usize) -> Option<u64> {
        self.modules.get(row).copied()
    }

    /// The value of `field` (a column) in row `row`.
    #[must_use]
    pub fn value(&self, row: usize, field: usize) -> Option<ValueRef<'_>> {
        if field >= self.fields.len() {
            return None;
        }
        let index = row.checked_mul(self.fields.len())?.checked_add(field)?;
        let payload = to_usize(*self.payloads.get(index)?);
        Some(match *self.tags.get(index)? {
            TAG_CONCEPT => ValueRef::Concept(Ordinal::new(*self.payloads.get(index)?)),
            TAG_COMPONENT => ValueRef::Component(*self.longs.get(payload)?),
            TAG_INTEGER => {
                ValueRef::Integer(i64::from_le_bytes(self.longs.get(payload)?.to_le_bytes()))
            }
            _ => ValueRef::String(self.strings.get(payload)?),
        })
    }

    /// The rows whose `field` holds the concept `target`.
    pub fn rows_with(&self, field: usize, target: Ordinal) -> impl Iterator<Item = usize> + '_ {
        (0..self.len())
            .filter(move |&row| self.value(row, field) == Some(ValueRef::Concept(target)))
    }

    /// The rows whose referenced concept is `concept`.
    pub fn rows_of(&self, concept: Ordinal) -> impl Iterator<Item = usize> + '_ {
        self.concepts
            .iter()
            .enumerate()
            .filter(move |&(_, &held)| held == concept.index())
            .map(|(row, _)| row)
    }

    fn check(&self) -> Result<(), RefsetsError> {
        let rows = self.concepts.len();
        let cells = rows.saturating_mul(self.fields.len());
        let consistent = self.kinds.len() == self.fields.len()
            && self.times.len() == rows
            && self.modules.len() == rows
            && self.tags.len() == cells
            && self.payloads.len() == cells
            && self
                .tags
                .iter()
                .zip(&self.payloads)
                .all(|(&tag, &payload)| match tag {
                    TAG_CONCEPT => true,
                    TAG_COMPONENT | TAG_INTEGER => to_usize(payload) < self.longs.len(),
                    TAG_STRING => to_usize(payload) < self.strings.len(),
                    _ => false,
                });
        consistent.then_some(()).ok_or(RefsetsError::Inconsistent)
    }
}

/// The member tables of every reference set, by SCTID.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RefsetMembers {
    tables: BTreeMap<u64, Table>,
}

impl RefsetMembers {
    /// The heap bytes every member table holds ([`crate::footprint`]).
    #[must_use]
    pub fn size_in_bytes(&self) -> usize {
        self.tables.values().fold(
            crate::footprint::entries::<u64, Table>(self.tables.len()),
            |total, table| total.saturating_add(table.size_in_bytes()),
        )
    }

    /// An empty set of tables.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the table of `refset` with `fields` (names and kinds beyond the
    /// referenced component) and its active `rows`.
    ///
    /// # Errors
    ///
    /// Returns [`RefsetsError::Arity`] when a row's values do not match the
    /// fields, and [`RefsetsError::TooMany`] past `u32`.
    pub fn insert(
        &mut self,
        refset: u64,
        fields: &[(String, FieldKind)],
        rows: Vec<MemberRow>,
    ) -> Result<(), RefsetsError> {
        let mut table = Table {
            fields: fields.iter().map(|(name, _)| name.clone()).collect(),
            kinds: fields.iter().map(|(_, kind)| *kind).collect(),
            ..Table::default()
        };
        let mut interned: BTreeMap<String, u32> = BTreeMap::new();
        let mut longs_seen: BTreeMap<u64, u32> = BTreeMap::new();
        for row in rows {
            if row.values.len() != table.fields.len() {
                return Err(RefsetsError::Arity {
                    refset,
                    values: row.values.len(),
                    fields: table.fields.len(),
                });
            }
            table.concepts.push(row.concept.index());
            table.members.insert(row.concept.index());
            table.times.push(row.effective_time);
            table.modules.push(row.module);
            for value in row.values {
                let (tag, payload) = match value {
                    FieldValue::Concept(concept) => (TAG_CONCEPT, concept.index()),
                    FieldValue::Component(id) => (
                        TAG_COMPONENT,
                        intern_long(&mut table.longs, &mut longs_seen, id)?,
                    ),
                    FieldValue::Integer(value) => (
                        TAG_INTEGER,
                        intern_long(
                            &mut table.longs,
                            &mut longs_seen,
                            u64::from_le_bytes(value.to_le_bytes()),
                        )?,
                    ),
                    FieldValue::String(text) => {
                        let index = if let Some(&index) = interned.get(&text) {
                            index
                        } else {
                            let index = u32::try_from(table.strings.len())
                                .map_err(RefsetsError::TooMany)?;
                            table.strings.push(text.clone());
                            interned.insert(text, index);
                            index
                        };
                        (TAG_STRING, index)
                    }
                };
                table.tags.push(tag);
                table.payloads.push(payload);
            }
        }
        u32::try_from(table.concepts.len()).map_err(RefsetsError::TooMany)?;
        self.tables.insert(refset, table);
        Ok(())
    }

    /// The table of `refset`.
    #[must_use]
    pub fn table(&self, refset: u64) -> Option<&Table> {
        self.tables.get(&refset)
    }

    /// The reference sets, ascending.
    pub fn refsets(&self) -> impl Iterator<Item = u64> + '_ {
        self.tables.keys().copied()
    }

    /// The number of reference sets.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tables.len()
    }

    /// Whether there are no reference sets.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    /// The number of rows over every reference set.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.tables
            .values()
            .map(|t| u64::try_from(t.len()).unwrap_or(u64::MAX))
            .sum()
    }

    /// Writes the layout.
    ///
    /// # Errors
    ///
    /// Returns [`RefsetsError::Io`] when writing fails.
    pub fn write_to(&self, out: &mut impl Write) -> Result<(), RefsetsError> {
        let count = |len: usize| u32::try_from(len).map_err(RefsetsError::TooMany);
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        out.write_all(&count(self.tables.len())?.to_le_bytes())?;
        for (refset, table) in &self.tables {
            out.write_all(&refset.to_le_bytes())?;
            out.write_all(&count(table.fields.len())?.to_le_bytes())?;
            for (name, kind) in table.fields.iter().zip(&table.kinds) {
                write_text(out, name)?;
                out.write_all(&[match kind {
                    FieldKind::Component => 0,
                    FieldKind::Integer => 1,
                    FieldKind::String => 2,
                }])?;
            }
            out.write_all(&count(table.concepts.len())?.to_le_bytes())?;
            write_u32s(out, &table.concepts)?;
            write_u32s(out, &table.times)?;
            for module in &table.modules {
                out.write_all(&module.to_le_bytes())?;
            }
            out.write_all(&table.tags)?;
            write_u32s(out, &table.payloads)?;
            out.write_all(&count(table.longs.len())?.to_le_bytes())?;
            for long in &table.longs {
                out.write_all(&long.to_le_bytes())?;
            }
            out.write_all(&count(table.strings.len())?.to_le_bytes())?;
            for text in &table.strings {
                write_text(out, text)?;
            }
        }
        Ok(())
    }

    /// Reads the layout and derives the member bitmaps.
    ///
    /// # Errors
    ///
    /// Returns [`RefsetsError`] for a truncated, inconsistent, or foreign
    /// artifact.
    pub fn read_from(input: &mut impl Read) -> Result<Self, RefsetsError> {
        let mut magic = [0_u8; 8];
        input.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(RefsetsError::Magic);
        }
        let version = read_u32(input)?;
        if version != VERSION {
            return Err(RefsetsError::Version {
                found: version,
                expected: VERSION,
            });
        }
        let mut tables = BTreeMap::new();
        for _ in 0..read_u32(input)? {
            let refset = read_u64(input)?;
            let field_count = to_usize(read_u32(input)?);
            let mut fields = Vec::with_capacity(field_count);
            let mut kinds = Vec::with_capacity(field_count);
            for _ in 0..field_count {
                fields.push(read_text(input)?);
                let mut kind = [0_u8; 1];
                input.read_exact(&mut kind)?;
                kinds.push(match kind[0] {
                    0 => FieldKind::Component,
                    1 => FieldKind::Integer,
                    2 => FieldKind::String,
                    _ => return Err(RefsetsError::Inconsistent),
                });
            }
            let rows = to_usize(read_u32(input)?);
            let concepts = read_u32s(input, rows)?;
            let times = read_u32s(input, rows)?;
            let mut modules = Vec::with_capacity(rows);
            for _ in 0..rows {
                modules.push(read_u64(input)?);
            }
            let cells = rows.saturating_mul(field_count);
            let mut tags = vec![0_u8; cells];
            input.read_exact(&mut tags)?;
            let payloads = read_u32s(input, cells)?;
            let mut longs = Vec::new();
            for _ in 0..read_u32(input)? {
                longs.push(read_u64(input)?);
            }
            let mut strings = Vec::new();
            for _ in 0..read_u32(input)? {
                strings.push(read_text(input)?);
            }
            let members = concepts.iter().copied().collect();
            let table = Table {
                fields,
                kinds,
                concepts,
                times,
                modules,
                tags,
                payloads,
                longs,
                strings,
                members,
            };
            table.check()?;
            tables.insert(refset, table);
        }
        Ok(Self { tables })
    }
}

fn intern_long(
    longs: &mut Vec<u64>,
    seen: &mut BTreeMap<u64, u32>,
    value: u64,
) -> Result<u32, RefsetsError> {
    if let Some(&index) = seen.get(&value) {
        return Ok(index);
    }
    let index = u32::try_from(longs.len()).map_err(RefsetsError::TooMany)?;
    longs.push(value);
    seen.insert(value, index);
    Ok(index)
}

fn write_text(out: &mut impl Write, text: &str) -> Result<(), RefsetsError> {
    let len = u32::try_from(text.len()).map_err(RefsetsError::TooMany)?;
    out.write_all(&len.to_le_bytes())?;
    out.write_all(text.as_bytes())?;
    Ok(())
}

fn read_text(input: &mut impl Read) -> Result<String, RefsetsError> {
    let len = to_usize(read_u32(input)?);
    let mut bytes = vec![0_u8; len];
    input.read_exact(&mut bytes)?;
    Ok(String::from_utf8(bytes)?)
}

fn write_u32s(out: &mut impl Write, values: &[u32]) -> Result<(), RefsetsError> {
    for value in values {
        out.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}

fn read_u32(input: &mut impl Read) -> Result<u32, RefsetsError> {
    let mut bytes = [0_u8; 4];
    input.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(input: &mut impl Read) -> Result<u64, RefsetsError> {
    let mut bytes = [0_u8; 8];
    input.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_u32s(input: &mut impl Read, count: usize) -> Result<Vec<u32>, RefsetsError> {
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
    use super::{FieldKind, FieldValue, MemberRow, RefsetMembers, RefsetsError, ValueRef};
    use crate::ordinal::Ordinal;

    fn sample() -> RefsetMembers {
        let mut members = RefsetMembers::new();
        members
            .insert(
                42,
                &[
                    (String::from("mapGroup"), FieldKind::Integer),
                    (String::from("mapTarget"), FieldKind::String),
                    (String::from("correlationId"), FieldKind::Component),
                ],
                vec![
                    MemberRow {
                        concept: Ordinal::new(2),
                        effective_time: 20_240_101,
                        module: 99,
                        values: vec![
                            FieldValue::Integer(1),
                            FieldValue::String(String::from("J45.9")),
                            FieldValue::Concept(Ordinal::new(7)),
                        ],
                    },
                    MemberRow {
                        concept: Ordinal::new(3),
                        effective_time: 20_230_731,
                        module: 99,
                        values: vec![
                            FieldValue::Integer(-2),
                            FieldValue::String(String::from("J45.9")),
                            FieldValue::Component(123_456_789_012),
                        ],
                    },
                ],
            )
            .expect("inserts");
        members
            .insert(7, &[], vec![])
            .expect("an empty simple reference set");
        members
    }

    #[test]
    fn rows_answer_by_field_and_the_layout_round_trips() {
        let members = sample();
        let table = members.table(42).expect("table");
        assert_eq!(table.len(), 2);
        assert_eq!(table.field("MAPTARGET"), Some(1));
        assert_eq!(table.value(0, 0), Some(ValueRef::Integer(1)));
        assert_eq!(table.value(1, 0), Some(ValueRef::Integer(-2)));
        assert_eq!(table.value(1, 1), Some(ValueRef::String("J45.9")));
        assert_eq!(table.value(0, 2), Some(ValueRef::Concept(Ordinal::new(7))));
        assert_eq!(
            table.value(1, 2),
            Some(ValueRef::Component(123_456_789_012))
        );
        assert_eq!(table.value(1, 3), None);
        assert_eq!(table.effective_time(1), Some(20_230_731));
        assert_eq!(table.members().iter().collect::<Vec<_>>(), [2, 3]);
        assert_eq!(table.rows_with(2, Ordinal::new(7)).collect::<Vec<_>>(), [0]);
        assert_eq!(table.rows_of(Ordinal::new(3)).collect::<Vec<_>>(), [1]);
        assert_eq!(
            table.rows_of(Ordinal::new(9)).collect::<Vec<_>>(),
            Vec::<usize>::new(),
            "a concept the reference set does not name owns no row"
        );
        assert_eq!(members.total(), 2);
        let mut bytes = Vec::new();
        members.write_to(&mut bytes).expect("writes");
        assert_eq!(
            RefsetMembers::read_from(&mut bytes.as_slice()).expect("reads"),
            members
        );
        assert!(matches!(
            RefsetMembers::read_from(&mut b"XXXXXXXX\0\0\0\0".as_slice()),
            Err(RefsetsError::Magic)
        ));
        let mut bad = RefsetMembers::new();
        assert!(matches!(
            bad.insert(
                1,
                &[(String::from("f"), FieldKind::String)],
                vec![MemberRow {
                    concept: Ordinal::new(0),
                    effective_time: 0,
                    module: 0,
                    values: Vec::new(),
                }]
            ),
            Err(RefsetsError::Arity { .. })
        ));
    }
}
