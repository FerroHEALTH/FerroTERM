//! The loaded server state: a registry of code system providers and the ids
//! their `CodeSystem` instances answer on.
//!
//! What an operation resolves is a [`Layer`]: the code systems, value sets,
//! and concept maps the deployment loaded from disk, with the client
//! resources persisted through the REST API on top. A write replaces the
//! layer, so a request in flight keeps the one it started with.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use fhir_terminology::artifact::{self, ArtifactError};
use fhir_terminology::classification::{self, ClassificationProvider};
use fhir_terminology::conceptmap;
use fhir_terminology::conceptmap::model::ConceptMapModel;
use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::fhir_codesystem::load::{FhirVersion, load_dir, package_version};
use fhir_terminology::fhir_codesystem::model::CodeSystemModel;
use fhir_terminology::fhir_codesystem::provider::{BuildError, FhirCodeSystem};
use fhir_terminology::fhir_core::CoreTerminology;
use fhir_terminology::icd11::{self, Icd11Provider};
use fhir_terminology::loinc::{self, LoincProvider};
use fhir_terminology::operations::Sources;
use fhir_terminology::provider::{CodeSystemProvider, ContentMode, ProviderError};
use fhir_terminology::registries::ucum::provider::UcumProvider;
use fhir_terminology::registries::{bcp13, bcp47, iso3166};
use fhir_terminology::registry::{RegisterError, Registry, Resolved};
use fhir_terminology::rxnorm::{self, RxNormProvider};
use fhir_terminology::snomed::{self, OpenError, SnomedProvider};
use fhir_terminology::supplement::Supplement;
use fhir_terminology::valueset;
use fhir_terminology::valueset::model::ValueSetModel;
use fhir_terminology::valueset::store::ValueSetStore;
use fhir_terminology::versioned::Duplicate;

use crate::config::Config;
use crate::persistence::{
    Closure, HistoryEntry, Method, Record, ResourceStore, ResourceType, StoreError,
};
use crate::scope::Caches;

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
    /// An artifact directory named by `FERROTERM_INDEX`.
    Index,
    /// A `CodeSystem` resource from a directory named by
    /// `FERROTERM_CODESYSTEMS`, a supplement included.
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

/// The code systems, value sets, and concept maps an operation resolves in.
///
/// One layer holds what the deployment loaded from disk and the persisted
/// client resources over it; a request scope layers its own `tx-resource`s on
/// a clone of the one it took at its start.
#[derive(Debug, Clone)]
pub struct Layer {
    registry: Registry,
    value_sets: ValueSetStore,
    concept_maps: ConceptMapStore,
}

impl Layer {
    /// An empty layer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry: Registry::new(),
            value_sets: ValueSetStore::new(),
            concept_maps: ConceptMapStore::new(),
        }
    }

    /// The layer these three stores form.
    #[must_use]
    pub const fn of(
        registry: Registry,
        value_sets: ValueSetStore,
        concept_maps: ConceptMapStore,
    ) -> Self {
        Self {
            registry,
            value_sets,
            concept_maps,
        }
    }

    /// The registry.
    #[must_use]
    pub const fn registry(&self) -> &Registry {
        &self.registry
    }

    /// The value sets.
    #[must_use]
    pub const fn value_sets(&self) -> &ValueSetStore {
        &self.value_sets
    }

    /// The concept maps.
    #[must_use]
    pub const fn concept_maps(&self) -> &ConceptMapStore {
        &self.concept_maps
    }

    /// The engine's view of this layer.
    #[must_use]
    pub const fn sources(&self) -> Sources<'_> {
        Sources {
            registry: &self.registry,
            value_sets: &self.value_sets,
            concept_maps: &self.concept_maps,
        }
    }

    /// Applies one persisted or request-scoped resource, replacing what it
    /// names.
    ///
    /// # Errors
    ///
    /// Returns [`LayerError`] when a `CodeSystem` cannot be served or a
    /// supplement names no code system.
    pub fn apply(&mut self, resource: &crate::scope::Loaded) -> Result<(), LayerError> {
        match resource {
            crate::scope::Loaded::CodeSystem(model) => {
                if model.content == ContentMode::Supplement {
                    let target = model.supplements.clone().ok_or_else(|| {
                        LayerError::SupplementWithoutTarget {
                            url: model.url.clone(),
                        }
                    })?;
                    // NOTE: a persisted supplement stays dormant like a loaded one until a
                    // request names it (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
                    self.registry
                        .register_supplement(target, supplement_of(model));
                    return Ok(());
                }
                let provider =
                    FhirCodeSystem::new(model.clone()).map_err(|source| LayerError::Build {
                        url: model.url.clone(),
                        source,
                    })?;
                self.registry.register_or_replace(Arc::new(provider));
            }
            crate::scope::Loaded::ValueSet(model) => self.value_sets.replace(model.clone()),
            crate::scope::Loaded::ConceptMap(model) => self.concept_maps.replace(model.clone()),
            // NOTE: a resource the server cannot use answers the request that
            // resolves it (`crate::scope::Unusable`); a layer holds nothing for it.
            crate::scope::Loaded::Unusable(_) => {}
        }
        Ok(())
    }
}

impl Default for Layer {
    fn default() -> Self {
        Self::new()
    }
}

/// A resource that cannot be layered over the loaded state.
#[derive(Debug, thiserror::Error)]
pub enum LayerError {
    /// A `CodeSystem` the engine cannot serve.
    #[error("the CodeSystem `{url}` cannot be served")]
    Build {
        /// The system.
        url: String,
        /// The cause.
        #[source]
        source: BuildError,
    },
    /// A supplement names no `supplements` canonical.
    #[error("the supplement `{url}` names no code system to supplement")]
    SupplementWithoutTarget {
        /// The supplement.
        url: String,
    },
}

/// The terminology each served version defines, by the FHIR version its
/// surface reports.
///
/// A code system the FHIR specification defines belongs to the version that
/// defines it (<https://hl7.org/fhir/R4B/terminologies-systems.html>), so
/// each surface answers over its own package's content.
const CORE_VERSIONS: [(&str, FhirVersion); 4] = [
    (crate::r4::metadata::FHIR_VERSION, FhirVersion::R4),
    (crate::r4b::metadata::FHIR_VERSION, FhirVersion::R4B),
    (crate::r5::metadata::FHIR_VERSION, FhirVersion::R5),
    (crate::r6::metadata::FHIR_VERSION, FhirVersion::R6),
];

/// The FHIR core terminology of every served version, read once per process.
///
/// The bundles are embedded and generated, so the result is the same on every
/// call; the cache keeps a second server in one process from reading them
/// again.
fn core_terminology() -> Result<BTreeMap<&'static str, CoreTerminology>, LoadError> {
    static CACHE: std::sync::OnceLock<BTreeMap<&'static str, CoreTerminology>> =
        std::sync::OnceLock::new();
    if let Some(cached) = CACHE.get() {
        return Ok(cached.clone());
    }
    let mut built = BTreeMap::new();
    for (fhir_version, bundle) in CORE_VERSIONS {
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
    /// The metrics a scrape reads, seeded with what this state loaded.
    metrics: Arc<crate::metrics::Metrics>,
}

/// The persisted client resources and the layer they form over the loaded
/// state, kept together so a reader sees a record and the layer that holds it.
#[derive(Debug)]
struct Persisted {
    /// Every current record, keyed `<type>/<id>`.
    records: BTreeMap<(ResourceType, String), Record>,
    /// The loaded state with `records` applied.
    layer: Arc<Layer>,
    /// The same layer per served version, each over the terminology that
    /// version defines.
    served: BTreeMap<&'static str, Arc<Layer>>,
}

/// A failure to persist a client resource.
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// The deployment configured no resource database.
    #[error(
        "this server persists no resources: set {}",
        crate::config::RESOURCES_ENV
    )]
    NotConfigured,
    /// The store cannot be read or written.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The resource does not layer over the loaded state.
    #[error(transparent)]
    Layer(#[from] LayerError),
    /// A stored resource does not decode as a resource of its FHIR version:
    /// a shape the version does not define, or a primitive outside its
    /// lexical form.
    #[error("the persisted {resource_type}/{id} does not decode: {source}")]
    Decode {
        /// The resource type.
        resource_type: String,
        /// The logical id.
        id: String,
        /// The codec's refusal, with the element path.
        #[source]
        source: fhir_types::codec::DecodeError,
    },
    /// Another resource of the type already carries the written resource's
    /// `url` and `version`.
    #[error("the {resource_type} `{canonical}` is already held by {resource_type}/{holder}")]
    Duplicate {
        /// The resource type.
        resource_type: &'static str,
        /// The `url|version` both resources carry.
        canonical: String,
        /// The logical id of the resource that holds it.
        holder: String,
    },
    /// The FHIR core terminology of a served version already carries the
    /// written resource's `url` and `version`.
    #[error(
        "the {resource_type} `{canonical}` is already held by the FHIR {fhir_version} core terminology"
    )]
    CoreDuplicate {
        /// The resource type.
        resource_type: &'static str,
        /// The `url|version` both resources carry.
        canonical: String,
        /// The FHIR version whose core terminology holds it.
        fhir_version: &'static str,
    },
    /// A stored resource does not convert into a model this server serves.
    #[error("the persisted {resource_type}/{id} does not convert: {reason}")]
    Convert {
        /// The resource type.
        resource_type: String,
        /// The logical id.
        id: String,
        /// What the conversion refused.
        reason: String,
    },
}

/// A provider before registration, with where it came from.
struct Loaded {
    path: PathBuf,
    provider: Arc<dyn CodeSystemProvider>,
}

/// What a rebuilt state takes over from the state it replaces.
///
/// The write store, the metrics, and the `$cache-control` caches outlive one
/// served set: reopening the store would meet `redb`'s own file lock, and a
/// counter or a cache handle that restarted with the set would lie to a
/// scrape and to a client holding an id.
#[derive(Debug, Default)]
struct Carried {
    /// The live write store, when the state being replaced opened one.
    store: Option<Arc<ResourceStore>>,
    /// The metrics registry a scrape reads.
    metrics: Option<Arc<crate::metrics::Metrics>>,
    /// The caches `$cache-control` started.
    caches: Option<Arc<Caches>>,
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
    /// state's write store, metrics, and caches.
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
                metrics: Some(Arc::clone(&self.metrics)),
                caches: Some(Arc::clone(&self.caches)),
            },
        )
    }

    /// The state `config` names, built over what `carried` hands on.
    fn build(config: &Config, carried: Carried) -> Result<Self, LoadError> {
        let mut loaded = Vec::new();
        for path in artifact_paths(&config.index)? {
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
        let mut value_sets = ValueSetStore::new();
        let mut concept_maps = ConceptMapStore::new();
        for path in &config.code_systems {
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
        let mut registry = Registry::new();
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
        if let Some(metrics) = carried.metrics {
            state.metrics = metrics;
        }
        if let Some(caches) = carried.caches {
            state.caches = caches;
        }
        state.paths = paths;
        state.origins = origins;
        state
            .security_services
            .clone_from(&config.security_services);
        state.base_url.clone_from(&config.base_url);
        state.register_supplements(supplements)?;
        state.register_instances(&value_sets, &concept_maps)?;
        state.base.value_sets = value_sets;
        state.base.concept_maps = concept_maps;
        state.core = core_terminology()?;
        state.store = match carried.store {
            Some(store) => Some(store),
            None => match &config.resources {
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
        state.seed_metrics();
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
    fn register_supplements(
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
    fn register_instances(
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
    fn check_persisted_ids(&self) -> Result<(), LoadError> {
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

    /// Reads every persisted record and rebuilds the served layer from them.
    fn reload_persisted(&mut self) -> Result<(), PersistError> {
        let mut records = BTreeMap::new();
        if let Some(store) = &self.store {
            for record in store.all()? {
                let Some(resource_type) = ResourceType::parse(&record.resource_type) else {
                    continue;
                };
                records.insert((resource_type, record.id.clone()), record);
            }
        }
        for duplicate in duplicate_canonicals(&records, &self.loaded_canonicals()) {
            tracing::warn!(
                resource_type = duplicate.resource_type.name(),
                canonical = %duplicate.canonical,
                ids = ?duplicate.ids,
                answering = %duplicate.answering,
                "several resources carry one canonical; the most recent write answers for it"
            );
        }
        for (fhir_version, core) in &self.core {
            for record in records.values().filter(|record| shadows_core(core, record)) {
                tracing::warn!(
                    resource_type = %record.resource_type,
                    id = %record.id,
                    canonical = %canonical(
                        record.url.as_deref().unwrap_or_default(),
                        record.version.as_deref(),
                    ),
                    fhir_version,
                    "a persisted resource carries the canonical of a FHIR core resource; it is not served, and the core terminology answers for its url"
                );
            }
        }
        let Layers { layer, served } = persisted_layers(&self.base, &self.core, &records)?;
        *self
            .persisted
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Persisted {
            records,
            layer,
            served,
        };
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
            value_sets: ValueSetStore::new(),
            concept_maps: ConceptMapStore::new(),
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
            metrics: Arc::new(crate::metrics::Metrics::new()),
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

    /// Declares every loaded code system version to the metrics registry,
    /// dropping the versions a previous set declared.
    fn seed_metrics(&self) {
        self.metrics
            .serving(self.instances().map(|(_, url, version)| (url, version)));
    }

    /// The metrics of this server, for the scrape endpoint and the request
    /// middleware.
    #[must_use]
    pub fn metrics(&self) -> &crate::metrics::Metrics {
        &self.metrics
    }

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

    /// The canonical of the resource the deployment loaded under `id`, when it
    /// loaded one of `resource_type`.
    ///
    /// A loaded resource is read at that id
    /// (<https://hl7.org/fhir/R4B/resource.html#id>), so a write the client
    /// aims at the same id is refused rather than layered over it.
    #[must_use]
    pub fn loaded_canonical(&self, resource_type: ResourceType, id: &str) -> Option<String> {
        match resource_type {
            ResourceType::CodeSystem => self
                .instances
                .get(id)
                .map(|(url, version)| canonical(url, Some(version)))
                .or_else(|| {
                    self.supplements.get(id).map(|model| {
                        canonical(
                            &model.url,
                            Some(model.version.as_str()).filter(|v| !v.is_empty()),
                        )
                    })
                }),
            ResourceType::ValueSet => self
                .value_set_instances
                .get(id)
                .map(|(url, version)| canonical(url, version.as_deref())),
            ResourceType::ConceptMap => self
                .concept_map_instances
                .get(id)
                .map(|(url, version)| canonical(url, version.as_deref())),
        }
    }

    /// Every resource the deployment loaded, as its type, id, `url`, and
    /// `version`.
    fn loaded_canonicals(&self) -> Vec<(ResourceType, &str, &str, Option<&str>)> {
        self.instances
            .iter()
            .map(|(id, (url, version))| {
                (
                    ResourceType::CodeSystem,
                    id.as_str(),
                    url.as_str(),
                    Some(version.as_str()),
                )
            })
            .chain(self.supplements.iter().map(|(id, model)| {
                (
                    ResourceType::CodeSystem,
                    id.as_str(),
                    model.url.as_str(),
                    Some(model.version.as_str()),
                )
            }))
            .chain(self.value_set_instances.iter().map(|(id, (url, version))| {
                (
                    ResourceType::ValueSet,
                    id.as_str(),
                    url.as_str(),
                    version.as_deref(),
                )
            }))
            .chain(
                self.concept_map_instances
                    .iter()
                    .map(|(id, (url, version))| {
                        (
                            ResourceType::ConceptMap,
                            id.as_str(),
                            url.as_str(),
                            version.as_deref(),
                        )
                    }),
            )
            .collect()
    }

    /// The id of the resource of `resource_type` the deployment loaded with
    /// `url` and `version`, other than `id`, when there is one.
    fn loaded_holder(
        &self,
        resource_type: ResourceType,
        id: &str,
        url: &str,
        version: Option<&str>,
    ) -> Option<String> {
        self.loaded_canonicals()
            .into_iter()
            .find(|(held_type, held, held_url, held_version)| {
                *held_type == resource_type
                    && *held != id
                    && same_canonical((held_url, *held_version), (url, version))
            })
            .map(|(_, held, _, _)| held.to_owned())
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

    /// Whether this deployment persists client resources.
    #[must_use]
    pub fn persists(&self) -> bool {
        self.store.is_some()
    }

    /// The persisted record of `resource_type` with `id`.
    #[must_use]
    pub fn persisted_record(&self, resource_type: ResourceType, id: &str) -> Option<Record> {
        self.persisted
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .records
            .get(&(resource_type, id.to_owned()))
            .cloned()
    }

    /// Every persisted record of `resource_type`, sorted by id.
    #[must_use]
    pub fn persisted_records(&self, resource_type: ResourceType) -> Vec<Record> {
        self.persisted
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .records
            .iter()
            .filter(|((held, _), _)| *held == resource_type)
            .map(|(_, record)| record.clone())
            .collect()
    }

    /// The persisted record of `resource_type` with `id` as of `version_id`.
    ///
    /// # Errors
    ///
    /// Returns [`PersistError::NotConfigured`] when the deployment persists no
    /// resources, and [`PersistError::Store`] when the store cannot be read.
    pub fn persisted_version(
        &self,
        resource_type: ResourceType,
        id: &str,
        version_id: u32,
    ) -> Result<Option<Record>, PersistError> {
        let store = self.store.as_ref().ok_or(PersistError::NotConfigured)?;
        Ok(store.version(resource_type, id, version_id)?)
    }

    /// Every version of the persisted resources of `resource_type`, or of the
    /// one with `id` when one is given, deletes included; empty when the
    /// deployment persists nothing.
    ///
    /// # Errors
    ///
    /// Returns [`PersistError::Store`] when the store cannot be read.
    pub fn persisted_history(
        &self,
        resource_type: ResourceType,
        id: Option<&str>,
    ) -> Result<Vec<HistoryEntry>, PersistError> {
        let Some(store) = self.store.as_ref() else {
            return Ok(Vec::new());
        };
        Ok(store.history(resource_type, id)?)
    }

    /// Whether version `version_id` of the persisted `resource_type` with `id`
    /// is a delete.
    ///
    /// # Errors
    ///
    /// Returns [`PersistError::NotConfigured`] when the deployment persists no
    /// resources, and [`PersistError::Store`] when the store cannot be read.
    pub fn persisted_is_delete(
        &self,
        resource_type: ResourceType,
        id: &str,
        version_id: u32,
    ) -> Result<bool, PersistError> {
        let store = self.store.as_ref().ok_or(PersistError::NotConfigured)?;
        Ok(store.is_delete(resource_type, id, version_id)?)
    }

    /// The closure table named `name`.
    ///
    /// # Errors
    ///
    /// Returns [`PersistError::NotConfigured`] when the deployment persists no
    /// resources, and [`PersistError::Store`] when the store cannot be read.
    pub fn closure(&self, name: &str) -> Result<Option<Closure>, PersistError> {
        let store = self.store.as_ref().ok_or(PersistError::NotConfigured)?;
        Ok(store.closure(name)?)
    }

    /// Writes the closure table `closure`.
    ///
    /// # Errors
    ///
    /// Returns [`PersistError::NotConfigured`] when the deployment persists no
    /// resources, and [`PersistError::Store`] when the write does not commit.
    pub fn put_closure(&self, closure: &Closure) -> Result<(), PersistError> {
        let store = self.store.as_ref().ok_or(PersistError::NotConfigured)?;
        Ok(store.put_closure(closure)?)
    }

    /// Persists `resource` as `resource_type` with `id`, raising
    /// `meta.versionId` and replacing the served layer.
    ///
    /// `fhir_version` is the version the resource arrived in, so a later read
    /// converts it exactly as the loader converts a resource from disk, and
    /// `method` is the interaction that wrote it, which its history states.
    /// The version counts on from the last one the store holds, a delete
    /// included, so a resource written again after a delete keeps its history.
    ///
    /// # Errors
    ///
    /// Returns [`PersistError::NotConfigured`] when the deployment persists no
    /// resources, [`PersistError::Duplicate`] when another resource of the type,
    /// loaded or persisted, carries its `url` and `version`,
    /// [`PersistError::CoreDuplicate`] when the FHIR core terminology of a
    /// served version does,
    /// [`PersistError::Convert`] when the resource does not convert
    /// into a model this server serves, [`PersistError::Layer`] when it cannot
    /// be layered over the loaded state, and [`PersistError::Store`] when the
    /// write does not commit.
    pub fn put_persisted(
        &self,
        resource_type: ResourceType,
        id: &str,
        fhir_version: &str,
        method: Method,
        mut resource: fhir_types::codec::Object,
    ) -> Result<Record, PersistError> {
        let store = self.store.as_ref().ok_or(PersistError::NotConfigured)?;
        let mut persisted = self
            .persisted
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(url) = text_of(&resource, "url") {
            let version = text_of(&resource, "version");
            let holder = persisted_holder(
                &persisted.records,
                resource_type,
                id,
                &url,
                version.as_deref(),
            )
            .or_else(|| self.loaded_holder(resource_type, id, &url, version.as_deref()));
            if let Some(holder) = holder {
                return Err(PersistError::Duplicate {
                    resource_type: resource_type.name(),
                    canonical: canonical(&url, version.as_deref()),
                    holder,
                });
            }
            if let Some((fhir_version, _)) = self
                .core
                .iter()
                .find(|(_, core)| core_holds(core, resource_type, &url, version.as_deref()))
            {
                return Err(PersistError::CoreDuplicate {
                    resource_type: resource_type.name(),
                    canonical: canonical(&url, version.as_deref()),
                    fhir_version,
                });
            }
        }
        let key = (resource_type, id.to_owned());
        let version_id = store
            .latest_version(resource_type, id)?
            .map_or(1, |latest| latest.saturating_add(1));
        let last_modified = fhir_terminology::clock::now().to_string();
        stamp(&mut resource, version_id, &last_modified);
        let record = Record {
            resource_type: resource_type.name().to_owned(),
            id: id.to_owned(),
            url: text_of(&resource, "url"),
            version: text_of(&resource, "version"),
            fhir_version: fhir_version.to_owned(),
            version_id,
            last_modified,
            resource,
        };
        let mut records = persisted.records.clone();
        records.insert(key, record.clone());
        let Layers { layer, served } = persisted_layers(&self.base, &self.core, &records)?;
        store.put_by(&record, method)?;
        *persisted = Persisted {
            records,
            layer,
            served,
        };
        Ok(record)
    }

    /// Removes the persisted resource of `resource_type` with `id` and
    /// replaces the served layer; `false` when there was none.
    ///
    /// # Errors
    ///
    /// Returns [`PersistError::NotConfigured`] when the deployment persists no
    /// resources, and [`PersistError::Store`] when the delete does not commit.
    pub fn delete_persisted(
        &self,
        resource_type: ResourceType,
        id: &str,
    ) -> Result<bool, PersistError> {
        let store = self.store.as_ref().ok_or(PersistError::NotConfigured)?;
        let mut persisted = self
            .persisted
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut records = persisted.records.clone();
        if records.remove(&(resource_type, id.to_owned())).is_none() {
            return Ok(false);
        }
        let Layers { layer, served } = persisted_layers(&self.base, &self.core, &records)?;
        store.delete(resource_type, id)?;
        *persisted = Persisted {
            records,
            layer,
            served,
        };
        Ok(true)
    }
}

/// What one rebuild of the served state produced.
struct Layers {
    /// The deployment's own layer with every persisted record over it.
    layer: Arc<Layer>,
    /// That layer per served version, each over the terminology that version
    /// defines.
    served: BTreeMap<&'static str, Arc<Layer>>,
}

/// The loaded state with every persisted record applied over it, and the same
/// layer per served version over the terminology that version defines.
///
/// A record that carries the canonical of a core resource of any served
/// version is left out of both ([`shadows_core`]).
fn persisted_layers(
    base: &Layer,
    core: &BTreeMap<&'static str, CoreTerminology>,
    records: &BTreeMap<(ResourceType, String), Record>,
) -> Result<Layers, PersistError> {
    let over_core: BTreeMap<(ResourceType, String), Record> = records
        .iter()
        .filter(|(_, record)| !core.values().any(|core| shadows_core(core, record)))
        .map(|(key, record)| (key.clone(), record.clone()))
        .collect();
    let layer = layered(base, &over_core)?;
    let served = core
        .iter()
        .map(|(fhir_version, core)| {
            (
                *fhir_version,
                Arc::new(Layer::of(
                    layer.registry.clone().with_beneath(core.code_systems()),
                    layer.value_sets.clone().with_beneath(core.value_sets()),
                    layer.concept_maps.clone(),
                )),
            )
        })
        .collect();
    Ok(Layers { layer, served })
}

/// Whether `record` carries the `url` and `version` of a resource in `core`.
///
/// Such a record is kept out of every served layer, so the core resource of
/// each version keeps answering for its `url`: the pair identifies the core
/// resource (<https://hl7.org/fhir/R4B/resource.html#canonical>), and a store
/// written before the server refused such a write may hold one.
fn shadows_core(core: &CoreTerminology, record: &Record) -> bool {
    let (Some(resource_type), Some(url)) = (
        ResourceType::parse(&record.resource_type),
        record.url.as_deref(),
    ) else {
        return false;
    };
    core_holds(core, resource_type, url, record.version.as_deref())
}

/// Whether the terminology `core` carries a resource of `resource_type` with
/// `url` and `version`.
fn core_holds(
    core: &CoreTerminology,
    resource_type: ResourceType,
    url: &str,
    version: Option<&str>,
) -> bool {
    match resource_type {
        ResourceType::CodeSystem => core.code_systems().versions(url).any(|provider| {
            same_canonical(
                (url, Some(provider.identity().version.as_str())),
                (url, version),
            )
        }),
        ResourceType::ValueSet => core
            .value_sets()
            .iter()
            .any(|model| same_canonical((&model.url, model.version.as_deref()), (url, version))),
        // NOTE: the core terminology beneath a served version holds code systems and
        // value sets alone; no FHIR/SNOMED spec governs this: our own design.
        ResourceType::ConceptMap => false,
    }
}

/// The id of the persisted record of `resource_type` with `url` and
/// `version`, other than `id`, when there is one.
fn persisted_holder(
    records: &BTreeMap<(ResourceType, String), Record>,
    resource_type: ResourceType,
    id: &str,
    url: &str,
    version: Option<&str>,
) -> Option<String> {
    records
        .iter()
        .find(|((held_type, held), record)| {
            *held_type == resource_type
                && held != id
                && record.url.as_deref().is_some_and(|held_url| {
                    same_canonical((held_url, record.version.as_deref()), (url, version))
                })
        })
        .map(|((_, held), _)| held.clone())
}

/// Whether two `url` and `version` pairs name one canonical resource, an empty
/// version counting as none
/// (<https://hl7.org/fhir/R4B/references.html#canonical>).
fn same_canonical(left: (&str, Option<&str>), right: (&str, Option<&str>)) -> bool {
    left.0 == right.0 && present_version(left.1) == present_version(right.1)
}

/// `version`, when it is present and not empty.
fn present_version(version: Option<&str>) -> Option<&str> {
    version.filter(|version| !version.is_empty())
}

/// The persisted records in the order they were written, oldest first, so
/// that of two records carrying one canonical the later write is layered last
/// and answers.
///
/// No FHIR/SNOMED spec governs this: our own design. A store written before
/// the server refused a duplicate canonical may hold one, and the most recent
/// write is the one its writer last meant.
fn in_write_order(records: &BTreeMap<(ResourceType, String), Record>) -> Vec<&Record> {
    let mut ordered: Vec<(Option<jiff::Timestamp>, &Record)> = records
        .values()
        // NOTE: an unreadable `lastUpdated` sorts first, as the oldest write, so the
        // record is still layered; no FHIR/SNOMED spec governs this: our own design.
        .map(|record| (record.last_modified.parse().ok(), record))
        .collect();
    ordered.sort_by(|(left_time, left), (right_time, right)| {
        left_time
            .cmp(right_time)
            .then_with(|| left.version_id.cmp(&right.version_id))
            .then_with(|| left.id.cmp(&right.id))
    });
    ordered.into_iter().map(|(_, record)| record).collect()
}

/// One canonical that more than one resource of a type carries, with every id
/// that carries it and the one that answers for it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DuplicateCanonical {
    /// The resource type.
    resource_type: ResourceType,
    /// The `url|version` the resources share.
    canonical: String,
    /// Every id that carries it, the loaded ones first, then the persisted
    /// ones oldest write first.
    ids: Vec<String>,
    /// The id whose resource answers: the persisted record written last,
    /// which the served layer applies over every other.
    answering: String,
}

/// Every canonical that a persisted record shares with another persisted
/// record or with a loaded resource of its type.
fn duplicate_canonicals(
    records: &BTreeMap<(ResourceType, String), Record>,
    loaded: &[(ResourceType, &str, &str, Option<&str>)],
) -> Vec<DuplicateCanonical> {
    let mut held: BTreeMap<(ResourceType, String), Vec<String>> = BTreeMap::new();
    for (resource_type, id, url, version) in loaded {
        held.entry((*resource_type, canonical(url, *version)))
            .or_default()
            .push((*id).to_owned());
    }
    let mut written: BTreeMap<(ResourceType, String), Vec<String>> = BTreeMap::new();
    for record in in_write_order(records) {
        let (Some(resource_type), Some(url)) =
            (ResourceType::parse(&record.resource_type), &record.url)
        else {
            continue;
        };
        written
            .entry((resource_type, canonical(url, record.version.as_deref())))
            .or_default()
            .push(record.id.clone());
    }
    written
        .into_iter()
        .filter_map(|((resource_type, canonical), persisted)| {
            let answering = persisted.last()?.clone();
            let mut ids = held
                .remove(&(resource_type, canonical.clone()))
                .unwrap_or_default();
            ids.extend(persisted);
            (ids.len() > 1).then_some(DuplicateCanonical {
                resource_type,
                canonical,
                ids,
                answering,
            })
        })
        .collect()
}

/// The loaded state with every persisted record applied over it.
fn layered(
    base: &Layer,
    records: &BTreeMap<(ResourceType, String), Record>,
) -> Result<Arc<Layer>, PersistError> {
    if records.is_empty() {
        return Ok(Arc::new(base.clone()));
    }
    let mut layer = base.clone();
    for record in in_write_order(records) {
        let (resource_type, id) = (&record.resource_type, &record.id);
        let resource =
            crate::version::loaded_of(&record.fhir_version, &record.resource).map_err(|error| {
                match error {
                    crate::version::ReadError::Decode(source) => PersistError::Decode {
                        resource_type: resource_type.clone(),
                        id: id.clone(),
                        source,
                    },
                    crate::version::ReadError::Convert(reason) => PersistError::Convert {
                        resource_type: resource_type.clone(),
                        id: id.clone(),
                        reason,
                    },
                }
            })?;
        layer.apply(&resource)?;
    }
    Ok(Arc::new(layer))
}

/// Writes `meta.versionId` and `meta.lastUpdated` into `resource`, keeping
/// whatever else its `meta` carries
/// (<https://hl7.org/fhir/R4B/resource.html#Meta>).
fn stamp(resource: &mut fhir_types::codec::Object, version_id: u32, last_modified: &str) {
    let mut meta = match resource.remove("meta") {
        Some(fhir_types::codec::Value::Object(held)) => held,
        _ => fhir_types::codec::Object::new(),
    };
    meta.insert(
        String::from("versionId"),
        fhir_types::codec::Value::String(version_id.to_string()),
    );
    meta.insert(
        String::from("lastUpdated"),
        fhir_types::codec::Value::String(last_modified.to_owned()),
    );
    resource.insert(String::from("meta"), fhir_types::codec::Value::Object(meta));
}

/// The string value of `field`, when the object carries one.
fn text_of(object: &fhir_types::codec::Object, field: &str) -> Option<String> {
    object.get(field)?.as_str().map(str::to_owned)
}

/// Loads the `CodeSystem`, `ValueSet`, and `ConceptMap` resources in `path`:
/// complete systems become providers, supplements are collected for
/// dormant registration, value sets and concept maps go to their stores.
///
/// The FHIR version is the one the directory's `package.json` declares;
/// a plain directory of resources is read as R4B, the version the server
/// serves.
fn load_code_systems(
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
fn check_supplement_targets(
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

/// `url|version` split into its parts.
fn split_canonical(canonical: &str) -> (&str, Option<&str>) {
    match canonical.split_once('|') {
        Some((url, version)) => (url, Some(version)),
        None => (canonical, None),
    }
}

/// `wanted`, or `wanted` with a numeric suffix when the reduced id is taken.
fn unique_id(taken: &BTreeMap<String, (String, String)>, wanted: String) -> String {
    unique_id_of(taken, wanted)
}

/// The `url|version` canonical of a resource, for a diagnostic.
fn canonical(url: &str, version: Option<&str>) -> String {
    match version.filter(|version| !version.is_empty()) {
        Some(version) => format!("{url}|{version}"),
        None => url.to_owned(),
    }
}

/// Every loaded resource of one type carries a distinct authored id.
///
/// # Errors
///
/// Returns [`LoadError::DuplicateId`] naming both canonicals. A server does not
/// rename a resource it loads (<https://hl7.org/fhir/R4B/resource.html#id>), so
/// the load fails instead of picking one.
fn check_distinct_ids(
    resource_type: ResourceType,
    authored: impl Iterator<Item = (String, String)>,
) -> Result<(), LoadError> {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for (id, canonical) in authored {
        if let Some(first) = seen.get(&id) {
            return Err(LoadError::DuplicateId {
                resource_type: resource_type.name(),
                id,
                first: first.clone(),
                second: canonical,
            });
        }
        seen.insert(id, canonical);
    }
    Ok(())
}

/// The id a loaded value set or concept map is registered under.
///
/// The id the resource was authored with, so a client reads it at the id it
/// knows (<https://hl7.org/fhir/R4B/resource.html#id>); a resource without one
/// keeps the id the server mints from its canonical, which no specification
/// governs: our own design.
///
/// # Errors
///
/// Returns [`LoadError::DuplicateId`] when another loaded resource of the type
/// already answers on the authored id.
fn registered_id(
    resource_type: ResourceType,
    authored: Option<&str>,
    url: &str,
    version: Option<&str>,
    taken: &BTreeMap<String, (String, Option<String>)>,
) -> Result<String, LoadError> {
    let Some(authored) = authored else {
        return Ok(unique_id_of(
            taken,
            instance_id(url, version.unwrap_or_default()),
        ));
    };
    if let Some((held_url, held_version)) = taken.get(authored) {
        return Err(LoadError::DuplicateId {
            resource_type: resource_type.name(),
            id: authored.to_owned(),
            first: canonical(held_url, held_version.as_deref()),
            second: canonical(url, version),
        });
    }
    Ok(authored.to_owned())
}

/// `wanted`, or `wanted` with a numeric suffix when the reduced id is a key
/// of `taken`.
fn unique_id_of<V>(taken: &BTreeMap<String, V>, wanted: String) -> String {
    if !taken.contains_key(&wanted) {
        return wanted;
    }
    let stem: String = wanted.chars().take(60).collect();
    (2..=taken.len().saturating_add(2))
        .map(|n| format!("{stem}-{n}"))
        .find(|candidate| !taken.contains_key(candidate))
        .unwrap_or(wanted)
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
fn artifact_paths(index: &[PathBuf]) -> Result<Vec<PathBuf>, LoadError> {
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
fn artifacts_under(root: &Path) -> Result<Vec<PathBuf>, LoadError> {
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
fn open_artifact(path: &Path, config: &Config) -> Result<Arc<dyn CodeSystemProvider>, LoadError> {
    let described = artifact::describe(path).map_err(|source| LoadError::Artifact {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(match described.system.as_str() {
        snomed::SYSTEM => Arc::new(
            SnomedProvider::open(path, &config.default_language).map_err(|source| {
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

#[cfg(test)]
mod tests {
    use super::{instance_id, split_canonical};

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

#[cfg(test)]
mod duplicate_tests {
    use std::collections::BTreeMap;

    use super::{DuplicateCanonical, duplicate_canonicals, in_write_order, same_canonical};
    use crate::persistence::{Record, ResourceType};

    fn record(id: &str, url: &str, version: Option<&str>, written: &str) -> Record {
        Record {
            resource_type: String::from("ValueSet"),
            id: id.to_owned(),
            url: Some(url.to_owned()),
            version: version.map(str::to_owned),
            fhir_version: String::from("4.3.0"),
            version_id: 1,
            last_modified: written.to_owned(),
            resource: fhir_types::codec::Object::new(),
        }
    }

    fn records(held: Vec<Record>) -> BTreeMap<(ResourceType, String), Record> {
        held.into_iter()
            .map(|record| ((ResourceType::ValueSet, record.id.clone()), record))
            .collect()
    }

    #[test]
    fn an_empty_version_is_no_version() {
        assert!(same_canonical(("u", Some("")), ("u", None)));
        assert!(!same_canonical(("u", Some("1")), ("u", None)));
        assert!(!same_canonical(("u", Some("1")), ("v", Some("1"))));
    }

    #[test]
    fn records_layer_in_the_order_they_were_written() {
        // The id that sorts last was written first, and the fractional second
        // sorts after the whole one although its text sorts before it.
        let held = records(vec![
            record(
                "zz",
                "https://a.example/vs",
                Some("1"),
                "2026-01-01T00:00:00Z",
            ),
            record(
                "aa",
                "https://a.example/vs",
                Some("1"),
                "2026-01-01T00:00:00.5Z",
            ),
        ]);
        let order: Vec<&str> = in_write_order(&held)
            .into_iter()
            .map(|record| record.id.as_str())
            .collect();
        assert_eq!(order, ["zz", "aa"]);
    }

    #[test]
    fn a_shared_canonical_names_every_holder_and_the_latest_write() {
        let held = records(vec![
            record(
                "aa",
                "https://a.example/vs",
                Some("1"),
                "2026-02-01T00:00:00Z",
            ),
            record(
                "zz",
                "https://a.example/vs",
                Some("1"),
                "2026-01-01T00:00:00Z",
            ),
            record(
                "other",
                "https://a.example/vs",
                Some("2"),
                "2026-01-01T00:00:00Z",
            ),
        ]);
        assert_eq!(
            duplicate_canonicals(&held, &[]),
            [DuplicateCanonical {
                resource_type: ResourceType::ValueSet,
                canonical: String::from("https://a.example/vs|1"),
                ids: vec![String::from("zz"), String::from("aa")],
                answering: String::from("aa"),
            }]
        );
    }

    #[test]
    fn a_persisted_record_over_a_loaded_canonical_is_named() {
        let held = records(vec![record(
            "written",
            "https://a.example/vs",
            Some("1"),
            "2026-01-01T00:00:00Z",
        )]);
        let loaded = [(
            ResourceType::ValueSet,
            "loaded",
            "https://a.example/vs",
            Some("1"),
        )];
        let found = duplicate_canonicals(&held, &loaded);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].ids, ["loaded", "written"]);
        assert_eq!(found[0].answering, "written");
        assert!(
            duplicate_canonicals(&held, &[]).is_empty(),
            "one record alone is no duplicate"
        );
    }
}
