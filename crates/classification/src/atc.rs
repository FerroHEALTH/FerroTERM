// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The WHO ATC/DDD classification, from the index table or the G-Standaard file.
//!
//! The index is the table the WHO Collaborating Centre sells, a spreadsheet
//! exported as CSV with the columns `ATC code`, `ATC level name`, `DDD`, `U`,
//! `Adm.R`, and `Note`; the G-Standaard file `BST801T` carries fixed-length
//! records with the code, the Dutch and the English name, and an indicator.
//!
//! ATC codes have five levels of one, three, four, five, and seven characters
//! (<https://atcddd.fhi.no/atc/structure_and_principles/>), each code with the
//! one parent its prefix names, so the tree is built from the codes alone. A
//! DDD row (value, unit, administration route, note) becomes a `ddd` rubric of
//! its substance; FHIR defines no properties or filters for ATC
//! (<https://terminology.hl7.org/CodeSystem-v3-WC.html>), so every rubric kind
//! here is the project's own.

use std::path::{Path, PathBuf};

use crate::{Class, Classification, PREFERRED, Rubric, collapse};

/// The system URI (<https://terminology.hl7.org/CodeSystem-v3-WC.html>).
pub const SYSTEM: &str = "http://www.whocc.no/atc";
/// The rubric kind of a defined daily dose (`2 g O`, with a note when the index carries one).
pub const DDD: &str = "ddd";
/// The rubric kind of the G-Standaard indicator column (`ATKIND`).
pub const INDICATOR: &str = "indicator";
/// The class kinds, root first.
pub const KINDS: [&str; 5] = [
    "anatomical-main-group",
    "therapeutic-subgroup",
    "pharmacological-subgroup",
    "chemical-subgroup",
    "chemical-substance",
];
/// The record length of `BST801T`.
const BST801_RECORD: usize = 192;

/// A failure to read the classification.
#[derive(Debug, thiserror::Error)]
pub enum AtcError {
    /// A file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The table does not parse.
    #[error("cannot parse {path}")]
    Csv {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: csv::Error,
    },
    /// The table lacks a column.
    #[error("{path} has no column `{column}`")]
    Column {
        /// The path.
        path: PathBuf,
        /// The column looked for.
        column: &'static str,
    },
    /// A code has none of the five ATC lengths.
    #[error("{path}: `{code}` is not an ATC code (1, 3, 4, 5, or 7 characters)")]
    Code {
        /// The path.
        path: PathBuf,
        /// The code.
        code: String,
    },
    /// A code's parent is not in the table.
    #[error("`{code}` has no parent `{parent}` in the table")]
    MissingParent {
        /// The code.
        code: String,
        /// The parent its prefix names.
        parent: String,
    },
    /// A `BST801T` record is shorter than its layout.
    #[error("{path}:{line}: the record is shorter than {BST801_RECORD} characters")]
    Short {
        /// The path.
        path: PathBuf,
        /// The 1-based line.
        line: usize,
    },
}

/// The class kind of an ATC code by its length.
#[must_use]
pub fn kind_of(code: &str) -> Option<&'static str> {
    match code.len() {
        1 => KINDS.first().copied(),
        3 => KINDS.get(1).copied(),
        4 => KINDS.get(2).copied(),
        5 => KINDS.get(3).copied(),
        7 => KINDS.get(4).copied(),
        _ => None,
    }
}

/// The parent an ATC code's prefix names, `None` for a main group.
#[must_use]
pub fn parent_of(code: &str) -> Option<String> {
    let length = match code.len() {
        3 => 1,
        4 => 3,
        5 => 4,
        7 => 5,
        _ => return None,
    };
    code.get(..length).map(str::to_owned)
}

/// The classes so far, by code, in first-seen order.
#[derive(Default)]
struct Table {
    classes: Vec<Class>,
}

impl Table {
    fn class(&mut self, code: &str, path: &Path) -> Result<&mut Class, AtcError> {
        let kind = kind_of(code).ok_or_else(|| AtcError::Code {
            path: path.to_path_buf(),
            code: code.to_owned(),
        })?;
        if let Some(index) = self.classes.iter().position(|c| c.code == code) {
            return self
                .classes
                .get_mut(index)
                .ok_or_else(|| AtcError::MissingParent {
                    code: code.to_owned(),
                    parent: String::new(),
                });
        }
        self.classes.push(Class {
            code: code.to_owned(),
            kind: kind.to_owned(),
            parent: parent_of(code),
            usage: None,
            valid: None,
            active: true,
            rubrics: Vec::new(),
        });
        self.classes
            .last_mut()
            .ok_or_else(|| AtcError::MissingParent {
                code: code.to_owned(),
                parent: String::new(),
            })
    }

    fn finish(self, version: Option<String>, language: &str) -> Result<Classification, AtcError> {
        for class in &self.classes {
            if let Some(parent) = &class.parent
                && !self.classes.iter().any(|c| &c.code == parent)
            {
                return Err(AtcError::MissingParent {
                    code: class.code.clone(),
                    parent: parent.clone(),
                });
            }
        }
        Ok(Classification {
            name: String::from("ATC"),
            title: String::from(
                "Anatomical Therapeutic Chemical Classification System with Defined Daily Doses",
            ),
            version,
            language: language.to_owned(),
            kinds: KINDS.iter().map(|k| (*k).to_owned()).collect(),
            classes: self.classes,
            ..Classification::default()
        })
    }
}

/// The delimiter of a CSV export: the first of `,`, `;`, and tab found on the header line.
fn delimiter(text: &str) -> u8 {
    let header = text.lines().next().unwrap_or_default();
    b",;\t"
        .iter()
        .copied()
        .find(|d| header.contains(char::from(*d)))
        .unwrap_or(b',')
}

/// Reads the WHO index exported as CSV.
///
/// Every row names a code and its level name; a row with a `DDD` value adds
/// a `ddd` rubric (`value unit route`, the note appended after a semicolon).
/// Rows repeat a code once per DDD.
///
/// # Errors
///
/// Returns [`AtcError`] when the file does not read or parse, lacks the
/// code or name column, or names a code outside the five levels or without
/// its parent.
pub fn read_index(path: &Path, version: Option<&str>) -> Result<Classification, AtcError> {
    let text = std::fs::read_to_string(path).map_err(|source| AtcError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter(&text))
        .flexible(true)
        .from_reader(text.as_bytes());
    let csv_error = |source| AtcError::Csv {
        path: path.to_path_buf(),
        source,
    };
    let headers: Vec<String> = reader
        .headers()
        .map_err(csv_error)?
        .iter()
        .map(|h| h.trim().to_lowercase())
        .collect();
    let column = |names: &[&str], required: &'static str| -> Result<Option<usize>, AtcError> {
        let found = headers.iter().position(|h| names.iter().any(|n| h == n));
        if found.is_none() && !required.is_empty() {
            return Err(AtcError::Column {
                path: path.to_path_buf(),
                column: required,
            });
        }
        Ok(found)
    };
    let code_at = column(&["atc code", "atc_code", "code"], "ATC code")?;
    let name_at = column(
        &["atc level name", "atc_level_name", "name"],
        "ATC level name",
    )?;
    let ddd_at = column(&["ddd"], "")?;
    let unit_at = column(&["u", "uom", "unit"], "")?;
    let route_at = column(&["adm.r", "adm_r", "route", "adm.route"], "")?;
    let note_at = column(&["note", "notes"], "")?;
    let mut table = Table::default();
    for record in reader.records() {
        let record = record.map_err(csv_error)?;
        let field = |at: Option<usize>| {
            at.and_then(|i| record.get(i))
                .map(str::trim)
                .filter(|v| !v.is_empty())
        };
        let Some(code) = field(code_at) else {
            continue;
        };
        let code = code.to_uppercase();
        let class = table.class(&code, path)?;
        if let Some(name) = field(name_at)
            && !class.rubrics.iter().any(|r| r.kind == PREFERRED)
        {
            class.rubrics.push(Rubric {
                kind: PREFERRED.to_owned(),
                language: String::from("en"),
                text: collapse(name),
            });
        }
        if let Some(ddd) = field(ddd_at) {
            let mut text = vec![ddd.to_owned()];
            text.extend(field(unit_at).map(str::to_owned));
            text.extend(field(route_at).map(str::to_owned));
            let mut text = text.join(" ");
            if let Some(note) = field(note_at) {
                text.push_str("; ");
                text.push_str(note);
            }
            class.rubrics.push(Rubric {
                kind: DDD.to_owned(),
                language: String::from("en"),
                text: collapse(&text),
            });
        }
    }
    table.finish(version.map(str::to_owned), "en")
}

/// Reads the G-Standaard file `BST801T`.
///
/// The record layout is `ATCODE` at positions 6 to 13, `ATOMS` (the Dutch
/// name) at 14 to 93, `ATOMSE` (the English name) at 94 to 173, and `ATKIND`
/// (the indicator) at 174
/// (<https://www.z-index.nl/documentatie/bestandsbeschrijvingen/bestand/BST801T>).
///
/// The file is Latin-1; a record with mutation code `9` (removed) is skipped.
///
/// # Errors
///
/// Returns [`AtcError`] when the file does not read, a record is short, or a
/// code is outside the five levels or without its parent.
pub fn read_bst801(path: &Path, version: Option<&str>) -> Result<Classification, AtcError> {
    let bytes = std::fs::read(path).map_err(|source| AtcError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let text: String = bytes.iter().map(|&b| char::from(b)).collect();
    let mut table = Table::default();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        if chars.len() < BST801_RECORD {
            return Err(AtcError::Short {
                path: path.to_path_buf(),
                line: index.saturating_add(1),
            });
        }
        let field = |from: usize, to: usize| -> String {
            chars
                .get(from..to)
                .unwrap_or_default()
                .iter()
                .collect::<String>()
                .trim()
                .to_owned()
        };
        if field(4, 5) == "9" {
            continue;
        }
        let code = field(5, 13);
        let class = table.class(&code, path)?;
        for (language, text) in [("nl", field(13, 93)), ("en", field(93, 173))] {
            if !text.is_empty() {
                class.rubrics.push(Rubric {
                    kind: PREFERRED.to_owned(),
                    language: language.to_owned(),
                    text: collapse(&text),
                });
            }
        }
        let indicator = field(173, 174);
        if !indicator.is_empty() {
            class.rubrics.push(Rubric {
                kind: INDICATOR.to_owned(),
                language: String::from("nl"),
                text: indicator,
            });
        }
    }
    table.finish(version.map(str::to_owned), "nl")
}

/// Reads `path` as the WHO index (a `.csv`, `.tsv`, or `.txt` with a header)
/// or as `BST801T` (a file whose name starts with `BST801`).
///
/// # Errors
///
/// Returns the errors of [`read_index`] and [`read_bst801`].
pub fn read(path: &Path, version: Option<&str>) -> Result<Classification, AtcError> {
    let name = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_uppercase();
    if name.starts_with("BST801") {
        read_bst801(path, version)
    } else {
        read_index(path, version)
    }
}

#[cfg(test)]
mod tests {
    use super::{kind_of, parent_of};

    #[test]
    fn the_levels_follow_the_code_length() {
        assert_eq!(kind_of("A"), Some("anatomical-main-group"));
        assert_eq!(kind_of("A10BA02"), Some("chemical-substance"));
        assert_eq!(kind_of("A10BA0"), None);
        assert_eq!(parent_of("A10BA02").as_deref(), Some("A10BA"));
        assert_eq!(parent_of("A10").as_deref(), Some("A"));
        assert_eq!(parent_of("A"), None);
    }
}
