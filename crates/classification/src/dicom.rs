// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The DICOM Controlled Terminology (PS3.16 Annex D, the `DCM` coding scheme).
//!
//! The source is the OWL rendering NEMA publishes with each edition, `dcm.owl`
//! or `dcm.owl.zip` (<https://dicom.nema.org/medical/dicom/resources/ontology/>):
//! one `rdf:Description` per code, an `owl:Class` with its `skos:notation`
//! (the code), `skos:prefLabel` (the meaning), `skos:definition`, and notes;
//! a retired code carries `owl:deprecated` and may name its replacement with
//! `owl:sameAs`. The ontology's own description carries the edition as
//! `owl:versionInfo` (`2026e_20261121`). The codes form a flat list.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::xml::{Element, records};
use crate::{Class, Classification, PREFERRED, Rubric};

/// The system URI (HL7 Terminology, `NamingSystem-v3-DCM`).
pub const SYSTEM: &str = "http://dicom.nema.org/resources/ontology/DCM";
/// The class kind of every code.
pub const KIND: &str = "dcm-code";
/// The rubric kinds of the texts kept as properties, by element local name.
pub const NOTES: [(&str, &str); 4] = [
    ("definition", "definition"),
    ("note", "note"),
    ("changeNote", "change-note"),
    ("example", "example"),
];
/// The rubric kind of a retired code's replacement.
pub const REPLACED_BY: &str = "replaced-by";

/// A failure to read the terminology.
#[derive(Debug, thiserror::Error)]
pub enum DicomError {
    /// The file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The zip does not read or holds no `.owl` entry.
    #[error("{path} is not a zip holding the OWL file")]
    Zip {
        /// The path.
        path: PathBuf,
        /// The cause, when the zip itself failed.
        #[source]
        source: Option<zip::result::ZipError>,
    },
    /// The file is not well-formed XML.
    #[error("{path} does not parse")]
    Xml {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: quick_xml::Error,
    },
    /// The file defines no code.
    #[error("{0} defines no DCM code")]
    Empty(PathBuf),
    /// No version was given and the ontology names none.
    #[error("the ontology names no owl:versionInfo; pass a version")]
    NoVersion,
}

/// The text of `path`, the largest `.owl` entry when it is a zip.
fn text_of(path: &Path) -> Result<String, DicomError> {
    let io = |source| DicomError::Io {
        path: path.to_path_buf(),
        source,
    };
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        return std::fs::read_to_string(path).map_err(io);
    }
    let zip_error = |source| DicomError::Zip {
        path: path.to_path_buf(),
        source,
    };
    let file = std::fs::File::open(path).map_err(io)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| zip_error(Some(e)))?;
    let mut best: Option<(usize, u64)> = None;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|e| zip_error(Some(e)))?;
        let owl = entry
            .enclosed_name()
            .is_some_and(|n| n.extension().is_some_and(|e| e.eq_ignore_ascii_case("owl")));
        if owl && best.is_none_or(|(_, size)| entry.size() > size) {
            best = Some((index, entry.size()));
        }
    }
    let (index, _) = best.ok_or_else(|| zip_error(None))?;
    let mut entry = archive.by_index(index).map_err(|e| zip_error(Some(e)))?;
    let mut text = String::new();
    entry.read_to_string(&mut text).map_err(io)?;
    Ok(text)
}

/// Whether `description` is typed `type_suffix` (`#Class`, `#Ontology`).
fn typed(description: &Element, type_suffix: &str) -> bool {
    description
        .all("type")
        .any(|t| t.attr("resource").is_some_and(|r| r.ends_with(type_suffix)))
}

/// Reads the DCM terminology from `dcm.owl` or `dcm.owl.zip`; `version`
/// overrides the ontology's `owl:versionInfo`.
///
/// # Errors
///
/// Returns [`DicomError`] when the file does not read or parse, defines no
/// code, or no version is known.
pub fn read(path: &Path, version: Option<&str>) -> Result<Classification, DicomError> {
    let text = text_of(path)?;
    let mut classes = Vec::new();
    let mut edition = None;
    records::<quick_xml::Error>(&text, "Description", |description| {
        if typed(&description, "#Ontology") {
            edition = description.text_at("versionInfo").map(str::to_owned);
            return Ok(());
        }
        if !typed(&description, "#Class") {
            return Ok(());
        }
        let Some(code) = description.text_at("notation") else {
            return Ok(());
        };
        let mut rubrics = Vec::new();
        if let Some(label) = description.text_at("prefLabel") {
            rubrics.push(Rubric::english(PREFERRED, label));
        }
        for (element, kind) in NOTES {
            for note in description.all(element) {
                rubrics.push(Rubric::english(kind, &note.text));
            }
        }
        for same in description.all("sameAs") {
            if let Some(target) = same.attr("resource").and_then(|r| r.rsplit('/').next()) {
                rubrics.push(Rubric::english(REPLACED_BY, target));
            }
        }
        let deprecated = description
            .text_at("deprecated")
            .is_some_and(|d| d.eq_ignore_ascii_case("true"));
        classes.push(Class {
            code: code.to_owned(),
            kind: KIND.to_owned(),
            active: !deprecated,
            rubrics,
            ..Class::default()
        });
        Ok(())
    })
    .map_err(|source| DicomError::Xml {
        path: path.to_path_buf(),
        source,
    })?;
    if classes.is_empty() {
        return Err(DicomError::Empty(path.to_path_buf()));
    }
    let version = version
        .map(str::to_owned)
        .or(edition)
        .ok_or(DicomError::NoVersion)?;
    Ok(Classification {
        name: String::from("DCM"),
        title: String::from("DICOM Controlled Terminology"),
        version: Some(version),
        language: String::from("en"),
        kinds: vec![KIND.to_owned()],
        classes,
        designation_kinds: vec![PREFERRED.to_owned()],
        hierarchy: None,
    })
}
