// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A store of canonical resources by `url` and `version`, with the
//! default-version rule the value sets and concept maps share.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A resource identified by a canonical `url` and an optional `version`.
pub trait Versioned {
    /// `url`.
    fn url(&self) -> &str;
    /// `version`.
    fn version(&self) -> Option<&str>;

    /// The canonical `url|version`, or `url` alone.
    fn canonical(&self) -> String {
        match self.version() {
            Some(version) => format!("{}|{version}", self.url()),
            None => self.url().to_owned(),
        }
    }
}

/// Two resources with the same `url` and `version`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{canonical}` is already stored")]
pub struct Duplicate {
    /// The `url|version`.
    pub canonical: String,
}

/// The resources by `url`, then `version` (`""` for none).
#[derive(Debug)]
pub struct VersionedStore<T> {
    by_url: BTreeMap<String, BTreeMap<String, Arc<T>>>,
    /// The store under this one: a `url` and `version` this store does not
    /// hold resolves there. The FHIR core terminology of the served version is what
    /// a server puts there, so cloning this store for one request costs the
    /// deployment's own resources alone.
    beneath: Option<Arc<Self>>,
}

impl<T> Default for VersionedStore<T> {
    fn default() -> Self {
        Self {
            by_url: BTreeMap::new(),
            beneath: None,
        }
    }
}

impl<T> Clone for VersionedStore<T> {
    fn clone(&self) -> Self {
        Self {
            by_url: self.by_url.clone(),
            beneath: self.beneath.clone(),
        }
    }
}

impl<T: Versioned> VersionedStore<T> {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// This store with `beneath` under it.
    ///
    /// The layers resolve per `url` and `version`: a version this store holds
    /// answers from it, every other version of the same `url` still answers
    /// from beneath, and the default is the greatest version of both, as if
    /// they were one store ([`Self::resolve`]). A version-specific canonical
    /// names one resource and a version-less one the latest
    /// (<https://hl7.org/fhir/R4B/references.html#canonical>); how a server
    /// layers its content is our own design, no FHIR version governs it.
    #[must_use]
    pub fn with_beneath(mut self, beneath: Arc<Self>) -> Self {
        self.beneath = Some(beneath);
        self
    }

    /// The versions of `url` across every layer, an upper layer's version
    /// replacing the same version beneath.
    fn versions_of(&self, url: &str) -> Option<Cow<'_, BTreeMap<String, Arc<T>>>> {
        let mut layers = Vec::new();
        let mut layer = Some(self);
        while let Some(current) = layer {
            if let Some(versions) = current.by_url.get(url) {
                layers.push(versions);
            }
            layer = current.beneath.as_deref();
        }
        match layers.as_slice() {
            [] => None,
            [only] => Some(Cow::Borrowed(*only)),
            _ => {
                let mut merged = BTreeMap::new();
                for versions in layers.iter().rev() {
                    merged.extend(
                        versions
                            .iter()
                            .map(|(version, resource)| (version.clone(), Arc::clone(resource))),
                    );
                }
                Some(Cow::Owned(merged))
            }
        }
    }

    /// Stores `resource`.
    ///
    /// # Errors
    ///
    /// Returns [`Duplicate`] when the same `url` and `version` is stored.
    pub fn insert(&mut self, resource: T) -> Result<(), Duplicate> {
        let versions = self.by_url.entry(resource.url().to_owned()).or_default();
        let key = resource.version().unwrap_or_default().to_owned();
        if versions.contains_key(&key) {
            return Err(Duplicate {
                canonical: resource.canonical(),
            });
        }
        versions.insert(key, Arc::new(resource));
        Ok(())
    }

    /// Stores `resource`, replacing the one with the same `url` and `version`.
    pub fn replace(&mut self, resource: T) {
        let key = resource.version().unwrap_or_default().to_owned();
        self.by_url
            .entry(resource.url().to_owned())
            .or_default()
            .insert(key, Arc::new(resource));
    }

    /// The resource at `url`, at `version` or the default version.
    ///
    /// `url` may carry its version as `url|version`. No FHIR version fixes
    /// which version is the default: the greatest, by numeric-aware
    /// comparison of dot-separated segments, is our own design.
    #[must_use]
    pub fn resolve(&self, url: &str, version: Option<&str>) -> Option<Arc<T>> {
        let (url, embedded) = match url.split_once('|') {
            Some((url, version)) => (url, Some(version)),
            None => (url, None),
        };
        let versions = self.versions_of(url)?;
        match version.or(embedded) {
            Some(wanted) => select_version(versions.keys().map(String::as_str), wanted)
                .and_then(|v| versions.get(v))
                .cloned(),
            // NOTE: a version-less canonical names the latest (<https://hl7.org/fhir/R4B/references.html#canonical>);
            // no spec governs the default among layers, our own design: the greatest of all of them.
            None => versions
                .iter()
                .max_by(|(a, _), (b, _)| version_order(a, b))
                .map(|(_, resource)| Arc::clone(resource)),
        }
    }

    /// This store with `overlay`'s resources on top; an overlay version wins.
    #[must_use]
    pub fn layered(&self, overlay: &Self) -> Self {
        let mut merged = self.clone();
        for (url, versions) in &overlay.by_url {
            let target = merged.by_url.entry(url.clone()).or_default();
            for (version, resource) in versions {
                target.insert(version.clone(), Arc::clone(resource));
            }
        }
        merged
    }

    /// Every resource stored here, by `url` then `version`.
    ///
    /// The store beneath is left out: these are the resources this deployment
    /// published, which is what the REST endpoints enumerate.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<T>> {
        self.by_url.values().flat_map(BTreeMap::values)
    }

    /// The number of resources stored here, the store beneath excluded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_url.values().map(BTreeMap::len).sum()
    }

    /// Whether nothing is stored here, the store beneath excluded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_url.is_empty()
    }
}

/// Whether `pattern` names `candidate`: equal segment by segment, an `x`
/// segment standing for any value.
///
/// `1.0.x` names every `1.0.*`, `1.x.x` every `1.*.*`, and a pattern without
/// `x` names itself alone; a bare `1` is no pattern and names only `1`. No
/// FHIR version defines this form; the terminology ecosystem test cases and
/// the reference server do (spec-silent, recorded on #174).
#[must_use]
pub fn version_matches(pattern: &str, candidate: &str) -> bool {
    let mut wanted = pattern.split('.');
    let mut have = candidate.split('.');
    loop {
        match (wanted.next(), have.next()) {
            (None, None) => return true,
            (None, Some(_)) => return false,
            (Some(p), None) => {
                if p != "x" {
                    return false;
                }
            }
            (Some("x"), Some(_)) => {}
            (Some(p), Some(c)) => {
                let equal = match (p.parse::<u64>(), c.parse::<u64>()) {
                    (Ok(p), Ok(c)) => p == c,
                    _ => p == c,
                };
                if !equal {
                    return false;
                }
            }
        }
    }
}

/// The version among `versions` that `wanted` names: itself when present,
/// else the greatest one a pattern matches.
#[must_use]
pub fn select_version<'a>(
    versions: impl Iterator<Item = &'a str>,
    wanted: &str,
) -> Option<&'a str> {
    let mut best: Option<&'a str> = None;
    for version in versions {
        if version == wanted {
            return Some(version);
        }
        if version_matches(wanted, version)
            && best.is_none_or(|current| version_order(version, current) == Ordering::Greater)
        {
            best = Some(version);
        }
    }
    best
}

/// Orders version strings segment by segment, numerically where both
/// segments are numbers, else lexically.
#[must_use]
pub fn version_order(a: &str, b: &str) -> Ordering {
    let mut left = a.split('.');
    let mut right = b.split('.');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let order = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    _ => x.cmp(y),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::{select_version, version_matches, version_order};

    #[test]
    fn x_segments_match_any_value_and_a_bare_prefix_matches_nothing_more() {
        assert!(version_matches("1.0.x", "1.0.0"));
        assert!(version_matches("1.0.x", "1.0.7"));
        assert!(!version_matches("1.0.x", "1.2.0"));
        assert!(version_matches("1.x.x", "1.2.0"));
        assert!(!version_matches("1.x.x", "2.0.0"));
        assert!(version_matches("1.2.0", "1.2.0"));
        assert!(!version_matches("1", "1.0.0"));
        assert!(version_matches("1.x", "1.0"));
        assert!(
            version_matches("1.0.x", "1.0"),
            "a trailing x may stand for an absent segment"
        );
    }

    #[test]
    fn a_pattern_selects_the_greatest_match_and_a_version_selects_itself() {
        let versions = ["1.0.0", "1.2.0", "2.0.0", "1.10.0"];
        assert_eq!(
            select_version(versions.into_iter(), "1.x.x"),
            Some("1.10.0")
        );
        assert_eq!(select_version(versions.into_iter(), "1.0.x"), Some("1.0.0"));
        assert_eq!(select_version(versions.into_iter(), "1.2.0"), Some("1.2.0"));
        assert_eq!(select_version(versions.into_iter(), "1"), None);
        assert_eq!(select_version(versions.into_iter(), "3.x.x"), None);
    }

    #[test]
    fn versions_order_numerically_by_segment() {
        assert_eq!(version_order("1.10.0", "1.9.0"), Ordering::Greater);
        assert_eq!(version_order("2.0", "2.0.1"), Ordering::Less);
        assert_eq!(version_order("2024", "2025"), Ordering::Less);
        assert_eq!(version_order("b", "a"), Ordering::Greater);
        assert_eq!(version_order("", "1"), Ordering::Less);
    }
}
