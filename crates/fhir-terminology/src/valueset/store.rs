// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The value sets a server holds, by `url` and `version`, and the resolver
//! that answers `include.valueSet` references over them.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use super::model::ValueSetModel;
use super::negotiation::Negotiation;
use crate::compose::{Compose, ComposeError, Expander, Expansion, Item, Options, ValueSetResolver};
use crate::registry::Registry;
use crate::versioned::VersionedStore;

/// The value sets by `url`, then `version`.
pub type ValueSetStore = VersionedStore<ValueSetModel>;

/// Resolves `include.valueSet` references from a store and the providers'
/// implicit value sets, refusing a cycle.
#[derive(Debug)]
pub struct Resolver<'a> {
    registry: &'a Registry,
    store: &'a ValueSetStore,
    negotiation: Option<&'a Negotiation>,
    contained: Option<&'a BTreeMap<String, ValueSetModel>>,
    language: Option<String>,
    active: RefCell<Vec<String>>,
    used: RefCell<Vec<String>>,
    open: RefCell<BTreeSet<String>>,
}

impl<'a> Resolver<'a> {
    /// A resolver over `registry` and `store`.
    #[must_use]
    pub fn new(registry: &'a Registry, store: &'a ValueSetStore) -> Self {
        Self {
            registry,
            store,
            negotiation: None,
            contained: None,
            language: None,
            active: RefCell::new(Vec::new()),
            used: RefCell::new(Vec::new()),
            open: RefCell::new(BTreeSet::new()),
        }
    }

    /// This resolver applying `negotiation` to every value set it references
    /// and every system those value sets select from.
    #[must_use]
    pub fn with_negotiation(mut self, negotiation: &'a Negotiation) -> Self {
        self.negotiation = Some(negotiation);
        self
    }

    /// This resolver answering a `#id` reference from `contained`, the value
    /// sets the resource under expansion carries inline.
    #[must_use]
    pub fn with_contained(mut self, contained: &'a BTreeMap<String, ValueSetModel>) -> Self {
        self.contained = Some(contained);
        self
    }

    /// This resolver answering every reference in `language`, the request's
    /// `displayLanguage` range list.
    ///
    /// The request carries one display language for the whole answer, so the
    /// resolver holds it for the reference chain: a display a value set
    /// reference contributes is then in the language the request asked for,
    /// which the ecosystem requires of every hop
    /// (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/languages.html>).
    #[must_use]
    pub fn with_language(mut self, language: Option<&str>) -> Self {
        self.language = language.map(str::to_owned);
        self
    }

    /// The referenced value sets resolved so far, as `url|version` canonicals
    /// in reference order (the `used-valueset` expansion parameters).
    #[must_use]
    pub fn used_value_sets(&self) -> Vec<String> {
        self.used.borrow().clone()
    }

    /// The systems the value sets resolved so far select from without naming a
    /// version, so a `system-version` rule for one of them is what supplied the
    /// version used.
    #[must_use]
    pub fn open_systems(&self) -> BTreeSet<String> {
        self.open.borrow().clone()
    }

    /// Records the systems `compose` selects from without naming a version.
    pub fn note_open_systems(&self, compose: &Compose) {
        let mut open = self.open.borrow_mut();
        for include in compose.include.iter().chain(&compose.exclude) {
            if let Some(system) = &include.system
                && system.version.is_none()
            {
                open.insert(system.url.clone());
            }
        }
    }

    /// The compose of the value set at `url` (which may carry `|version`):
    /// stored, or implicit, at the negotiated version and with its systems
    /// pinned.
    ///
    /// # Errors
    ///
    /// Returns [`ComposeError::UnknownValueSet`] when neither knows it,
    /// [`ComposeError::Negotiation`] when a check disagrees, or the provider's
    /// error for a malformed implicit form.
    pub fn compose(&self, url: &str) -> Result<Compose, ComposeError> {
        // NOTE: a canonical reference to a contained resource is a `#id`
        // fragment of the container, which no registry or store holds
        // (<https://hl7.org/fhir/R4B/references.html#canonical>).
        if let Some(id) = url.strip_prefix('#') {
            let model = self
                .contained
                .and_then(|contained| contained.get(id))
                .ok_or_else(|| ComposeError::UnknownValueSet(url.to_owned()))?;
            self.note_open_systems(&model.compose);
            return match self.negotiation {
                Some(negotiation) => Ok(negotiation.pin_lenient(&model.compose)),
                None => Ok(model.compose.clone()),
            };
        }
        let (url, version) = match self.negotiation {
            Some(negotiation) => negotiation.value_set(url, None)?,
            None => match url.split_once('|') {
                Some((url, version)) => (url.to_owned(), Some(version.to_owned())),
                None => (url.to_owned(), None),
            },
        };
        if let Some(model) = self.store.resolve(&url, version.as_deref()) {
            let canonical = match model.version.as_deref() {
                Some(v) => format!("{url}|{v}"),
                None => url.clone(),
            };
            self.used.borrow_mut().push(canonical);
            self.note_open_systems(&model.compose);
            return Ok(match self.negotiation {
                Some(negotiation) => negotiation.pin_lenient(&model.compose),
                None => model.compose.clone(),
            });
        }
        match self.registry.implicit_value_set(&url) {
            Some(Ok(compose)) => {
                self.note_open_systems(&compose);
                Ok(match self.negotiation {
                    Some(negotiation) => negotiation.pin_lenient(&compose),
                    None => compose,
                })
            }
            Some(Err(source)) => Err(ComposeError::Provider {
                system: url,
                source,
            }),
            None => Err(ComposeError::UnknownValueSet(match version {
                Some(v) => format!("{url}|{v}"),
                None => url,
            })),
        }
    }

    /// Expands the value set at `url` under `options`, tracking the reference
    /// chain so a value set that reaches itself is an error.
    ///
    /// # Errors
    ///
    /// Returns [`ComposeError`] when the value set is unknown, cyclic, or fails
    /// to expand.
    pub fn expand_with(&self, url: &str, options: &Options) -> Result<Expansion, ComposeError> {
        let compose = self.compose(url)?;
        self.expand_compose(url, &compose, options)
    }

    /// Expands `compose`, named `url` for cycle detection, under `options`.
    ///
    /// # Errors
    ///
    /// Returns [`ComposeError`] for a cycle or an expansion failure.
    pub fn expand_compose(
        &self,
        url: &str,
        compose: &Compose,
        options: &Options,
    ) -> Result<Expansion, ComposeError> {
        if self
            .active
            .borrow()
            .iter()
            .any(|active| re_enters(active, url))
        {
            return Err(ComposeError::Cycle(url.to_owned()));
        }
        self.active.borrow_mut().push(url.to_owned());
        let result = Expander::with_resolver(self.registry, self).expand(compose, options);
        self.active.borrow_mut().pop();
        result
    }
}

/// Whether `reference` names the value set an `active` chain entry names.
///
/// A chain entry is spelled the way its caller had it: a root as `url|version`,
/// a reference as whatever the compose wrote. A reference naming no version
/// means the version the server resolves to
/// (<https://hl7.org/fhir/R4B/references.html#canonical>), so it re-enters an
/// entry of the same url whatever version that entry carries, and a reference
/// that does name one re-enters only that version.
fn re_enters(active: &str, reference: &str) -> bool {
    let split = |canonical: &str| match canonical.split_once('|') {
        Some((url, version)) => (url.to_owned(), Some(version.to_owned())),
        None => (canonical.to_owned(), None),
    };
    let (active_url, active_version) = split(active);
    let (reference_url, reference_version) = split(reference);
    active_url == reference_url
        && match (active_version, reference_version) {
            (Some(held), Some(asked)) => held == asked,
            _ => true,
        }
}

impl Resolver<'_> {
    /// Refuses `compose`, named `url`, when it reaches itself through
    /// `include.valueSet` or `exclude.valueSet`.
    ///
    /// Only the value set references are walked, so the answer costs no
    /// expansion. A reference the server does not hold ends that branch: an
    /// unknown import is the operation's own answer, and a cycle is a value
    /// set no operation can evaluate.
    ///
    /// # Errors
    ///
    /// Returns [`ComposeError::Cycle`] naming the value set that re-enters.
    pub fn check_acyclic(&self, url: &str, compose: &Compose) -> Result<(), ComposeError> {
        let mut walked = BTreeSet::new();
        self.walk_references(url, compose, &mut walked)
    }

    /// `check_acyclic` with `url` on the reference chain.
    fn walk_references(
        &self,
        url: &str,
        compose: &Compose,
        walked: &mut BTreeSet<String>,
    ) -> Result<(), ComposeError> {
        self.active.borrow_mut().push(url.to_owned());
        let result = self.references(compose, walked);
        self.active.borrow_mut().pop();
        result
    }

    /// Every value set `compose` references, each walked once.
    fn references(
        &self,
        compose: &Compose,
        walked: &mut BTreeSet<String>,
    ) -> Result<(), ComposeError> {
        for criterion in compose.include.iter().chain(&compose.exclude) {
            for referenced in &criterion.value_sets {
                if self
                    .active
                    .borrow()
                    .iter()
                    .any(|active| re_enters(active, referenced))
                {
                    return Err(ComposeError::Cycle(referenced.clone()));
                }
                if !walked.insert(referenced.clone()) {
                    continue;
                }
                // NOTE: a reference the server does not hold is legitimately absent
                // here; the operation reports it, and only a cycle is this walk's answer.
                let Ok(compose) = self.compose(referenced) else {
                    continue;
                };
                self.walk_references(referenced, &compose, walked)?;
            }
        }
        Ok(())
    }

    /// Whether `compose`, named `url` for cycle detection, contains `code` of
    /// `system`.
    ///
    /// # Errors
    ///
    /// Returns [`ComposeError`] for a cycle or a failure to evaluate.
    pub fn contains_compose(
        &self,
        url: &str,
        compose: &Compose,
        system: &str,
        version: Option<&str>,
        code: &str,
        language: Option<&str>,
    ) -> Result<Option<Item>, ComposeError> {
        if self
            .active
            .borrow()
            .iter()
            .any(|active| re_enters(active, url))
        {
            return Err(ComposeError::Cycle(url.to_owned()));
        }
        self.active.borrow_mut().push(url.to_owned());
        let result = Expander::with_resolver(self.registry, self)
            .contains(compose, system, version, code, language);
        self.active.borrow_mut().pop();
        result
    }
}

impl ValueSetResolver for Resolver<'_> {
    fn expand(&self, url: &str) -> Result<Expansion, ComposeError> {
        // A referenced value set is expanded whole, so only the display
        // language of the request carries into it.
        let options = Options {
            language: self.language.clone(),
            ..Options::default()
        };
        self.expand_with(url, &options)
    }

    fn contains(&self, url: &str, system: &str, code: &str) -> Result<Option<Item>, ComposeError> {
        let compose = self.compose(url)?;
        self.contains_compose(url, &compose, system, None, code, self.language.as_deref())
    }
}
