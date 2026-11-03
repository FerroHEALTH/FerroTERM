// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The provider every stored system is served by, and what a system decides.

use std::fmt;

use concept_graph::subsumption::Outcome;
use designation_index::index::Query;

use super::artifact::Artifact;
use super::primary_subtag;
use crate::provider::compose::Compose;
use crate::provider::concept_map::ConceptMapModel;
use crate::provider::declaration::{Capability, Declaration};
use crate::provider::filter::Filter;
use crate::provider::source::Source;
use crate::provider::{
    CodeSystemProvider, Concept, ConceptSet, Designation, DesignationUse, Hierarchy, Identity,
    ImplicitMetadata, Located, MapSelection, Property, ProviderError, Status, Successor,
};

/// One stored system behind a [`StoredProvider`]: what its specification
/// decides beyond the reads every artifact answers alike.
///
/// Each function takes the provider, so a system reads the shared
/// [`Artifact`] and its own state, and can ask the provider any question of
/// the seam. A default answers as [`CodeSystemProvider`]'s own default does;
/// a hook that returns `None` leaves the answer to the behaviour every
/// provider shares.
pub trait StoredSystem: fmt::Debug + Send + Sync + Sized {
    /// The language of the system's own displays, as
    /// [`CodeSystemProvider::language`].
    fn language(provider: &StoredProvider<Self>) -> Option<&str>;

    /// Finds a code, as [`CodeSystemProvider::locate`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn locate(
        provider: &StoredProvider<Self>,
        code: &str,
    ) -> Result<Option<Located>, ProviderError>;

    /// The display, as [`CodeSystemProvider::display`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn display(
        provider: &StoredProvider<Self>,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError>;

    /// The status, as [`CodeSystemProvider::status`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn status(provider: &StoredProvider<Self>, concept: Concept) -> Result<Status, ProviderError>;

    /// The properties, as [`CodeSystemProvider::properties`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn properties(
        provider: &StoredProvider<Self>,
        concept: Concept,
    ) -> Result<Vec<Property>, ProviderError>;

    /// The code, as [`CodeSystemProvider::code`]; the stored code unless the
    /// system spells it otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn code(
        provider: &StoredProvider<Self>,
        concept: Concept,
    ) -> Result<Option<String>, ProviderError> {
        provider.stored().code(concept)
    }

    /// The use of a stored designation; the use's own name as a code of the
    /// system unless the system codes its uses otherwise.
    fn designation_use(provider: &StoredProvider<Self>, use_ordinal: u32) -> DesignationUse {
        provider
            .stored()
            .use_coding(&provider.identity().url, use_ordinal)
    }

    /// As [`CodeSystemProvider::definition`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn definition(
        _provider: &StoredProvider<Self>,
        _concept: Concept,
    ) -> Result<Option<String>, ProviderError> {
        Ok(None)
    }

    /// As [`CodeSystemProvider::is_expression`].
    fn is_expression(_provider: &StoredProvider<Self>, _code: &str) -> bool {
        false
    }

    /// As [`CodeSystemProvider::rejection`].
    fn rejection(_provider: &StoredProvider<Self>, _code: &str) -> Option<String> {
        None
    }

    /// As [`CodeSystemProvider::admits_post_coordination`].
    fn admits_post_coordination(_provider: &StoredProvider<Self>, _filter: &Filter) -> bool {
        false
    }

    /// As [`CodeSystemProvider::is_postcoordinated`]; such a concept carries
    /// no stored designation.
    fn is_postcoordinated(_provider: &StoredProvider<Self>, _concept: Concept) -> bool {
        false
    }

    /// As [`CodeSystemProvider::implicit_value_set`].
    fn implicit_value_set(
        _provider: &StoredProvider<Self>,
        _url: &str,
    ) -> Option<Result<Compose, ProviderError>> {
        None
    }

    /// As [`CodeSystemProvider::implicit_metadata`].
    fn implicit_metadata(_provider: &StoredProvider<Self>, _url: &str) -> ImplicitMetadata {
        ImplicitMetadata::default()
    }

    /// As [`CodeSystemProvider::answers_version`].
    fn answers_version(_provider: &StoredProvider<Self>, _version: &str) -> bool {
        false
    }

    /// As [`CodeSystemProvider::generated_properties`].
    fn generated_properties(_provider: &StoredProvider<Self>) -> &[&'static str] {
        &[]
    }

    /// As [`CodeSystemProvider::generated_property`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn generated_property(
        _provider: &StoredProvider<Self>,
        _concept: Concept,
        _name: &str,
    ) -> Result<Option<Property>, ProviderError> {
        Ok(None)
    }

    /// As [`CodeSystemProvider::unserved_properties`].
    fn unserved_properties(_provider: &StoredProvider<Self>) -> &[&'static str] {
        &[]
    }

    /// As [`CodeSystemProvider::successors`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn successors(
        _provider: &StoredProvider<Self>,
        _concept: Concept,
    ) -> Result<Vec<Successor>, ProviderError> {
        Ok(Vec::new())
    }

    /// As [`CodeSystemProvider::implicit_concept_map`].
    fn implicit_concept_map(
        _provider: &StoredProvider<Self>,
        _url: &str,
        _selection: MapSelection<'_>,
    ) -> Option<Result<ConceptMapModel, ProviderError>> {
        None
    }

    /// As [`CodeSystemProvider::unclosed`].
    fn unclosed(_provider: &StoredProvider<Self>, _filters: &[Filter]) -> bool {
        false
    }

    /// As [`CodeSystemProvider::subsumes`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::CannotDetermine`] when the system cannot say.
    fn subsumes(
        _provider: &StoredProvider<Self>,
        _a: Concept,
        _b: Concept,
    ) -> Result<Option<Outcome>, ProviderError> {
        Ok(None)
    }

    /// The concepts a filter of the system's own selects; `None` leaves the
    /// filter to [`crate::provider::filter::evaluate`].
    fn filter(
        _provider: &StoredProvider<Self>,
        _filter: &Filter,
    ) -> Option<Result<ConceptSet, ProviderError>> {
        None
    }

    /// Whether `concept` passes `filter`, when the system decides it from the
    /// concept; `None` leaves it to [`crate::provider::filter::selected_by`].
    fn filter_matches(
        _provider: &StoredProvider<Self>,
        _concept: Concept,
        _filter: &Filter,
    ) -> Option<Result<bool, ProviderError>> {
        None
    }

    /// Every inactive concept, when the system has a cheaper answer; `None`
    /// leaves it to [`crate::provider::filter::inactive_by_status`].
    fn inactive(_provider: &StoredProvider<Self>) -> Option<Result<ConceptSet, ProviderError>> {
        None
    }
}

/// A stored system as a provider.
pub struct StoredProvider<S> {
    identity: Identity,
    declaration: Declaration,
    source: Source,
    artifact: Artifact,
    system: S,
}

impl<S> StoredProvider<S> {
    /// The provider of `system` over `artifact`, read from `source`.
    pub(crate) fn new(
        identity: Identity,
        declaration: Declaration,
        source: Source,
        artifact: Artifact,
        system: S,
    ) -> Self {
        Self {
            identity,
            declaration,
            source,
            artifact,
            system,
        }
    }

    /// The artifact the provider opened.
    pub(crate) fn stored(&self) -> &Artifact {
        &self.artifact
    }

    /// The system's own state.
    pub(crate) fn system(&self) -> &S {
        &self.system
    }
}

impl<S> fmt::Debug for StoredProvider<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredProvider")
            .field("system", &self.identity.url)
            .field("version", &self.identity.version)
            .field("concepts", &self.artifact.concepts)
            .finish_non_exhaustive()
    }
}

impl<S: StoredSystem> CodeSystemProvider for StoredProvider<S> {
    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn artifact(&self) -> Option<&Source> {
        Some(&self.source)
    }

    fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        S::locate(self, code)
    }

    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        S::code(self, concept)
    }

    fn display(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        S::display(self, concept, language)
    }

    fn definition(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        S::definition(self, concept)
    }

    fn is_expression(&self, code: &str) -> bool {
        S::is_expression(self, code)
    }

    fn rejection(&self, code: &str) -> Option<String> {
        S::rejection(self, code)
    }

    fn admits_post_coordination(&self, filter: &Filter) -> bool {
        S::admits_post_coordination(self, filter)
    }

    fn language(&self) -> Option<&str> {
        S::language(self)
    }

    fn status(&self, concept: Concept) -> Result<Status, ProviderError> {
        S::status(self, concept)
    }

    fn inactive(&self) -> Result<ConceptSet, ProviderError> {
        S::inactive(self).unwrap_or_else(|| crate::provider::filter::inactive_by_status(self))
    }

    fn is_postcoordinated(&self, concept: Concept) -> bool {
        S::is_postcoordinated(self, concept)
    }

    fn designations(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Vec<Designation>, ProviderError> {
        if S::is_postcoordinated(self, concept) {
            return Ok(Vec::new());
        }
        self.artifact
            .designations(concept, language, |use_ordinal| {
                S::designation_use(self, use_ordinal)
            })
    }

    fn properties(&self, concept: Concept) -> Result<Vec<Property>, ProviderError> {
        S::properties(self, concept)
    }

    /// The tree, for a system that declares subsumption.
    fn hierarchy(&self) -> Option<&dyn Hierarchy> {
        self.declaration
            .capabilities
            .contains(&Capability::Subsumption)
            .then_some(&self.artifact.tree)
    }

    fn all(&self) -> Result<ConceptSet, ProviderError> {
        Ok(crate::provider::every(self.artifact.concepts))
    }

    fn search(&self, text: &str, language: Option<&str>) -> Result<ConceptSet, ProviderError> {
        let query = Query {
            text: text.to_owned(),
            language: language.map(primary_subtag),
            ..Query::default()
        };
        let mut concepts = ConceptSet::new();
        for designation in self.artifact.text.matches(&query) {
            if let Some(entry) = self.artifact.text.entry(designation) {
                concepts.insert(entry.concept.index());
            }
        }
        Ok(concepts)
    }

    fn implicit_value_set(&self, url: &str) -> Option<Result<Compose, ProviderError>> {
        S::implicit_value_set(self, url)
    }

    fn implicit_metadata(&self, url: &str) -> ImplicitMetadata {
        S::implicit_metadata(self, url)
    }

    fn answers_version(&self, version: &str) -> bool {
        S::answers_version(self, version)
    }

    fn generated_properties(&self) -> &[&'static str] {
        S::generated_properties(self)
    }

    fn generated_property(
        &self,
        concept: Concept,
        name: &str,
    ) -> Result<Option<Property>, ProviderError> {
        S::generated_property(self, concept, name)
    }

    fn unserved_properties(&self) -> &[&'static str] {
        S::unserved_properties(self)
    }

    fn successors(&self, concept: Concept) -> Result<Vec<Successor>, ProviderError> {
        S::successors(self, concept)
    }

    fn implicit_concept_map(
        &self,
        url: &str,
        selection: MapSelection<'_>,
    ) -> Option<Result<ConceptMapModel, ProviderError>> {
        S::implicit_concept_map(self, url, selection)
    }

    fn filter(&self, filter: &Filter) -> Result<ConceptSet, ProviderError> {
        S::filter(self, filter).unwrap_or_else(|| crate::provider::filter::evaluate(self, filter))
    }

    fn unclosed(&self, filters: &[Filter]) -> bool {
        S::unclosed(self, filters)
    }

    fn filter_matches(&self, concept: Concept, filter: &Filter) -> Result<bool, ProviderError> {
        S::filter_matches(self, concept, filter)
            .unwrap_or_else(|| crate::provider::filter::selected_by(self, concept, filter))
    }

    fn subsumes(&self, a: Concept, b: Concept) -> Result<Option<Outcome>, ProviderError> {
        S::subsumes(self, a, b)
    }
}
