// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! EMA SPOR master data: the referential lists (RMS), the organisations
//! (OMS), the substances (SMS), and the medicinal products (PMS).
//!
//! The EMA distributes SPOR through its API behind a registered account. The
//! SPOR API v1 serves RMS terms as XML (`controlled-term` with `term-id`,
//! `term-names`, `status`, and the `list-reference` of its list) and OMS
//! organisations as JSON (`organisation-id`, `name`, `status`), as the EMA's
//! SPOR API access guide shows (<https://www.ema.europa.eu/cs/media/49047>).
//! The SPOR API v2 serves SMS and PMS as FHIR R5 `Bundle`s of
//! `SubstanceDefinition` and `MedicinalProductDefinition` (EMA/567640/2018,
//! <https://www.ema.europa.eu/en/documents/other/spor-api-v2-specification_en.pdf>).
//! Saved responses are the release; one build reads one kind, and RMS terms
//! of one list, since each list is its own code system.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::xml::records;
use crate::{Class, Classification, PREFERRED, Rubric, collapse};

/// The system of an RMS list: this prefix and the list id (HL7 Europe UNICOM
/// aliases, `https://spor.ema.europa.eu/v1/lists/200000000004`).
pub const LISTS: &str = "https://spor.ema.europa.eu/v1/lists/";
/// The system of OMS organisation ids (`ORG-100013412`).
pub const ORGANISATIONS: &str = "https://spor.ema.europa.eu/v1/organisations";
/// The system of SMS substance ids (the MyHealth@EU substance value set).
pub const SUBSTANCES: &str = "https://spor.ema.europa.eu/v2/SubstanceDefinition";
/// The system of PMS medicinal product identifiers (MPID, SPOR API v2 §5).
pub const PRODUCTS: &str = "http://ema.europa.eu/fhir/MPID";
/// The designation kind of a short name, other name, or further name.
pub const SYNONYM: &str = "synonym";
/// The rubric kind of the record status.
pub const STATUS: &str = "status";

/// A failure to read the saved responses.
#[derive(Debug, thiserror::Error)]
pub enum SporError {
    /// A file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// An XML response does not parse.
    #[error("{path} does not parse as XML")]
    Xml {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: quick_xml::Error,
    },
    /// A JSON response does not parse.
    #[error("{path} does not parse as JSON")]
    Json {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: serde_json::Error,
    },
    /// A file holds no SPOR record this reader knows.
    #[error("{0} holds no RMS term, OMS organisation, SMS substance, or PMS product")]
    Unknown(PathBuf),
    /// The files hold records of two code systems.
    #[error("the files mix `{first}` and `{second}`; build each list or kind on its own")]
    Mixed {
        /// The system read first.
        first: String,
        /// The other system.
        second: String,
    },
    /// No version was given and no record carries a modification date.
    #[error("the records carry no modification date; pass a version")]
    NoVersion,
}

/// What a set of files read to: the system they belong to and the classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    /// The code system the records belong to.
    pub system: String,
    /// The records.
    pub classification: Classification,
}

/// The records of one file, with their system, title, and newest date.
struct Batch {
    system: String,
    title: String,
    classes: Vec<Class>,
    newest: Option<String>,
}

fn newer(newest: &mut Option<String>, date: Option<&str>) {
    if let Some(date) = date.and_then(|d| d.get(..10))
        && newest.as_deref().is_none_or(|n| date > n)
    {
        *newest = Some(date.to_owned());
    }
}

/// RMS terms (SPOR API v1 XML).
fn rms(text: &str, path: &Path) -> Result<Option<Batch>, SporError> {
    let mut batch: Option<Batch> = None;
    let mut mixed = None;
    records::<quick_xml::Error>(text, "controlled-term", |term| {
        let Some(code) = term.child("term-id").and_then(|t| t.attr("id")) else {
            return Ok(());
        };
        let list = term.child("list-reference");
        let system = format!(
            "{LISTS}{}",
            list.and_then(|l| l.attr("id")).unwrap_or_default()
        );
        let b = batch.get_or_insert_with(|| Batch {
            system: system.clone(),
            title: list
                .and_then(|l| l.attr("name"))
                .unwrap_or("RMS list")
                .to_owned(),
            classes: Vec::new(),
            newest: None,
        });
        if b.system != system {
            mixed.get_or_insert((b.system.clone(), system));
            return Ok(());
        }
        let mut rubrics = Vec::new();
        for (group, item, kind) in [
            ("term-names", "term-name", PREFERRED),
            ("short-names", "short-name", SYNONYM),
            ("other-names", "other-name", SYNONYM),
        ] {
            for name in term
                .child(group)
                .into_iter()
                .flat_map(|g| g.all(item))
                .filter_map(|n| n.child("name"))
            {
                rubrics.push(Rubric {
                    kind: kind.to_owned(),
                    language: name.attr("lang").unwrap_or("en").to_lowercase(),
                    text: collapse(&name.text),
                });
            }
        }
        let status = term.child("status").and_then(|s| s.attr("code"));
        rubrics.extend(status.map(|s| Rubric::english(STATUS, s)));
        newer(
            &mut b.newest,
            term.child("operational-attributes")
                .and_then(|o| o.attr("modified-on")),
        );
        b.classes.push(Class {
            code: code.to_owned(),
            kind: String::from("term"),
            active: status.is_none_or(|s| s.eq_ignore_ascii_case("current")),
            rubrics,
            ..Class::default()
        });
        Ok(())
    })
    .map_err(|source| SporError::Xml {
        path: path.to_path_buf(),
        source,
    })?;
    if let Some((first, second)) = mixed {
        return Err(SporError::Mixed { first, second });
    }
    Ok(batch)
}

/// The value at the JSON pointer `pointer` in `value`, `Null` when absent.
fn at<'a>(value: &'a Value, pointer: &str) -> &'a Value {
    value.pointer(pointer).unwrap_or(&Value::Null)
}

/// Every object in `value` (itself included) that has the member `key`.
fn objects_with<'a>(value: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            if map.contains_key(key) {
                out.push(value);
            }
            for child in map.values() {
                objects_with(child, key, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                objects_with(item, key, out);
            }
        }
        _ => {}
    }
}

/// OMS organisations (SPOR API v1 JSON).
fn oms(json: &Value) -> Option<Batch> {
    let mut found = Vec::new();
    objects_with(json, "organisation-id", &mut found);
    let mut classes = Vec::new();
    let mut newest = None;
    for org in found {
        let (Some(code), Some(name)) = (
            at(org, "/organisation-id/id").as_str(),
            at(org, "/name").as_str(),
        ) else {
            continue;
        };
        let status = at(org, "/status").as_str();
        newer(
            &mut newest,
            at(org, "/operational-attributes/modified-on").as_str(),
        );
        let mut rubrics = vec![Rubric::english(PREFERRED, name)];
        rubrics.extend(status.map(|s| Rubric::english(STATUS, s)));
        classes.push(Class {
            code: code.to_owned(),
            kind: String::from("organisation"),
            active: status.is_none_or(|s| s.eq_ignore_ascii_case("active")),
            rubrics,
            ..Class::default()
        });
    }
    (!classes.is_empty()).then(|| Batch {
        system: ORGANISATIONS.to_owned(),
        title: String::from("SPOR OMS organisations"),
        classes,
        newest,
    })
}

/// The FHIR resources of `resource_type` in a `Bundle` (or one bare resource).
fn resources<'a>(json: &'a Value, resource_type: &str) -> Vec<&'a Value> {
    let entries = at(json, "/entry").as_array().map_or_else(
        || vec![json],
        |entries| entries.iter().map(|e| at(e, "/resource")).collect(),
    );
    entries
        .into_iter()
        .filter(|r| at(r, "/resourceType") == resource_type)
        .collect()
}

/// Whether a FHIR `status` `CodeableConcept` says current; an absent status is current.
fn fhir_current(status: &Value) -> bool {
    if status.is_null() {
        return true;
    }
    let mut texts: Vec<&str> = at(status, "/text").as_str().into_iter().collect();
    for coding in at(status, "/coding").as_array().into_iter().flatten() {
        texts.extend(at(coding, "/display").as_str());
        texts.extend(at(coding, "/code").as_str());
    }
    texts.iter().any(|t| t.eq_ignore_ascii_case("current"))
}

/// SMS substances or PMS products (SPOR API v2 FHIR).
fn fhir(json: &Value) -> Option<Batch> {
    let substances = resources(json, "SubstanceDefinition");
    let (kind, system, title, found) = if substances.is_empty() {
        (
            "medicinal-product",
            PRODUCTS,
            "SPOR PMS medicinal products",
            resources(json, "MedicinalProductDefinition"),
        )
    } else {
        ("substance", SUBSTANCES, "SPOR SMS substances", substances)
    };
    let mut classes = Vec::new();
    let mut newest = None;
    for resource in found {
        let identifiers: Vec<&Value> = at(resource, "/identifier")
            .as_array()
            .into_iter()
            .flatten()
            .collect();
        let code = if kind == "substance" {
            identifiers
                .iter()
                .filter_map(|i| at(i, "/value").as_str())
                .find(|v| v.chars().all(|c| c.is_ascii_digit()))
                .or_else(|| identifiers.first().and_then(|i| at(i, "/value").as_str()))
        } else {
            identifiers
                .iter()
                .find(|i| {
                    at(i, "/system")
                        .as_str()
                        .is_some_and(|s| s.eq_ignore_ascii_case(PRODUCTS))
                })
                .and_then(|i| at(i, "/value").as_str())
        };
        let Some(code) = code else {
            continue;
        };
        let names: Vec<&str> = at(resource, "/name")
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| {
                at(n, "/name")
                    .as_str()
                    .or_else(|| at(n, "/productName").as_str())
            })
            .collect();
        let preferred = at(resource, "/name")
            .as_array()
            .into_iter()
            .flatten()
            .position(|n| at(n, "/preferred") == true)
            .unwrap_or(0);
        let mut rubrics = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let kind = if i == preferred { PREFERRED } else { SYNONYM };
            rubrics.push(Rubric::english(kind, name));
        }
        newer(&mut newest, at(resource, "/meta/lastUpdated").as_str());
        classes.push(Class {
            code: code.to_owned(),
            kind: kind.to_owned(),
            active: fhir_current(at(resource, "/status")),
            rubrics,
            ..Class::default()
        });
    }
    (!classes.is_empty()).then(|| Batch {
        system: system.to_owned(),
        title: title.to_owned(),
        classes,
        newest,
    })
}

fn read_file(path: &Path) -> Result<Batch, SporError> {
    let text = std::fs::read_to_string(path).map_err(|source| SporError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let unknown = || SporError::Unknown(path.to_path_buf());
    if text.trim_start().starts_with('<') {
        return rms(&text, path)?.ok_or_else(unknown);
    }
    let json: Value = serde_json::from_str(&text).map_err(|source| SporError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    oms(&json).or_else(|| fhir(&json)).ok_or_else(unknown)
}

/// Reads saved SPOR API responses of one kind (and, for RMS, one list); a
/// code met twice keeps its first reading. The version is `version`, or the
/// newest modification date among the records.
///
/// # Errors
///
/// Returns [`SporError`] when a file does not read or parse, holds no SPOR
/// record, the files mix code systems, or no version is known.
pub fn read(paths: &[&Path], version: Option<&str>) -> Result<Read, SporError> {
    let mut merged: Option<Batch> = None;
    for path in paths {
        let batch = read_file(path)?;
        match &mut merged {
            None => merged = Some(batch),
            Some(m) if m.system != batch.system => {
                return Err(SporError::Mixed {
                    first: m.system.clone(),
                    second: batch.system,
                });
            }
            Some(m) => {
                newer(&mut m.newest, batch.newest.as_deref());
                m.classes.extend(batch.classes);
            }
        }
    }
    let Some(mut batch) = merged else {
        return Err(SporError::NoVersion);
    };
    let mut seen = std::collections::BTreeSet::new();
    batch.classes.retain(|c| seen.insert(c.code.clone()));
    let version = version
        .map(str::to_owned)
        .or(batch.newest)
        .ok_or(SporError::NoVersion)?;
    let kinds: std::collections::BTreeSet<String> =
        batch.classes.iter().map(|c| c.kind.clone()).collect();
    Ok(Read {
        system: batch.system,
        classification: Classification {
            name: String::from("SPOR"),
            title: batch.title,
            version: Some(version),
            language: String::from("en"),
            kinds: kinds.into_iter().collect(),
            classes: batch.classes,
            designation_kinds: vec![PREFERRED.to_owned(), SYNONYM.to_owned()],
            hierarchy: None,
        },
    })
}
