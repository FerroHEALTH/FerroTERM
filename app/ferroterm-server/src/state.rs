// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The loaded server state: a registry of code system providers and the ids
//! their `CodeSystem` instances answer on.
//!
//! What an operation resolves is a [`Layer`]: the code systems, value sets,
//! and concept maps the deployment loaded from disk, with the client
//! resources persisted through the REST API on top. A write replaces the
//! layer, so a request in flight keeps the one it started with.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use fhir_terminology::artifact::ArtifactError;
use fhir_terminology::classification;
use fhir_terminology::fhir_codesystem::provider::BuildError;
use fhir_terminology::fhir_core::CoreTerminology;
use fhir_terminology::icd11;
use fhir_terminology::loinc;
use fhir_terminology::provider::code_system::CodeSystemModel;
use fhir_terminology::provider::concept_map::ConceptMapModel;
use fhir_terminology::provider::{CodeSystemProvider, ProviderError};
use fhir_terminology::registries::iso3166;
use fhir_terminology::registry::{RegisterError, Resolved};
use fhir_terminology::rxnorm;
use fhir_terminology::snomed::OpenError;
use fhir_terminology::valueset::model::ValueSetModel;
use fhir_terminology::versioned::Duplicate;

use crate::artefact::Caches;
use crate::persistence::{ResourceStore, ResourceType, StoreError};
use crate::state::layer::Layer;
use crate::state::persisted::{PersistError, Persisted};

mod duplicate;
mod ids;
pub mod layer;
pub mod load;
pub mod persisted;

/// A failure to load the state.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// An artifact directory does not open.
    #[error("cannot open the artifact at {path}")]
    Open {
        /// The directory.
        path: PathBuf,
        /// The cause.
        #[source]
        source: Box<OpenError>,
    },
    /// A LOINC artifact directory does not open.
    #[error("cannot open the LOINC artifact at {path}")]
    OpenLoinc {
        /// The directory.
        path: PathBuf,
        /// The cause.
        #[source]
        source: Box<loinc::OpenError>,
    },
    /// An `RxNorm` artifact directory does not open.
    #[error("cannot open the RxNorm artifact at {path}")]
    OpenRxNorm {
        /// The directory.
        path: PathBuf,
        /// The cause.
        #[source]
        source: Box<rxnorm::OpenError>,
    },
    /// An ICD-11 artifact directory does not open.
    #[error("cannot open the ICD-11 artifact at {path}")]
    OpenIcd11 {
        /// The directory.
        path: PathBuf,
        /// The cause.
        #[source]
        source: Box<icd11::OpenError>,
    },
    /// A classification artifact directory does not open.
    #[error("cannot open the classification artifact at {path}")]
    OpenClassification {
        /// The directory.
        path: PathBuf,
        /// The cause.
        #[source]
        source: Box<classification::OpenError>,
    },
    /// An artifact's manifest does not say which system it serves.
    #[error("cannot read the artifact at {path}")]
    Artifact {
        /// The directory.
        path: PathBuf,
        /// The cause.
        #[source]
        source: ArtifactError,
    },
    /// A configured index root does not list.
    #[error("cannot list the index root at {path}")]
    Root {
        /// The directory.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// An artifact serves a system this server has no provider for.
    #[error("the artifact at {path} serves `{system}`, which this server cannot open")]
    UnknownArtifact {
        /// The directory.
        path: PathBuf,
        /// The system.
        system: String,
    },
    /// A directory of `CodeSystem` resources does not load.
    #[error("cannot load the CodeSystem resources at {path}")]
    CodeSystems {
        /// The directory.
        path: PathBuf,
        /// The cause.
        #[source]
        source: Box<fhir_terminology::fhir_codesystem::load::LoadError>,
    },
    /// A `CodeSystem` resource does not build into a provider.
    #[error("cannot serve the CodeSystem `{url}` from {path}")]
    Build {
        /// The directory.
        path: PathBuf,
        /// The system.
        url: String,
        /// The cause.
        #[source]
        source: BuildError,
    },
    /// The embedded FHIR core terminology of a served version does not read.
    #[error("cannot read the FHIR {fhir_version} core terminology")]
    Core {
        /// The served version.
        fhir_version: &'static str,
        /// The cause.
        #[source]
        source: Box<fhir_terminology::fhir_core::CoreError>,
    },
    /// The vendored registry data of a registry code system does not build.
    #[error("cannot build the registry code system `{url}`")]
    Registry {
        /// The system.
        url: String,
        /// The cause.
        #[source]
        source: iso3166::DataError,
    },
    /// A supplement names a code system that is not loaded.
    #[error("the supplement `{url}` supplements `{target}`, which is not loaded")]
    SupplementTarget {
        /// The supplement.
        url: String,
        /// `CodeSystem.supplements`.
        target: String,
    },
    /// A supplement names no `supplements` canonical.
    #[error("the supplement `{url}` names no code system to supplement")]
    SupplementWithoutTarget {
        /// The supplement.
        url: String,
    },
    /// The persisted resource database does not open.
    #[error("cannot open the resource database at {path}")]
    Resources {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: Box<StoreError>,
    },
    /// A persisted resource does not layer over the loaded state.
    #[error("cannot serve the persisted resources")]
    Persisted(#[source] PersistError),
    /// Two loaded resources of one type answer on the same logical id.
    #[error("the {resource_type} `{second}` carries the id `{id}`, which `{first}` answers on")]
    DuplicateId {
        /// The resource type both are of.
        resource_type: &'static str,
        /// The id they share.
        id: String,
        /// The canonical of the resource already answering on the id.
        first: String,
        /// The canonical of the resource that carries it too.
        second: String,
    },
    /// A loaded resource carries the logical id of a persisted record.
    #[error(
        "the loaded {resource_type} `{canonical}` carries the id `{id}`, which a persisted {resource_type} holds"
    )]
    PersistedId {
        /// The resource type.
        resource_type: &'static str,
        /// The id both carry.
        id: String,
        /// The canonical of the loaded resource.
        canonical: String,
    },
    /// Two sources serve the same system version.
    #[error(transparent)]
    Register(#[from] RegisterError),
    /// Two sources carry the same value set or concept map version.
    #[error(transparent)]
    Duplicate(#[from] Duplicate),
}

/// One loaded code system version, for the startup summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceSummary {
    /// The `CodeSystem` instance id.
    pub id: String,
    /// The system URI.
    pub url: String,
    /// The version.
    pub version: String,
    /// The concept count, when the system enumerates its concepts.
    pub concepts: Option<u64>,
    /// The designation languages.
    pub languages: Vec<String>,
    /// The directory the system was loaded from, when loaded from one.
    pub path: Option<PathBuf>,
}

/// Where a code system the deployment loaded came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    /// An artifact directory named by `FERROTERM__TERMINOLOGY__INDEX`.
    Index,
    /// A `CodeSystem` resource from a directory named by
    /// `FERROTERM__TERMINOLOGY__CODE_SYSTEMS`, a supplement included.
    CodeSystems,
}

impl Origin {
    /// Returns the origin of the source loaded at `index`, when the
    /// `CodeSystem` directories were loaded from `first_code_system` on.
    const fn at(index: usize, first_code_system: usize) -> Self {
        if index >= first_code_system {
            Self::CodeSystems
        } else {
            Self::Index
        }
    }
}

/// What the handlers share.
#[derive(Debug)]
pub struct AppState {
    /// What the deployment loaded from disk, without the persisted resources.
    base: Layer,
    /// The terminology the FHIR specification defines, by served version; it
    /// sits under the deployment's own in every served layer.
    core: BTreeMap<&'static str, CoreTerminology>,
    /// The persisted client resources and the layer they and [`AppState::base`]
    /// form, replaced whole by every write.
    persisted: RwLock<Persisted>,
    /// The durable store of the persisted resources, when the deployment
    /// configured one. A reload carries it over: `redb` holds the file for as
    /// long as the handle lives, so the store is opened once per process
    /// (<https://docs.rs/redb/latest/redb/struct.Database.html>).
    store: Option<Arc<ResourceStore>>,
    /// `ValueSet` instance id to (url, version).
    value_set_instances: BTreeMap<String, (String, Option<String>)>,
    /// `ConceptMap` instance id to (url, version).
    concept_map_instances: BTreeMap<String, (String, Option<String>)>,
    caches: Arc<Caches>,
    /// `CodeSystem` instance id to (system, version).
    instances: BTreeMap<String, (String, String)>,
    /// The `CodeSystem` supplements the deployment loaded, by the id each is
    /// read at. A supplement is layered onto the system it supplements and is
    /// no instance of the registry, so it is held here beside them.
    supplements: BTreeMap<String, Arc<CodeSystemModel>>,
    /// The directory each version was loaded from.
    paths: BTreeMap<(String, String), PathBuf>,
    /// Where each system the deployment loaded came from, by system URL; the
    /// registry systems that ship with the server are not here.
    origins: BTreeMap<String, Origin>,
    /// The software version reported in the capability statements.
    software_version: &'static str,
    /// The authentication the deployment declares, as codes of the FHIR
    /// `restful-security-service` value set.
    security_services: Vec<String>,
    /// The base URL clients reach this server at, when the deployment named
    /// one; the capability statements state it per version.
    base_url: Option<String>,
}

impl AppState {
    /// What is loaded, one entry per code system version, sorted by id.
    ///
    /// # Errors
    ///
    /// Returns the provider's error when a concept count cannot be read.
    pub fn summaries(&self) -> Result<Vec<InstanceSummary>, ProviderError> {
        let layer = self.layer();
        let mut out = Vec::new();
        for (id, (url, version)) in &self.instances {
            let Ok(resolved) = layer.registry.resolve(url, Some(version)) else {
                continue;
            };
            let concepts = match resolved.provider.all() {
                Ok(set) => Some(set.len()),
                Err(ProviderError::NotEnumerable) => None,
                Err(error) => return Err(error),
            };
            out.push(InstanceSummary {
                id: id.clone(),
                url: url.clone(),
                version: version.clone(),
                concepts,
                languages: resolved.provider.declaration().languages.clone(),
                path: self.paths.get(&(url.clone(), version.clone())).cloned(),
            });
        }
        Ok(out)
    }

    /// What every operation resolves in: the loaded state with the persisted
    /// client resources over it.
    ///
    /// The layer is replaced whole by a write, so a caller keeps the one it
    /// took for as long as it holds the returned handle.
    #[must_use]
    pub fn layer(&self) -> Arc<Layer> {
        Arc::clone(
            &self
                .persisted
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .layer,
        )
    }

    /// What an operation on the surface of `fhir_version` resolves in: the
    /// layer above, with the terminology that FHIR version defines beneath it.
    ///
    /// A state built without the core terminology, and a version this server
    /// does not serve, answer with the layer above alone.
    #[must_use]
    pub fn served_layer(&self, fhir_version: &str) -> Arc<Layer> {
        let persisted = self
            .persisted
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        persisted
            .served
            .get(fhir_version)
            .map_or_else(|| Arc::clone(&persisted.layer), Arc::clone)
    }

    /// The caches `$cache-control` started.
    #[must_use]
    pub fn caches(&self) -> &Caches {
        &self.caches
    }

    /// The `ValueSet` instance ids and what they serve, the loaded and the
    /// persisted ones together, sorted by id.
    #[must_use]
    pub fn value_set_instances(&self) -> Vec<(String, String, Option<String>)> {
        let mut out: Vec<(String, String, Option<String>)> = self
            .value_set_instances
            .iter()
            .map(|(id, (url, version))| (id.clone(), url.clone(), version.clone()))
            .collect();
        let persisted = self
            .persisted
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for ((resource_type, id), record) in &persisted.records {
            if *resource_type != ResourceType::ValueSet {
                continue;
            }
            let Some(url) = record.url.clone() else {
                continue;
            };
            out.push((id.clone(), url, record.version.clone()));
        }
        out.sort_by(|(left, ..), (right, ..)| left.cmp(right));
        out
    }

    /// Resolves a `ValueSet` instance id, a persisted one first.
    #[must_use]
    pub fn value_set_instance(&self, id: &str) -> Option<Arc<ValueSetModel>> {
        // NOTE: one read guard for the record and the layer it belongs to; a second
        // nested read of a `std::sync::RwLock` can deadlock behind a waiting writer
        // (<https://doc.rust-lang.org/std/sync/struct.RwLock.html>).
        let persisted = self
            .persisted
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // NOTE: `ValueSet.url` is 0..1, and the layer stores a resource without one
        // under the empty canonical (<https://hl7.org/fhir/R4B/valueset.html>), so the
        // id still names it.
        if let Some(record) = persisted
            .records
            .get(&(ResourceType::ValueSet, id.to_owned()))
        {
            return persisted.layer.value_sets.resolve(
                record.url.as_deref().unwrap_or_default(),
                record.version.as_deref(),
            );
        }
        let (url, version) = self.value_set_instances.get(id)?;
        persisted.layer.value_sets.resolve(url, version.as_deref())
    }

    /// The `ConceptMap` instance ids and what they serve, the loaded and the
    /// persisted ones together, sorted by id.
    #[must_use]
    pub fn concept_map_instances(&self) -> Vec<(String, String, Option<String>)> {
        let mut out: Vec<(String, String, Option<String>)> = self
            .concept_map_instances
            .iter()
            .map(|(id, (url, version))| (id.clone(), url.clone(), version.clone()))
            .collect();
        let persisted = self
            .persisted
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for ((resource_type, id), record) in &persisted.records {
            if *resource_type != ResourceType::ConceptMap {
                continue;
            }
            let Some(url) = record.url.clone() else {
                continue;
            };
            out.push((id.clone(), url, record.version.clone()));
        }
        out.sort_by(|(left, ..), (right, ..)| left.cmp(right));
        out
    }

    /// Resolves a `ConceptMap` instance id, a persisted one first.
    #[must_use]
    pub fn concept_map_instance(&self, id: &str) -> Option<Arc<ConceptMapModel>> {
        // NOTE: one read guard for the record and the layer it belongs to; a second
        // nested read of a `std::sync::RwLock` can deadlock behind a waiting writer
        // (<https://doc.rust-lang.org/std/sync/struct.RwLock.html>).
        let persisted = self
            .persisted
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // NOTE: `ConceptMap.url` is 0..1, and the layer stores a resource without one
        // under the empty canonical (<https://hl7.org/fhir/R4B/conceptmap.html>), so the
        // id still names it.
        if let Some(record) = persisted
            .records
            .get(&(ResourceType::ConceptMap, id.to_owned()))
        {
            return persisted.layer.concept_maps.resolve(
                record.url.as_deref().unwrap_or_default(),
                record.version.as_deref(),
            );
        }
        let (url, version) = self.concept_map_instances.get(id)?;
        persisted
            .layer
            .concept_maps
            .resolve(url, version.as_deref())
    }

    /// The `CodeSystem` instance ids and what they serve, sorted by id.
    pub fn instances(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.instances
            .iter()
            .map(|(id, (url, version))| (id.as_str(), url.as_str(), version.as_str()))
    }

    /// Resolves a `CodeSystem` instance id, a persisted one first.
    #[must_use]
    pub fn instance(&self, id: &str) -> Option<Resolved> {
        let persisted = self
            .persisted
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(record) = persisted
            .records
            .get(&(ResourceType::CodeSystem, id.to_owned()))
            && let Some(url) = &record.url
        {
            return persisted
                .layer
                .registry
                .resolve(url, record.version.as_deref())
                .ok();
        }
        let (url, version) = self.instances.get(id)?;
        persisted.layer.registry.resolve(url, Some(version)).ok()
    }

    /// The loaded supplements and the resource each is read at its id, sorted
    /// by id.
    #[must_use]
    pub fn supplement_instances(&self) -> Vec<(String, Arc<CodeSystemModel>)> {
        self.supplements
            .iter()
            .map(|(id, model)| (id.clone(), Arc::clone(model)))
            .collect()
    }

    /// Resolves a loaded supplement's instance id.
    #[must_use]
    pub fn supplement_instance(&self, id: &str) -> Option<Arc<CodeSystemModel>> {
        self.supplements.get(id).map(Arc::clone)
    }

    /// The systems the deployment loaded and where each came from, sorted by
    /// system URL; the registry systems that ship with the server are left
    /// out.
    pub fn origins(&self) -> impl Iterator<Item = (&str, Origin)> {
        self.origins
            .iter()
            .map(|(url, origin)| (url.as_str(), *origin))
    }

    /// The software version.
    #[must_use]
    pub fn software_version(&self) -> &'static str {
        self.software_version
    }

    /// The authentication the deployment declares, as codes of the FHIR
    /// `restful-security-service` value set; empty when it declares none.
    #[must_use]
    pub fn security_services(&self) -> &[String] {
        &self.security_services
    }

    /// The base URL clients reach this server at, without a version prefix,
    /// when the deployment named one.
    #[must_use]
    pub fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    /// The default provider of a system, for callers that need one.
    #[must_use]
    pub fn provider(&self, url: &str) -> Option<Arc<dyn CodeSystemProvider>> {
        self.layer()
            .registry
            .resolve(url, None)
            .ok()
            .map(|resolved| resolved.provider)
    }
}

/// A FHIR resource id for a code system version.
///
/// The version URI when it carries the system (a SNOMED CT edition), otherwise
/// the system URL and the version, reduced to the id alphabet (`[A-Za-z0-9.-]`,
/// at most 64 characters, <https://hl7.org/fhir/R4B/datatypes.html#id>),
/// scheme dropped. No spec governs how a server names its instances: our own design.
#[must_use]
pub fn instance_id(url: &str, version: &str) -> String {
    let text = if version.starts_with(url) {
        version.to_owned()
    } else {
        format!("{url}-{version}")
    };
    let stripped = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"))
        .unwrap_or(&text);
    let mut id = String::with_capacity(stripped.len());
    let mut dash = false;
    for c in stripped.chars() {
        if c.is_ascii_alphanumeric() || c == '.' {
            id.push(c);
            dash = false;
        } else if !dash && !id.is_empty() {
            id.push('-');
            dash = true;
        }
    }
    let trimmed = id.trim_end_matches('-');
    trimmed.chars().take(64).collect()
}

#[cfg(test)]
mod tests {
    use super::instance_id;
    use crate::state::ids::split_canonical;

    #[test]
    fn instance_ids_fit_the_fhir_id_alphabet() {
        assert_eq!(
            instance_id(
                "http://snomed.info/sct",
                "http://snomed.info/sct/11000146104/version/20260630"
            ),
            "snomed.info-sct-11000146104-version-20260630"
        );
        assert_eq!(
            instance_id("http://terminology.hl7.org/CodeSystem/v2-0001", "2.0.0"),
            "terminology.hl7.org-CodeSystem-v2-0001-2.0.0"
        );
        assert!(instance_id("http://example.org/x", &"x".repeat(100)).len() <= 64);
    }

    #[test]
    fn canonicals_split_on_the_version_bar() {
        assert_eq!(
            split_canonical("http://a.example/cs|2.0"),
            ("http://a.example/cs", Some("2.0"))
        );
        assert_eq!(
            split_canonical("http://a.example/cs"),
            ("http://a.example/cs", None)
        );
    }
}
