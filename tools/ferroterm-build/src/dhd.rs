// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The concept maps a DHD delivery carries.
//!
//! The thesaurus links to SNOMED CT (the fully specified names' identifiers)
//! and to ICD-10 (the derivations) are written as FHIR R4B `ConceptMap`
//! resources beside the artifact, so a `FERROTERM_CODESYSTEMS` directory can
//! serve them with `$translate` (<https://hl7.org/fhir/R4B/conceptmap.html>).

use std::io;
use std::path::{Path, PathBuf};

use ::dhd_thesaurus::{SYSTEM, Thesaurus};
use fhir_types::codec::Json;
use fhir_types::r4b::concept_map::{
    ConceptMap, ConceptMapGroup, ConceptMapGroupElement, ConceptMapGroupElementTarget,
};

/// The directory under the artifact that holds the concept maps.
pub const MAPS_DIR: &str = "conceptmaps";
/// The SNOMED CT system URI the first map targets.
pub const SNOMED_SYSTEM: &str = "http://snomed.info/sct";
/// The ICD-10-NL system URI the second map targets (the derivations are the
/// Dutch translation's codes, <https://hl7.org/fhir/R4B/icd.html>).
pub const ICD10_SYSTEM: &str = "http://hl7.org/fhir/sid/icd-10-nl";

/// A failure to write the concept maps.
#[derive(Debug, thiserror::Error)]
pub enum MapError {
    /// A file cannot be written.
    #[error("cannot write {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: io::Error,
    },
    /// The resource cannot be encoded.
    #[error("cannot encode the concept map")]
    Encode(#[from] fhir_types::codec::EncodeError),
}

fn map(
    name: &str,
    version: &str,
    target: &str,
    elements: Vec<ConceptMapGroupElement>,
) -> ConceptMap {
    ConceptMap {
        url: Some(format!("{SYSTEM}/conceptmap/{name}").into()),
        version: Some(version.into()),
        name: Some(format!("DhdTo{name}").into()),
        status: "active".into(),
        group: vec![ConceptMapGroup {
            source: Some(SYSTEM.into()),
            source_version: Some(version.into()),
            target: Some(target.into()),
            element: elements,
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn element(code: &str, targets: &[String], equivalence: &str) -> ConceptMapGroupElement {
    ConceptMapGroupElement {
        code: Some(code.into()),
        target: targets
            .iter()
            .map(|t| ConceptMapGroupElementTarget {
                code: Some(t.as_str().into()),
                equivalence: equivalence.into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn write(dir: &Path, name: &str, resource: &ConceptMap) -> Result<(), MapError> {
    let path = dir.join(format!("{name}.json"));
    let io = |source| MapError::Io {
        path: path.clone(),
        source,
    };
    let object = resource.to_json()?;
    let text = serde_json::to_string_pretty(&fhir_types::codec::Value::Object(object))
        .map_err(|e| io(io::Error::other(e)))?;
    std::fs::write(&path, format!("{text}\n")).map_err(io)
}

/// Writes the two concept maps under `<out>/conceptmaps/`.
///
/// `dhd-to-snomed.json` has one `equivalent` target per concept with a SNOMED
/// CT identifier; `dhd-to-icd10.json` has the ICD-10 derivations as `wider`
/// targets, since several codes describe one concept.
///
/// # Errors
///
/// Returns [`MapError`] when a file cannot be written or encoded.
pub fn write_concept_maps(
    thesaurus: &Thesaurus,
    version: &str,
    out: &Path,
) -> Result<(), MapError> {
    let dir = out.join(MAPS_DIR);
    std::fs::create_dir_all(&dir).map_err(|source| MapError::Io {
        path: dir.clone(),
        source,
    })?;
    let snomed: Vec<ConceptMapGroupElement> = thesaurus
        .snomed
        .iter()
        .map(|(concept, sctid)| element(concept, std::slice::from_ref(sctid), "equivalent"))
        .collect();
    write(
        &dir,
        "dhd-to-snomed",
        &map("Snomed", version, SNOMED_SYSTEM, snomed),
    )?;
    // NOTE: an ICD-10 derivation is one-to-many, "meerdere ICD-10 codes horen
    // bij één term" (Uitleverformaat 5.0 §3.2.5), so each target is `wider`.
    let icd10: Vec<ConceptMapGroupElement> = thesaurus
        .icd10
        .iter()
        .map(|(concept, codes)| element(concept, codes, "wider"))
        .collect();
    write(
        &dir,
        "dhd-to-icd10",
        &map("Icd10", version, ICD10_SYSTEM, icd10),
    )
}
