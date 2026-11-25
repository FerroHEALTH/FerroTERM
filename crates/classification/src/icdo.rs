// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! ICD-O-3, the International Classification of Diseases for Oncology.
//!
//! The source is the WHO topography and morphology tables: the ICD-O-3.2
//! workbook the IACR distributes for WHO
//! (<http://www.iacr.com.fr/index.php?option=com_content&view=article&id=149:icd-o-3-2&catid=80&Itemid=545>)
//! or its delimited export.
//!
//! Each table has a code, a level, and a term column; a code repeats once
//! per term. The level says whether the term is the code's title (`3` or `4`
//! in the topography, `Preferred` in the morphology), a synonym or inclusion
//! (`incl`, `Synonym`), or a related term (`Related`). A topography code
//! (`C00.0`) nests in its three-character category (`C00`); a morphology
//! code (`8000/3`, histology and behaviour) stands alone.

use std::path::Path;

use crate::sheet::{self, Column, SheetError};
use crate::{Class, Classification, PREFERRED, Rubric, with_period};

/// The system URI (HL7 Terminology, `NamingSystem-icd-o-3`).
pub const SYSTEM: &str = "http://terminology.hl7.org/CodeSystem/icd-o-3";
/// The designation kind of a synonym or inclusion term.
pub const SYNONYM: &str = "synonym";
/// The designation kind of a related term.
pub const RELATED: &str = "related";
/// The class kind of a three-character topography code.
pub const CATEGORY: &str = "topography-category";
/// The class kind of a four-character topography code.
pub const SUBCATEGORY: &str = "topography-subcategory";
/// The class kind of a morphology code.
pub const MORPHOLOGY: &str = "morphology";

const COLUMNS: [Column; 3] = [
    Column {
        name: "Code",
        aliases: &["code", "kode", "icdo3.2", "icd-o-3.2", "icdo3", "icd-o-3"],
        required: true,
    },
    Column {
        name: "Term",
        aliases: &["term", "title", "label", "description"],
        required: true,
    },
    Column {
        name: "Level",
        aliases: &["level", "lvl", "struct"],
        required: false,
    },
];

/// A code in its served form: a topography code with its period, a
/// morphology code with the `/` before its behaviour digit.
#[must_use]
pub fn normalize(code: &str) -> String {
    let code = code.trim().to_uppercase();
    if code.starts_with('C') {
        return with_period(&code);
    }
    if code.len() == 5 && code.chars().all(|c| c.is_ascii_digit()) {
        return code
            .split_at_checked(4)
            .map_or(code.clone(), |(histology, behaviour)| {
                format!("{histology}/{behaviour}")
            });
    }
    code
}

/// The designation kind a level names; `None` for the title.
fn term_kind(level: Option<&str>) -> Option<&'static str> {
    match level.map(str::to_lowercase).as_deref() {
        None | Some("3" | "4" | "preferred" | "pt" | "title") => None,
        Some("related" | "rt") => Some(RELATED),
        Some(_) => Some(SYNONYM),
    }
}

/// Reads one or more ICD-O-3 tables (topography, morphology) as the release `version`.
///
/// # Errors
///
/// Returns [`SheetError`] when a file does not read or lacks the code or
/// term column.
pub fn read(paths: &[&Path], version: &str) -> Result<Classification, SheetError> {
    let mut classes: Vec<Class> = Vec::new();
    let mut by_code: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for path in paths {
        let sheet = sheet::read(path)?;
        let header = sheet.header(path, &COLUMNS)?;
        for row in sheet.data(&header) {
            let (Some(code), Some(term)) = (header.cell(row, 0), header.cell(row, 1)) else {
                continue;
            };
            let code = normalize(code);
            let index = if let Some(&i) = by_code.get(&code) {
                i
            } else {
                let kind = match (code.starts_with('C'), code.contains('.')) {
                    (true, false) => CATEGORY,
                    (true, true) => SUBCATEGORY,
                    _ => MORPHOLOGY,
                };
                classes.push(Class {
                    code: code.clone(),
                    kind: kind.to_owned(),
                    parent: code
                        .split_once('.')
                        .map(|(category, _)| category.to_owned()),
                    ..Class::default()
                });
                let i = classes.len().saturating_sub(1);
                by_code.insert(code, i);
                i
            };
            let Some(class) = classes.get_mut(index) else {
                continue;
            };
            let has_title = class.rubrics.iter().any(|r| r.kind == PREFERRED);
            let kind = match term_kind(header.cell(row, 2)) {
                None if !has_title => PREFERRED,
                None => SYNONYM,
                Some(kind) => kind,
            };
            class.rubrics.push(Rubric::english(kind, term));
        }
    }
    for class in &mut classes {
        if class
            .parent
            .as_ref()
            .is_some_and(|p| !by_code.contains_key(p))
        {
            class.parent = None;
        }
    }
    Ok(Classification {
        name: String::from("ICD-O-3"),
        title: String::from("International Classification of Diseases for Oncology, third edition"),
        version: Some(version.to_owned()),
        language: String::from("en"),
        kinds: [CATEGORY, SUBCATEGORY, MORPHOLOGY]
            .map(str::to_owned)
            .to_vec(),
        classes,
        designation_kinds: vec![PREFERRED.to_owned(), SYNONYM.to_owned(), RELATED.to_owned()],
        ..Classification::default()
    })
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn codes_take_their_served_form() {
        assert_eq!(normalize("C000"), "C00.0");
        assert_eq!(normalize("C00.0"), "C00.0");
        assert_eq!(normalize("C00"), "C00");
        assert_eq!(normalize("80003"), "8000/3");
        assert_eq!(normalize("8000/3"), "8000/3");
    }
}
