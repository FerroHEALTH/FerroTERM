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
use std::path::{Path, PathBuf};

use ::classification::{Class, Classification};
use concept_graph::ordinal::Ordinal;
use concept_store::builder::StoreBuilder;
use concept_store::record::{Concept, Designation, PropertyValue};
use concept_store::tables;
use serde_json::json;

use crate::artifact::{self, LanguageKey, Placed, PropertyKeys};
use crate::pipeline::{HIERARCHY_FILE, MANIFEST_VERSION, STORE_FILE, TEXT_FILE};

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
    /// A part of the artifact cannot be built or written.
    #[error(transparent)]
    Artifact(#[from] artifact::Error),
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

/// The classes numbered by code, with their child-to-parent edges.
struct Numbered<'a> {
    classes: BTreeMap<&'a str, &'a Class>,
    ordinals: BTreeMap<&'a str, Ordinal>,
    edges: Vec<(Ordinal, Ordinal)>,
}

/// Numbers the classes by code and resolves every parent, refusing a
/// duplicate code or a parent that is not a class.
fn number(classification: &Classification) -> Result<Numbered<'_>, Error> {
    let classes = by_code(classification)?;
    let mut ordinals: BTreeMap<&str, Ordinal> = BTreeMap::new();
    for (i, code) in classes.keys().enumerate() {
        ordinals.insert(code, artifact::ordinal(i)?);
    }
    let mut edges: Vec<(Ordinal, Ordinal)> = Vec::new();
    for (code, class) in &classes {
        let (Some(&ordinal), Some(parent)) = (ordinals.get(code), &class.parent) else {
            continue;
        };
        let Some(&parent_ordinal) = ordinals.get(parent.as_str()) else {
            return Err(Error::UnknownParent {
                code: class.code.clone(),
                parent: parent.clone(),
            });
        };
        edges.push((ordinal, parent_ordinal));
    }
    Ok(Numbered {
        classes,
        ordinals,
        edges,
    })
}

/// The rubrics of one class: a rubric of a designation kind is a designation,
/// numbered within the class; every other kind is a property of its own name.
fn class_rubrics(
    class: &Class,
    ordinal: Ordinal,
    designation_kinds: &[String],
    keys: &PropertyKeys,
    placed: &mut Vec<Placed>,
    languages: &mut BTreeSet<String>,
    properties: &mut BTreeMap<u32, Vec<PropertyValue>>,
) -> Result<(), artifact::Error> {
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
                .entry(keys.key(&rubric.kind)?)
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
    let numbered = number(classification)?;
    Ok(write(classification, &numbered, system, version, out)?)
}

/// The properties of one class: its kind, usage, and validity.
fn class_properties(
    class: &Class,
    keys: &PropertyKeys,
) -> Result<BTreeMap<u32, Vec<PropertyValue>>, artifact::Error> {
    let mut properties: BTreeMap<u32, Vec<PropertyValue>> = BTreeMap::new();
    properties.insert(
        keys.key(KIND_KEY)?,
        vec![PropertyValue::Code(class.kind.clone())],
    );
    if let Some(usage) = &class.usage {
        properties.insert(
            keys.key(USAGE_KEY)?,
            vec![PropertyValue::Code(usage.clone())],
        );
    }
    if let Some(valid) = class.valid {
        properties.insert(keys.key(VALID_KEY)?, vec![PropertyValue::Boolean(valid)]);
    }
    Ok(properties)
}

/// Writes the numbered classes as the artifacts under `out`.
fn write(
    classification: &Classification,
    numbered: &Numbered<'_>,
    system: &str,
    version: String,
    out: &Path,
) -> Result<Report, artifact::Error> {
    std::fs::create_dir_all(out).map_err(artifact::io_error(out))?;
    let store_path = out.join(STORE_FILE);
    let mut builder = artifact::open_store(
        &store_path,
        system,
        &version,
        &classification.designation_kinds,
    )?;
    let keys = PropertyKeys::write(&mut builder, &property_keys(classification))?;
    let mut placed: Vec<Placed> = Vec::new();
    let mut languages: BTreeSet<String> = BTreeSet::new();
    for (code, class) in &numbered.classes {
        let Some(&ordinal) = numbered.ordinals.get(code) else {
            continue;
        };
        write_class(
            &mut builder,
            class,
            ordinal,
            &classification.designation_kinds,
            &keys,
            &mut placed,
            &mut languages,
        )?;
    }
    artifact::write_designations(&mut builder, &placed)?;
    artifact::write_hierarchy(out, numbered.classes.len(), numbered.edges.iter().copied())?;
    let words = artifact::write_text_index(out, &placed, LanguageKey::PrimarySubtag)?;
    artifact::finish(builder)?;
    artifact::write_manifest(
        out,
        &json!({
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
            "concepts": numbered.classes.len(),
            "designations": placed.len(),
            "words": words,
            "languages": languages,
        }),
    )?;
    Ok(Report {
        system: system.to_owned(),
        version,
        store: store_path,
        concepts: u64::try_from(numbered.classes.len()).unwrap_or(u64::MAX),
        designations: u64::try_from(placed.len()).unwrap_or(u64::MAX),
        words: u64::try_from(words).unwrap_or(u64::MAX),
    })
}

/// Writes one class with its properties, placing its designations.
fn write_class(
    builder: &mut StoreBuilder,
    class: &Class,
    ordinal: Ordinal,
    designation_kinds: &[String],
    keys: &PropertyKeys,
    placed: &mut Vec<Placed>,
    languages: &mut BTreeSet<String>,
) -> Result<(), artifact::Error> {
    builder.concept(
        ordinal,
        &Concept {
            code: class.code.clone(),
            active: class.active,
            effective_time: None,
            module: None,
        },
    )?;
    let mut properties = class_properties(class, keys)?;
    class_rubrics(
        class,
        ordinal,
        designation_kinds,
        keys,
        placed,
        languages,
        &mut properties,
    )?;
    for (key, values) in &properties {
        builder.properties(ordinal, *key, values)?;
    }
    Ok(())
}
