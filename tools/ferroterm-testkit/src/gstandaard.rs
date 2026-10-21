// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic G-Standaard-shaped release: the ladder files as fixed-length
//! records at the published positions, with invented codes and names.

use std::path::Path;

/// The release the fixture claims.
pub const VERSION: &str = "209901";
/// The generic product.
pub const GPK: &str = "12345";
/// The prescription product under it.
pub const PRK: &str = "234567";
/// The trade product under that.
pub const HPK: &str = "3456789";
/// The article (ZI number) under that.
pub const ARTICLE: &str = "45678901";
/// An article removed from the release (mutation code 9).
pub const REMOVED_ARTICLE: &str = "45678902";
/// An article with a removal date, served inactive.
pub const ENDED_ARTICLE: &str = "45678903";
/// The ATC code the generic product carries.
pub const ATC: &str = "Z10BA02";

/// A record of `length` characters with `fields` at their 1-based positions.
fn record(length: usize, fields: &[(usize, &str)]) -> String {
    let mut chars: Vec<char> = vec![' '; length];
    for (start, value) in fields {
        for (i, c) in value.chars().enumerate() {
            if let Some(slot) = chars.get_mut(start - 1 + i) {
                *slot = c;
            }
        }
    }
    let mut out: String = chars.into_iter().collect();
    out.push('\n');
    out
}

/// `BST020T`: the names by number.
#[must_use]
pub fn names() -> String {
    let name = |number: &str, label: &str, short: &str, full: &str| {
        record(
            160,
            &[
                (1, "0020"),
                (5, "1"),
                (6, number),
                (19, label),
                (46, short),
                (86, full),
            ],
        )
    };
    [
        name(
            "0000001",
            "METFORMINOIDE 500",
            "metforminoide tablet 500mg",
            "METFORMINOIDE TABLET 500MG",
        ),
        name("0000002", "METFORMINOIDE", "metforminoide", "METFORMINOIDE"),
        name(
            "0000003",
            "METFORMINOIDE 500",
            "metforminoide tablet 500mg",
            "METFORMINOIDE TABLET 500MG",
        ),
        name(
            "0000004",
            "SYNTHOMET 500",
            "synthomet tablet 500mg",
            "SYNTHOMET TABLET 500MG",
        ),
        name(
            "0000005",
            "SYNTHOMET 500 30ST",
            "synthomet tablet 500mg 30 stuks",
            "SYNTHOMET TABLET 500MG 30 STUKS",
        ),
    ]
    .concat()
}

/// `BST902T`: the thesaurus items the coded fields point at.
#[must_use]
pub fn thesauri() -> String {
    let item = |thesaurus: &str, number: &str, name: &str| {
        record(
            128,
            &[
                (1, "0902"),
                (5, "1"),
                (6, thesaurus),
                (10, number),
                (62, name),
            ],
        )
    };
    [
        item("0006", "000012", "tablet"),
        item("0007", "000003", "oraal"),
        item("0002", "000245", "stuk"),
    ]
    .concat()
}

/// `BST711T`: the generic products.
#[must_use]
pub fn gpk() -> String {
    record(
        160,
        &[
            (1, "0711"),
            (5, "1"),
            (6, "00012345"),
            (22, "006"),
            (25, "012"),
            (28, "007"),
            (31, "003"),
            (34, "0000001"),
            (41, "0000002"),
            (48, "500MG"),
            (119, ATC),
        ],
    )
}

/// `BST052T`: the prescription products.
#[must_use]
pub fn prk() -> String {
    record(
        128,
        &[
            (1, "0052"),
            (5, "1"),
            (6, "00234567"),
            (14, "0000003"),
            (21, "00012345"),
            (49, "0002"),
            (53, "000245"),
        ],
    )
}

/// `BST031T`: the trade products.
#[must_use]
pub fn hpk() -> String {
    record(
        480,
        &[
            (1, "0031"),
            (5, "1"),
            (6, "03456789"),
            (14, "00234567"),
            (30, "0000004"),
            (37, "SYNTHOMET"),
            (87, "SYNTHETICA BV"),
        ],
    )
}

/// `BST004T`: the articles, one current, one removed, one ended.
#[must_use]
pub fn articles() -> String {
    let article = |mutation: &str, code: &str, removed_on: &str| {
        record(
            320,
            &[
                (1, "0004"),
                (5, mutation),
                (6, code),
                (14, "03456789"),
                (22, "0000005"),
                (151, removed_on),
            ],
        )
    };
    [
        article("1", ARTICLE, "00000000"),
        article("9", REMOVED_ARTICLE, "00000000"),
        article("1", ENDED_ARTICLE, "20980601"),
    ]
    .concat()
}

/// Writes the six files under `dir`, Latin-1 encoded.
///
/// # Errors
///
/// Returns the I/O error when a file cannot be written.
pub fn write_release(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (name, text) in [
        ("BST020T", names()),
        ("BST902T", thesauri()),
        ("BST711T", gpk()),
        ("BST052T", prk()),
        ("BST031T", hpk()),
        ("BST004T", articles()),
    ] {
        let bytes: Vec<u8> = text
            .chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
            .collect();
        std::fs::write(dir.join(name), bytes)?;
    }
    Ok(())
}

/// Builds the release into `dir/{gpk,prk,hpk,artikel}`.
///
/// # Errors
///
/// Returns an I/O error wrapping the build failure.
pub fn write_artifacts(dir: &Path) -> std::io::Result<()> {
    let release = tempfile::tempdir()?;
    write_release(release.path())?;
    let ladder = ::gstandaard::read(release.path(), VERSION).map_err(std::io::Error::other)?;
    for (name, system, classification) in ladder.rungs() {
        ferroterm_build::classification::build(
            classification,
            system,
            Some(VERSION),
            &dir.join(name),
        )
        .map_err(std::io::Error::other)?;
    }
    Ok(())
}
