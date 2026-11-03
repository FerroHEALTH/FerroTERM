// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A code system defined by a grammar over a registry, served as a provider.
//!
//! BCP 13 media types and BCP 47 language tags are both grammars: a code is a
//! code when it parses, the system cannot be enumerated, and each code is
//! interned the first time it is seen
//! (<https://hl7.org/fhir/R4B/valueset-all-languages.html>). A
//! [`GrammarProvider`] answers what every such system answers alike, and its
//! [`Grammar`] decides the rest.

use std::fmt;

use concept_graph::subsumption::Outcome;

use super::interned::Interned;
use crate::filter::Filter;
use crate::provider::{
    CodeSystemProvider, Concept, ConceptSet, Declaration, Designation, Identity, Located, Property,
    ProviderError, Status,
};

/// What one grammar system decides about its codes.
///
/// Every method that names a concept reads its code through the [`Table`],
/// the interned codes and the declaration of the provider.
pub trait Grammar: fmt::Debug + Send + Sync {
    /// The canonical spelling of `code`, when `code` is a code of the system.
    fn canonical(&self, code: &str) -> Option<String>;

    /// The display of `concept`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] when the display cannot be read.
    fn display(&self, table: &Table<'_>, concept: Concept)
    -> Result<Option<String>, ProviderError>;

    /// The status of `concept`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] when the status cannot be read.
    fn status(&self, table: &Table<'_>, concept: Concept) -> Result<Status, ProviderError>;

    /// The properties of `concept`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] when the properties cannot be read.
    fn properties(
        &self,
        table: &Table<'_>,
        concept: Concept,
    ) -> Result<Vec<Property>, ProviderError>;

    /// The concepts every filter of one include selects, as
    /// [`CodeSystemProvider::filter_all`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] as [`CodeSystemProvider::filter_all`].
    fn filter_all(
        &self,
        table: &Table<'_>,
        filters: &[Filter],
    ) -> Result<ConceptSet, ProviderError>;

    /// The selected concepts in the order they are listed, as
    /// [`CodeSystemProvider::filter_ordered`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] as [`CodeSystemProvider::filter_ordered`].
    fn filter_ordered(
        &self,
        _table: &Table<'_>,
        _filters: &[Filter],
    ) -> Result<Option<Vec<u32>>, ProviderError> {
        Ok(None)
    }

    /// Whether `concept` passes `filter`, as
    /// [`CodeSystemProvider::filter_matches`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] as [`CodeSystemProvider::filter_matches`].
    fn filter_matches(
        &self,
        table: &Table<'_>,
        concept: Concept,
        filter: &Filter,
    ) -> Result<bool, ProviderError>;

    /// Whether `a` subsumes `b`, as [`CodeSystemProvider::subsumes`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] as [`CodeSystemProvider::subsumes`].
    fn subsumes(
        &self,
        table: &Table<'_>,
        a: Concept,
        b: Concept,
    ) -> Result<Option<Outcome>, ProviderError>;

    /// Why a selection is incomplete, as
    /// [`CodeSystemProvider::unclosed_reason`].
    fn unclosed_reason(&self) -> Option<String> {
        None
    }
}

/// The interned codes and the declaration a [`Grammar`] reads.
#[derive(Debug, Clone, Copy)]
pub struct Table<'a> {
    interned: &'a Interned,
    declaration: &'a Declaration,
}

impl Table<'_> {
    /// The code of `concept`, when it is interned.
    #[must_use]
    pub fn code(&self, concept: Concept) -> Option<String> {
        self.interned.code(concept)
    }

    /// The concept of the canonical code `canonical`, interning it when new.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] when the table is full.
    pub fn intern(&self, canonical: &str) -> Result<Concept, ProviderError> {
        self.interned.intern(canonical)
    }

    /// Refuses a filter on a property the system does not declare.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::UnsupportedFilter`] when `filter` names a
    /// property no declared filter names.
    pub fn declared(&self, filter: &Filter) -> Result<(), ProviderError> {
        if self
            .declaration
            .filters
            .iter()
            .any(|declared| declared.code == filter.property)
        {
            Ok(())
        } else {
            Err(ProviderError::UnsupportedFilter {
                property: filter.property.clone(),
                operator: filter.op.code().to_owned(),
            })
        }
    }
}

/// The outcome of two containment tests: whether `a` covers `b`, and whether
/// `b` covers `a`.
#[must_use]
pub fn outcome(a_covers_b: bool, b_covers_a: bool) -> Outcome {
    match (a_covers_b, b_covers_a) {
        (true, true) => Outcome::Equivalent,
        (true, false) => Outcome::Subsumes,
        (false, true) => Outcome::SubsumedBy,
        (false, false) => Outcome::NotSubsumed,
    }
}

/// A grammar system as a provider.
#[derive(Debug)]
pub struct GrammarProvider<G> {
    identity: Identity,
    declaration: Declaration,
    interned: Interned,
    grammar: G,
}

impl<G> GrammarProvider<G> {
    /// The provider of `grammar` with `identity` and `declaration`, over the
    /// codes already in `interned`.
    pub(crate) fn from_parts(
        identity: Identity,
        declaration: Declaration,
        interned: Interned,
        grammar: G,
    ) -> Self {
        Self {
            identity,
            declaration,
            interned,
            grammar,
        }
    }

    /// The grammar.
    pub(crate) fn grammar(&self) -> &G {
        &self.grammar
    }

    fn table(&self) -> Table<'_> {
        Table {
            interned: &self.interned,
            declaration: &self.declaration,
        }
    }
}

impl<G: Grammar> CodeSystemProvider for GrammarProvider<G> {
    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        let Some(canonical) = self.grammar.canonical(code) else {
            return Ok(None);
        };
        let concept = self.interned.intern(&canonical)?;
        Ok(Some(Located {
            concept,
            code: canonical,
        }))
    }

    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        Ok(self.interned.code(concept))
    }

    fn display(
        &self,
        concept: Concept,
        _language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        self.grammar.display(&self.table(), concept)
    }

    fn status(&self, concept: Concept) -> Result<Status, ProviderError> {
        self.grammar.status(&self.table(), concept)
    }

    fn designations(
        &self,
        _concept: Concept,
        _language: Option<&str>,
    ) -> Result<Vec<Designation>, ProviderError> {
        Ok(Vec::new())
    }

    fn properties(&self, concept: Concept) -> Result<Vec<Property>, ProviderError> {
        self.grammar.properties(&self.table(), concept)
    }

    fn all(&self) -> Result<ConceptSet, ProviderError> {
        Err(ProviderError::NotEnumerable)
    }

    fn search(&self, _text: &str, _language: Option<&str>) -> Result<ConceptSet, ProviderError> {
        Err(ProviderError::NotEnumerable)
    }

    fn filter(&self, filter: &Filter) -> Result<ConceptSet, ProviderError> {
        self.filter_all(std::slice::from_ref(filter))
    }

    fn filter_all(&self, filters: &[Filter]) -> Result<ConceptSet, ProviderError> {
        self.grammar.filter_all(&self.table(), filters)
    }

    fn filter_ordered(&self, filters: &[Filter]) -> Result<Option<Vec<u32>>, ProviderError> {
        self.grammar.filter_ordered(&self.table(), filters)
    }

    /// Every selection of a grammar system is incomplete: the registry is
    /// finite, and the codes the grammar builds on it are not.
    fn unclosed(&self, _filters: &[Filter]) -> bool {
        true
    }

    fn unclosed_reason(&self) -> Option<String> {
        self.grammar.unclosed_reason()
    }

    fn filter_matches(&self, concept: Concept, filter: &Filter) -> Result<bool, ProviderError> {
        self.grammar.filter_matches(&self.table(), concept, filter)
    }

    fn subsumes(&self, a: Concept, b: Concept) -> Result<Option<Outcome>, ProviderError> {
        self.grammar.subsumes(&self.table(), a, b)
    }
}
