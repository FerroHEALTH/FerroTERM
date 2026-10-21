// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Walking a local deployment of the ICD-API into a cache of entity JSON.
//!
//! The deployment (`docker run -e acceptLicense=true -e include=2026-01_en
//! whoicd/icd-api`) needs no authentication and answers the same JSON as
//! `https://id.who.int/icd`, with the canonical `http://id.who.int` host in
//! every URI, so the walker rewrites that host to its base URL. A
//! linearization is enumerated from its root's children, each with
//! `?include=descendant`; the Foundation from its two roots the same way.
//! Every entity is then fetched once per language and written as
//! `<cache>/<linearization>/<language>/<id>.json`, the residual `/` spelled
//! `~`; the root document goes to `<cache>/<linearization>/<language>/_root.json`.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

use crate::{CANONICAL, Linearization};

/// A failure while fetching or caching.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The request failed or the response was not JSON.
    #[error("cannot fetch {url}")]
    Fetch {
        /// The URL.
        url: String,
        /// The cause.
        #[source]
        source: reqwest::Error,
    },
    /// The response was not a JSON document of the shape expected.
    #[error("{url} did not return the expected JSON")]
    Shape {
        /// The URL.
        url: String,
    },
    /// A cache file cannot be written.
    #[error("cannot write {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
}

/// A client for one deployment and release.
#[derive(Debug, Clone)]
pub struct Client {
    base: String,
    release: String,
    http: reqwest::blocking::Client,
}

/// What a download did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Downloaded {
    /// Entities fetched and written.
    pub fetched: usize,
    /// Entities already in the cache.
    pub cached: usize,
}

/// The cache file of an entity.
#[must_use]
pub fn entity_file(
    cache: &Path,
    linearization: Linearization,
    language: &str,
    id: &str,
) -> PathBuf {
    cache
        .join(linearization.name())
        .join(language)
        .join(format!("{}.json", id.replace('/', "~")))
}

/// The cache file of a linearization's root document.
#[must_use]
pub fn root_file(cache: &Path, linearization: Linearization, language: &str) -> PathBuf {
    cache
        .join(linearization.name())
        .join(language)
        .join("_root.json")
}

impl Client {
    /// A client for the deployment at `base` (`http://127.0.0.1:80`) and `release` (`2026-01`).
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::Fetch`] when the HTTP client cannot be built.
    pub fn new(base: &str, release: &str) -> Result<Self, ApiError> {
        let http = reqwest::blocking::Client::builder()
            .build()
            .map_err(|source| ApiError::Fetch {
                url: base.to_owned(),
                source,
            })?;
        Ok(Self {
            base: base.trim_end_matches('/').to_owned(),
            release: release.to_owned(),
            http,
        })
    }

    /// The release id.
    #[must_use]
    pub fn release(&self) -> &str {
        &self.release
    }

    fn local(&self, uri: &str) -> String {
        uri.replacen(CANONICAL, &self.base, 1)
    }

    /// Fetches the JSON at `url` (canonical or local) in `language`.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError`] when the request fails or the body is not JSON.
    pub fn get(&self, url: &str, language: &str) -> Result<Value, ApiError> {
        let local = self.local(url);
        let fetch = |source| ApiError::Fetch {
            url: local.clone(),
            source,
        };
        self.http
            .get(&local)
            .header("Accept", "application/json")
            .header("API-Version", "v2")
            .header("Accept-Language", language)
            .send()
            .map_err(fetch)?
            .error_for_status()
            .map_err(fetch)?
            .json()
            .map_err(fetch)
    }

    /// The root document of `linearization`.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError`] when the request fails.
    pub fn root(&self, linearization: Linearization, language: &str) -> Result<Value, ApiError> {
        let url = format!("{}{}", self.base, linearization.root_path(&self.release));
        self.get(&url, language)
    }

    /// Every entity id of `linearization`: the root's children and, through
    /// `?include=descendant`, everything under each.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError`] when a request fails or a document lacks its children.
    pub fn ids(&self, linearization: Linearization) -> Result<Vec<String>, ApiError> {
        let root = self.root(linearization, "en")?;
        let url = format!("{}{}", self.base, linearization.root_path(&self.release));
        let children = root
            .get("child")
            .and_then(Value::as_array)
            .ok_or_else(|| ApiError::Shape { url: url.clone() })?;
        let mut ids = Vec::new();
        for child in children.iter().filter_map(Value::as_str) {
            let Some(id) = linearization.id_of(child) else {
                continue;
            };
            ids.push(id);
            let subtree_url = format!("{}?include=descendant", self.local(child));
            let subtree = self.get(&subtree_url, "en")?;
            for uri in subtree
                .get("descendant")
                .and_then(Value::as_array)
                .ok_or_else(|| ApiError::Shape {
                    url: subtree_url.clone(),
                })?
                .iter()
                .filter_map(Value::as_str)
            {
                if let Some(id) = linearization.id_of(uri) {
                    ids.push(id);
                }
            }
        }
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// Writes the root document of `linearization` in `language` into `cache`,
    /// with the directory that holds the language, when it is not there yet.
    fn cache_root(
        &self,
        cache: &Path,
        linearization: Linearization,
        language: &str,
    ) -> Result<(), ApiError> {
        let dir = cache.join(linearization.name()).join(language);
        std::fs::create_dir_all(&dir).map_err(|source| ApiError::Io {
            path: dir.clone(),
            source,
        })?;
        let root_path = root_file(cache, linearization, language);
        if !root_path.exists() {
            let root = self.root(linearization, language)?;
            write_json(&root_path, &root)?;
        }
        Ok(())
    }

    /// The (language, id) pairs `cache` is missing, and how many it already holds.
    fn missing(
        &self,
        cache: &Path,
        linearization: Linearization,
        ids: &[String],
        languages: &[String],
    ) -> Result<(VecDeque<(String, String)>, usize), ApiError> {
        let mut queue: VecDeque<(String, String)> = VecDeque::new();
        let mut cached = 0usize;
        for language in languages {
            self.cache_root(cache, linearization, language)?;
            for id in ids {
                if entity_file(cache, linearization, language, id).exists() {
                    cached = cached.saturating_add(1);
                } else {
                    queue.push_back((language.clone(), id.clone()));
                }
            }
        }
        Ok((queue, cached))
    }

    /// Takes (language, id) pairs off `queue` and writes each entity into
    /// `cache`, until the queue empties or a worker records a failure.
    fn worker(
        &self,
        cache: &Path,
        linearization: Linearization,
        queue: &Mutex<VecDeque<(String, String)>>,
        failure: &Mutex<Option<ApiError>>,
    ) {
        loop {
            if failure.lock().is_ok_and(|f| f.is_some()) {
                return;
            }
            let Some((language, id)) = queue.lock().ok().and_then(|mut q| q.pop_front()) else {
                return;
            };
            let url = format!(
                "{}{}/{}",
                self.base,
                linearization.root_path(&self.release),
                id
            );
            let result = self.get(&url, &language).and_then(|json| {
                write_json(&entity_file(cache, linearization, &language, &id), &json)
            });
            if let Err(error) = result
                && let Ok(mut slot) = failure.lock()
                && slot.is_none()
            {
                *slot = Some(error);
            }
        }
    }

    /// Fetches every entity in `ids` in each of `languages` into `cache`,
    /// skipping the files already there, with `threads` workers.
    ///
    /// # Errors
    ///
    /// Returns the first [`ApiError`] a worker met.
    pub fn download(
        &self,
        cache: &Path,
        linearization: Linearization,
        ids: &[String],
        languages: &[String],
        threads: usize,
    ) -> Result<Downloaded, ApiError> {
        let (queue, cached) = self.missing(cache, linearization, ids, languages)?;
        let total = queue.len();
        let queue = Mutex::new(queue);
        let failure: Mutex<Option<ApiError>> = Mutex::new(None);
        std::thread::scope(|scope| {
            for _ in 0..threads.max(1) {
                scope.spawn(|| self.worker(cache, linearization, &queue, &failure));
            }
        });
        if let Some(error) = failure.into_inner().ok().flatten() {
            return Err(error);
        }
        Ok(Downloaded {
            fetched: total,
            cached,
        })
    }
}

fn write_json(path: &Path, json: &Value) -> Result<(), ApiError> {
    let io = |source| ApiError::Io {
        path: path.to_path_buf(),
        source,
    };
    let text = serde_json::to_string(json).map_err(|e| io(std::io::Error::other(e)))?;
    std::fs::write(path, text).map_err(io)
}
