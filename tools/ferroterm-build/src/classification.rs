// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The classification pipeline: a [`Classification`] (`ClaML` or the ICD-10-CM
//! release, read by `classification`) numbered and written as the
//! artifacts every provider reads.
//!
//! Every class is a concept, sorted by code; the single-parent tree is the
//! graph (`hierarchyMeaning = classified-with`, the FHIR ICD page); the
//! titles, inclusion terms, and short descriptions are designations indexed
//! for search; every other rubric kind is a property under its own name
//! beside `kind`, `usage`, and `valid`. No FHIR specification governs the
//! artifact layout: our own design, shared with the SNOMED CT and LOINC
//! builds.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use ::classification::{Class, Classification};
use concept_graph::closure::{Closure, ClosureError};
use concept_graph::csr::{Csr, CsrError};
use concept_graph::ordinal::Ordinal;
use concept_graph::persist::Hierarchy;
use concept_store::builder::{BuildError, PreferredRule, StoreBuilder};
use concept_store::record::{Concept, Designation, PropertyValue};
use concept_store::store::Vocabulary;
use concept_store::tables;
use designation_index::index::{IndexBuilder, Input};
use serde_json::json;

use crate::pipeline::{HIERARCHY_FILE, MANIFEST_FILE, MANIFEST_VERSION, STORE_FILE, TEXT_FILE};

/// The manifest `kind` that marks an artifact built by this pipeline.
pub const KIND: &str = "classification";
/// The `hierarchyMeaning` the manifest records
/// (<https://terminology.hl7.org/ICD.html>).
pub const HIERARCHY_MEANING: &str = "classified-with";
/// The ICD-10-CM system URI (<https://hl7.org/fhir/R4B/icd.html>).
pub const ICD10CM_SYSTEM: &str = "http://hl7.org/fhir/sid/icd-10-cm";
/// The rubric kinds stored as designations by default, by use ordinal
/// (a classification may name its own).
pub const DESIGNATION_KINDS: [&str; 5] = ::classification::DEFAULT_DESIGNATION_KINDS;
/// The property that carries the class kind (`chapter`, `block`, ...).
pub const KIND_KEY: &str = "kind";
/// The property that carries the usage mark (`dagger`, `aster`).
pub const USAGE_KEY: &str = "usage";
/// The property that says whether the code is valid for use as a code.
pub const VALID_KEY: &str = "valid";

/// What the build wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The system URI recorded.
    pub system: String,
    /// The version recorded.
    pub version: String,
    /// The store file.
    pub store: PathBuf,
    /// Concepts written.
    pub concepts: u64,
    /// Designations written.
    pub designations: u64,
    /// Words indexed.
    pub words: u64,
}

/// A failure of the classification build.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The source names no version and none was given.
    #[error("the classification names no version; pass one")]
    NoVersion,
    /// Two classes carry one code.
    #[error("the code `{0}` occurs twice")]
    Duplicate(String),
    /// A class names a parent that is not a class.
    #[error("`{code}` has the parent `{parent}`, which is not a class")]
    UnknownParent {
        /// The class.
        code: String,
        /// The parent named.
        parent: String,
    },
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

/// One designation to write and index.
struct Placed {
    ordinal: Ordinal,
    index: u32,
    record: Designation,
}

fn use_ordinal(kinds: &[String], kind: &str) -> Option<u32> {
    kinds
        .iter()
        .position(|k| k == kind)
        .and_then(|i| u32::try_from(i).ok())
}

/// The classes by code, refusing a duplicate.
fn by_code(classification: &Classification) -> Result<BTreeMap<&str, &Class>, Error> {
    let mut out = BTreeMap::new();
    for class in &classification.classes {
        if out.insert(class.code.as_str(), class).is_some() {
            return Err(Error::Duplicate(class.code.clone()));
        }
    }
    Ok(out)
}

/// The rubrics of one class: a rubric of a designation kind is a designation,
/// numbered within the class; every other kind is a property of its own name.
fn class_rubrics(
    class: &Class,
    ordinal: Ordinal,
    designation_kinds: &[String],
    keys: &BTreeMap<String, u32>,
    placed: &mut Vec<Placed>,
    languages: &mut BTreeSet<String>,
    properties: &mut BTreeMap<u32, Vec<PropertyValue>>,
) -> Result<(), Error> {
    let mut index = 0;
    for rubric in &class.rubrics {
        languages.insert(rubric.language.clone());
        match use_ordinal(designation_kinds, &rubric.kind) {
            Some(use_ordinal) => {
                placed.push(Placed {
                    ordinal,
                    index,
                    record: Designation {
                        id: None,
                        term: rubric.text.clone(),
                        language: rubric.language.clone(),
                        use_ordinal,
                        active: true,
                    },
                });
                index += 1;
            }
            None => properties
                .entry(keys.get(&rubric.kind).copied().ok_or(Error::TooMany)?)
                .or_default()
                .push(PropertyValue::String(rubric.text.clone())),
        }
    }
    Ok(())
}

/// The property keys: the fixed three, then every rubric kind that is not a
/// designation, sorted.
fn property_keys(classification: &Classification) -> Vec<String> {
    let mut keys: Vec<String> = [KIND_KEY, USAGE_KEY, VALID_KEY].map(str::to_owned).to_vec();
    let mut rubric_kinds: BTreeSet<&str> = BTreeSet::new();
    for class in &classification.classes {
        for rubric in &class.rubrics {
            if use_ordinal(&classification.designation_kinds, &rubric.kind).is_none() {
                rubric_kinds.insert(rubric.kind.as_str());
            }
        }
    }
    keys.extend(rubric_kinds.into_iter().map(str::to_owned));
    keys
}

/// Builds the artifacts for `classification` under `system` into `out`.
///
/// `version` overrides the version the source states; without either the
/// build refuses.
///
/// # Errors
///
/// Returns [`Error`] when no version is known, a code occurs twice, a parent
/// is not a class, the tree has a cycle, or an artifact cannot be written.
#[expect(
    clippy::too_many_lines,
    reason = "one pass per artifact, read top to bottom"
)]
pub fn build(
    classification: &Classification,
    system: &str,
    version: Option<&str>,
    out: &Path,
) -> Result<Report, Error> {
    let version = version
        .map(str::to_owned)
        .or_else(|| classification.version.clone())
        .ok_or(Error::NoVersion)?;
    let classes = by_code(classification)?;
    std::fs::create_dir_all(out).map_err(io_error(out))?;
    let store_path = out.join(STORE_FILE);
    let mut builder = StoreBuilder::create(&store_path, system, &version)?;
    for (i, name) in classification.designation_kinds.iter().enumerate() {
        builder.vocabulary(Vocabulary::DesignationUses, ordinal(i)?.index(), name)?;
    }
    let mut keys: BTreeMap<String, u32> = BTreeMap::new();
    for name in property_keys(classification) {
        let key = ordinal(keys.len())?.index();
        builder.vocabulary(Vocabulary::PropertyKeys, key, &name)?;
        keys.insert(name, key);
    }
    let key = |name: &str| keys.get(name).copied().ok_or(Error::TooMany);
    let mut ordinals: BTreeMap<&str, Ordinal> = BTreeMap::new();
    for (i, code) in classes.keys().enumerate() {
        ordinals.insert(code, ordinal(i)?);
    }
    let mut placed: Vec<Placed> = Vec::new();
    let mut languages: BTreeSet<String> = BTreeSet::new();
    let mut edges: Vec<(Ordinal, Ordinal)> = Vec::new();
    for (code, class) in &classes {
        let Some(&ordinal) = ordinals.get(code) else {
            continue;
        };
        builder.concept(
            ordinal,
            &Concept {
                code: class.code.clone(),
                active: class.active,
                effective_time: None,
                module: None,
            },
        )?;
        if let Some(parent) = &class.parent {
            let Some(&parent_ordinal) = ordinals.get(parent.as_str()) else {
                return Err(Error::UnknownParent {
                    code: class.code.clone(),
                    parent: parent.clone(),
                });
            };
            edges.push((ordinal, parent_ordinal));
        }
        let mut properties: BTreeMap<u32, Vec<PropertyValue>> = BTreeMap::new();
        properties.insert(
            key(KIND_KEY)?,
            vec![PropertyValue::Code(class.kind.clone())],
        );
        if let Some(usage) = &class.usage {
            properties.insert(key(USAGE_KEY)?, vec![PropertyValue::Code(usage.clone())]);
        }
        if let Some(valid) = class.valid {
            properties.insert(key(VALID_KEY)?, vec![PropertyValue::Boolean(valid)]);
        }
        class_rubrics(
            class,
            ordinal,
            &classification.designation_kinds,
            &keys,
            &mut placed,
            &mut languages,
            &mut properties,
        )?;
        for (key, values) in &properties {
            builder.properties(ordinal, *key, values)?;
        }
    }
    for p in &placed {
        builder.designation(p.ordinal, p.index, &p.record)?;
    }
    let count = ordinal(classes.len())?.index();
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
            // NOTE: the index keys designations by primary language subtag, the way
            // the providers query it; the store keeps the full BCP 47 tag.
            language: p
                .record
                .language
                .split('-')
                .next()
                .unwrap_or(&p.record.language),
            use_ordinal: p.record.use_ordinal,
            active: p.record.active,
            refsets: &[],
        })?;
    }
    let index = index.build()?;
    let mut text_bytes = Vec::new();
    designation_index::persist::write_to(&index, &mut text_bytes)?;
    let text_path = out.join(TEXT_FILE);
    std::fs::write(&text_path, &text_bytes).map_err(io_error(&text_path))?;
    builder.finish(&PreferredRule {
        preferred: 0,
        // Nothing reads a display column for this system; its provider
        // chooses a display from the designations it already reads.
        display_use: None,
    })?;
    let manifest = json!({
        "manifest": MANIFEST_VERSION,
        "kind": KIND,
        "system": system,
        "edition": system,
        "version": version,
        "name": classification.name,
        "title": classification.title,
        "hierarchyMeaning": classification.hierarchy,
        "language": classification.language,
        "store": STORE_FILE,
        "storeLayout": tables::LAYOUT_VERSION,
        "hierarchy": HIERARCHY_FILE,
        "text": TEXT_FILE,
        "concepts": classes.len(),
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
        system: system.to_owned(),
        version,
        store: store_path,
        concepts: u64::try_from(classes.len()).unwrap_or(u64::MAX),
        designations: u64::try_from(placed.len()).unwrap_or(u64::MAX),
        words: u64::try_from(index.words()).unwrap_or(u64::MAX),
    })
}
