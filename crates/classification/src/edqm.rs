// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! EDQM Standard Terms (dose forms, routes, containers, and their companions).
//!
//! The EDQM distributes the database through its Standard Terms API
//! (<https://standardterms.edqm.eu>, "Help API"), behind a free account. The
//! `full_data_by_class` responses are JSON objects whose `content` array
//! holds one term each: `class` (`PDF`, `ROA`, ...), `code`, `english`,
//! `status`, `domain`, `definition`, `comment`, `modification_date`, the
//! `translations` (`language`, `term`), and the `links` to terms of other
//! classes. Saved responses are the release; the database names no release
//! number, so the version is the newest modification date unless one is
//! given. The terms form a flat list across classes.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{Class, Classification, PREFERRED, Rubric, collapse};

/// The system URI used for EDQM Standard Terms in the HL7 Europe and IPS
/// guides (`http://standardterms.edqm.eu`).
pub const SYSTEM: &str = "http://standardterms.edqm.eu";
/// The rubric kinds of the term's facts kept as properties.
pub const CLASS: &str = "class";
/// The rubric kind of the domain (`Human and Veterinary`).
pub const DOMAIN: &str = "domain";
/// The rubric kind of the status (`Current`, `Deprecated`).
pub const STATUS: &str = "status";
/// The rubric kind of the definition.
pub const DEFINITION: &str = "definition";
/// The rubric kind of the comment.
pub const COMMENT: &str = "comment";
/// The prefix of a link's rubric kind, followed by the linked class in lower case (`link-bdf`).
pub const LINK: &str = "link-";

/// A failure to read the saved responses.
#[derive(Debug, thiserror::Error)]
pub enum EdqmError {
    /// A file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// A file is not an API response.
    #[error("{path} is not a Standard Terms API response")]
    Json {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: serde_json::Error,
    },
    /// No version was given and no term carries a modification date.
    #[error("the terms carry no modification date; pass a version")]
    NoVersion,
}

#[derive(Debug, Deserialize)]
struct Response {
    content: Vec<Term>,
}

#[derive(Debug, Deserialize)]
struct Term {
    #[serde(rename = "class")]
    class: Option<String>,
    code: String,
    english: Option<String>,
    status: Option<String>,
    domain: Option<String>,
    definition: Option<String>,
    comment: Option<String>,
    modification_date: Option<String>,
    #[serde(default)]
    translations: Vec<Translation>,
    #[serde(default)]
    links: Option<std::collections::BTreeMap<String, Vec<Link>>>,
}

#[derive(Debug, Deserialize)]
struct Translation {
    language: String,
    term: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Link {
    code: String,
}

/// Whether a status names a current term (`Current`, or its short form `C`).
fn current(status: Option<&str>) -> bool {
    status.is_none_or(|s| s.eq_ignore_ascii_case("current") || s.eq_ignore_ascii_case("c"))
}

/// Reads saved Standard Terms API responses; a code met twice keeps its
/// first reading.
///
/// # Errors
///
/// Returns [`EdqmError`] when a file does not read or parse, or no version is known.
pub fn read(paths: &[&Path], version: Option<&str>) -> Result<Classification, EdqmError> {
    let mut classes: Vec<Class> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut newest: Option<String> = None;
    let mut classes_seen = std::collections::BTreeSet::new();
    for path in paths {
        let text = std::fs::read_to_string(path).map_err(|source| EdqmError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let response: Response = serde_json::from_str(&text).map_err(|source| EdqmError::Json {
            path: path.to_path_buf(),
            source,
        })?;
        for term in response.content {
            if !seen.insert(term.code.clone()) {
                continue;
            }
            if let Some(date) = term.modification_date.as_deref().and_then(|d| d.get(..10))
                && newest.as_deref().is_none_or(|n| date > n)
            {
                newest = Some(date.to_owned());
            }
            let mut rubrics = Vec::new();
            rubrics.extend(
                term.english
                    .as_deref()
                    .map(|e| Rubric::english(PREFERRED, e)),
            );
            for translation in &term.translations {
                let Some(text) = translation.term.as_deref().filter(|t| !t.trim().is_empty())
                else {
                    continue;
                };
                rubrics.push(Rubric {
                    kind: PREFERRED.to_owned(),
                    language: translation.language.to_lowercase(),
                    text: collapse(text),
                });
            }
            for (kind, value) in [
                (CLASS, &term.class),
                (DOMAIN, &term.domain),
                (STATUS, &term.status),
                (DEFINITION, &term.definition),
                (COMMENT, &term.comment),
            ] {
                if let Some(value) = value.as_deref().filter(|v| !v.trim().is_empty()) {
                    rubrics.push(Rubric::english(kind, value));
                }
            }
            for (class, links) in term.links.iter().flatten() {
                for link in links {
                    rubrics.push(Rubric::english(
                        &format!("{LINK}{}", class.to_lowercase()),
                        &link.code,
                    ));
                }
            }
            if let Some(class) = &term.class {
                classes_seen.insert(class.clone());
            }
            classes.push(Class {
                kind: term.class.clone().unwrap_or_default(),
                active: current(term.status.as_deref()),
                code: term.code,
                rubrics,
                ..Class::default()
            });
        }
    }
    let version = version
        .map(str::to_owned)
        .or(newest)
        .ok_or(EdqmError::NoVersion)?;
    Ok(Classification {
        name: String::from("EDQM_Standard_Terms"),
        title: String::from("EDQM Standard Terms"),
        version: Some(version),
        language: String::from("en"),
        kinds: classes_seen.into_iter().collect(),
        classes,
        designation_kinds: vec![PREFERRED.to_owned()],
        hierarchy: None,
    })
}
