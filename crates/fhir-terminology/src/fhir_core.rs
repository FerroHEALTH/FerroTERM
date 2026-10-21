// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology the FHIR specification itself defines, per served version.
//!
//! Each FHIR version defines its own code systems and value sets and publishes
//! them in its core package
//! (<https://hl7.org/fhir/R4B/terminologies-systems.html> and
//! <https://hl7.org/fhir/R4B/terminologies-valuesets.html>), so a server that
//! serves that version answers over them without a deployment supplying
//! anything. The content is embedded as one bundle per version, generated from
//! the core packages and committed under `data/fhir/`; the same code systems
//! and value sets are served on the surface of the version that defines them.

use std::sync::Arc;

use crate::fhir_codesystem::load::FhirVersion;
use crate::fhir_codesystem::provider::{BuildError, FhirCodeSystem};
use crate::registry::{RegisterError, Registry};
use crate::valueset::store::ValueSetStore;
use crate::versioned::Duplicate;

/// The R4 code systems, generated from `hl7.fhir.r4.core` 4.0.1.
const R4_CODE_SYSTEMS: &str = include_str!("../data/fhir/r4/code-systems.json");
/// The R4 value sets, generated from `hl7.fhir.r4.core` 4.0.1.
const R4_VALUE_SETS: &str = include_str!("../data/fhir/r4/value-sets.json");
/// The R4B code systems, generated from `hl7.fhir.r4b.core` 4.3.0.
const R4B_CODE_SYSTEMS: &str = include_str!("../data/fhir/r4b/code-systems.json");
/// The R4B value sets, generated from `hl7.fhir.r4b.core` 4.3.0.
const R4B_VALUE_SETS: &str = include_str!("../data/fhir/r4b/value-sets.json");
/// The R5 code systems, generated from `hl7.fhir.r5.core` 5.0.0.
const R5_CODE_SYSTEMS: &str = include_str!("../data/fhir/r5/code-systems.json");
/// The R5 value sets, generated from `hl7.fhir.r5.core` 5.0.0.
const R5_VALUE_SETS: &str = include_str!("../data/fhir/r5/value-sets.json");
/// The R6 code systems, generated from `hl7.fhir.r6.core` 6.0.0-ballot5.
const R6_CODE_SYSTEMS: &str = include_str!("../data/fhir/r6/code-systems.json");
/// The R6 value sets, generated from `hl7.fhir.r6.core` 6.0.0-ballot5.
const R6_VALUE_SETS: &str = include_str!("../data/fhir/r6/value-sets.json");

/// A failure to read an embedded bundle.
///
/// Every variant means the generated bundle disagrees with the engine, which
/// a regeneration fixes; nothing a request sends can cause one.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A bundle is not a JSON array of resources.
    #[error("the {version:?} {resource_type} bundle is not JSON")]
    Json {
        /// The version whose bundle failed.
        version: FhirVersion,
        /// The resource type the bundle holds.
        resource_type: &'static str,
        /// The cause.
        #[source]
        source: serde_json::Error,
    },
    /// A resource does not fit the version's definition.
    #[error("a {version:?} {resource_type} in the bundle does not decode")]
    Decode {
        /// The version whose bundle failed.
        version: FhirVersion,
        /// The resource type the bundle holds.
        resource_type: &'static str,
        /// The cause.
        #[source]
        source: fhir_types::codec::DecodeError,
    },
    /// A `CodeSystem` cannot be modelled.
    #[error("a {version:?} CodeSystem in the bundle cannot be modelled")]
    CodeSystemModel {
        /// The version whose bundle failed.
        version: FhirVersion,
        /// The cause.
        #[source]
        source: crate::fhir_codesystem::model::ModelError,
    },
    /// A `ValueSet` cannot be modelled.
    #[error("a {version:?} ValueSet in the bundle cannot be modelled")]
    ValueSetModel {
        /// The version whose bundle failed.
        version: FhirVersion,
        /// The cause.
        #[source]
        source: crate::valueset::model::ModelError,
    },
    /// A `CodeSystem` cannot be served.
    #[error("the {version:?} CodeSystem `{url}` cannot be served")]
    Build {
        /// The version whose bundle failed.
        version: FhirVersion,
        /// The system.
        url: String,
        /// The cause.
        #[source]
        source: BuildError,
    },
    /// Two bundled resources share a canonical.
    #[error("the {version:?} bundle registers `{canonical}` twice")]
    Duplicate {
        /// The version whose bundle failed.
        version: FhirVersion,
        /// The `url|version` canonical.
        canonical: String,
    },
}

/// The code systems and value sets one FHIR version defines.
#[derive(Debug, Clone)]
pub struct CoreTerminology {
    code_systems: Arc<Registry>,
    value_sets: Arc<ValueSetStore>,
}

impl CoreTerminology {
    /// Reads the bundle `version` publishes.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the embedded bundle does not parse, decode,
    /// model, or build, which means the bundle and the engine disagree.
    pub fn load(version: FhirVersion) -> Result<Self, CoreError> {
        let (code_systems, value_sets) = match version {
            FhirVersion::R4 => (R4_CODE_SYSTEMS, R4_VALUE_SETS),
            FhirVersion::R4B => (R4B_CODE_SYSTEMS, R4B_VALUE_SETS),
            FhirVersion::R5 => (R5_CODE_SYSTEMS, R5_VALUE_SETS),
            FhirVersion::R6 => (R6_CODE_SYSTEMS, R6_VALUE_SETS),
        };
        Ok(Self {
            code_systems: Arc::new(registry_of(code_systems, version)?),
            value_sets: Arc::new(store_of(value_sets, version)?),
        })
    }

    /// The code systems, as a registry to put under a deployment's own.
    #[must_use]
    pub fn code_systems(&self) -> Arc<Registry> {
        Arc::clone(&self.code_systems)
    }

    /// The value sets, as a store to put under a deployment's own.
    #[must_use]
    pub fn value_sets(&self) -> Arc<ValueSetStore> {
        Arc::clone(&self.value_sets)
    }
}

/// Parses one bundle into the JSON resources it holds.
fn resources_of(
    bundle: &str,
    version: FhirVersion,
    resource_type: &'static str,
) -> Result<Vec<fhir_types::codec::Value>, CoreError> {
    serde_json::from_str(bundle).map_err(|source| CoreError::Json {
        version,
        resource_type,
        source,
    })
}

/// The registry of one version's bundled code systems.
fn registry_of(bundle: &str, version: FhirVersion) -> Result<Registry, CoreError> {
    let mut registry = Registry::new();
    for value in resources_of(bundle, version, "CodeSystem")? {
        let model =
            crate::fhir_codesystem::load::model_from_value(&value, version).map_err(|decoded| {
                match decoded {
                    crate::fhir_codesystem::load::Decoded::Decode(source) => CoreError::Decode {
                        version,
                        resource_type: "CodeSystem",
                        source,
                    },
                    crate::fhir_codesystem::load::Decoded::Model(source) => {
                        CoreError::CodeSystemModel { version, source }
                    }
                }
            })?;
        let url = model.url.clone();
        let provider = FhirCodeSystem::new(model).map_err(|source| CoreError::Build {
            version,
            url: url.clone(),
            source,
        })?;
        registry.register(Arc::new(provider)).map_err(
            |RegisterError::Duplicate { url, version: v }| CoreError::Duplicate {
                version,
                canonical: format!("{url}|{v}"),
            },
        )?;
    }
    Ok(registry)
}

/// The store of one version's bundled value sets.
fn store_of(bundle: &str, version: FhirVersion) -> Result<ValueSetStore, CoreError> {
    let mut store = ValueSetStore::new();
    for value in resources_of(bundle, version, "ValueSet")? {
        let model =
            crate::valueset::load::model_from_value(&value, version).map_err(|decoded| {
                match decoded {
                    crate::valueset::load::Decoded::Decode(source) => CoreError::Decode {
                        version,
                        resource_type: "ValueSet",
                        source,
                    },
                    crate::valueset::load::Decoded::Model(source) => {
                        CoreError::ValueSetModel { version, source }
                    }
                }
            })?;
        store
            .insert(model)
            .map_err(|Duplicate { canonical }| CoreError::Duplicate { version, canonical })?;
    }
    Ok(store)
}
