// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic DHD-shaped delivery: the Uitleverformaat 5.0 tables with
//! invented concept identifiers, terms, and derivations.

use std::path::Path;

/// The thesaurus version the directory name carries.
pub const VERSION: &str = "9.99";
/// The delivery date the file names carry.
pub const DATE: &str = "20990101";
/// The directory name of the unpacked delivery.
pub const DIRECTORY: &str = "20990101_090000_Diagnosethesaurus_9.99_uitleverformaat_5.0";
/// A current diagnosis with two ICD-10 derivations and a SNOMED CT id.
pub const FRACTURE: &str = "9000001";
/// A current diagnosis with one ICD-10 derivation, under an umbrella term.
pub const SPRAIN: &str = "9000002";
/// An ended concept, replaced by the fracture.
pub const OLD: &str = "9000003";
/// The umbrella term.
pub const INJURY: &str = "9000004";
/// The SNOMED CT identifier the fracture's fully specified name carries.
pub const FRACTURE_SCTID: &str = "999000111";

fn table(header: &str, rows: &[&str]) -> String {
    let mut out = String::from(header);
    for row in rows {
        out.push('\n');
        out.push_str(row);
    }
    out.push('\n');
    out
}

/// The `ThesaurusConcept` table.
#[must_use]
pub fn concepts() -> String {
    table(
        "\"ConceptID\",\"TypeConcept\",\"Complicatie\",\"GebruiktImplantaat\",\"Gradatie\",\"Lateraliteit\",\"LOINCCode\",\"Begindatum\",\"Einddatum\"",
        &[
            "\"9000001\",\"Diagnose\",\"false\",\"false\",\"false\",\"true\",\"\",\"20100101\",\"20991231\"",
            "\"9000002\",\"Diagnose\",\"false\",\"false\",\"false\",\"true\",\"\",\"20100101\",\"20991231\"",
            "\"9000003\",\"Diagnose\",\"false\",\"false\",\"false\",\"false\",\"\",\"20100101\",\"20200101\"",
            "\"9000004\",\"Diagnose\",\"false\",\"false\",\"false\",\"false\",\"\",\"20100101\",\"20991231\"",
        ],
    )
}

/// The `ThesaurusTerm` table.
#[must_use]
pub fn terms() -> String {
    table(
        "\"TermID\",\"ConceptID\",\"TypeTerm\",\"Omschrijving\",\"TaalCode\",\"SnomedID\",\"Begindatum\",\"Einddatum\"",
        &[
            "\"1\",\"9000001\",\"Voorkeursterm\",\"fractuur van de synthetische knobbel\",\"nl-NL\",\"\",\"20100101\",\"20991231\"",
            "\"2\",\"9000001\",\"Synoniem\",\"knobbelfractuur\",\"nl-NL\",\"\",\"20100101\",\"20991231\"",
            "\"3\",\"9000001\",\"FSN\",\"Fracture of synthetic knob (disorder)\",\"en-GB\",\"999000111\",\"20100101\",\"20991231\"",
            "\"4\",\"9000001\",\"PVT\",\"gebroken knobbel\",\"nl-NL\",\"\",\"20100101\",\"20991231\"",
            "\"5\",\"9000001\",\"Zoekterm\",\"knobbel gebroken\",\"nl-NL\",\"\",\"20100101\",\"20120101\"",
            "\"6\",\"9000002\",\"Voorkeursterm\",\"verstuiking van de synthetische knobbel\",\"nl-NL\",\"\",\"20100101\",\"20991231\"",
            "\"7\",\"9000003\",\"Voorkeursterm\",\"oude knobbelaandoening\",\"nl-NL\",\"\",\"20100101\",\"20200101\"",
            "\"8\",\"9000004\",\"Voorkeursterm\",\"letsel van de synthetische knobbel\",\"nl-NL\",\"\",\"20100101\",\"20991231\"",
        ],
    )
}

/// The `ThesaurusConceptRelaties` table.
#[must_use]
pub fn relations() -> String {
    table(
        "\"ConceptID1\",\"ConceptID2\",\"TypeRelatie\",\"Begindatum\",\"Einddatum\"",
        &[
            "\"9000003\",\"9000001\",\"Vervanging\",\"20200101\",\"20991231\"",
            "\"9000003\",\"9000002\",\"Vervanging\",\"20150101\",\"20180101\"",
        ],
    )
}

/// The `Parapluterm` table.
#[must_use]
pub fn umbrellas() -> String {
    table(
        "\"ConceptID1\",\"ConceptID2\",\"Begindatum\",\"Einddatum\"",
        &[
            "\"9000004\",\"9000001\",\"20100101\",\"20991231\"",
            "\"9000004\",\"9000002\",\"20100101\",\"20991231\"",
        ],
    )
}

/// The `ThesaurusConceptRol` table.
#[must_use]
pub fn roles() -> String {
    table(
        "\"ConceptID\",\"SpecialismeGroepCode\",\"Rolnaam\",\"Rolwaarde\",\"Begindatum\",\"Einddatum\"",
        &["\"9000001\",\"ORT\",\"Hoofddiagnose\",\"Ja\",\"20100101\",\"20991231\""],
    )
}

/// The `AfleidingICD10` table.
#[must_use]
pub fn icd10() -> String {
    table(
        "\"ConceptID\",\"Volgnummer\",\"ICD10\",\"Advies\",\"Begindatum\",\"Einddatum\"",
        &[
            "\"9000001\",\"2\",\"Z99.1\",\"\",\"20100101\",\"20991231\"",
            "\"9000001\",\"1\",\"Z99.0\",\"\",\"20100101\",\"20991231\"",
            "\"9000002\",\"1\",\"Z98.0\",\"\",\"20100101\",\"20991231\"",
        ],
    )
}

/// The `AfleidingDBC` table.
#[must_use]
pub fn dbc() -> String {
    table(
        "\"ConceptID\",\"SpecialismeCode\",\"DBC_ID\",\"Begindatum\",\"Einddatum\"",
        &["\"9000001\",\"0305\",\"1234\",\"20100101\",\"20991231\""],
    )
}

/// The `CodeMapping` table.
#[must_use]
pub fn mappings() -> String {
    table(
        "\"ConceptID\",\"Codestelsel\",\"Code\",\"Begindatum\",\"Einddatum\"",
        &["\"9000002\",\"ICPC-2\",\"L99\",\"20100101\",\"20991231\""],
    )
}

/// Writes the delivery under `dir/DIRECTORY` and returns that directory.
///
/// # Errors
///
/// Returns the I/O error when a file cannot be written.
pub fn write_delivery(dir: &Path) -> std::io::Result<std::path::PathBuf> {
    let root = dir.join(DIRECTORY);
    std::fs::create_dir_all(&root)?;
    for (name, text) in [
        ("ThesaurusConcept", concepts()),
        ("ThesaurusTerm", terms()),
        ("ThesaurusConceptRelaties", relations()),
        ("Parapluterm", umbrellas()),
        ("ThesaurusConceptRol", roles()),
        ("AfleidingICD10", icd10()),
        ("AfleidingDBC", dbc()),
        ("CodeMapping", mappings()),
    ] {
        std::fs::write(
            root.join(format!("{DATE}_090000_uitleverformaat5.0_{name}.csv")),
            text,
        )?;
    }
    Ok(root)
}

/// Builds the delivery into an artifact directory (with its concept maps).
///
/// # Errors
///
/// Returns an I/O error wrapping the build failure.
pub fn write_artifact(dir: &Path) -> std::io::Result<()> {
    let release = tempfile::tempdir()?;
    let root = write_delivery(release.path())?;
    let thesaurus = ::dhd_thesaurus::read(&root, None).map_err(std::io::Error::other)?;
    let report = ferroterm_build::classification::build(
        &thesaurus.classification,
        ::dhd_thesaurus::SYSTEM,
        None,
        dir,
    )
    .map_err(std::io::Error::other)?;
    ferroterm_build::dhd::write_concept_maps(&thesaurus, &report.version, dir)
        .map_err(std::io::Error::other)
}
