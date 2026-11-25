// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The European Medical Device Nomenclature, from the workbook the European
//! Commission publishes (`EMDN v<year>_EN.xlsx`,
//! <https://webgate.ec.europa.eu/dyna2/emdn/>).
//!
//! The sheet is named for the release (`EMDN_v.2026`) and its header row
//! carries `CATEGORY`, `CATEGORY DESCRIPTION`, `CODE`, `TERM EMDN`, `LEVEL`,
//! and `TERMINAL LEVEL (YES/NO)`. A code is a category letter followed by
//! two digits per level below it, so each code nests in the code its prefix
//! names.

use std::path::Path;

use crate::sheet::{self, Column, SheetError};
use crate::{Class, Classification, PREFERRED, Rubric, prefix_parents};

/// The system URI: the canonical of the one published FHIR `CodeSystem` for
/// EMDN (`ans.fr.terminologies`, `CodeSystem-terminologie-emdn`); HL7
/// Terminology registers none.
pub const SYSTEM: &str = "https://smt.esante.gouv.fr/terminologie-emdn";
/// The class kind of a category (level 1).
pub const CATEGORY: &str = "category";
/// The class kind of every level below a category.
pub const TYPE: &str = "type";
/// The rubric kind of the level number.
pub const LEVEL: &str = "level";
/// The rubric kind of the terminal flag (`YES`, `NO`).
pub const TERMINAL: &str = "terminal";

const COLUMNS: [Column; 4] = [
    Column {
        name: "CODE",
        aliases: &["code"],
        required: true,
    },
    Column {
        name: "TERM EMDN",
        aliases: &["term emdn", "term"],
        required: false,
    },
    Column {
        name: "LEVEL",
        aliases: &["level"],
        required: false,
    },
    Column {
        name: "TERMINAL LEVEL (YES/NO)",
        aliases: &["terminal level (yes/no)", "terminal level"],
        required: false,
    },
];

/// The term column: a header starting `TERM EMDN`, which carries the release (`TERM EMDN (v.2026)`).
fn term_column(row: &[String]) -> Option<usize> {
    row.iter()
        .position(|c| c.to_lowercase().starts_with("term emdn"))
}

/// The release a sheet or header names (`EMDN_v.2026`, `(v.2026)`): the digits after `v.`.
fn release_in(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let at = lower.find("v.")?;
    let digits: String = lower
        .get(at.saturating_add(2)..)?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (!digits.is_empty()).then_some(digits)
}

/// Reads the EMDN workbook at `path`; `version` overrides the release the
/// sheet names.
///
/// # Errors
///
/// Returns [`SheetError`] when the file does not read or has no `CODE` column.
pub fn read(path: &Path, version: Option<&str>) -> Result<Classification, SheetError> {
    let sheet = sheet::read(path)?;
    let header = sheet.header(path, &COLUMNS)?;
    let header_row = sheet.rows.get(header.row).cloned().unwrap_or_default();
    let term_at = term_column(&header_row).or_else(|| header.column(1));
    let release = version.map(str::to_owned).or_else(|| {
        sheet.name.as_deref().and_then(release_in).or_else(|| {
            term_at
                .and_then(|i| header_row.get(i))
                .and_then(|h| release_in(h))
        })
    });
    let mut classes = Vec::new();
    for (code, row) in sheet.coded(&header) {
        let mut rubrics = Vec::new();
        if let Some(term) = term_at.and_then(|i| row.get(i)).filter(|t| !t.is_empty()) {
            rubrics.push(Rubric::english(PREFERRED, term));
        }
        rubrics.extend(header.rubrics(row, &[(2, LEVEL), (3, TERMINAL)]));
        classes.push(Class {
            code: code.to_owned(),
            kind: if code.len() == 1 { CATEGORY } else { TYPE }.to_owned(),
            rubrics,
            ..Class::default()
        });
    }
    prefix_parents(&mut classes);
    Ok(Classification {
        name: String::from("EMDN"),
        title: String::from("European Medical Device Nomenclature"),
        version: release,
        language: String::from("en"),
        kinds: vec![CATEGORY.to_owned(), TYPE.to_owned()],
        classes,
        designation_kinds: vec![PREFERRED.to_owned()],
        ..Classification::default()
    })
}

#[cfg(test)]
mod tests {
    use super::release_in;

    #[test]
    fn the_release_is_read_from_the_sheet_or_header_name() {
        assert_eq!(release_in("EMDN_v.2026").as_deref(), Some("2026"));
        assert_eq!(release_in("TERM EMDN (v.2025)").as_deref(), Some("2025"));
        assert_eq!(release_in("EMDN"), None);
    }
}
