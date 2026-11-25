// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The NPU terminology (Nomenclature for Properties and Units), from the code
//! file the IFCC publishes (`npu-codes-latest.csv`,
//! <https://npu-terminology.org/npu-database/>).
//!
//! The file is comma-separated with a header row; each row is one code with
//! its short and full definition, the system, component, kind-of-property,
//! and unit parts, the speciality, the scale type, the replacement links,
//! and the `active` flag. NPU is a flat list. The file names no release, so
//! the build is given one.

use std::path::Path;

use crate::sheet::{self, Column, SheetError};
use crate::{Class, Classification, PREFERRED, Rubric};

/// The system URI (HL7 Terminology, `NamingSystem-NPU`).
pub const SYSTEM: &str = "http://npu-terminology.org";
/// The designation kind of the full definition.
pub const FULL: &str = "full-definition";
/// The class kind of every code.
pub const KIND: &str = "npu-code";

/// The code, the two definitions, and the active flag, then every part kept
/// as a property under the column's own name.
const COLUMNS: [Column; 4] = [
    Column {
        name: "npu_code",
        aliases: &["npu_code"],
        required: true,
    },
    Column {
        name: "short_definition",
        aliases: &["short_definition"],
        required: true,
    },
    Column {
        name: "full_definition",
        aliases: &["full_definition"],
        required: false,
    },
    Column {
        name: "active",
        aliases: &["active"],
        required: false,
    },
];

/// The columns served as properties, by header name.
pub const PROPERTIES: [&str; 12] = [
    "system",
    "sys_spec",
    "component",
    "comp_spec",
    "kind_of_property",
    "proc",
    "unit",
    "specialty",
    "scale_type",
    "replaces",
    "replaced_by",
    "effective_to",
];

/// Reads the NPU code file at `path` as the release `version`.
///
/// A row whose `active` is `0` or `false` is an inactive code.
///
/// # Errors
///
/// Returns [`SheetError`] when the file does not read or lacks the code or
/// short-definition column.
pub fn read(path: &Path, version: &str) -> Result<Classification, SheetError> {
    let sheet = sheet::read(path)?;
    let header = sheet.header(path, &COLUMNS)?;
    let names: Vec<String> = sheet
        .rows
        .get(header.row)
        .map(|r| r.iter().map(|c| c.to_lowercase()).collect())
        .unwrap_or_default();
    let property_at: Vec<(&str, usize)> = PROPERTIES
        .iter()
        .filter_map(|p| names.iter().position(|n| n == p).map(|i| (*p, i)))
        .collect();
    let mut classes = Vec::new();
    for (code, row) in sheet.coded(&header) {
        let mut rubrics = Vec::new();
        if let Some(short) = header.cell(row, 1) {
            rubrics.push(Rubric::english(PREFERRED, short));
        }
        if let Some(full) = header.cell(row, 2) {
            rubrics.push(Rubric::english(FULL, full));
        }
        for (name, at) in &property_at {
            if let Some(value) = row
                .get(*at)
                .filter(|v| !v.is_empty() && !v.starts_with("0000-00-00"))
            {
                rubrics.push(Rubric::english(name, value));
            }
        }
        let active = !matches!(
            header.cell(row, 3).map(str::to_ascii_lowercase).as_deref(),
            Some("0" | "false")
        );
        classes.push(Class {
            code: code.to_owned(),
            kind: KIND.to_owned(),
            active,
            rubrics,
            ..Class::default()
        });
    }
    Ok(Classification {
        name: String::from("NPU"),
        title: String::from("Nomenclature for Properties and Units"),
        version: Some(version.to_owned()),
        language: String::from("en"),
        kinds: vec![KIND.to_owned()],
        classes,
        designation_kinds: vec![PREFERRED.to_owned(), FULL.to_owned()],
        hierarchy: None,
    })
}
