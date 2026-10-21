// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Code system supplements applied to a provider's designations and properties
//! (`CodeSystem.content = supplement`, <https://hl7.org/fhir/R4B/codesystem.html#supplements>).

use std::collections::BTreeMap;
use std::sync::Arc;

use concept_graph::subsumption::Outcome;

use crate::compose::Compose;
use crate::provider::{
    CodeSystemProvider, Concept, ConceptSet, Declaration, Designation, Hierarchy, Identity,
    ImplicitMetadata, Located, MapSelection, Property, ProviderError, Standing, Status, Successor,
};

/// What a supplement adds to one concept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Additions {
    /// Extra designations.
    pub designations: Vec<Designation>,
    /// Extra properties.
    pub properties: Vec<Property>,
}

/// A supplement: additions keyed by the supplemented system's codes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Supplement {
    /// The supplement's own canonical URL.
    pub url: String,
    /// The supplement's version.
    pub version: Option<String>,
    /// The additions per code.
    pub concepts: BTreeMap<String, Additions>,
}

impl Supplement {
    /// The supplement a `CodeSystem` with `content = supplement` describes:
    /// its designations and properties keyed by the supplemented codes.
    #[must_use]
    pub fn from_code_system(model: &crate::fhir_codesystem::model::CodeSystemModel) -> Self {
        let mut concepts = BTreeMap::new();
        for entry in &model.concepts {
            concepts.insert(
                entry.code.clone(),
                Additions {
                    designations: entry.designations.clone(),
                    properties: entry.properties.clone(),
                },
            );
        }
        Self {
            url: model.url.clone(),
            version: Some(model.version.clone()).filter(|v| !v.is_empty()),
            concepts,
        }
    }
}

/// A provider with supplements layered on.
#[derive(Debug)]
pub struct Supplemented {
    inner: Arc<dyn CodeSystemProvider>,
    supplements: Vec<Supplement>,
    declaration: Declaration,
}

impl Supplemented {
    /// Layers `supplements` over `inner`; their languages join the declaration.
    #[must_use]
    pub fn new(inner: Arc<dyn CodeSystemProvider>, supplements: Vec<Supplement>) -> Self {
        let mut declaration = inner.declaration().clone();
        for supplement in &supplements {
            for additions in supplement.concepts.values() {
                for designation in &additions.designations {
                    if let Some(language) = &designation.language
                        && !declaration.languages.contains(language)
                    {
                        declaration.languages.push(language.clone());
                    }
                }
            }
        }
        declaration.languages.sort();
        Self {
            inner,
            supplements,
            declaration,
        }
    }

    /// The supplements applied.
    #[must_use]
    pub fn supplements(&self) -> &[Supplement] {
        &self.supplements
    }

    fn additions(&self, concept: Concept) -> Result<Vec<&Additions>, ProviderError> {
        let code = self.inner.code(concept)?;
        Ok(self
            .supplements
            .iter()
            .filter_map(|supplement| {
                code.as_deref()
                    .and_then(|code| supplement.concepts.get(code))
            })
            .collect())
    }
}

// NOTE: a supplement "extends an existing code system with additional
// designations and properties" (<https://hl7.org/fhir/R4B/codesystem.html>), so
// an answer that is neither stays the supplemented system's own.
impl CodeSystemProvider for Supplemented {
    fn identity(&self) -> &Identity {
        self.inner.identity()
    }

    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn code_system(&self) -> Option<&crate::fhir_codesystem::model::CodeSystemModel> {
        self.inner.code_system()
    }

    fn answers_version(&self, version: &str) -> bool {
        self.inner.answers_version(version)
    }

    fn unserved_properties(&self) -> &[&'static str] {
        self.inner.unserved_properties()
    }

    fn generated_properties(&self) -> &[&'static str] {
        self.inner.generated_properties()
    }

    fn generated_property(
        &self,
        concept: Concept,
        name: &str,
    ) -> Result<Option<Property>, ProviderError> {
        // A supplement adds designations and properties of its own and does
        // not redefine the system's relationships, so a generated property is
        // the supplemented system's answer unchanged.
        self.inner.generated_property(concept, name)
    }

    fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        self.inner.locate(code)
    }

    fn is_expression(&self, code: &str) -> bool {
        self.inner.is_expression(code)
    }

    fn rejection(&self, code: &str) -> Option<String> {
        self.inner.rejection(code)
    }

    fn admits_post_coordination(&self, filter: &crate::filter::Filter) -> bool {
        self.inner.admits_post_coordination(filter)
    }

    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        self.inner.code(concept)
    }

    fn display(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        // NOTE: a supplement adds designations the system lacks, so its term in
        // the requested language beats the system's fallback display
        // (<https://hl7.org/fhir/R4B/codesystem.html#supplements>).
        if let Some(wanted) = language
            && self.inner.designations(concept, Some(wanted))?.is_empty()
            && let Some(added) = self
                .additions(concept)?
                .into_iter()
                .flat_map(|additions| additions.designations.iter())
                .find(|designation| designation.language.as_deref() == Some(wanted))
        {
            return Ok(Some(added.value.clone()));
        }
        if let Some(display) = self.inner.display(concept, language)? {
            return Ok(Some(display));
        }
        Ok(self
            .additions(concept)?
            .into_iter()
            .flat_map(|additions| additions.designations.iter())
            .find(|designation| {
                language.is_none_or(|wanted| designation.language.as_deref() == Some(wanted))
            })
            .map(|designation| designation.value.clone()))
    }

    fn language(&self) -> Option<&str> {
        self.inner.language()
    }

    fn artifact(&self) -> Option<&crate::artifact::Source> {
        self.inner.artifact()
    }

    fn standing(&self) -> Standing {
        self.inner.standing()
    }

    fn definition(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        self.inner.definition(concept)
    }

    fn status(&self, concept: Concept) -> Result<Status, ProviderError> {
        self.inner.status(concept)
    }

    fn inactive(&self) -> Result<ConceptSet, ProviderError> {
        self.inner.inactive()
    }

    fn not_for_ui(&self) -> Result<ConceptSet, ProviderError> {
        self.inner.not_for_ui()
    }

    fn is_postcoordinated(&self, concept: Concept) -> bool {
        self.inner.is_postcoordinated(concept)
    }

    fn designations(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Vec<Designation>, ProviderError> {
        let mut designations = self.inner.designations(concept, language)?;
        designations.extend(
            self.additions(concept)?
                .into_iter()
                .flat_map(|additions| additions.designations.iter())
                .filter(|designation| {
                    language.is_none_or(|wanted| designation.language.as_deref() == Some(wanted))
                })
                .cloned(),
        );
        Ok(designations)
    }

    fn properties(&self, concept: Concept) -> Result<Vec<Property>, ProviderError> {
        let mut properties = self.inner.properties(concept)?;
        properties.extend(
            self.additions(concept)?
                .into_iter()
                .flat_map(|additions| additions.properties.iter())
                .cloned(),
        );
        Ok(properties)
    }

    fn hierarchy(&self) -> Option<&dyn Hierarchy> {
        self.inner.hierarchy()
    }

    fn all(&self) -> Result<ConceptSet, ProviderError> {
        self.inner.all()
    }

    fn search(&self, text: &str, language: Option<&str>) -> Result<ConceptSet, ProviderError> {
        self.inner.search(text, language)
    }

    fn implicit_value_set(&self, url: &str) -> Option<Result<Compose, ProviderError>> {
        self.inner.implicit_value_set(url)
    }

    fn implicit_metadata(&self, url: &str) -> ImplicitMetadata {
        self.inner.implicit_metadata(url)
    }

    fn successors(&self, concept: Concept) -> Result<Vec<Successor>, ProviderError> {
        self.inner.successors(concept)
    }

    fn implicit_concept_map(
        &self,
        url: &str,
        selection: MapSelection<'_>,
    ) -> Option<Result<crate::conceptmap::model::ConceptMapModel, ProviderError>> {
        self.inner.implicit_concept_map(url, selection)
    }

    fn filter(&self, filter: &crate::filter::Filter) -> Result<ConceptSet, ProviderError> {
        self.inner.filter(filter)
    }

    fn filter_all(&self, filters: &[crate::filter::Filter]) -> Result<ConceptSet, ProviderError> {
        self.inner.filter_all(filters)
    }

    fn filter_ordered(
        &self,
        filters: &[crate::filter::Filter],
    ) -> Result<Option<Vec<u32>>, ProviderError> {
        self.inner.filter_ordered(filters)
    }

    fn unclosed(&self, filters: &[crate::filter::Filter]) -> bool {
        self.inner.unclosed(filters)
    }

    fn unclosed_reason(&self) -> Option<String> {
        self.inner.unclosed_reason()
    }

    fn filter_matches(
        &self,
        concept: Concept,
        filter: &crate::filter::Filter,
    ) -> Result<bool, ProviderError> {
        self.inner.filter_matches(concept, filter)
    }

    fn subsumes(&self, a: Concept, b: Concept) -> Result<Option<Outcome>, ProviderError> {
        self.inner.subsumes(a, b)
    }
}

#[cfg(test)]
mod tests {
    /// The method names a block declares: the lines from the one starting with
    /// `opens` to the file's next unindented `}`.
    fn methods(source: &str, opens: &str) -> Vec<String> {
        source
            .lines()
            .skip_while(|line| !line.starts_with(opens))
            .skip(1)
            .take_while(|line| !line.starts_with('}'))
            .filter_map(|line| line.trim_start().strip_prefix("fn "))
            .map(|rest| rest.split(['(', '<']).next().unwrap_or(rest).to_owned())
            .collect()
    }

    #[test]
    fn the_wrapper_answers_every_method_of_the_seam() {
        let seam = methods(include_str!("provider.rs"), "pub trait CodeSystemProvider");
        let wrapper = methods(
            include_str!("supplement.rs"),
            "impl CodeSystemProvider for Supplemented",
        );
        assert!(
            !seam.is_empty() && !wrapper.is_empty(),
            "the scan read no method: `CodeSystemProvider` or its wrapper moved"
        );
        let missing: Vec<&String> = seam.iter().filter(|name| !wrapper.contains(name)).collect();
        assert!(
            missing.is_empty(),
            "`Supplemented` leaves {missing:?} to the trait default, so a supplemented system \
             answers less than the system it supplements; forward each one to `self.inner`, or \
             write the deliberate override a supplement's designations and properties call for"
        );
    }
}
