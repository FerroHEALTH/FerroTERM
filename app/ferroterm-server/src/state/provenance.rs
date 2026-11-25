// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `Provenance` resources a deployment loads beside its terminology.
//!
//! A `Provenance` records who produced or validated a resource and when
//! (<https://hl7.org/fhir/R4B/provenance.html>). The draft `EEHRxF` implementing
//! act Art 6 admits a national code only with a validated mapping whose
//! versions and provenance are recorded, so a `ConceptMap`'s provenance is
//! kept from the same package directories and read by the canonical its
//! `target` names.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fhir_types::codec::{Object, Value};

use super::LoadError;

/// One loaded `Provenance`, as written, with the FHIR version it was read in.
#[derive(Debug, Clone)]
pub struct Held {
    /// The FHIR version of the package it came from (`4.0.1`, `5.0.0`, ...).
    pub fhir_version: &'static str,
    /// The resource as the package carries it.
    pub object: Object,
}

/// The `target.reference` values of a `Provenance` resource.
pub fn targets(object: &Object) -> impl Iterator<Item = &str> {
    object
        .get("target")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|target| target.get("reference").and_then(Value::as_str))
}

/// Reads every `Provenance` in the package directory `path`, written in
/// `fhir_version`, by its id.
///
/// Each one is decoded with the generated model of its version, so a
/// malformed one refuses the start rather than a later read.
///
/// # Errors
///
/// A file that cannot be read, a `Provenance` that does not decode in its
/// version, one without an `id`, and two with one `id`.
pub(super) fn load(
    path: &Path,
    fhir_version: &'static str,
    held: &mut BTreeMap<String, Held>,
) -> Result<(), LoadError> {
    let failed = |file: &Path, reason: String| LoadError::Provenance {
        path: file.to_path_buf(),
        reason,
    };
    let entries = std::fs::read_dir(path).map_err(|source| LoadError::ProvenanceDir {
        path: path.to_path_buf(),
        source,
    })?;
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let file = entry
            .map_err(|source| LoadError::ProvenanceDir {
                path: path.to_path_buf(),
                source,
            })?
            .path();
        if file
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            files.push(file);
        }
    }
    files.sort();
    for file in files {
        let text = std::fs::read(&file).map_err(|source| LoadError::ProvenanceDir {
            path: file.clone(),
            source,
        })?;
        // NOTE: a package directory holds JSON of every kind (its manifest, an
        // index); a file that is no `Provenance` object is another loader's.
        let Ok(Value::Object(object)) = serde_json::from_slice::<Value>(&text) else {
            continue;
        };
        if object.get("resourceType").and_then(Value::as_str) != Some("Provenance") {
            continue;
        }
        crate::served::decode(fhir_version, &object).map_err(|reason| failed(&file, reason))?;
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| failed(&file, String::from("the Provenance states no `id`")))?;
        if held.contains_key(&id) {
            return Err(failed(
                &file,
                format!("another loaded Provenance has the id `{id}`"),
            ));
        }
        held.insert(
            id,
            Held {
                fhir_version,
                object,
            },
        );
    }
    Ok(())
}
