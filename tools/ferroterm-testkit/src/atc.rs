// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic ATC-shaped table: the WHO index as a CSV export and the
//! G-Standaard `BST801T` records, with ATC-shaped codes and invented names.

use std::path::Path;

/// The index year the fixture claims.
pub const VERSION: &str = "2099";
/// The main group.
pub const GROUP: &str = "Z";
/// The therapeutic subgroup.
pub const THERAPEUTIC: &str = "Z10";
/// The pharmacological subgroup.
pub const PHARMACOLOGICAL: &str = "Z10B";
/// The chemical subgroup.
pub const CHEMICAL: &str = "Z10BA";
/// A substance with two DDDs (oral and parenteral).
pub const SUBSTANCE: &str = "Z10BA02";
/// A substance without a DDD.
pub const OTHER_SUBSTANCE: &str = "Z10BA03";

/// The index as a CSV export.
#[must_use]
pub fn index_csv() -> String {
    [
        "ATC code,ATC level name,DDD,U,Adm.R,Note",
        "Z,Synthetic tract,,,,",
        "Z10,Drugs used in synthesis,,,,",
        "Z10B,Synthesis lowering drugs,,,,",
        "Z10BA,Biguanoids,,,,",
        "Z10BA02,metforminoid,2,g,O,",
        "Z10BA02,metforminoid,1,g,P,parenteral form",
        "Z10BA03,fenforminoid,,,,",
    ]
    .join("\n")
}

/// The same table as `BST801T` records (192 characters, Latin-1).
#[must_use]
pub fn bst801() -> String {
    let rows: [(&str, &str, &str, &str); 6] = [
        (GROUP, "Synthetisch kanaal", "Synthetic tract", "0"),
        (
            THERAPEUTIC,
            "Middelen bij synthese",
            "Drugs used in synthesis",
            "0",
        ),
        (
            PHARMACOLOGICAL,
            "Syntheseverlagende middelen",
            "Synthesis lowering drugs",
            "0",
        ),
        (CHEMICAL, "Biguanoïden", "Biguanoids", "0"),
        (SUBSTANCE, "metforminoïde", "metforminoid", "1"),
        (OTHER_SUBSTANCE, "fenforminoïde", "fenforminoid", "1"),
    ];
    let record = |mutation: &str, code: &str, nl: &str, en: &str, kind: &str| {
        format!("0801{mutation}{code:<8}{nl:<80}{en:<80}{kind}{:<18}\n", "")
    };
    let mut lines: Vec<String> = rows
        .iter()
        .map(|(code, nl, en, kind)| record("1", code, nl, en, kind))
        .collect();
    lines.push(record("9", "Z10BA09", "verwijderd", "removed", "1"));
    lines.concat()
}

/// Writes the index CSV to `path`.
///
/// # Errors
///
/// Returns the I/O error when the file cannot be written.
pub fn write_index(path: &Path) -> std::io::Result<()> {
    std::fs::write(path, index_csv())
}

/// Writes `BST801T` to `path`, Latin-1 encoded.
///
/// # Errors
///
/// Returns the I/O error when the file cannot be written.
pub fn write_bst801(path: &Path) -> std::io::Result<()> {
    let bytes: Vec<u8> = bst801()
        .chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
        .collect();
    std::fs::write(path, bytes)
}

/// Builds the index fixture into an artifact directory.
///
/// # Errors
///
/// Returns an I/O error wrapping the build failure.
pub fn write_artifact(dir: &Path) -> std::io::Result<()> {
    let release = tempfile::tempdir()?;
    let csv = release.path().join("atc-index.csv");
    write_index(&csv)?;
    let classification =
        ::classification::atc::read(&csv, Some(VERSION)).map_err(std::io::Error::other)?;
    ferroterm_build::classification::build(
        &classification,
        ::classification::atc::SYSTEM,
        Some(VERSION),
        dir,
    )
    .map(|_| ())
    .map_err(std::io::Error::other)
}
