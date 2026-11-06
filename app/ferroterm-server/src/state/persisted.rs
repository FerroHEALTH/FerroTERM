// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The client resources persisted through the REST API, layered over the loaded state.

use std::collections::BTreeMap;
use std::sync::Arc;

use fhir_terminology::fhir_core::CoreTerminology;

use crate::artefact::ReadError;
use crate::persistence::{
    HistoryEntry, Method, Record, ResourceType, StoreError, closure::Closure,
};
use crate::state::AppState;
use crate::state::duplicate::{duplicate_canonicals, in_write_order, same_canonical};
use crate::state::ids::canonical;
use crate::state::layer::{Layer, LayerError};

/// The persisted client resources and the layer they form over the loaded
/// state, kept together so a reader sees a record and the layer that holds it.
#[derive(Debug)]
pub(super) struct Persisted {
    /// Every current record, keyed `<type>/<id>`.
    pub(super) records: BTreeMap<(ResourceType, String), Record>,
    /// The loaded state with `records` applied.
    pub(super) layer: Arc<Layer>,
    /// The same layer per served version, each over the terminology that
    /// version defines.
    pub(super) served: BTreeMap<&'static str, Arc<Layer>>,
}

/// A failure to persist a client resource.
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// The deployment configured no resource database.
    #[error("this server persists no resources: set terminology.resources")]
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

/// What one rebuild of the served state produced.
pub(super) struct Layers {
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
pub(super) fn persisted_layers(
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
pub(super) fn shadows_core(core: &CoreTerminology, record: &Record) -> bool {
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
pub(super) fn core_holds(
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
pub(super) fn persisted_holder(
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

/// The loaded state with every persisted record applied over it.
pub(super) fn layered(
    base: &Layer,
    records: &BTreeMap<(ResourceType, String), Record>,
) -> Result<Arc<Layer>, PersistError> {
    if records.is_empty() {
        return Ok(Arc::new(base.clone()));
    }
    let mut layer = base.clone();
    for record in in_write_order(records) {
        let (resource_type, id) = (&record.resource_type, &record.id);
        let resource = crate::served::read(&record.fhir_version, &record.resource).map_err(
            |error| match error {
                ReadError::Decode(source) => PersistError::Decode {
                    resource_type: resource_type.clone(),
                    id: id.clone(),
                    source,
                },
                ReadError::Convert(reason) => PersistError::Convert {
                    resource_type: resource_type.clone(),
                    id: id.clone(),
                    reason,
                },
            },
        )?;
        layer.apply(&resource)?;
    }
    Ok(Arc::new(layer))
}

/// Writes `meta.versionId` and `meta.lastUpdated` into `resource`, keeping
/// whatever else its `meta` carries
/// (<https://hl7.org/fhir/R4B/resource.html#Meta>).
pub(super) fn stamp(
    resource: &mut fhir_types::codec::Object,
    version_id: u32,
    last_modified: &str,
) {
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
pub(super) fn text_of(object: &fhir_types::codec::Object, field: &str) -> Option<String> {
    object.get(field)?.as_str().map(str::to_owned)
}

impl AppState {
    /// Reads every persisted record and rebuilds the served layer from them.
    pub(super) fn reload_persisted(&mut self) -> Result<(), PersistError> {
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
    pub(super) fn loaded_canonicals(&self) -> Vec<(ResourceType, &str, &str, Option<&str>)> {
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
    pub(super) fn loaded_holder(
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
