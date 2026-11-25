// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! ISCO-08, the International Standard Classification of Occupations.
//!
//! The source is the structure workbook the ILO publishes
//! (`ISCO-08 EN Structure and definitions.xlsx`,
//! <https://ilostat.ilo.org/methods/concepts-and-definitions/classification-occupation/>).
//!
//! The workbook's first sheet has one row per group with the columns `Level`,
//! `ISCO 08 Code`, `Title EN`, `Definition`, `Tasks include`, `Included
//! occupations`, `Excluded occupations`, and `Notes`. The four levels (major,
//! sub-major, minor, and unit group) have codes of one to four digits, each
//! nested in the group its prefix names.

use std::path::Path;

use crate::sheet::{self, Column, SheetError};
use crate::{Class, Classification, PREFERRED, Rubric, prefix_parents};

/// The system URI (HL7 Terminology, `NamingSystem-ISCO08`).
pub const SYSTEM: &str = "urn:oid:2.16.840.1.113883.2.9.6.2.7";
/// The version recorded when the build is given none: the 2008 revision.
pub const VERSION: &str = "2008";
/// The class kinds by level, root first.
pub const KINDS: [&str; 4] = [
    "major-group",
    "sub-major-group",
    "minor-group",
    "unit-group",
];

const COLUMNS: [Column; 8] = [
    Column {
        name: "ISCO 08 Code",
        aliases: &["isco 08 code", "isco-08 code", "isco08 code", "code"],
        required: true,
    },
    Column {
        name: "Title EN",
        aliases: &["title en", "title"],
        required: true,
    },
    Column {
        name: "Level",
        aliases: &["level"],
        required: false,
    },
    Column {
        name: "Definition",
        aliases: &["definition"],
        required: false,
    },
    Column {
        name: "Tasks include",
        aliases: &["tasks include"],
        required: false,
    },
    Column {
        name: "Included occupations",
        aliases: &["included occupations"],
        required: false,
    },
    Column {
        name: "Excluded occupations",
        aliases: &["excluded occupations"],
        required: false,
    },
    Column {
        name: "Notes",
        aliases: &["notes"],
        required: false,
    },
];

/// The rubric kinds of the text columns after the title, in column order.
pub const NOTES: [&str; 5] = [
    "definition",
    "tasks",
    "included-occupations",
    "excluded-occupations",
    "note",
];

/// Reads the ISCO-08 structure at `path`, a workbook or its delimited export.
///
/// # Errors
///
/// Returns [`SheetError`] when the file does not read or lacks the code or
/// title column.
pub fn read(path: &Path, version: Option<&str>) -> Result<Classification, SheetError> {
    let sheet = sheet::read(path)?;
    let header = sheet.header(path, &COLUMNS)?;
    let mut classes = Vec::new();
    for (code, row) in sheet.coded(&header) {
        let Some(kind) = code.len().checked_sub(1).and_then(|i| KINDS.get(i)) else {
            continue;
        };
        let mut rubrics = Vec::new();
        if let Some(title) = header.cell(row, 1) {
            rubrics.push(Rubric::english(PREFERRED, title));
        }
        for (offset, kind) in NOTES.iter().enumerate() {
            if let Some(text) = header.cell(row, offset.saturating_add(3)) {
                rubrics.push(Rubric::english(kind, text));
            }
        }
        classes.push(Class {
            code: code.to_owned(),
            kind: (*kind).to_owned(),
            rubrics,
            ..Class::default()
        });
    }
    prefix_parents(&mut classes);
    Ok(Classification {
        name: String::from("ISCO-08"),
        title: String::from("International Standard Classification of Occupations 2008"),
        version: Some(version.unwrap_or(VERSION).to_owned()),
        language: String::from("en"),
        kinds: KINDS.iter().map(|k| (*k).to_owned()).collect(),
        classes,
        designation_kinds: vec![PREFERRED.to_owned()],
        ..Classification::default()
    })
}
