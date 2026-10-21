// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Request-scoped resources: `tx-resource` parameters and `$cache-control`.
//!
//! The terminology ecosystem's runner never uploads its setup resources: it
//! sends them as `tx-resource` parameters on every operation, or front-loads
//! them once with `$cache-control?mode=start` and names the returned cache
//! with the `X-Cache-Id` header
//! (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
//! A request's resources form a [`Scope`]: the loaded registry and value set
//! store with those resources layered on top, for that request only. Each
//! served version converts its own generated resources to the models held
//! here, so a cache started on one version serves every version.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use fhir_terminology::conceptmap::model::ConceptMapModel;
use fhir_terminology::fhir_codesystem::model::CodeSystemModel;
use fhir_terminology::fhir_codesystem::provider::FhirCodeSystem;
use fhir_terminology::operations::{OperationError, Sources};
use fhir_terminology::provider::ContentMode;
use fhir_terminology::registry::Registry;
use fhir_terminology::valueset::model::ValueSetModel;
use fhir_terminology::valueset::store::ValueSetStore;
use http::{HeaderMap, StatusCode};

use crate::outcome::Failure;
use crate::state::{AppState, Layer, supplement_of};

/// The parameter that carries a request-scoped resource.
pub const TX_RESOURCE: &str = "tx-resource";
/// The ecosystem runner's profile parameter naming the expansion identifier
/// to use; not declared by any version, peeled off with the resources.
pub const UUID: &str = "uuid";
/// The header that names a cache started with `$cache-control`.
pub const CACHE_ID_HEADER: &str = "X-Cache-Id";
/// How long an unused cache lives. No spec fixes this: our own design.
pub const CACHE_IDLE: Duration = Duration::from_mins(30);

/// A request-scoped resource as the model the engine serves.
#[derive(Debug, Clone)]
pub enum Loaded {
    /// A `CodeSystem`, complete or a supplement.
    CodeSystem(CodeSystemModel),
    /// A `ValueSet`.
    ValueSet(ValueSetModel),
    /// A `ConceptMap`.
    ConceptMap(ConceptMapModel),
    /// One the server cannot use, held until a request reaches it.
    Unusable(Unusable),
}

/// A supplied resource the server cannot use, and the failure it answers with
/// once a request reaches it.
///
/// A request is refused for what it asks for, so a resource it was handed but
/// never resolves costs it nothing; validating a resource is a separate act a
/// server performs at its discretion
/// (<https://hl7.org/fhir/R4B/validation.html>).
#[derive(Debug, Clone)]
pub struct Unusable {
    /// The `url` the resource states, when it states one.
    pub url: Option<String>,
    /// What to answer when a request resolves that `url`.
    pub failure: Failure,
}

/// The registry, value set store, and concept map store one request sees.
#[derive(Debug)]
pub struct Scope {
    /// The layer the request started with: the loaded state and the persisted
    /// client resources.
    served: Arc<Layer>,
    /// The same layer with the request's own resources over it, when it sent
    /// any.
    layered: Option<Layer>,
    /// The supplied resources the server cannot use.
    unusable: Vec<Unusable>,
}

impl Scope {
    /// The layer the surface of `fhir_version` serves, unchanged.
    #[must_use]
    pub fn base(state: &AppState, fhir_version: &str) -> Self {
        Self {
            served: state.served_layer(fhir_version),
            layered: None,
            unusable: Vec::new(),
        }
    }

    /// The loaded registry and stores with `resources` layered on top.
    ///
    /// A resource with the url and version of a loaded one replaces it for
    /// this request; a `CodeSystem` supplement is kept for the request to name.
    ///
    /// # Errors
    ///
    /// A `CodeSystem` the engine cannot serve is a 400; a supplement whose
    /// target is not loaded is a 404.
    pub fn layered(
        state: &AppState,
        fhir_version: &str,
        resources: &[Loaded],
    ) -> Result<Self, Failure> {
        if resources.is_empty() {
            return Ok(Self::base(state, fhir_version));
        }
        let served = state.served_layer(fhir_version);
        let mut registry = served.registry().clone();
        let mut value_sets = served.value_sets().clone();
        let mut concept_maps = served.concept_maps().clone();
        let mut supplements = Vec::new();
        let mut unusable = Vec::new();
        for resource in resources {
            match resource {
                Loaded::Unusable(held) => unusable.push(held.clone()),
                Loaded::CodeSystem(model) => {
                    if model.content == ContentMode::Supplement {
                        supplements.push(model);
                        continue;
                    }
                    let provider = FhirCodeSystem::new(model.clone()).map_err(|e| {
                        Failure::new(
                            StatusCode::BAD_REQUEST,
                            "invalid",
                            format!("a `{TX_RESOURCE}` cannot be served: {e}"),
                        )
                    })?;
                    registry.register_or_replace(Arc::new(provider));
                }
                Loaded::ValueSet(model) => value_sets.replace(model.clone()),
                Loaded::ConceptMap(model) => concept_maps.replace(model.clone()),
            }
        }
        for model in supplements {
            let Some(target) = &model.supplements else {
                return Err(Failure::new(
                    StatusCode::BAD_REQUEST,
                    "invalid",
                    format!(
                        "the supplement `{}` names no code system to supplement",
                        model.url
                    ),
                ));
            };
            let (url, version) = match target.split_once('|') {
                Some((url, version)) => (url, Some(version)),
                None => (target.as_str(), None),
            };
            registry.resolve(url, version).map_err(|e| {
                Failure::new(
                    StatusCode::NOT_FOUND,
                    "not-found",
                    format!("cannot supplement: {e}"),
                )
            })?;
            // NOTE: a supplied supplement is dormant like a loaded one: a request
            // applies it by naming it in `useSupplement`, or through the value set's
            // `valueset-supplement` (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
            registry.register_supplement(target.clone(), supplement_of(model));
        }
        Ok(Self {
            served,
            layered: Some(Layer::of(registry, value_sets, concept_maps)),
            unusable,
        })
    }

    /// The failure `error` really names: the recorded one when it is the
    /// not-found for a supplied resource the server could not use, and the
    /// operation's own otherwise.
    #[must_use]
    pub fn refused(&self, error: OperationError) -> Failure {
        let named = match &error {
            OperationError::UnknownValueSet(url)
            | OperationError::UnknownImport(url)
            | OperationError::UnknownSystem(url)
            | OperationError::UnknownConceptMap(url) => url.as_str(),
            _ => return Failure::from(error),
        };
        let named = named.split('|').next().unwrap_or(named);
        self.unusable
            .iter()
            .find(|held| held.url.as_deref() == Some(named))
            .map_or_else(|| Failure::from(error), |held| held.failure.clone())
    }

    /// The layer this request resolves in.
    fn view(&self) -> &Layer {
        self.layered.as_ref().unwrap_or(&self.served)
    }

    /// The registry this request resolves systems in.
    #[must_use]
    pub fn registry(&self) -> &Registry {
        self.view().registry()
    }

    /// The value sets this request resolves.
    #[must_use]
    pub fn value_sets(&self) -> &ValueSetStore {
        self.view().value_sets()
    }

    /// The engine's view of this scope.
    #[must_use]
    pub fn sources(&self) -> Sources<'_> {
        self.view().sources()
    }
}

/// The scope of one request: the named cache's resources, then the request's
/// own, over the loaded state.
///
/// # Errors
///
/// A malformed or unknown `X-Cache-Id`, or a resource that cannot be layered.
pub fn scope_of(
    state: &AppState,
    fhir_version: &str,
    headers: &HeaderMap,
    mut resources: Vec<Loaded>,
) -> Result<Scope, Failure> {
    if let Some(id) = cache_id(headers)? {
        let cached = state.caches().get(&id)?;
        let mut all = Vec::with_capacity(cached.len() + resources.len());
        all.extend(cached.iter().cloned());
        all.append(&mut resources);
        resources = all;
    }
    Scope::layered(state, fhir_version, &resources)
}

/// The cache the request names, if any.
///
/// # Errors
///
/// More than one `X-Cache-Id`, or one that is not text.
pub fn cache_id(headers: &HeaderMap) -> Result<Option<String>, Failure> {
    let mut values = headers.get_all(CACHE_ID_HEADER).iter();
    let Some(first) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "invalid",
            format!("the request carries more than one `{CACHE_ID_HEADER}`"),
        ));
    }
    // The failure names the header; the header crate's error adds nothing to it.
    let Ok(id) = first.to_str() else {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "invalid",
            format!("`{CACHE_ID_HEADER}` is not text"),
        ));
    };
    Ok(Some(id.to_owned()))
}

struct Entry {
    resources: Arc<[Loaded]>,
    last_used: Instant,
}

/// The caches `$cache-control` started, by id; an unused one expires after
/// [`CACHE_IDLE`].
#[derive(Default)]
pub struct Caches {
    entries: Mutex<BTreeMap<String, Entry>>,
}

impl std::fmt::Debug for Caches {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len();
        f.debug_struct("Caches").field("count", &count).finish()
    }
}

impl Caches {
    /// Starts a cache holding `resources` and returns its id.
    pub fn start(&self, resources: Vec<Loaded>) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        Self::prune(&mut entries);
        entries.insert(
            id.clone(),
            Entry {
                resources: Arc::from(resources),
                last_used: Instant::now(),
            },
        );
        id
    }

    /// The resources of cache `id`, touching it.
    ///
    /// # Errors
    ///
    /// The cache is unknown or expired.
    pub fn get(&self, id: &str) -> Result<Arc<[Loaded]>, Failure> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        Self::prune(&mut entries);
        let entry = entries.get_mut(id).ok_or_else(|| unknown(id))?;
        entry.last_used = Instant::now();
        Ok(Arc::clone(&entry.resources))
    }

    /// Ends cache `id`.
    ///
    /// # Errors
    ///
    /// The cache is unknown or expired.
    pub fn end(&self, id: &str) -> Result<(), Failure> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.remove(id).map(|_| ()).ok_or_else(|| unknown(id))
    }

    fn prune(entries: &mut BTreeMap<String, Entry>) {
        let now = Instant::now();
        entries.retain(|_, entry| now.duration_since(entry.last_used) < CACHE_IDLE);
    }
}

fn unknown(id: &str) -> Failure {
    Failure::new(
        StatusCode::NOT_FOUND,
        "not-found",
        format!("cache `{id}` is not known: it was never started, or it expired or was ended"),
    )
}
