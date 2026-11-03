// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ICD-11 pipeline: a cached linearization (or the Foundation) numbered
//! and written as the artifacts the provider reads.
//!
//! Every entity is a concept; its store key is the short code when it has
//! one, else its unversioned URI, and `keys.bin` maps every entity id to its
//! ordinal so the URI forms resolve. The parent edges are the graph (the
//! Foundation is a polyhierarchy, the linearizations trees); titles, fully
//! specified names, inclusions, and index terms are designations by language;
//! `id`, `classKind`, `notSelectable`, `definition`, `exclusion`, `source`,
//! and `browserUrl` are properties; the postcoordination scales sit in
//! `scales.json`. No FHIR specification governs the layout: our own design.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ::icd11::Linearization;
use ::icd11::cache::{self, CacheError, Cached};
use concept_graph::ordinal::Ordinal;
use concept_store::builder::StoreBuilder;
use concept_store::record::{Concept, Designation, PropertyValue};
use concept_store::tables;
use serde::Serialize;
use serde_json::json;

use crate::artifact::{self, LanguageKey, Layout, Placed, PropertyKeys};
use crate::pipeline::{HIERARCHY_FILE, MANIFEST_VERSION, STORE_FILE, TEXT_FILE};

/// The manifest `kind` of an artifact this pipeline writes.
pub const KIND: &str = "icd11";
/// The key table beside the store.
pub const KEYS_FILE: &str = "keys.bin";
/// The postcoordination scales beside the store.
pub const SCALES_FILE: &str = "scales.json";
/// The designation uses, by ordinal.
pub const DESIGNATION_KINDS: [&str; 4] = ["title", "fullySpecifiedName", "inclusion", "indexTerm"];
/// The property keys, by ordinal.
pub const PROPERTY_KEYS: [&str; 7] = [
    "id",
    "classKind",
    "notSelectable",
    "definition",
    "exclusion",
    "source",
    "browserUrl",
];

/// One postcoordination scale as written to `scales.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct StoredScale {
    /// The stem's ordinal.
    pub stem: u32,
    /// The axis name (a schema URI).
    pub axis: String,
    /// Whether the axis is required.
    pub required: bool,
    /// `AllowAlways`, `NotAllowed`, or `AllowedExceptFromSameBlock`.
    pub multiple: String,
    /// The ordinals of the entities whose subtrees supply the values.
    pub entities: Vec<u32>,
}

/// What the build wrote for one code system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The system URI.
    pub system: String,
    /// The release recorded as the version.
    pub version: String,
    /// The artifact directory.
    pub dir: PathBuf,
    /// Concepts written.
    pub concepts: u64,
    /// Designations written.
    pub designations: u64,
    /// Postcoordination scales written.
    pub scales: u64,
    /// Words indexed.
    pub words: u64,
}

/// A failure of the ICD-11 build.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The cache does not read.
    #[error(transparent)]
    Cache(#[from] CacheError),
    /// The cache names no release and none was given.
    #[error("the cache names no release for `{0}`; pass `--icd11-release`")]
    NoRelease(&'static str),
    /// A part of the artifact cannot be built or written.
    #[error(transparent)]
    Artifact(#[from] artifact::Error),
}

/// The designations of one entity: its titles, fully specified names,
/// inclusions, and index terms, numbered in that order.
fn entity_designations(
    entity: &::icd11::entity::Entity,
    ordinal: Ordinal,
    placed: &mut Vec<Placed>,
    languages: &mut BTreeSet<String>,
) {
    let mut index = 0u32;
    let mut place = |texts: &[::icd11::entity::Text], use_ordinal: u32| {
        for text in texts {
            languages.insert(text.language.clone());
            placed.push(Placed {
                ordinal,
                index,
                record: Designation {
                    id: None,
                    term: text.value.clone(),
                    language: text.language.clone(),
                    use_ordinal,
                    active: true,
                },
            });
            index = index.saturating_add(1);
        }
    };
    place(&entity.titles, 0);
    place(&entity.fully_specified, 1);
    place(&entity.inclusions, 2);
    place(&entity.index_terms, 3);
}

/// The parent edges of one entity; a parent outside the cache has no edge.
fn entity_edges(
    entity: &::icd11::entity::Entity,
    ordinal: Ordinal,
    ordinals: &BTreeMap<&str, Ordinal>,
    edges: &mut Vec<(Ordinal, Ordinal)>,
) {
    for parent in &entity.parents {
        if let Some(&parent_ordinal) = ordinals.get(parent.as_str()) {
            edges.push((ordinal, parent_ordinal));
        }
    }
}

/// The properties of one entity, each under its key.
fn entity_properties(
    builder: &mut StoreBuilder,
    entity: &::icd11::entity::Entity,
    id: &str,
    ordinal: Ordinal,
    linearization: Linearization,
    keys: &PropertyKeys,
) -> Result<(), artifact::Error> {
    builder.properties(
        ordinal,
        keys.key("id")?,
        &[PropertyValue::Code(linearization.uri(id))],
    )?;
    if let Some(kind) = &entity.class_kind {
        builder.properties(
            ordinal,
            keys.key("classKind")?,
            &[PropertyValue::Code(kind.clone())],
        )?;
    }
    if entity.code.is_none() {
        builder.properties(
            ordinal,
            keys.key("notSelectable")?,
            &[PropertyValue::Boolean(true)],
        )?;
    }
    if !entity.definitions.is_empty() {
        let values: Vec<PropertyValue> = entity
            .definitions
            .iter()
            .map(|t| PropertyValue::String(t.value.clone()))
            .collect();
        builder.properties(ordinal, keys.key("definition")?, &values)?;
    }
    if !entity.exclusions.is_empty() {
        let values: Vec<PropertyValue> = entity
            .exclusions
            .iter()
            .map(|t| PropertyValue::String(t.value.clone()))
            .collect();
        builder.properties(ordinal, keys.key("exclusion")?, &values)?;
    }
    if let Some(source) = &entity.source {
        builder.properties(
            ordinal,
            keys.key("source")?,
            &[PropertyValue::Code(source.clone())],
        )?;
    }
    if let Some(url) = &entity.browser_url {
        builder.properties(
            ordinal,
            keys.key("browserUrl")?,
            &[PropertyValue::String(url.clone())],
        )?;
    }
    Ok(())
}

/// The postcoordination scales of one entity, their axis entities numbered.
fn entity_scales(
    entity: &::icd11::entity::Entity,
    ordinal: Ordinal,
    ordinals: &BTreeMap<&str, Ordinal>,
    scales: &mut Vec<StoredScale>,
) {
    for scale in &entity.scales {
        let entities: Vec<u32> = scale
            .entities
            .iter()
            .filter_map(|e| ordinals.get(e.as_str()).map(|o| o.index()))
            .collect();
        scales.push(StoredScale {
            stem: ordinal.index(),
            axis: scale.axis.clone(),
            required: scale.required,
            multiple: scale.multiple.clone(),
            entities,
        });
    }
}

/// Builds the artifacts for `cached` into `out`, with `release` overriding
/// the release the cache names.
///
/// # Errors
///
/// Returns [`Error`] when no release is known, the hierarchy has a cycle,
/// or an artifact cannot be written.
pub fn build(cached: &Cached, release: Option<&str>, out: &Path) -> Result<Report, Error> {
    let release = release
        .map(str::to_owned)
        .or_else(|| cached.release.clone())
        .ok_or(Error::NoRelease(cached.linearization.name()))?;
    Ok(write(cached, release, out)?)
}

/// What the entity pass gathers for the parts beside the store.
#[derive(Default)]
struct Gathered {
    placed: Vec<Placed>,
    keys: Vec<(u64, u32)>,
    edges: Vec<(Ordinal, Ordinal)>,
    scales: Vec<StoredScale>,
    languages: BTreeSet<String>,
}

/// Writes every entity into the store, gathering what the other parts hold.
fn write_entities(
    builder: &mut StoreBuilder,
    cached: &Cached,
    ordinals: &BTreeMap<&str, Ordinal>,
    keys: &PropertyKeys,
) -> Result<Gathered, artifact::Error> {
    let linearization = cached.linearization;
    let mut gathered = Gathered::default();
    for (id, entity) in &cached.entities {
        let Some(&ordinal) = ordinals.get(id.as_str()) else {
            continue;
        };
        if let Some(k) = Linearization::key_of(id) {
            gathered.keys.push((k, ordinal.index()));
        }
        let code = entity.code.clone().unwrap_or_else(|| linearization.uri(id));
        builder.concept(
            ordinal,
            &Concept {
                code,
                active: true,
                effective_time: None,
                module: None,
            },
        )?;
        entity_edges(entity, ordinal, ordinals, &mut gathered.edges);
        entity_designations(
            entity,
            ordinal,
            &mut gathered.placed,
            &mut gathered.languages,
        );
        entity_properties(builder, entity, id, ordinal, linearization, keys)?;
        entity_scales(entity, ordinal, ordinals, &mut gathered.scales);
    }
    Ok(gathered)
}

/// Writes the artifacts of `cached` under `out` as the release `release`.
fn write(cached: &Cached, release: String, out: &Path) -> Result<Report, artifact::Error> {
    let linearization = cached.linearization;
    std::fs::create_dir_all(out).map_err(artifact::io_error(out))?;
    let mut ordinals: BTreeMap<&str, Ordinal> = BTreeMap::new();
    for (i, id) in cached.entities.keys().enumerate() {
        ordinals.insert(id, artifact::ordinal(i)?);
    }
    let store_path = out.join(STORE_FILE);
    let mut builder = artifact::open_store(
        &store_path,
        linearization.system(),
        &release,
        &DESIGNATION_KINDS,
    )?;
    let keys = PropertyKeys::write(&mut builder, &PROPERTY_KEYS)?;
    let gathered = write_entities(&mut builder, cached, &ordinals, &keys)?;
    artifact::write_designations(&mut builder, &gathered.placed)?;
    artifact::write_hierarchy(out, cached.entities.len(), gathered.edges)?;
    let words = artifact::write_text_index(out, &gathered.placed, LanguageKey::PrimarySubtag)?;
    artifact::write_key_table(&out.join(KEYS_FILE), gathered.keys)?;
    artifact::write_json(&out.join(SCALES_FILE), &gathered.scales, Layout::Compact)?;
    artifact::finish(builder)?;
    let title = cached
        .titles
        .get("en")
        .or_else(|| cached.titles.values().next())
        .cloned();
    artifact::write_manifest(
        out,
        &json!({
            "manifest": MANIFEST_VERSION,
            "kind": KIND,
            "linearization": linearization.name(),
            "system": linearization.system(),
            "edition": linearization.system(),
            "version": release,
            "releaseDate": cached.release_date,
            "title": title,
            "titles": cached.titles,
            "store": STORE_FILE,
            "storeLayout": tables::LAYOUT_VERSION,
            "hierarchy": HIERARCHY_FILE,
            "text": TEXT_FILE,
            "keys": KEYS_FILE,
            "scales": SCALES_FILE,
            "concepts": cached.entities.len(),
            "designations": gathered.placed.len(),
            "words": words,
            "languages": gathered.languages,
        }),
    )?;
    Ok(Report {
        system: linearization.system().to_owned(),
        version: release,
        dir: out.to_path_buf(),
        concepts: u64::try_from(cached.entities.len()).unwrap_or(u64::MAX),
        designations: u64::try_from(gathered.placed.len()).unwrap_or(u64::MAX),
        scales: u64::try_from(gathered.scales.len()).unwrap_or(u64::MAX),
        words: u64::try_from(words).unwrap_or(u64::MAX),
    })
}

/// Builds every code system the cache under `cache` holds into `out/<name>`.
///
/// `release` overrides the release for all of them; the Foundation's root
/// names none, so the MMS release serves as its version when no override is
/// given.
///
/// # Errors
///
/// Returns [`Error`] when a cached code system does not read or build.
pub fn build_all(cache: &Path, release: Option<&str>, out: &Path) -> Result<Vec<Report>, Error> {
    let mut reports = Vec::new();
    let mut fallback: Option<String> = release.map(str::to_owned);
    for linearization in Linearization::ALL {
        if !cache.join(linearization.name()).is_dir() {
            continue;
        }
        let cached = cache::read(cache, linearization)?;
        if fallback.is_none() {
            fallback.clone_from(&cached.release);
        }
        let report = build(
            &cached,
            fallback.as_deref(),
            &out.join(linearization.name()),
        )?;
        reports.push(report);
    }
    Ok(reports)
}
