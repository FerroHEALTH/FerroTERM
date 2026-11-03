// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The artefacts the server holds loaded, below the state and the version
//! surfaces that read them.
//!
//! A `CodeSystem`, `ValueSet` or `ConceptMap` of any served version reads as
//! one model of the engine ([`Loaded`]), so a resource stored or supplied in
//! one version serves every version; a cache `$cache-control` started holds
//! those models ([`Caches`]). No specification governs this layering: our own
//! design.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use fhir_terminology::provider::code_system::CodeSystemModel;
use fhir_terminology::provider::concept_map::ConceptMapModel;
use fhir_terminology::valueset::model::ValueSetModel;
use http::StatusCode;

use crate::outcome::Failure;

/// The parameter that carries a request-scoped resource.
pub const TX_RESOURCE: &str = "tx-resource";
/// How long an unused cache lives. No spec fixes this: our own design.
pub const CACHE_IDLE: Duration = Duration::from_mins(30);

/// A resource as the model the engine serves: supplied with a request, held
/// in a cache, or stored.
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

/// Why a stored object does not read as a model this server serves.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// The codec refused the object: a shape or a primitive outside its
    /// lexical form, with the element path.
    #[error(transparent)]
    Decode(fhir_types::codec::DecodeError),
    /// The object decoded and the conversion to the model refused it.
    #[error("{0}")]
    Convert(String),
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
