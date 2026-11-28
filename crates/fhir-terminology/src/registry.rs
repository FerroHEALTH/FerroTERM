// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The loaded code system versions, by system URI, with the default-version rule.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::provider::compose::Compose;
use crate::provider::{CodeSystemProvider, error::ProviderError};

/// A failure to resolve a system or version.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    /// No provider serves the system.
    #[error("unknown code system `{0}`")]
    UnknownSystem(String),
    /// The system is served, this version is not.
    #[error("unknown version `{version}` of code system `{url}`")]
    UnknownVersion {
        /// The system.
        url: String,
        /// The version asked for.
        version: String,
        /// The versions the registry does hold for the system, in order.
        ///
        /// An answer that says a version is unknown and not which ones exist
        /// leaves the caller guessing; the ecosystem's own wording names them
        /// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
        available: Vec<String>,
    },
}

/// A failure to register a provider.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegisterError {
    /// The same system version is already registered.
    #[error("code system `{url}` version `{version}` is already registered")]
    Duplicate {
        /// The system.
        url: String,
        /// The version.
        version: String,
    },
}

/// A resolved provider and how its version was chosen.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// The provider.
    pub provider: Arc<dyn CodeSystemProvider>,
    /// Whether the version came from the default rule rather than the request.
    pub defaulted: bool,
}

#[derive(Debug, Default, Clone)]
struct System {
    versions: BTreeMap<String, Arc<dyn CodeSystemProvider>>,
    default: Option<String>,
}

impl System {
    /// The configured default, else the greatest version string.
    fn default_version(&self) -> Option<&str> {
        self.default
            .as_deref()
            .or_else(|| self.versions.keys().next_back().map(String::as_str))
    }
}

/// A loaded supplement a request named as if it were a code system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantSupplement {
    /// The supplement's version, when it states one.
    pub version: Option<String>,
}

/// The providers a server has loaded.
#[derive(Debug, Default, Clone)]
pub struct Registry {
    systems: BTreeMap<String, System>,
    /// The supplements loaded but dormant, by their own canonical, with the
    /// system canonical each supplements; a request applies them by name.
    supplements: BTreeMap<String, (String, crate::supplement::Supplement)>,
    /// The registry under this one: a system version this registry does not
    /// hold resolves there. The FHIR core terminology of the served version is
    /// what a server puts there, so cloning this registry for one request
    /// costs the deployment's own systems alone.
    beneath: Option<Arc<Registry>>,
}

/// Whether the supplement target `target` (a `url` or `url|version`
/// canonical) names version `version` of `url`.
fn targets(target: &str, url: &str, version: &str) -> bool {
    match target.split_once('|') {
        Some((u, v)) => u == url && v == version,
        None => target == url,
    }
}

/// A `useSupplement` names no loaded supplement.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("Required supplement not found: {0}")]
pub struct UnknownSupplement(pub String);

/// The host of an absolute URI, `snomed.info` of `http://snomed.info/sct`.
fn uri_host(uri: &str) -> Option<&str> {
    uri.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
}

impl Registry {
    /// This registry with `beneath` under it.
    ///
    /// The layers resolve per system version: a version this registry holds
    /// answers from it, every other version of the same URI still answers from
    /// beneath, and the default is chosen over the versions of both as if they
    /// were one registry ([`Self::default_version`]). A version-specific
    /// canonical names one resource and a version-less one the latest
    /// (<https://hl7.org/fhir/R4B/references.html#canonical>); how a server
    /// layers its content is our own design, no FHIR version governs it.
    #[must_use]
    pub fn with_beneath(mut self, beneath: Arc<Self>) -> Self {
        self.beneath = Some(beneath);
        self
    }

    /// Every layer's entry for `url`, this registry's first.
    fn layers(&self, url: &str) -> Vec<&System> {
        let mut found = Vec::new();
        let mut layer = Some(self);
        while let Some(current) = layer {
            if let Some(system) = current.systems.get(url) {
                found.push(system);
            }
            layer = current.beneath.as_deref();
        }
        found
    }

    /// The versions of `url` across every layer, an upper layer's version
    /// replacing the same version beneath, with the uppermost configured
    /// default.
    fn system(&self, url: &str) -> Option<Cow<'_, System>> {
        let layers = self.layers(url);
        match layers.as_slice() {
            [] => None,
            [only] => Some(Cow::Borrowed(*only)),
            _ => {
                let mut merged = System::default();
                for layer in layers.iter().rev() {
                    merged.versions.extend(
                        layer
                            .versions
                            .iter()
                            .map(|(version, provider)| (version.clone(), Arc::clone(provider))),
                    );
                }
                merged.default = layers.iter().find_map(|layer| layer.default.clone());
                Some(Cow::Owned(merged))
            }
        }
    }

    /// Adds a provider.
    ///
    /// # Errors
    ///
    /// Returns [`RegisterError::Duplicate`] when that system version is present.
    pub fn register(&mut self, provider: Arc<dyn CodeSystemProvider>) -> Result<(), RegisterError> {
        let provider = self.with_globals(provider);
        let (url, version) = {
            let identity = provider.identity();
            (identity.url.clone(), identity.version.clone())
        };
        let system = self.systems.entry(url.clone()).or_default();
        if system.versions.contains_key(&version) {
            return Err(RegisterError::Duplicate { url, version });
        }
        system.versions.insert(version, provider);
        Ok(())
    }

    /// Adds a provider, replacing the one already registered for that system
    /// version (a request-scoped resource shadows a loaded one).
    pub fn register_or_replace(&mut self, provider: Arc<dyn CodeSystemProvider>) {
        let provider = self.with_globals(provider);
        self.replace(provider);
    }

    /// `provider` with the registered global supplements of its system
    /// version layered on, so a system registered after its global supplement
    /// still receives it.
    fn with_globals(&self, provider: Arc<dyn CodeSystemProvider>) -> Arc<dyn CodeSystemProvider> {
        let identity = provider.identity();
        let globals: Vec<crate::supplement::Supplement> = self
            .supplements
            .values()
            .filter(|(target, supplement)| {
                supplement.supplementing.global && targets(target, &identity.url, &identity.version)
            })
            .map(|(_, supplement)| supplement.clone())
            .collect();
        if globals.is_empty() {
            return provider;
        }
        Arc::new(crate::supplement::Supplemented::new(provider, globals))
    }

    /// Puts `provider` in place of the one registered for its system version.
    fn replace(&mut self, provider: Arc<dyn CodeSystemProvider>) {
        let (url, version) = {
            let identity = provider.identity();
            (identity.url.clone(), identity.version.clone())
        };
        self.systems
            .entry(url)
            .or_default()
            .versions
            .insert(version, provider);
    }

    /// Registers `supplement`, which supplements the system `target` (a `url`
    /// or `url|version` canonical).
    ///
    /// No FHIR version says when a supplement applies; the terminology
    /// ecosystem does (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>,
    /// `CodeSystem` supplements): a global one "for the code system it
    /// supplements, whether or not the request or the value set refers to
    /// it", so it is layered over the system's registered versions now, and
    /// "any other supplement SHALL only be used when it is referenced", so it
    /// stays dormant until a request names it. A system registered later
    /// receives the global supplements registered before it.
    pub fn register_supplement(
        &mut self,
        target: String,
        supplement: crate::supplement::Supplement,
    ) {
        if supplement.supplementing.global {
            self.layer_supplement(&target, &supplement);
        }
        self.supplements
            .insert(supplement.canonical(), (target, supplement));
    }

    /// Layers `supplement` over every registered version of `target` it
    /// supplements.
    fn layer_supplement(&mut self, target: &str, supplement: &crate::supplement::Supplement) {
        let (target_url, target_version) = match target.split_once('|') {
            Some((u, v)) => (u, Some(v)),
            None => (target, None),
        };
        let providers: Vec<Arc<dyn CodeSystemProvider>> = self
            .versions(target_url)
            .filter(|p| target_version.is_none_or(|v| p.identity().version == v))
            .cloned()
            .collect();
        for provider in providers {
            self.replace(Arc::new(crate::supplement::Supplemented::new(
                provider,
                vec![supplement.clone()],
            )));
        }
    }

    /// The canonicals of the global supplements that apply to version
    /// `version` of `url`, which report as `used-supplement`
    /// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>, `CodeSystem` supplements).
    #[must_use]
    pub fn global_supplements(&self, url: &str, version: &str) -> Vec<String> {
        self.supplements
            .values()
            .filter(|(target, supplement)| {
                supplement.supplementing.global && targets(target, url, version)
            })
            .map(|(_, supplement)| supplement.canonical())
            .collect()
    }

    /// The dormant supplement at `url`, when a system URI names a supplement
    /// rather than a code system.
    #[must_use]
    pub fn supplement_named(&self, url: &str) -> Option<DormantSupplement> {
        self.supplements
            .values()
            .find(|(_, s)| s.url == url)
            .map(|(_, s)| DormantSupplement {
                version: s.version.clone(),
            })
    }

    /// The registered supplements `wanted` names, one per supplement `url`.
    ///
    /// Only one version of a supplement applies: the one a canonical names by
    /// version, else the latest (terminology ecosystem IG test cases 1.11.0,
    /// `expand-supplement-version`).
    fn chosen_supplements(
        &self,
        wanted: &[String],
    ) -> Result<Vec<&(String, crate::supplement::Supplement)>, UnknownSupplement> {
        let mut chosen: Vec<(&str, bool, &(String, crate::supplement::Supplement))> = Vec::new();
        for canonical in wanted {
            let (url, version) = match canonical.split_once('|') {
                Some((url, version)) => (url, Some(version)),
                None => (canonical.as_str(), None),
            };
            let found = self
                .supplements
                .values()
                .filter(|(_, s)| {
                    s.url == url && version.is_none_or(|v| s.version.as_deref() == Some(v))
                })
                .max_by(|(_, a), (_, b)| {
                    crate::versioned::version_order(
                        a.version.as_deref().unwrap_or_default(),
                        b.version.as_deref().unwrap_or_default(),
                    )
                })
                .ok_or_else(|| UnknownSupplement(canonical.clone()))?;
            let pinned = version.is_some();
            match chosen.iter_mut().find(|(seen, _, _)| *seen == url) {
                Some(entry) if pinned && !entry.1 => *entry = (url, pinned, found),
                Some(_) => {}
                None => chosen.push((url, pinned, found)),
            }
        }
        Ok(chosen.into_iter().map(|(_, _, found)| found).collect())
    }

    /// The canonicals, versioned, of the supplements `wanted` names.
    ///
    /// A request names a supplement with or without a version, and the answer
    /// has to say which one it applied, so the resolution is reported rather
    /// than echoed (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>,
    /// the `used-*` family).
    ///
    /// # Errors
    ///
    /// Returns [`UnknownSupplement`] naming the first one no registered
    /// supplement answers.
    pub fn applied_supplements(&self, wanted: &[String]) -> Result<Vec<String>, UnknownSupplement> {
        Ok(self
            .chosen_supplements(wanted)?
            .into_iter()
            .map(|(_, supplement)| supplement.canonical())
            .collect())
    }

    /// This registry with the dormant supplements `wanted` names (each a `url`
    /// or `url|version` canonical) layered over their systems.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownSupplement`] for a canonical no loaded supplement
    /// answers to.
    pub fn with_supplements(&self, wanted: &[String]) -> Result<Self, UnknownSupplement> {
        let mut layered = self.clone();
        for (target, supplement) in self.chosen_supplements(wanted)? {
            // NOTE: a global supplement already applies, and one that is also
            // referenced "is used once" (terminology ecosystem IG test cases
            // 1.11.0, `ec-lookup-global-referenced`).
            if !supplement.supplementing.global {
                layered.layer_supplement(target, supplement);
            }
        }
        Ok(layered)
    }

    /// Configures the version a request without one resolves to.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] when the system or version is not registered.
    pub fn set_default(&mut self, url: &str, version: &str) -> Result<(), ResolveError> {
        let system = self
            .systems
            .get_mut(url)
            .ok_or_else(|| ResolveError::UnknownSystem(url.to_owned()))?;
        if !system.versions.contains_key(version) {
            return Err(ResolveError::UnknownVersion {
                url: url.to_owned(),
                version: version.to_owned(),
                available: system.versions.keys().cloned().collect(),
            });
        }
        system.default = Some(version.to_owned());
        Ok(())
    }

    /// The default version of a system: the configured one, else the greatest
    /// version string, over the versions of every layer.
    ///
    /// The greatest-string rule is our own design for an unconfigured system;
    /// the FHIR terminology service only asks that the resolved version is
    /// echoed (<https://hl7.org/fhir/R4B/terminology-service.html>). Among
    /// layers the uppermost configured default wins, else the greatest version
    /// any layer holds, the rule one registry holding them all would apply.
    #[must_use]
    pub fn default_version(&self, url: &str) -> Option<&str> {
        let layers = self.layers(url);
        // NOTE: a version-less canonical names the latest (<https://hl7.org/fhir/R4B/references.html#canonical>);
        // no spec governs the default among layers, our own design: one registry's rule over all of them.
        layers
            .iter()
            .find_map(|layer| layer.default.as_deref())
            .or_else(|| {
                layers
                    .iter()
                    .filter_map(|layer| layer.versions.keys().next_back())
                    .max()
                    .map(String::as_str)
            })
    }

    /// Resolves a system and optional version to a provider.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] for an unknown system or version.
    pub fn resolve(&self, url: &str, version: Option<&str>) -> Result<Resolved, ResolveError> {
        let system = self
            .system(url)
            .ok_or_else(|| ResolveError::UnknownSystem(url.to_owned()))?;
        let (wanted, defaulted) = match version {
            Some(version) => (version, false),
            None => (
                system
                    .default_version()
                    .ok_or_else(|| ResolveError::UnknownSystem(url.to_owned()))?,
                true,
            ),
        };
        // NOTE: `1.0.x` and `1.x.x` name the greatest matching version, the
        // ecosystem's form (`crate::versioned::version_matches`).
        let provider =
            crate::versioned::select_version(system.versions.keys().map(String::as_str), wanted)
                .and_then(|v| system.versions.get(v))
                .or_else(|| Self::answering(&system, wanted))
                .ok_or_else(|| ResolveError::UnknownVersion {
                    url: url.to_owned(),
                    version: wanted.to_owned(),
                    available: system.versions.keys().cloned().collect(),
                })?;
        Ok(Resolved {
            provider: Arc::clone(provider),
            defaulted,
        })
    }

    /// The greatest loaded version of `system` that answers to `wanted` under
    /// the code system's own version scheme.
    ///
    /// A SNOMED CT edition URI with no release date names a distribution
    /// rather than one release of it, and the FHIR SNOMED CT page tells
    /// clients to send that form: "At minimum the URI SHOULD contain the sctid
    /// of the SNOMED CT distribution", and "if the date of release is not
    /// provided, the Terminology Service may default to the most recent
    /// version of the named SNOMED CT distribution"
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "Versions").
    fn answering<'a>(system: &'a System, wanted: &str) -> Option<&'a Arc<dyn CodeSystemProvider>> {
        system
            .versions
            .iter()
            .filter(|(_, provider)| provider.answers_version(wanted))
            .max_by(|(a, _), (b, _)| crate::versioned::version_order(a, b))
            .map(|(_, provider)| provider)
    }

    /// The providers to ask about the implicit URI `url`: for every system
    /// whose URI prefixes it, the default version first, then its other
    /// loaded versions.
    ///
    /// An implicit URI may carry a version of its own (a SNOMED CT edition
    /// version, <https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value
    /// Sets"), so every loaded version of the system is a candidate and the
    /// default only decides the versionless form.
    fn implicit_candidates(&self, url: &str) -> Vec<Arc<dyn CodeSystemProvider>> {
        let mut candidates = Vec::new();
        let base = url.split_once('?').map_or(url, |(base, _)| base);
        for canonical in self.canonicals() {
            let prefixed = url.starts_with(canonical);
            if !prefixed && uri_host(canonical) != uri_host(base) {
                continue;
            }
            let Some(system) = self.system(canonical) else {
                continue;
            };
            // NOTE: a version that is itself a URI names its system too, as an
            // unpublished SNOMED CT edition's `http://snomed.info/xsct/` URI does
            // (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-uri-standard>).
            if !prefixed
                && !system
                    .versions
                    .keys()
                    .any(|version| version.starts_with(base))
            {
                continue;
            }
            let Ok(resolved) = self.resolve(canonical, None) else {
                continue;
            };
            let default = resolved.provider.identity().version.clone();
            candidates.push(resolved.provider);
            candidates.extend(
                system
                    .versions
                    .iter()
                    .filter(|(version, _)| **version != default)
                    .map(|(_, provider)| Arc::clone(provider)),
            );
        }
        candidates
    }

    /// The compose an implicit value set URI denotes, asking every loaded
    /// version of every system whose URI prefixes `url`.
    ///
    /// `None` when no system claims the URI; the unknown-version error when a
    /// system claims it but no loaded version of it serves the version the URI
    /// names.
    #[must_use]
    pub fn implicit_value_set(&self, url: &str) -> Option<Result<Compose, ProviderError>> {
        let mut unserved = None;
        for provider in self.implicit_candidates(url) {
            match provider.implicit_value_set(url) {
                None => {}
                Some(Err(error @ ProviderError::UnservedImplicitVersion { .. })) => {
                    if unserved.is_none() {
                        unserved = Some(error);
                    }
                }
                Some(answer) => return Some(answer),
            }
        }
        unserved.map(Err)
    }

    /// The metadata of the implicit value set `url` denotes, from the system
    /// version that defines it; empty when no registered version does.
    #[must_use]
    pub fn implicit_metadata(&self, url: &str) -> crate::provider::ImplicitMetadata {
        self.implicit_candidates(url)
            .into_iter()
            .find(|provider| {
                !matches!(
                    provider.implicit_value_set(url),
                    None | Some(Err(ProviderError::UnservedImplicitVersion { .. }))
                )
            })
            .map(|provider| provider.implicit_metadata(url))
            .unwrap_or_default()
    }

    /// The `ConceptMap` an implicit concept map URI denotes, carrying the
    /// elements `selection` asks for, from every loaded version of every
    /// system whose URI prefixes `url`.
    ///
    /// `None` when no system claims the URI; the unknown-version error when a
    /// system claims it but no loaded version of it serves the version the URI
    /// names.
    #[must_use]
    pub fn implicit_concept_map(
        &self,
        url: &str,
        selection: crate::provider::MapSelection<'_>,
    ) -> Option<Result<crate::provider::concept_map::ConceptMapModel, ProviderError>> {
        let mut unserved = None;
        for provider in self.implicit_candidates(url) {
            match provider.implicit_concept_map(url, selection) {
                None => {}
                Some(Err(error @ ProviderError::UnservedImplicitVersion { .. })) => {
                    if unserved.is_none() {
                        unserved = Some(error);
                    }
                }
                Some(answer) => return Some(answer),
            }
        }
        unserved.map(Err)
    }

    /// The implicit concept maps every loaded version defines from codes of
    /// `source` to codes of `target` ([`CodeSystemProvider::implicit_concept_maps`]).
    ///
    /// # Errors
    ///
    /// Returns the first provider's [`ProviderError`].
    pub fn implicit_concept_maps(
        &self,
        source: &str,
        target: &str,
    ) -> Result<Vec<String>, ProviderError> {
        let mut urls = Vec::new();
        for canonical in self.canonicals() {
            let Some(system) = self.system(canonical) else {
                continue;
            };
            for provider in system.versions.values() {
                for url in provider.implicit_concept_maps(source, target)? {
                    if !urls.contains(&url) {
                        urls.push(url);
                    }
                }
            }
        }
        Ok(urls)
    }

    /// The system URIs registered here, sorted.
    ///
    /// The registry beneath is left out: these are the code systems this
    /// deployment published, which is what the `CodeSystem` endpoints and the
    /// `TerminologyCapabilities` statement enumerate. Use [`Self::canonicals`]
    /// for every URI that resolves.
    pub fn systems(&self) -> impl Iterator<Item = &str> {
        self.systems.keys().map(String::as_str)
    }

    /// Every system URI that resolves, sorted, this registry's and the ones
    /// beneath it.
    #[must_use]
    pub fn canonicals(&self) -> Vec<&str> {
        let mut all: std::collections::BTreeSet<&str> =
            self.systems.keys().map(String::as_str).collect();
        if let Some(below) = self.beneath.as_deref() {
            all.extend(below.canonicals());
        }
        all.into_iter().collect()
    }

    /// The registered versions of a system across every layer, sorted, with
    /// the provider; an upper layer's version replaces the same one beneath.
    pub fn versions(&self, url: &str) -> impl Iterator<Item = &Arc<dyn CodeSystemProvider>> {
        let mut merged: BTreeMap<&str, &Arc<dyn CodeSystemProvider>> = BTreeMap::new();
        for layer in self.layers(url).into_iter().rev() {
            merged.extend(
                layer
                    .versions
                    .iter()
                    .map(|(version, provider)| (version.as_str(), provider)),
            );
        }
        merged.into_values()
    }
}
