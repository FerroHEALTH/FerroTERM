// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The layer an operation resolves in: code systems, value sets and concept maps.

use std::sync::Arc;

use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::fhir_codesystem::provider::{BuildError, FhirCodeSystem};
use fhir_terminology::operations::sources::Sources;
use fhir_terminology::provider::declaration::ContentMode;
use fhir_terminology::registry::Registry;
use fhir_terminology::valueset::store::ValueSetStore;

use crate::artefact;
use fhir_terminology::supplement::Supplement;

/// The code systems, value sets, and concept maps an operation resolves in.
///
/// One layer holds what the deployment loaded from disk and the persisted
/// client resources over it; a request scope layers its own `tx-resource`s on
/// a clone of the one it took at its start.
#[derive(Debug, Clone)]
pub struct Layer {
    pub(super) registry: Registry,
    pub(super) value_sets: ValueSetStore,
    pub(super) concept_maps: ConceptMapStore,
}

impl Layer {
    /// An empty layer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry: Registry::default(),
            value_sets: ValueSetStore::default(),
            concept_maps: ConceptMapStore::default(),
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
    pub fn apply(&mut self, resource: &artefact::Loaded) -> Result<(), LayerError> {
        match resource {
            artefact::Loaded::CodeSystem(model) => {
                if model.content == ContentMode::Supplement {
                    let target = model.supplements.clone().ok_or_else(|| {
                        LayerError::SupplementWithoutTarget {
                            url: model.url.clone(),
                        }
                    })?;
                    // NOTE: a persisted supplement stays dormant like a loaded one until a
                    // request names it (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
                    self.registry
                        .register_supplement(target, Supplement::from_code_system(model));
                    return Ok(());
                }
                let provider =
                    FhirCodeSystem::new(model.clone()).map_err(|source| LayerError::Build {
                        url: model.url.clone(),
                        source,
                    })?;
                self.registry.register_or_replace(Arc::new(provider));
            }
            artefact::Loaded::ValueSet(model) => self.value_sets.replace(model.clone()),
            artefact::Loaded::ConceptMap(model) => self.concept_maps.replace(model.clone()),
            // NOTE: a resource the server cannot use answers the request that
            // resolves it (`crate::artefact::Unusable`); a layer holds nothing for it.
            artefact::Loaded::Unusable(_) => {}
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
