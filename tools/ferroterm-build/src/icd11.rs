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
use std::io;
use std::path::{Path, PathBuf};

use ::icd11::Linearization;
use ::icd11::cache::{self, CacheError, Cached};
use concept_graph::closure::{Closure, ClosureError};
use concept_graph::csr::{Csr, CsrError};
use concept_graph::ordinal::Ordinal;
use concept_graph::persist::Hierarchy;
use concept_store::builder::{BuildError, PreferredRule, StoreBuilder};
use concept_store::keys::{KeyTable, KeyTableError};
use concept_store::record::{Concept, Designation, PropertyValue};
use concept_store::store::Vocabulary;
use concept_store::tables;
use designation_index::index::{IndexBuilder, Input};
use serde::Serialize;
use serde_json::json;

use crate::pipeline::{HIERARCHY_FILE, MANIFEST_FILE, MANIFEST_VERSION, STORE_FILE, TEXT_FILE};

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
    /// The store cannot be written.
    #[error(transparent)]
    Store(#[from] BuildError),
    /// The hierarchy cannot be built.
    #[error(transparent)]
    Csr(#[from] CsrError),
    /// The hierarchy has a cycle.
    #[error(transparent)]
    Closure(#[from] ClosureError),
    /// The hierarchy cannot be written.
    #[error(transparent)]
    Graph(#[from] concept_graph::persist::PersistError),
    /// The key table cannot be written.
    #[error(transparent)]
    Keys(#[from] KeyTableError),
    /// The text index cannot be built.
    #[error(transparent)]
    Text(#[from] designation_index::index::BuildError),
    /// The text index cannot be written.
    #[error(transparent)]
    TextPersist(#[from] designation_index::persist::PersistError),
    /// A file cannot be written.
    #[error("cannot write {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: io::Error,
    },
    /// More concepts than an ordinal can number.
    #[error("too many concepts")]
    TooMany,
}

fn ordinal(index: usize) -> Result<Ordinal, Error> {
    // A count past u32::MAX is the whole message; the conversion error adds nothing.
    let Ok(index) = u32::try_from(index) else {
        return Err(Error::TooMany);
    };
    Ok(Ordinal::new(index))
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn primary(language: &str) -> &str {
    language.split(['-', '_']).next().unwrap_or(language)
}

/// One designation to write and index.
struct Placed {
    ordinal: Ordinal,
    index: u32,
    record: Designation,
}

/// The ordinal of the property key `name`.
fn property_key(name: &str) -> Result<u32, Error> {
    PROPERTY_KEYS
        .iter()
        .position(|k| *k == name)
        .and_then(|i| u32::try_from(i).ok())
        .ok_or(Error::TooMany)
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
) -> Result<(), Error> {
    builder.properties(
        ordinal,
        property_key("id")?,
        &[PropertyValue::Code(linearization.uri(id))],
    )?;
    if let Some(kind) = &entity.class_kind {
        builder.properties(
            ordinal,
            property_key("classKind")?,
            &[PropertyValue::Code(kind.clone())],
        )?;
    }
    if entity.code.is_none() {
        builder.properties(
            ordinal,
            property_key("notSelectable")?,
            &[PropertyValue::Boolean(true)],
        )?;
    }
    if !entity.definitions.is_empty() {
        let values: Vec<PropertyValue> = entity
            .definitions
            .iter()
            .map(|t| PropertyValue::String(t.value.clone()))
            .collect();
        builder.properties(ordinal, property_key("definition")?, &values)?;
    }
    if !entity.exclusions.is_empty() {
        let values: Vec<PropertyValue> = entity
            .exclusions
            .iter()
            .map(|t| PropertyValue::String(t.value.clone()))
            .collect();
        builder.properties(ordinal, property_key("exclusion")?, &values)?;
    }
    if let Some(source) = &entity.source {
        builder.properties(
            ordinal,
            property_key("source")?,
            &[PropertyValue::Code(source.clone())],
        )?;
    }
    if let Some(url) = &entity.browser_url {
        builder.properties(
            ordinal,
            property_key("browserUrl")?,
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
#[expect(
    clippy::too_many_lines,
    reason = "one pass per artifact, read top to bottom"
)]
pub fn build(cached: &Cached, release: Option<&str>, out: &Path) -> Result<Report, Error> {
    let linearization = cached.linearization;
    let release = release
        .map(str::to_owned)
        .or_else(|| cached.release.clone())
        .ok_or(Error::NoRelease(linearization.name()))?;
    std::fs::create_dir_all(out).map_err(io_error(out))?;
    let mut ordinals: BTreeMap<&str, Ordinal> = BTreeMap::new();
    for (i, id) in cached.entities.keys().enumerate() {
        ordinals.insert(id, ordinal(i)?);
    }
    let store_path = out.join(STORE_FILE);
    let mut builder = StoreBuilder::create(&store_path, linearization.system(), &release)?;
    for (i, name) in DESIGNATION_KINDS.iter().enumerate() {
        builder.vocabulary(Vocabulary::DesignationUses, ordinal(i)?.index(), name)?;
    }
    for (i, name) in PROPERTY_KEYS.iter().enumerate() {
        builder.vocabulary(Vocabulary::PropertyKeys, ordinal(i)?.index(), name)?;
    }
    let mut placed: Vec<Placed> = Vec::new();
    let mut keys: Vec<(u64, u32)> = Vec::new();
    let mut edges: Vec<(Ordinal, Ordinal)> = Vec::new();
    let mut scales: Vec<StoredScale> = Vec::new();
    let mut languages: BTreeSet<String> = BTreeSet::new();
    for (id, entity) in &cached.entities {
        let Some(&ordinal) = ordinals.get(id.as_str()) else {
            continue;
        };
        if let Some(k) = Linearization::key_of(id) {
            keys.push((k, ordinal.index()));
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
        entity_edges(entity, ordinal, &ordinals, &mut edges);
        entity_designations(entity, ordinal, &mut placed, &mut languages);
        entity_properties(&mut builder, entity, id, ordinal, linearization)?;
        entity_scales(entity, ordinal, &ordinals, &mut scales);
    }
    for p in &placed {
        builder.designation(p.ordinal, p.index, &p.record)?;
    }
    let count = ordinal(cached.entities.len())?.index();
    let csr = Csr::build(count, edges)?;
    let closure = Closure::compute(&csr)?;
    let hierarchy = Hierarchy { is_a: csr, closure };
    let hierarchy_path = out.join(HIERARCHY_FILE);
    let mut graph_bytes = Vec::new();
    hierarchy.write_to(&mut graph_bytes)?;
    std::fs::write(&hierarchy_path, &graph_bytes).map_err(io_error(&hierarchy_path))?;
    let mut index = IndexBuilder::new();
    for p in &placed {
        index.add(&Input {
            concept: p.ordinal,
            index: p.index,
            term: &p.record.term,
            language: primary(&p.record.language),
            use_ordinal: p.record.use_ordinal,
            active: true,
            refsets: &[],
        })?;
    }
    let index = index.build()?;
    let mut text_bytes = Vec::new();
    designation_index::persist::write_to(&index, &mut text_bytes)?;
    let text_path = out.join(TEXT_FILE);
    std::fs::write(&text_path, &text_bytes).map_err(io_error(&text_path))?;
    let table = KeyTable::new(keys);
    let mut key_bytes = Vec::new();
    table.write_to(&mut key_bytes)?;
    let keys_path = out.join(KEYS_FILE);
    std::fs::write(&keys_path, &key_bytes).map_err(io_error(&keys_path))?;
    let scales_path = out.join(SCALES_FILE);
    let scales_text = serde_json::to_string(&scales).map_err(|source| Error::Io {
        path: scales_path.clone(),
        source: io::Error::other(source),
    })?;
    std::fs::write(&scales_path, scales_text).map_err(io_error(&scales_path))?;
    builder.finish(&PreferredRule {
        preferred: 0,
        // Nothing reads a display column for this system; its provider
        // chooses a display from the designations it already reads.
        display_use: None,
    })?;
    let title = cached
        .titles
        .get("en")
        .or_else(|| cached.titles.values().next())
        .cloned();
    let manifest = json!({
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
        "designations": placed.len(),
        "words": index.words(),
        "languages": languages,
    });
    let manifest_path = out.join(MANIFEST_FILE);
    let text = serde_json::to_string_pretty(&manifest).map_err(|source| Error::Io {
        path: manifest_path.clone(),
        source: io::Error::other(source),
    })?;
    std::fs::write(&manifest_path, format!("{text}\n")).map_err(io_error(&manifest_path))?;
    Ok(Report {
        system: linearization.system().to_owned(),
        version: release,
        dir: out.to_path_buf(),
        concepts: u64::try_from(cached.entities.len()).unwrap_or(u64::MAX),
        designations: u64::try_from(placed.len()).unwrap_or(u64::MAX),
        scales: u64::try_from(scales.len()).unwrap_or(u64::MAX),
        words: u64::try_from(index.words()).unwrap_or(u64::MAX),
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
