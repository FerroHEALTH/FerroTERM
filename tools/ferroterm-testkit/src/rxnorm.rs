// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic `RxNorm`-shaped release: the `RRF` files `rxnorm-rrf`
//! reads, with invented identifiers.
//!
//! The concept identifiers and atom identifiers are not `RxNorm`'s; the
//! shape (the pipe-delimited columns, the `RXNORM` and `MTHSPL` sources, the
//! term types, the concept- and atom-level relationships with `REL` and
//! `RELA`, the attributes, a semantic type) is the release's.

use std::path::Path;

/// The release date the readme states.
pub const VERSION: &str = "01012099";
/// An ingredient (`IN`) with a synonym atom and a semantic type.
pub const ASPIRIN: &str = "900001";
/// A clinical drug (`SCD`) of the ingredient, with an `NDC` and a strength.
pub const ASPIRIN_TABLET: &str = "900002";
/// A brand name (`BN`).
pub const BRAND: &str = "900003";
/// A branded drug (`SBD`) of the clinical drug.
pub const BRANDED_TABLET: &str = "900004";
/// A dose form (`DF`).
pub const ORAL_TABLET: &str = "900005";
/// An obsolete clinical drug (`SUPPRESS = O`).
pub const OLD_TABLET: &str = "900006";
/// A concept with an `MTHSPL` atom only, which is not a code.
pub const LABEL_ONLY: &str = "900007";
/// The atom of the ingredient's `RXNORM` name.
pub const ASPIRIN_ATOM: &str = "800001";
/// The synonym atom of the ingredient.
pub const ASPIRIN_SYNONYM_ATOM: &str = "800002";
/// An atom of the clinical drug from a source the UMLS licence restricts (`MMSL`).
pub const RESTRICTED_ATOM: &str = "800012";

/// The `RXNCONSO.RRF` rows: `RXCUI`, `RXAUI`, `SAB`, `TTY`, `CODE`, `STR`, `SUPPRESS`.
const ATOMS: [[&str; 7]; 12] = [
    [
        ASPIRIN,
        ASPIRIN_ATOM,
        "RXNORM",
        "IN",
        ASPIRIN,
        "aspirin",
        "N",
    ],
    [
        ASPIRIN,
        ASPIRIN_SYNONYM_ATOM,
        "RXNORM",
        "SY",
        ASPIRIN,
        "acetylsalicylic acid",
        "N",
    ],
    [
        ASPIRIN,
        "800003",
        "MTHSPL",
        "SU",
        "R16CO5Y76E",
        "ASPIRIN",
        "N",
    ],
    [
        ASPIRIN_TABLET,
        "800004",
        "RXNORM",
        "SCD",
        ASPIRIN_TABLET,
        "aspirin 81 MG Oral Tablet",
        "N",
    ],
    [
        ASPIRIN_TABLET,
        "800005",
        "RXNORM",
        "PSN",
        ASPIRIN_TABLET,
        "aspirin 81 MG Oral Tablet",
        "N",
    ],
    [
        ASPIRIN_TABLET,
        "800006",
        "MTHSPL",
        "DP",
        "0000-0001",
        "ASPIRIN 81MG TABLET",
        "N",
    ],
    [BRAND, "800007", "RXNORM", "BN", BRAND, "Bayer", "N"],
    [
        BRANDED_TABLET,
        "800008",
        "RXNORM",
        "SBD",
        BRANDED_TABLET,
        "aspirin 81 MG Oral Tablet [Bayer]",
        "N",
    ],
    [
        ORAL_TABLET,
        "800009",
        "RXNORM",
        "DF",
        ORAL_TABLET,
        "Oral Tablet",
        "N",
    ],
    [
        OLD_TABLET,
        "800010",
        "RXNORM",
        "SCD",
        OLD_TABLET,
        "aspirin 80 MG Oral Tablet",
        "O",
    ],
    [
        LABEL_ONLY,
        "800011",
        "MTHSPL",
        "DP",
        "0000-0002",
        "SOME LABEL",
        "N",
    ],
    [
        ASPIRIN_TABLET,
        RESTRICTED_ATOM,
        "MMSL",
        "CD",
        "1234",
        "Aspirin 81mg tablet",
        "N",
    ],
];

/// `RXNCONSO.RRF`.
#[must_use]
pub fn conso() -> String {
    ATOMS
        .iter()
        .map(|[rxcui, rxaui, sab, tty, code, name, suppress]| {
            format!("{rxcui}|ENG||||||{rxaui}|{rxaui}|{code}||{sab}|{tty}|{code}|{name}||{suppress}|4096|\n")
        })
        .collect::<Vec<String>>()
        .concat()
}

/// `RXNREL.RRF`: the second concept has the relationship to the first.
#[must_use]
pub fn rel() -> String {
    let concept_rows: [(&str, &str, &str, &str); 8] = [
        // rxcui1, rel, rxcui2, rela
        (ASPIRIN, "RO", ASPIRIN_TABLET, "has_ingredient"),
        (ASPIRIN_TABLET, "RO", ASPIRIN, "ingredient_of"),
        (BRAND, "RO", BRANDED_TABLET, "has_ingredient"),
        (ASPIRIN_TABLET, "RN", BRANDED_TABLET, "tradename_of"),
        (BRANDED_TABLET, "RB", ASPIRIN_TABLET, "has_tradename"),
        (ORAL_TABLET, "RO", ASPIRIN_TABLET, "has_dose_form"),
        (ASPIRIN_TABLET, "RO", ORAL_TABLET, "dose_form_of"),
        (ASPIRIN, "RO", OLD_TABLET, "has_ingredient"),
    ];
    let mut rows: Vec<String> = concept_rows
        .iter()
        .map(|(cui1, rel, cui2, rela)| {
            format!("{cui1}||CUI|{rel}|{cui2}||CUI|{rela}|||RXNORM|||||4096|\n")
        })
        .collect();
    rows.push(format!(
        "|{ASPIRIN_ATOM}|AUI|SY||{ASPIRIN_SYNONYM_ATOM}|AUI||||RXNORM|||||4096|\n"
    ));
    rows.push(format!(
        "{ASPIRIN}||CUI|RO|{ASPIRIN_TABLET}||CUI|has_ingredient|||MTHSPL|||||4096|\n"
    ));
    rows.concat()
}

/// `RXNSAT.RRF`.
#[must_use]
pub fn sat() -> String {
    let rows: [(&str, &str, &str, &str, &str); 5] = [
        // rxcui, rxaui, atn, sab, atv
        (ASPIRIN_TABLET, "800004", "NDC", "RXNORM", "00000000101"),
        (ASPIRIN_TABLET, "800004", "NDC", "RXNORM", "00000000102"),
        (
            ASPIRIN_TABLET,
            "800004",
            "RXN_AVAILABLE_STRENGTH",
            "RXNORM",
            "81 MG",
        ),
        (ASPIRIN_TABLET, "800006", "SPL_SET_ID", "MTHSPL", "2a2a526f"),
        (ASPIRIN, "800001", "RXN_HUMAN_DRUG", "RXNORM", "US"),
    ];
    rows.iter()
        .map(|(rxcui, rxaui, atn, sab, atv)| {
            format!("{rxcui}|||{rxaui}|AUI|{rxcui}|||{atn}|{sab}|{atv}|N|4096|\n")
        })
        .collect::<Vec<String>>()
        .concat()
}

/// `RXNSTY.RRF`.
#[must_use]
pub fn sty() -> String {
    format!(
        "{ASPIRIN}|T109|A1.4.1.2.1|Organic Chemical||4096|\n{ASPIRIN}|T121|A1.4.1.1.1|Pharmacologic Substance||4096|\n"
    )
}

/// Writes the release under `root`: `Readme_Full_Prescribe_01012099.txt` and
/// `rrf/RXNCONSO.RRF`, `RXNREL.RRF`, `RXNSAT.RRF`, `RXNSTY.RRF`.
///
/// # Errors
///
/// Returns the I/O error when a file cannot be written.
pub fn write_release(root: &Path) -> std::io::Result<()> {
    let rrf = root.join("rrf");
    std::fs::create_dir_all(&rrf)?;
    std::fs::write(
        root.join(format!("Readme_Full_Prescribe_{VERSION}.txt")),
        "README: synthetic RxNorm-shaped release\n",
    )?;
    std::fs::write(rrf.join("RXNCONSO.RRF"), conso())?;
    std::fs::write(rrf.join("RXNREL.RRF"), rel())?;
    std::fs::write(rrf.join("RXNSAT.RRF"), sat())?;
    std::fs::write(rrf.join("RXNSTY.RRF"), sty())
}

/// Builds the release into an artifact directory.
///
/// # Errors
///
/// Returns an I/O error wrapping the build failure.
pub fn write_artifact(dir: &Path) -> std::io::Result<()> {
    let release = tempfile::tempdir()?;
    write_release(release.path())?;
    ferroterm_build::rxnorm::build(release.path(), None, &[], dir)
        .map(|_| ())
        .map_err(std::io::Error::other)
}
