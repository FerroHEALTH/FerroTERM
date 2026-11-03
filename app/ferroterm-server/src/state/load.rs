// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Loading the state from the configured artifacts, and reloading it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use fhir_terminology::artifact;
use fhir_terminology::classification::{self, ClassificationProvider};
use fhir_terminology::conceptmap;
use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::fhir_codesystem::load::{FhirVersion, load_dir, package_version};
use fhir_terminology::fhir_codesystem::provider::FhirCodeSystem;
use fhir_terminology::fhir_core::CoreTerminology;
use fhir_terminology::icd11::{self, Icd11Provider};
use fhir_terminology::loinc::{self, LoincProvider};
use fhir_terminology::provider::CodeSystemProvider;
use fhir_terminology::provider::code_system::CodeSystemModel;
use fhir_terminology::provider::declaration::ContentMode;
use fhir_terminology::registries::ucum::provider::UcumProvider;
use fhir_terminology::registries::{bcp13, bcp47, iso3166};
use fhir_terminology::registry::Registry;
use fhir_terminology::rxnorm::{self, RxNormProvider};
use fhir_terminology::snomed::{self, SnomedProvider};
use fhir_terminology::supplement::Supplement;
use fhir_terminology::valueset;
use fhir_terminology::valueset::store::ValueSetStore;

use crate::artefact::Caches;
use crate::config::Config;
use crate::persistence::{ResourceStore, ResourceType};
use crate::state::ids::{canonical, check_distinct_ids, registered_id, split_canonical, unique_id};
use crate::state::layer::Layer;
use crate::state::persisted::Persisted;
use crate::state::{AppState, LoadError, Origin, instance_id};

/// The FHIR core terminology of every served version, read once per process.
///
/// The bundles are embedded and generated, so the result is the same on every
/// call; the cache keeps a second server in one process from reading them
/// again.
pub(super) fn core_terminology() -> Result<BTreeMap<&'static str, CoreTerminology>, LoadError> {
    static CACHE: std::sync::OnceLock<BTreeMap<&str, CoreTerminology>> = std::sync::OnceLock::new();
    if let Some(cached) = CACHE.get() {
        return Ok(cached.clone());
    }
    let mut built = BTreeMap::new();
    for (fhir_version, bundle) in crate::served::CORE {
        built.insert(
            fhir_version,
            CoreTerminology::load(bundle).map_err(|source| LoadError::Core {
                fhir_version,
                source: Box::new(source),
            })?,
        );
    }
    Ok(CACHE.get_or_init(|| built).clone())
}
/// A provider before registration, with where it came from.
pub(super) struct Loaded {
    path: PathBuf,
    provider: Arc<dyn CodeSystemProvider>,
}
/// What a rebuilt state takes over from the state it replaces.
///
/// The write store and the `$cache-control` caches outlive one served set:
/// reopening the store would meet `redb`'s own file lock, and a cache handle
/// that restarted with the set would lie to a client holding an id.
#[derive(Debug, Default)]
pub(super) struct Carried {
    /// The live write store, when the state being replaced opened one.
    store: Option<Arc<ResourceStore>>,
    /// The caches `$cache-control` started.
    caches: Option<Arc<Caches>>,
}
/// Loads the `CodeSystem`, `ValueSet`, and `ConceptMap` resources in `path`:
/// complete systems become providers, supplements are collected for
/// dormant registration, value sets and concept maps go to their stores.
///
/// The FHIR version is the one the directory's `package.json` declares;
/// a plain directory of resources is read as R4B, the version the server
/// serves.
pub(super) fn load_code_systems(
    path: &Path,
    loaded: &mut Vec<Loaded>,
    supplements: &mut Vec<(String, CodeSystemModel)>,
    value_sets: &mut ValueSetStore,
    concept_maps: &mut ConceptMapStore,
) -> Result<(), LoadError> {
    let failed = |source| LoadError::CodeSystems {
        path: path.to_path_buf(),
        source: Box::new(source),
    };
    let version = package_version(path)
        .map_err(failed)?
        .unwrap_or(FhirVersion::R4B);
    for model in load_dir(path, version).map_err(failed)? {
        if model.content == ContentMode::Supplement {
            let target =
                model
                    .supplements
                    .clone()
                    .ok_or_else(|| LoadError::SupplementWithoutTarget {
                        url: model.url.clone(),
                    })?;
            supplements.push((target, model));
            continue;
        }
        let url = model.url.clone();
        let provider = FhirCodeSystem::new(model).map_err(|source| LoadError::Build {
            path: path.to_path_buf(),
            url,
            source,
        })?;
        loaded.push(Loaded {
            path: path.to_path_buf(),
            provider: Arc::new(provider),
        });
    }
    for model in valueset::load::load_dir(path, version).map_err(failed)? {
        value_sets.insert(model)?;
    }
    for model in conceptmap::load::load_dir(path, version).map_err(failed)? {
        concept_maps.insert(model)?;
    }
    Ok(())
}
/// The supplement a `CodeSystem` resource describes.
pub(crate) fn supplement_of(model: &CodeSystemModel) -> Supplement {
    Supplement::from_code_system(model)
}
/// Every supplement names a loaded system (`CodeSystem.supplements`, a `url`
/// or `url|version` canonical,
/// <https://hl7.org/fhir/R4B/codesystem-definitions.html#CodeSystem.supplements>).
pub(super) fn check_supplement_targets(
    loaded: &[Loaded],
    supplements: &[(String, CodeSystemModel)],
) -> Result<(), LoadError> {
    for (target, supplement) in supplements {
        let (url, version) = split_canonical(target);
        let served = loaded.iter().any(|l| {
            let identity = l.provider.identity();
            identity.url == url && version.is_none_or(|v| identity.version == v)
        });
        if !served {
            return Err(LoadError::SupplementTarget {
                url: supplement.url.clone(),
                target: target.clone(),
            });
        }
    }
    Ok(())
}
/// The artifact directories the configured index paths name.
///
/// A path holding a manifest is one artifact, and so is a path that is no
/// directory at all, which keeps a path naming nothing a refused start. A
/// directory holding no manifest is a root: the child directories that hold
/// one are its artifacts, in the order their names sort, and a child that
/// holds none is passed over.
///
/// No FHIR or SNOMED specification governs this: our own design.
///
/// # Errors
///
/// Returns [`LoadError::Root`] when a root does not list.
pub(super) fn artifact_paths(index: &[PathBuf]) -> Result<Vec<PathBuf>, LoadError> {
    let mut out = Vec::new();
    for path in index {
        if !path.is_dir() || artifact::is_artifact(path) {
            out.push(path.clone());
            continue;
        }
        let children = artifacts_under(path)?;
        if children.is_empty() {
            tracing::warn!(root = %path.display(), "the index root holds no artifact");
        }
        out.extend(children);
    }
    Ok(out)
}
/// The child directories of `root` that hold a manifest, sorted by path.
///
/// # Errors
///
/// Returns [`LoadError::Root`] when the directory does not list.
pub(super) fn artifacts_under(root: &Path) -> Result<Vec<PathBuf>, LoadError> {
    let entries = std::fs::read_dir(root).map_err(|source| LoadError::Root {
        path: root.to_path_buf(),
        source,
    })?;
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| LoadError::Root {
            path: root.to_path_buf(),
            source,
        })?;
        let child = entry.path();
        // NOTE: a directory a build is still writing carries no manifest yet, so staging
        // beside the root and renaming into it races with no reload: our own design.
        if artifact::is_artifact(&child) {
            out.push(child);
        }
    }
    // `read_dir` yields entries in whatever order the filesystem holds them
    // (<https://doc.rust-lang.org/std/fs/fn.read_dir.html>), so the order is ours to set.
    out.sort();
    Ok(out)
}
/// Opens the artifact directory `path` with the provider its manifest calls
/// for: SNOMED CT, LOINC, and `RxNorm` by system, a classification by kind.
///
/// # Errors
///
/// A manifest that is missing, unreadable, or names a system no provider
/// serves, and an artifact whose files fail to open, are `LoadError::Artifact`
/// with the path and the cause.
pub(super) fn open_artifact(
    path: &Path,
    config: &Config,
) -> Result<Arc<dyn CodeSystemProvider>, LoadError> {
    let described = artifact::describe(path).map_err(|source| LoadError::Artifact {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(match described.system.as_str() {
        snomed::SYSTEM => Arc::new(
            SnomedProvider::open(path, &config.terminology.default_language).map_err(|source| {
                LoadError::Open {
                    path: path.to_path_buf(),
                    source: Box::new(source),
                }
            })?,
        ),
        loinc::SYSTEM => {
            Arc::new(
                LoincProvider::open(path).map_err(|source| LoadError::OpenLoinc {
                    path: path.to_path_buf(),
                    source: Box::new(source),
                })?,
            )
        }
        rxnorm::SYSTEM => {
            Arc::new(
                RxNormProvider::open(path).map_err(|source| LoadError::OpenRxNorm {
                    path: path.to_path_buf(),
                    source: Box::new(source),
                })?,
            )
        }
        _ if described.kind.as_deref() == Some(icd11::KIND) => Arc::new(
            Icd11Provider::open(path).map_err(|source| LoadError::OpenIcd11 {
                path: path.to_path_buf(),
                source: Box::new(source),
            })?,
        ),
        _ if described.kind.as_deref() == Some(classification::KIND) => {
            Arc::new(ClassificationProvider::open(path).map_err(|source| {
                LoadError::OpenClassification {
                    path: path.to_path_buf(),
                    source: Box::new(source),
                }
            })?)
        }
        other => {
            return Err(LoadError::UnknownArtifact {
                path: path.to_path_buf(),
                system: other.to_owned(),
            });
        }
    })
}

impl AppState {
    /// Loads every artifact and every `CodeSystem` directory `config` names
    /// into a registry, supplements applied to the systems they name.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError`] when a source does not load, a supplement names
    /// a system that is not loaded, or two sources serve the same system
    /// version. A server never starts on a bad index.
    pub fn load(config: &Config) -> Result<Self, LoadError> {
        Self::build(config, Carried::default())
    }
    /// The same configuration read again into a fresh state, carrying this
    /// state's write store and caches.
    ///
    /// The caller swaps the result in; this state keeps answering until it
    /// does, and until the last request holding it finishes.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError`] exactly as [`AppState::load`] does. The failure
    /// leaves this state untouched, because nothing has been swapped.
    pub fn reloaded(&self, config: &Config) -> Result<Self, LoadError> {
        Self::build(
            config,
            Carried {
                store: self.store.clone(),
                caches: Some(Arc::clone(&self.caches)),
            },
        )
    }
    /// The state `config` names, built over what `carried` hands on.
    pub(super) fn build(config: &Config, carried: Carried) -> Result<Self, LoadError> {
        let mut loaded = Vec::new();
        for path in artifact_paths(&config.terminology.index)? {
            let provider = open_artifact(&path, config)?;
            loaded.push(Loaded { path, provider });
        }
        // NOTE: the registry systems ship with the server, so a validator finds BCP 47,
        // BCP 13, UCUM, and ISO 3166 without configuration
        // (<https://hl7.org/fhir/R4B/terminologies-systems.html>).
        loaded.push(Loaded {
            path: PathBuf::new(),
            provider: Arc::new(bcp47::Bcp47Provider::new()),
        });
        loaded.push(Loaded {
            path: PathBuf::new(),
            provider: Arc::new(bcp13::Bcp13Provider::new()),
        });
        loaded.push(Loaded {
            path: PathBuf::new(),
            provider: Arc::new(UcumProvider::new()),
        });
        loaded.push(Loaded {
            path: PathBuf::new(),
            provider: Arc::new(iso3166::provider().map_err(|source| LoadError::Registry {
                url: iso3166::URL.to_owned(),
                source,
            })?),
        });
        // Everything pushed from here on came from a `CodeSystem` directory.
        let from_code_systems = loaded.len();
        let mut supplements = Vec::new();
        let mut value_sets = ValueSetStore::default();
        let mut concept_maps = ConceptMapStore::default();
        for path in &config.terminology.code_systems {
            load_code_systems(
                path,
                &mut loaded,
                &mut supplements,
                &mut value_sets,
                &mut concept_maps,
            )?;
        }
        check_supplement_targets(&loaded, &supplements)?;
        check_distinct_ids(
            ResourceType::CodeSystem,
            loaded.iter().filter_map(|l| {
                let model = l.provider.code_system()?;
                Some((
                    model.id.clone()?,
                    canonical(&model.url, Some(&model.version)),
                ))
            }),
        )?;
        let mut registry = Registry::default();
        let mut paths = BTreeMap::new();
        let mut origins = BTreeMap::new();
        for (index, Loaded { path, provider }) in loaded.into_iter().enumerate() {
            let identity = provider.identity();
            if !path.as_os_str().is_empty() {
                origins.insert(identity.url.clone(), Origin::at(index, from_code_systems));
                paths.insert((identity.url.clone(), identity.version.clone()), path);
            }
            registry.register(provider)?;
        }
        // NOTE: a loaded supplement stays dormant until a request names it
        // (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
        for (target, model) in &supplements {
            origins.insert(model.url.clone(), Origin::CodeSystems);
            registry.register_supplement(target.clone(), supplement_of(model));
        }
        let mut state = Self::from_registry(registry);
        if let Some(caches) = carried.caches {
            state.caches = caches;
        }
        state.paths = paths;
        state.origins = origins;
        state
            .security_services
            .clone_from(&config.fhir.security_services);
        state.base_url = config.base_url();
        state.register_supplements(supplements)?;
        state.register_instances(&value_sets, &concept_maps)?;
        state.base.value_sets = value_sets;
        state.base.concept_maps = concept_maps;
        state.core = core_terminology()?;
        state.store = match carried.store {
            Some(store) => Some(store),
            None => match &config.terminology.resources {
                Some(path) => Some(Arc::new(ResourceStore::open(path).map_err(|source| {
                    LoadError::Resources {
                        path: path.clone(),
                        source: Box::new(source),
                    }
                })?)),
                None => None,
            },
        };
        state.reload_persisted().map_err(LoadError::Persisted)?;
        state.check_persisted_ids()?;
        Ok(state)
    }
    /// Names every loaded supplement by the `id` its resource carries, else by
    /// the id minted from its canonical.
    ///
    /// A supplement is a `CodeSystem` resource this server holds
    /// (<https://hl7.org/fhir/R4B/codesystem.html#supplements>), so it is read
    /// and searched like any other; it shares the `CodeSystem` id space with
    /// the systems the registry serves.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError::DuplicateId`] when the authored id is one another
    /// loaded `CodeSystem` already answers on.
    pub(super) fn register_supplements(
        &mut self,
        models: Vec<(String, CodeSystemModel)>,
    ) -> Result<(), LoadError> {
        let mut taken: BTreeMap<String, (String, Option<String>)> = self
            .instances
            .iter()
            .map(|(id, (url, version))| (id.clone(), (url.clone(), Some(version.clone()))))
            .collect();
        for (_, model) in models {
            let version = Some(model.version.clone()).filter(|version| !version.is_empty());
            let id = registered_id(
                ResourceType::CodeSystem,
                model.id.as_deref(),
                &model.url,
                version.as_deref(),
                &taken,
            )?;
            taken.insert(id.clone(), (model.url.clone(), version));
            self.supplements.insert(id, Arc::new(model));
        }
        Ok(())
    }
    /// Names every loaded value set and concept map: each by the `id` its
    /// resource carries, else by the id minted from its canonical.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError::DuplicateId`] when two resources of one type carry
    /// the same id.
    pub(super) fn register_instances(
        &mut self,
        value_sets: &ValueSetStore,
        concept_maps: &ConceptMapStore,
    ) -> Result<(), LoadError> {
        for model in value_sets.iter() {
            let id = registered_id(
                ResourceType::ValueSet,
                model.id.as_deref(),
                &model.url,
                model.version.as_deref(),
                &self.value_set_instances,
            )?;
            self.value_set_instances
                .insert(id, (model.url.clone(), model.version.clone()));
        }
        for model in concept_maps.iter() {
            let id = registered_id(
                ResourceType::ConceptMap,
                model.id.as_deref(),
                &model.url,
                model.version.as_deref(),
                &self.concept_map_instances,
            )?;
            self.concept_map_instances
                .insert(id, (model.url.clone(), model.version.clone()));
        }
        Ok(())
    }
    /// No loaded resource is addressed by an id a persisted record already
    /// answers on.
    ///
    /// A persisted record is resolved before a loaded resource of the same id,
    /// so the loaded one would be unreachable; the load says so instead.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError::PersistedId`] naming the loaded canonical and the
    /// id it shares with the record.
    pub(super) fn check_persisted_ids(&self) -> Result<(), LoadError> {
        let persisted = self
            .persisted
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (resource_type, id, url, version) in self.loaded_canonicals() {
            if persisted
                .records
                .contains_key(&(resource_type, id.to_owned()))
            {
                return Err(LoadError::PersistedId {
                    resource_type: resource_type.name(),
                    id: id.to_owned(),
                    canonical: canonical(url, version),
                });
            }
        }
        Ok(())
    }
    /// Wraps an already-built registry (tests and embedders).
    ///
    /// The state holds no FHIR core terminology; add it with
    /// [`Self::with_core`], which [`Self::load`] does for a real deployment.
    #[must_use]
    pub fn from_registry(registry: Registry) -> Self {
        // NOTE: an authored id names the resource that carries it
        // (<https://hl7.org/fhir/R4B/resource.html#id>), so every one of them is taken
        // first and only a minted id ever yields the suffix.
        let mut instances = BTreeMap::new();
        let mut minting = Vec::new();
        for url in registry.systems() {
            for provider in registry.versions(url) {
                let identity = provider.identity();
                let served = (identity.url.clone(), identity.version.clone());
                match provider.code_system().and_then(|model| model.id.clone()) {
                    Some(authored) => {
                        instances.insert(unique_id(&instances, authored), served);
                    }
                    None => minting.push(served),
                }
            }
        }
        for (url, version) in minting {
            let id = unique_id(&instances, instance_id(&url, &version));
            instances.insert(id, (url, version));
        }
        let base = Layer {
            registry,
            value_sets: ValueSetStore::default(),
            concept_maps: ConceptMapStore::default(),
        };
        Self {
            persisted: RwLock::new(Persisted {
                records: BTreeMap::new(),
                layer: Arc::new(base.clone()),
                served: BTreeMap::new(),
            }),
            base,
            core: BTreeMap::new(),
            store: None,
            value_set_instances: BTreeMap::new(),
            concept_map_instances: BTreeMap::new(),
            caches: Arc::new(Caches::default()),
            instances,
            supplements: BTreeMap::new(),
            paths: BTreeMap::new(),
            origins: BTreeMap::new(),
            software_version: env!("CARGO_PKG_VERSION"),
            security_services: Vec::new(),
            base_url: None,
        }
    }
    /// This state holding the terminology every served FHIR version defines,
    /// under whatever the deployment loaded.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError::Core`] when an embedded bundle does not read, and
    /// [`LoadError::Persisted`] when the served layers do not rebuild.
    pub fn with_core(mut self) -> Result<Self, LoadError> {
        self.core = core_terminology()?;
        self.reload_persisted().map_err(LoadError::Persisted)?;
        Ok(self)
    }
}
