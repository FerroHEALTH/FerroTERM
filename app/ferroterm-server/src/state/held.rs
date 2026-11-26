// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resources a deployment loads beside its terminology and serves from
//! their record: the `Provenance` of a map and the `NamingSystem` of an
//! identifier or code system namespace.
//!
//! The terminology engine holds no model of either, so each one is kept as the
//! package wrote it and read and searched through the reading version's codec.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fhir_types::codec::{Object, Value};

use super::LoadError;
use crate::persistence::ResourceType;

/// The resource types a package directory contributes as held records.
pub const HELD: [ResourceType; 2] = [ResourceType::Provenance, ResourceType::NamingSystem];

/// One loaded resource, as written, with the FHIR version it was read in.
#[derive(Debug, Clone)]
pub struct Held {
    /// The FHIR version of the package it came from (`4.0.1`, `5.0.0`, ...).
    pub fhir_version: &'static str,
    /// The resource as the package carries it.
    pub object: Object,
}

/// Every held resource the deployment loaded, by type and id.
pub type HeldResources = BTreeMap<(ResourceType, String), Held>;

/// Reads every resource of a [`HELD`] type in the package directory `path`,
/// written in `fhir_version`, into `held`.
///
/// Each one is decoded with the generated model of its version, so a
/// malformed one refuses the start rather than a later read.
///
/// # Errors
///
/// A file that cannot be read, a resource that does not decode in its
/// version, one without an `id`, and two of one type with one `id`.
pub(super) fn load(
    path: &Path,
    fhir_version: &'static str,
    held: &mut HeldResources,
) -> Result<(), LoadError> {
    let unreadable = |path: &Path, source| LoadError::PackageDir {
        path: path.to_path_buf(),
        source,
    };
    let entries = std::fs::read_dir(path).map_err(|source| unreadable(path, source))?;
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let file = entry.map_err(|source| unreadable(path, source))?.path();
        if file
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            files.push(file);
        }
    }
    files.sort();
    for file in files {
        let text = std::fs::read(&file).map_err(|source| unreadable(&file, source))?;
        // NOTE: a package directory holds JSON of every kind (its manifest, an
        // index); a file that is no object of a held type is another loader's.
        let Ok(Value::Object(object)) = serde_json::from_slice::<Value>(&text) else {
            continue;
        };
        let Some(resource_type) = object
            .get("resourceType")
            .and_then(Value::as_str)
            .and_then(ResourceType::parse)
            .filter(|parsed| HELD.contains(parsed))
        else {
            continue;
        };
        let refused = |reason: String| LoadError::Held {
            resource_type: resource_type.name(),
            path: file.clone(),
            reason,
        };
        crate::served::decode(fhir_version, &object).map_err(refused)?;
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| refused(String::from("it states no `id`")))?;
        let key = (resource_type, id);
        if held.contains_key(&key) {
            return Err(refused(format!(
                "another loaded {} has the id `{}`",
                resource_type.name(),
                key.1
            )));
        }
        held.insert(
            key,
            Held {
                fhir_version,
                object,
            },
        );
    }
    Ok(())
}
