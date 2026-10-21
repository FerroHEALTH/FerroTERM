// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading `CodeSystem` resources from JSON files: a directory of resources or
//! a FHIR package's `package/` directory, in the FHIR version the files use.

use std::path::{Path, PathBuf};

use fhir_types::codec::{DecodeError, Json, Path as ElementPath, expect_object};

use super::convert;
use super::model::{CodeSystemModel, ModelError};

/// The FHIR version the JSON files are written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FhirVersion {
    /// FHIR R4 (4.0.1).
    R4,
    /// FHIR R4B (4.3.0).
    R4B,
    /// FHIR R5 (5.0.0).
    R5,
    /// FHIR R6 (ballot).
    R6,
}

impl FhirVersion {
    /// The version a FHIR package's `fhirVersions` entry names, by its major
    /// and minor (`4.0.x` R4, `4.3.x` R4B, `5.0.x` R5, `6.0.x` R6).
    #[must_use]
    pub fn from_fhir_version(version: &str) -> Option<Self> {
        let mut parts = version.split('.');
        match (parts.next()?, parts.next()?) {
            ("4", "0") => Some(Self::R4),
            ("4", "3") => Some(Self::R4B),
            ("5", "0") => Some(Self::R5),
            ("6", "0") => Some(Self::R6),
            _ => None,
        }
    }
}

/// A failure to load.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// A file or directory cannot be read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// A file is not JSON.
    #[error("{path} is not JSON")]
    Json {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: serde_json::Error,
    },
    /// A resource file does not fit the version's definition.
    #[error("{path} is not a {version:?} {resource_type}")]
    Decode {
        /// The path.
        path: PathBuf,
        /// The version tried.
        version: FhirVersion,
        /// The resource type expected.
        resource_type: &'static str,
        /// The cause.
        #[source]
        source: DecodeError,
    },
    /// A `CodeSystem` cannot be modelled.
    #[error("{path}: cannot model the CodeSystem")]
    Model {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: ModelError,
    },
    /// A `ValueSet` cannot be modelled.
    #[error("{path}: cannot model the ValueSet")]
    ValueSet {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: crate::valueset::model::ModelError,
    },
    /// A `ConceptMap` cannot be modelled.
    #[error("{path}: cannot model the ConceptMap")]
    ConceptMap {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: crate::conceptmap::model::ModelError,
    },
}

/// Loads a `CodeSystem` JSON file.
///
/// # Errors
///
/// Returns [`LoadError`] when the file does not read, parse, decode, or model.
pub fn load_file(path: &Path, version: FhirVersion) -> Result<CodeSystemModel, LoadError> {
    let text = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let value: fhir_types::codec::Value =
        serde_json::from_str(&text).map_err(|source| LoadError::Json {
            path: path.to_path_buf(),
            source,
        })?;
    model_from_value(&value, version).map_err(|source| source.at(path, version))
}

/// A decode or model failure before its path is known.
pub(crate) enum Decoded {
    Decode(DecodeError),
    Model(ModelError),
}

impl Decoded {
    fn at(self, path: &Path, version: FhirVersion) -> LoadError {
        match self {
            Self::Decode(source) => LoadError::Decode {
                path: path.to_path_buf(),
                version,
                resource_type: "CodeSystem",
                source,
            },
            Self::Model(source) => LoadError::Model {
                path: path.to_path_buf(),
                source,
            },
        }
    }
}

pub(crate) fn model_from_value(
    value: &fhir_types::codec::Value,
    version: FhirVersion,
) -> Result<CodeSystemModel, Decoded> {
    let mut path = ElementPath::root("CodeSystem");
    let object = expect_object(value, &path).map_err(Decoded::Decode)?;
    let model = match version {
        FhirVersion::R4 => convert::r4::convert(
            &fhir_types::r4::code_system::CodeSystem::from_json(object, &mut path)
                .map_err(Decoded::Decode)?,
        ),
        FhirVersion::R4B => convert::r4b::convert(
            &fhir_types::r4b::code_system::CodeSystem::from_json(object, &mut path)
                .map_err(Decoded::Decode)?,
        ),
        FhirVersion::R5 => convert::r5::convert(
            &fhir_types::r5::code_system::CodeSystem::from_json(object, &mut path)
                .map_err(Decoded::Decode)?,
        ),
        FhirVersion::R6 => convert::r6::convert(
            &fhir_types::r6::code_system::CodeSystem::from_json(object, &mut path)
                .map_err(Decoded::Decode)?,
        ),
    };
    model.map_err(Decoded::Model)
}

/// Loads every `CodeSystem` resource in a directory.
///
/// Files whose `resourceType` is not `CodeSystem` are skipped; the result is
/// sorted by file name so it is deterministic.
///
/// # Errors
///
/// Returns [`LoadError`] when the directory or a `CodeSystem` file fails.
pub fn load_dir(dir: &Path, version: FhirVersion) -> Result<Vec<CodeSystemModel>, LoadError> {
    let mut models = Vec::new();
    for (path, value) in scan_json(dir, "CodeSystem")? {
        models.push(model_from_value(&value, version).map_err(|source| source.at(&path, version))?);
    }
    Ok(models)
}

/// The JSON files in `dir` whose `resourceType` is `resource_type`, sorted by
/// file name so the result is deterministic.
///
/// # Errors
///
/// Returns [`LoadError::Io`] when the directory or a file does not read.
pub(crate) fn scan_json(
    dir: &Path,
    resource_type: &str,
) -> Result<Vec<(PathBuf, fhir_types::codec::Value)>, LoadError> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|source| LoadError::Io {
            path: dir.to_path_buf(),
            source,
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        })
        .collect();
    paths.sort();
    let mut found = Vec::new();
    for path in paths {
        let text = std::fs::read_to_string(&path).map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        let value: fhir_types::codec::Value = match serde_json::from_str(&text) {
            Ok(value) => value,
            // NOTE: a package directory holds non-resource JSON (package.json,
            // .index.json); a file that is not a resource is not a CodeSystem.
            Err(_) => continue,
        };
        if value.get("resourceType").and_then(|t| t.as_str()) == Some(resource_type) {
            found.push((path, value));
        }
    }
    Ok(found)
}

/// The FHIR version a package declares in its `package.json`
/// (`fhirVersions[0]`), when the directory is a package's `package/` dir.
///
/// # Errors
///
/// Returns [`LoadError`] when `package.json` exists but does not read or parse.
pub fn package_version(dir: &Path) -> Result<Option<FhirVersion>, LoadError> {
    let manifest = dir.join("package.json");
    if !manifest.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&manifest).map_err(|source| LoadError::Io {
        path: manifest.clone(),
        source,
    })?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| LoadError::Json {
            path: manifest,
            source,
        })?;
    Ok(value
        .get("fhirVersions")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .and_then(FhirVersion::from_fhir_version))
}
