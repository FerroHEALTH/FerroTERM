// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Orphanet nomenclature of rare diseases (the `ORPHAcode` system).
//!
//! The source is the Orphadata nomenclature file `<lang>_product1.xml`
//! (<https://www.orphadata.com/orphanet-nomenclature/>), one per language:
//! a `JDBOR` document whose `DisorderList` holds one `Disorder` per entity
//! with its `OrphaCode`, `Name`, `SynonymList`, `DisorderType`,
//! `DisorderGroup`, and `DisorderFlagList`. An entity flagged `Inactive`
//! (obsolete, deprecated, or moved) is served inactive. The nomenclature is
//! read as a flat list; the classification trees are separate files.

use std::path::{Path, PathBuf};

use crate::xml::{Element, first, records};
use crate::{Class, Classification, PREFERRED, Rubric, collapse};

/// The system URI (HL7 Terminology, `NamingSystem-orphanet`).
pub const SYSTEM: &str = "https://www.orpha.net";
/// The designation kind of a synonym.
pub const SYNONYM: &str = "synonym";
/// The rubric kind of the disorder type (`Disease`, `Clinical subtype`).
pub const TYPE: &str = "type";
/// The rubric kind of the disorder group (`Disorder`, `Group of disorders`).
pub const GROUP: &str = "group";
/// The rubric kind of each flag label (`Obsolete entity`).
pub const FLAG: &str = "flag";
/// The class kind of every entity.
pub const KIND: &str = "clinical-entity";
/// The flag label that marks an entity inactive.
const INACTIVE: &str = "Inactive";

/// A failure to read the nomenclature.
#[derive(Debug, thiserror::Error)]
pub enum OrphaError {
    /// A file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// A file is not well-formed XML.
    #[error("{path} does not parse")]
    Xml {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: quick_xml::Error,
    },
    /// A file holds no `Disorder`.
    #[error("{0} is not an Orphadata nomenclature file")]
    NotOrphadata(PathBuf),
    /// No version was given and the first file carries no `JDBOR` date.
    #[error("the nomenclature names no date; pass a version")]
    NoVersion,
}

/// One file read: its language, its `JDBOR` date, and its disorders.
struct Product {
    language: String,
    date: Option<String>,
    disorders: Vec<Element>,
}

fn read_product(path: &Path) -> Result<Product, OrphaError> {
    let text = std::fs::read_to_string(path).map_err(|source| OrphaError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let xml_error = |source| OrphaError::Xml {
        path: path.to_path_buf(),
        source,
    };
    let date = first(&text, "JDBOR").map_err(xml_error)?.and_then(|root| {
        root.attr("date")
            .and_then(|d| d.get(..10))
            .map(str::to_owned)
    });
    let mut disorders = Vec::new();
    records::<quick_xml::Error>(&text, "Disorder", |d| {
        if d.child("OrphaCode").is_some() {
            disorders.push(d);
        }
        Ok(())
    })
    .map_err(xml_error)?;
    if disorders.is_empty() {
        return Err(OrphaError::NotOrphadata(path.to_path_buf()));
    }
    let language = disorders
        .first()
        .and_then(|d| d.child("Name"))
        .and_then(|n| n.attr("lang"))
        .unwrap_or("en")
        .to_owned();
    Ok(Product {
        language,
        date,
        disorders,
    })
}

/// Reads one or more `<lang>_product1.xml` files; the first file's language
/// is the display language and every other file adds designations.
///
/// The version is `version`, or the date of the first file's `JDBOR`
/// element (`2026-06-23`).
///
/// # Errors
///
/// Returns [`OrphaError`] when a file does not read or parse, holds no
/// disorder, or no version is known.
pub fn read(paths: &[&Path], version: Option<&str>) -> Result<Classification, OrphaError> {
    let mut classes: Vec<Class> = Vec::new();
    let mut by_code: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut first: Option<(String, Option<String>)> = None;
    for path in paths {
        let product = read_product(path)?;
        let primary = first.is_none();
        if primary {
            first = Some((product.language.clone(), product.date.clone()));
        }
        for disorder in &product.disorders {
            let Some(code) = disorder.text_at("OrphaCode") else {
                continue;
            };
            let index = if let Some(&i) = by_code.get(code) {
                i
            } else {
                classes.push(Class {
                    code: code.to_owned(),
                    kind: KIND.to_owned(),
                    ..Class::default()
                });
                let i = classes.len().saturating_sub(1);
                by_code.insert(code.to_owned(), i);
                i
            };
            let Some(class) = classes.get_mut(index) else {
                continue;
            };
            let text = |kind: &str, text: &str| Rubric {
                kind: kind.to_owned(),
                language: product.language.clone(),
                text: collapse(text),
            };
            if let Some(name) = disorder.text_at("Name") {
                class.rubrics.push(text(PREFERRED, name));
            }
            if let Some(list) = disorder.child("SynonymList") {
                for synonym in list.all("Synonym") {
                    class.rubrics.push(text(SYNONYM, &synonym.text));
                }
            }
            if !primary {
                continue;
            }
            for (kind, path) in [(TYPE, "DisorderType/Name"), (GROUP, "DisorderGroup/Name")] {
                if let Some(value) = disorder.text_at(path) {
                    class.rubrics.push(text(kind, value));
                }
            }
            if let Some(flags) = disorder.child("DisorderFlagList") {
                for label in flags.all("DisorderFlag").filter_map(|f| f.text_at("Label")) {
                    if label == INACTIVE {
                        class.active = false;
                    }
                    class.rubrics.push(text(FLAG, label));
                }
            }
        }
    }
    let (language, date) = first.unwrap_or_default();
    let version = version
        .map(str::to_owned)
        .or(date)
        .ok_or(OrphaError::NoVersion)?;
    Ok(Classification {
        name: String::from("ORPHAcode"),
        title: String::from("Orphanet nomenclature of rare diseases"),
        version: Some(version),
        language,
        kinds: vec![KIND.to_owned()],
        classes,
        designation_kinds: vec![PREFERRED.to_owned(), SYNONYM.to_owned()],
        hierarchy: None,
    })
}
