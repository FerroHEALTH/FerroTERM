// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The parts, the multiaxial hierarchy, and the primary part links.
//!
//! `Part.csv` lists the parts; `ComponentHierarchyBySystem.csv` places parts
//! as branches and terms as leaves (a code may sit under more than one parent),
//! and names class parts that `Part.csv` does not list, which are parts all
//! the same with the hierarchy's text as their name; `LoincPartLink_Primary.csv`
//! gives the part of each term on each axis.

use crate::id;
use crate::release::{HIERARCHY, LINKS, PARTS, Release, ReleaseError, Table, csv_at, field};

/// One part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// `PartNumber` (`LP…`).
    pub code: String,
    /// `PartTypeName` (`COMPONENT`, `SYSTEM`, …).
    pub type_name: String,
    /// `PartName`.
    pub name: String,
    /// `PartDisplayName`.
    pub display_name: String,
    /// `Status`.
    pub status: String,
}

/// One edge of the multiaxial hierarchy: `code` sits under `parent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// `CODE`: a part or a term.
    pub code: String,
    /// `IMMEDIATE_PARENT`, empty for a root.
    pub parent: Option<String>,
    /// `CODE_TEXT`.
    pub text: String,
}

/// Reads `Part.csv`.
///
/// # Errors
///
/// Returns [`ReleaseError`] when the file is missing, does not parse, or lacks
/// a column.
pub fn read_parts(release: &Release) -> Result<Vec<Part>, ReleaseError> {
    let mut table = Table::open(release.file(PARTS)?)?;
    let code_at = table.column("PartNumber")?;
    let type_at = table.column("PartTypeName")?;
    let name_at = table.column("PartName")?;
    let display_at = table.column("PartDisplayName")?;
    let status_at = table.column("Status")?;
    let mut rows = Vec::new();
    let path = table.path.clone();
    for record in table.reader.records() {
        let record = record.map_err(|e| csv_at(&path, e))?;
        let code = field(&record, code_at).to_owned();
        if !id::is_valid(&code) {
            return Err(ReleaseError::Code {
                path: table.path.clone(),
                code,
            });
        }
        rows.push(Part {
            code,
            type_name: field(&record, type_at).to_owned(),
            name: field(&record, name_at).to_owned(),
            display_name: field(&record, display_at).to_owned(),
            status: field(&record, status_at).to_owned(),
        });
    }
    Ok(rows)
}

/// Reads the multiaxial hierarchy.
///
/// # Errors
///
/// Returns [`ReleaseError`] when the file is missing, does not parse, or lacks
/// a column.
pub fn read_hierarchy(release: &Release) -> Result<Vec<Edge>, ReleaseError> {
    let mut table = Table::open(release.file(HIERARCHY)?)?;
    let code_at = table.column("CODE")?;
    let parent_at = table.column("IMMEDIATE_PARENT")?;
    let text_at = table.column("CODE_TEXT")?;
    let mut rows = Vec::new();
    let path = table.path.clone();
    for record in table.reader.records() {
        let record = record.map_err(|e| csv_at(&path, e))?;
        let parent = field(&record, parent_at);
        rows.push(Edge {
            code: field(&record, code_at).to_owned(),
            parent: (!parent.is_empty()).then(|| parent.to_owned()),
            text: field(&record, text_at).to_owned(),
        });
    }
    Ok(rows)
}

/// One primary link: the part of a term on one axis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The term code.
    pub code: String,
    /// The part code.
    pub part: String,
    /// The part name, as the link file spells it.
    pub part_name: String,
    /// The axis (`PartTypeName`: `COMPONENT`, `PROPERTY`, `TIME`, `SYSTEM`,
    /// `SCALE`, `METHOD`, or another attribute name).
    pub axis: String,
}

/// Reads the primary part links; a release without the file has none.
///
/// # Errors
///
/// Returns [`ReleaseError`] when the file does not parse or lacks a column.
pub fn read_links(release: &Release) -> Result<Vec<Link>, ReleaseError> {
    let Some(path) = release.optional(LINKS) else {
        return Ok(Vec::new());
    };
    let mut table = Table::open(path)?;
    let code_at = table.column("LoincNumber")?;
    let part_at = table.column("PartNumber")?;
    let name_at = table.column("PartName")?;
    let axis_at = table.column("PartTypeName")?;
    let mut rows = Vec::new();
    let path = table.path.clone();
    for record in table.reader.records() {
        let record = record.map_err(|e| csv_at(&path, e))?;
        rows.push(Link {
            code: field(&record, code_at).to_owned(),
            part: field(&record, part_at).to_owned(),
            part_name: field(&record, name_at).to_owned(),
            axis: field(&record, axis_at).to_owned(),
        });
    }
    Ok(rows)
}
