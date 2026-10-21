// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading a cache the walker wrote back into entities.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::Linearization;
use crate::entity::{Entity, EntityError};

/// A failure to read the cache.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// A directory or file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// A file is not JSON.
    #[error("{path} is not JSON")]
    Json {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: serde_json::Error,
    },
    /// An entity file names no entity.
    #[error("{path}: {source}")]
    Entity {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: EntityError,
    },
    /// The cache holds no language directory for the linearization.
    #[error("no `{name}/<language>/_root.json` under {cache}")]
    NoRoot {
        /// The cache directory.
        cache: PathBuf,
        /// The linearization name.
        name: &'static str,
    },
}

/// One code system as cached: its root document and its entities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cached {
    /// The code system.
    pub linearization: Linearization,
    /// The release id (`2026-01`); the Foundation's root names none.
    pub release: Option<String>,
    /// The release date (`2026-01-17`), when the root states it.
    pub release_date: Option<String>,
    /// The title per language.
    pub titles: BTreeMap<String, String>,
    /// The languages cached, sorted.
    pub languages: Vec<String>,
    /// The entities by id.
    pub entities: BTreeMap<String, Entity>,
}

fn read_json(path: &Path) -> Result<Value, CacheError> {
    let text = std::fs::read_to_string(path).map_err(|source| CacheError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| CacheError::Json {
        path: path.to_path_buf(),
        source,
    })
}

/// Reads every entity file of one language directory into `entities`, in file
/// name order; an entity another language already read takes the new language.
fn read_entities(
    language_dir: &Path,
    linearization: Linearization,
    entities: &mut BTreeMap<String, Entity>,
) -> Result<(), CacheError> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(language_dir)
        .map_err(|source| CacheError::Io {
            path: language_dir.to_path_buf(),
            source,
        })?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json")
                && p.file_name().is_some_and(|n| n != "_root.json")
        })
        .collect();
    files.sort();
    for path in files {
        let json = read_json(&path)?;
        let entity = Entity::parse(&json, linearization).map_err(|source| CacheError::Entity {
            path: path.clone(),
            source,
        })?;
        match entities.get_mut(&entity.id) {
            Some(existing) => existing.merge_language(entity),
            None => {
                entities.insert(entity.id.clone(), entity);
            }
        }
    }
    Ok(())
}

/// Reads `linearization` from `cache`.
///
/// # Errors
///
/// Returns [`CacheError`] when the cache does not read, a file is not JSON,
/// or a file names no entity.
pub fn read(cache: &Path, linearization: Linearization) -> Result<Cached, CacheError> {
    let dir = cache.join(linearization.name());
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| CacheError::Io { path, source }
    };
    let mut languages: Vec<String> = std::fs::read_dir(&dir)
        .map_err(io(&dir))?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .collect();
    languages.sort();
    let mut release = None;
    let mut release_date = None;
    let mut titles = BTreeMap::new();
    let mut entities: BTreeMap<String, Entity> = BTreeMap::new();
    for language in &languages {
        let language_dir = dir.join(language);
        let root_path = language_dir.join("_root.json");
        if !root_path.exists() {
            continue;
        }
        let root = read_json(&root_path)?;
        if let Some(id) = root.get("releaseId").and_then(Value::as_str) {
            release = Some(id.to_owned());
        }
        if let Some(date) = root.get("releaseDate").and_then(Value::as_str) {
            release_date = Some(date.to_owned());
        }
        if let Some(title) = root
            .get("title")
            .and_then(|t| t.get("@value"))
            .and_then(Value::as_str)
        {
            titles.insert(language.clone(), title.to_owned());
        }
        read_entities(&language_dir, linearization, &mut entities)?;
    }
    if titles.is_empty() {
        return Err(CacheError::NoRoot {
            cache: cache.to_path_buf(),
            name: linearization.name(),
        });
    }
    Ok(Cached {
        linearization,
        release,
        release_date,
        titles,
        languages,
        entities,
    })
}
