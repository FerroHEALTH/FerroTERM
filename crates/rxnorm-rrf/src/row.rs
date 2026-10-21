// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The typed rows, by the documented column positions.

use std::path::Path;

use crate::RrfError;

/// A row type read from a byte record.
pub trait FromRecord: Sized {
    /// Reads the row at `line` of `path`.
    ///
    /// # Errors
    ///
    /// Returns [`RrfError`] when the record is short, not UTF-8, or carries
    /// a malformed identifier.
    fn from_record(record: &csv::ByteRecord, path: &Path, line: u64) -> Result<Self, RrfError>;
}

/// The fields of a record as strings, checked for count and UTF-8.
struct Fields<'a> {
    record: &'a csv::ByteRecord,
    path: &'a Path,
    line: u64,
}

impl Fields<'_> {
    fn new<'a>(
        record: &'a csv::ByteRecord,
        path: &'a Path,
        line: u64,
        expected: usize,
    ) -> Result<Fields<'a>, RrfError> {
        if record.len() < expected {
            return Err(RrfError::Columns {
                path: path.to_path_buf(),
                line,
                columns: record.len(),
                expected,
            });
        }
        Ok(Fields { record, path, line })
    }

    fn text(&self, index: usize) -> Result<String, RrfError> {
        let bytes = self.record.get(index).unwrap_or_default();
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|source| RrfError::Utf8 {
                path: self.path.to_path_buf(),
                line: self.line,
                source,
            })
    }

    fn optional(&self, index: usize) -> Result<Option<String>, RrfError> {
        let text = self.text(index)?;
        Ok((!text.is_empty()).then_some(text))
    }

    fn id(&self, index: usize) -> Result<u64, RrfError> {
        let text = self.text(index)?;
        text.parse().map_err(|source| RrfError::Identifier {
            path: self.path.to_path_buf(),
            line: self.line,
            value: text,
            source,
        })
    }

    fn optional_id(&self, index: usize) -> Result<Option<u64>, RrfError> {
        match self.optional(index)? {
            None => Ok(None),
            Some(text) => text
                .parse()
                .map(Some)
                .map_err(|source| RrfError::Identifier {
                    path: self.path.to_path_buf(),
                    line: self.line,
                    value: text,
                    source,
                }),
        }
    }
}

/// One `RXNCONSO` row: an atom (a name from one source) of a concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atom {
    /// `RXCUI`: the concept.
    pub rxcui: u64,
    /// `LAT`: the language (`ENG`).
    pub language: String,
    /// `RXAUI`: the atom.
    pub rxaui: u64,
    /// `SAB`: the source vocabulary (`RXNORM`, `MTHSPL`, ...).
    pub sab: String,
    /// `TTY`: the term type (`SCD`, `SBD`, `IN`, ...).
    pub tty: String,
    /// `CODE`: the source's own code for the atom.
    pub code: String,
    /// `STR`: the name.
    pub name: String,
    /// `SUPPRESS`: `N`, `O`, `Y`, or `E`.
    pub suppress: String,
}

impl FromRecord for Atom {
    fn from_record(record: &csv::ByteRecord, path: &Path, line: u64) -> Result<Self, RrfError> {
        let f = Fields::new(record, path, line, 18)?;
        Ok(Self {
            rxcui: f.id(0)?,
            language: f.text(1)?,
            rxaui: f.id(7)?,
            sab: f.text(11)?,
            tty: f.text(12)?,
            code: f.text(13)?,
            name: f.text(14)?,
            suppress: f.text(16)?,
        })
    }
}

/// One `RXNREL` row: the second concept or atom has `rel` (and `rela`) to the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    /// `RXCUI1`, absent for an atom-level row.
    pub rxcui1: Option<u64>,
    /// `RXAUI1`, absent for a concept-level row.
    pub rxaui1: Option<u64>,
    /// `REL`: `RO`, `RN`, `RB`, `SY`, `PAR`, `CHD`, `SIB`.
    pub rel: String,
    /// `RXCUI2`, absent for an atom-level row.
    pub rxcui2: Option<u64>,
    /// `RXAUI2`, absent for a concept-level row.
    pub rxaui2: Option<u64>,
    /// `RELA`: the relationship label (`has_ingredient`, `isa`, ...).
    pub rela: Option<String>,
    /// `SAB`: the source asserting the relationship.
    pub sab: String,
}

impl FromRecord for Relationship {
    fn from_record(record: &csv::ByteRecord, path: &Path, line: u64) -> Result<Self, RrfError> {
        let f = Fields::new(record, path, line, 16)?;
        Ok(Self {
            rxcui1: f.optional_id(0)?,
            rxaui1: f.optional_id(1)?,
            rel: f.text(3)?,
            rxcui2: f.optional_id(4)?,
            rxaui2: f.optional_id(5)?,
            rela: f.optional(7)?,
            sab: f.text(10)?,
        })
    }
}

/// One `RXNSAT` row: an attribute of a concept or atom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    /// `RXCUI`.
    pub rxcui: u64,
    /// `RXAUI`, absent for a concept-level attribute.
    pub rxaui: Option<u64>,
    /// `ATN`: the attribute name (`NDC`, `RXN_AVAILABLE_STRENGTH`, ...).
    pub name: String,
    /// `SAB`: the source asserting the attribute.
    pub sab: String,
    /// `ATV`: the value.
    pub value: String,
    /// `SUPPRESS`.
    pub suppress: String,
}

impl FromRecord for Attribute {
    fn from_record(record: &csv::ByteRecord, path: &Path, line: u64) -> Result<Self, RrfError> {
        let f = Fields::new(record, path, line, 13)?;
        Ok(Self {
            rxcui: f.id(0)?,
            rxaui: f.optional_id(3)?,
            name: f.text(8)?,
            sab: f.text(9)?,
            value: f.text(10)?,
            suppress: f.text(11)?,
        })
    }
}

/// One `RXNSTY` row: a semantic type of a concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticType {
    /// `RXCUI`.
    pub rxcui: u64,
    /// `TUI`: the semantic type identifier (`T121`).
    pub tui: String,
    /// `STY`: the semantic type name (`Pharmacologic Substance`).
    pub name: String,
}

impl FromRecord for SemanticType {
    fn from_record(record: &csv::ByteRecord, path: &Path, line: u64) -> Result<Self, RrfError> {
        let f = Fields::new(record, path, line, 6)?;
        Ok(Self {
            rxcui: f.id(0)?,
            tui: f.text(1)?,
            name: f.text(3)?,
        })
    }
}
