// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Code system supplements applied to a provider's designations and properties
//! (`CodeSystem.content = supplement`, <https://hl7.org/fhir/R4B/codesystem.html#supplements>).

use std::collections::BTreeMap;
use std::sync::Arc;

use concept_graph::subsumption::Outcome;

use crate::provider::code_system::{SupplementKind, Supplementing};
use crate::provider::compose::Compose;
use crate::provider::declaration::Declaration;
use crate::provider::{
    CodeSystemProvider, Concept, ConceptSet, Designation, DisplayStanding, Hierarchy, Identity,
    ImplicitMetadata, Located, MapSelection, Property, Standing, Status, Successor,
    error::ProviderError,
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
    /// Its kind, and whether it applies without a reference.
    pub supplementing: Supplementing,
}

impl Supplement {
    /// The supplement a `CodeSystem` with `content = supplement` describes:
    /// its designations and properties keyed by the supplemented codes.
    #[must_use]
    pub fn from_code_system(model: &crate::provider::code_system::CodeSystemModel) -> Self {
        let extension_codes = model.supplementing.kind == SupplementKind::ExtensionCodes;
        let mut concepts = BTreeMap::new();
        for entry in &model.concepts {
            let mut designations = Vec::with_capacity(entry.designations.len() + 1);
            // NOTE: an `extension-codes` supplement "defines codes in a reserved code
            // space, with their displays" (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>),
            // so its display is a term in the supplement's language.
            if extension_codes && let Some(display) = &entry.display {
                designations.push(Designation {
                    language: model.language.clone(),
                    use_: None,
                    value: display.clone(),
                    standards_status: None,
                });
            }
            designations.extend(entry.designations.iter().cloned());
            concepts.insert(
                entry.code.clone(),
                Additions {
                    designations,
                    properties: entry.properties.clone(),
                },
            );
        }
        Self {
            url: model.url.clone(),
            version: Some(model.version.clone()).filter(|v| !v.is_empty()),
            concepts,
            supplementing: model.supplementing,
        }
    }

    /// The canonical, versioned when the supplement states a version.
    #[must_use]
    pub fn canonical(&self) -> String {
        match &self.version {
            Some(version) => format!("{}|{version}", self.url),
            None => self.url.clone(),
        }
    }

    /// Whether this supplement defines `code` as an extension code.
    fn defines(&self, code: &str) -> bool {
        self.supplementing.kind == SupplementKind::ExtensionCodes
            && self.concepts.contains_key(code)
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

    /// Whether `filter` is on a property a supplement gives and the code system
    /// itself does not define.
    fn supplements_property(&self, filter: &crate::provider::filter::Filter) -> bool {
        !self
            .inner
            .declaration()
            .properties
            .iter()
            .any(|definition| definition.code == filter.property)
            && self.supplements.iter().any(|supplement| {
                supplement.concepts.values().any(|additions| {
                    additions
                        .properties
                        .iter()
                        .any(|property| property.code == filter.property)
                })
            })
    }

    /// Whether the values `additions` give `filter.property` satisfy `filter`;
    /// `None` for an operator this layer does not evaluate.
    fn satisfies(
        additions: &[&Additions],
        filter: &crate::provider::filter::Filter,
    ) -> Option<bool> {
        use crate::provider::filter::FilterOperator;
        let mut values = additions
            .iter()
            .flat_map(|additions| additions.properties.iter())
            .filter(|property| property.code == filter.property)
            .map(|property| property.value.as_text());
        match filter.op {
            FilterOperator::Equal => Some(values.any(|value| value == filter.value)),
            FilterOperator::In => {
                let wanted: Vec<&str> = filter.value.split(',').map(str::trim).collect();
                Some(values.any(|value| wanted.contains(&value.as_str())))
            }
            FilterOperator::NotIn => {
                let refused: Vec<&str> = filter.value.split(',').map(str::trim).collect();
                Some(!values.any(|value| refused.contains(&value.as_str())))
            }
            FilterOperator::Exists => Some(values.next().is_some() == (filter.value == "true")),
            _ => None,
        }
    }

    /// The concepts of the code system that pass `filter`, a filter on a
    /// supplement's property.
    fn supplement_filter(
        &self,
        filter: &crate::provider::filter::Filter,
    ) -> Result<ConceptSet, ProviderError> {
        let mut selected = ConceptSet::new();
        for index in self.inner.all()? {
            let concept = Concept::new(index);
            let additions = self.additions(concept)?;
            match Self::satisfies(&additions, filter) {
                Some(true) => {
                    selected.insert(index);
                }
                Some(false) => {}
                None => return self.inner.filter(filter),
            }
        }
        Ok(selected)
    }

    /// The extension-codes supplement that defines `concept`, when the
    /// supplemented system holds it only as a reserved code.
    fn defining(&self, concept: Concept) -> Result<Option<&Supplement>, ProviderError> {
        if !matches!(
            self.inner.display_standing(concept)?,
            DisplayStanding::Placeholder | DisplayStanding::Generated
        ) {
            return Ok(None);
        }
        let Some(code) = self.inner.code(concept)? else {
            return Ok(None);
        };
        Ok(self.supplements.iter().find(|s| s.defines(&code)))
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

    fn code_system(&self) -> Option<&crate::provider::code_system::CodeSystemModel> {
        self.inner.code_system()
    }

    fn answers_version(&self, version: &str) -> bool {
        self.inner.answers_version(version)
    }

    fn unserved_properties(&self) -> &[&'static str] {
        self.inner.unserved_properties()
    }

    fn requested_properties(&self) -> &[&'static str] {
        self.inner.requested_properties()
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

    fn advisories(&self, concept: Concept) -> Result<Vec<String>, ProviderError> {
        self.inner.advisories(concept)
    }

    fn admits_post_coordination(&self, filters: &[crate::provider::filter::Filter]) -> bool {
        self.inner.admits_post_coordination(filters)
    }

    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        self.inner.code(concept)
    }

    fn display(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        // NOTE: a reserved code an extension-codes supplement defines has the
        // supplement's display (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
        if let Some(defining) = self.defining(concept)?
            && let Some(code) = self.inner.code(concept)?
            && let Some(additions) = defining.concepts.get(&code)
            && let Some(display) = additions
                .designations
                .iter()
                .find(|d| language.is_none_or(|wanted| d.language.as_deref() == Some(wanted)))
                .or_else(|| additions.designations.first())
        {
            return Ok(Some(display.value.clone()));
        }
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

    // NOTE: a global extension-codes supplement is "as if the code system defined its
    // codes", and another one's display "is a warning" when it differs
    // (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>, CodeSystem supplements).
    fn display_standing(&self, concept: Concept) -> Result<DisplayStanding, ProviderError> {
        Ok(match self.defining(concept)? {
            Some(defining) if defining.supplementing.global => DisplayStanding::Defined,
            Some(_) => DisplayStanding::Suggested,
            None => self.inner.display_standing(concept)?,
        })
    }

    fn artifact(&self) -> Option<&crate::provider::source::Source> {
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
        // NOTE: for a code a grammar makes, "the display the grammar generates stays a
        // valid display" beside the supplement's (the terminology ecosystem IG, CodeSystem supplements).
        if self.defining(concept)?.is_some()
            && self.inner.display_standing(concept)? == DisplayStanding::Generated
            && let Some(generated) = self.inner.display(concept, language)?
        {
            designations.push(Designation {
                language: self.inner.language().map(str::to_owned),
                use_: None,
                value: generated,
                standards_status: None,
            });
        }
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

    // NOTE: a text filter searches the designations of the supplements in use, in
    // the language searched (terminology ecosystem IG test cases 1.11.0,
    // `search-all-supplement-fr-yes`).
    fn search(&self, text: &str, language: Option<&str>) -> Result<ConceptSet, ProviderError> {
        let mut hits = self.inner.search(text, language)?;
        let words = designation_index::tokenize::prefixes(text);
        let searched = language.or_else(|| self.inner.language());
        for supplement in &self.supplements {
            for (code, additions) in &supplement.concepts {
                let terms: Vec<&str> = additions
                    .designations
                    .iter()
                    .filter(|d| {
                        d.language
                            .as_deref()
                            .zip(searched)
                            .is_some_and(|(l, wanted)| crate::text_match::same_language(l, wanted))
                    })
                    .map(|d| d.value.as_str())
                    .collect();
                if !terms.is_empty()
                    && crate::text_match::matches_all(&words, &terms)
                    && let Some(located) = self.inner.locate(code)?
                {
                    hits.insert(located.concept.index());
                }
            }
        }
        Ok(hits)
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
    ) -> Option<Result<crate::provider::concept_map::ConceptMapModel, ProviderError>> {
        self.inner.implicit_concept_map(url, selection)
    }

    fn implicit_concept_maps(
        &self,
        source: &str,
        target: &str,
    ) -> Result<Vec<String>, ProviderError> {
        self.inner.implicit_concept_maps(source, target)
    }

    // NOTE: a supplement adds properties to the concepts of the code system it
    // supplements (<https://hl7.org/fhir/R5/codesystem-definitions.html#CodeSystem.supplements>),
    // so a filter on one selects by the supplement's values.
    fn filter(
        &self,
        filter: &crate::provider::filter::Filter,
    ) -> Result<ConceptSet, ProviderError> {
        if self.supplements_property(filter) {
            return self.supplement_filter(filter);
        }
        self.inner.filter(filter)
    }

    fn filter_in_language(
        &self,
        filter: &crate::provider::filter::Filter,
        requested: Option<&str>,
    ) -> Option<Result<ConceptSet, ProviderError>> {
        self.inner.filter_in_language(filter, requested)
    }

    fn filter_all(
        &self,
        filters: &[crate::provider::filter::Filter],
    ) -> Result<ConceptSet, ProviderError> {
        let (own, base): (Vec<_>, Vec<_>) = filters
            .iter()
            .cloned()
            .partition(|filter| self.supplements_property(filter));
        if own.is_empty() {
            return self.inner.filter_all(filters);
        }
        let mut selected = if base.is_empty() {
            self.inner.all()?
        } else {
            self.inner.filter_all(&base)?
        };
        for filter in &own {
            selected &= self.supplement_filter(filter)?;
        }
        Ok(selected)
    }

    fn filter_ordered(
        &self,
        filters: &[crate::provider::filter::Filter],
    ) -> Result<Option<Vec<u32>>, ProviderError> {
        if filters
            .iter()
            .any(|filter| self.supplements_property(filter))
        {
            return Ok(None);
        }
        self.inner.filter_ordered(filters)
    }

    fn unclosed(&self, filters: &[crate::provider::filter::Filter]) -> bool {
        let base: Vec<_> = filters
            .iter()
            .filter(|filter| !self.supplements_property(filter))
            .cloned()
            .collect();
        self.inner.unclosed(&base)
    }

    fn unclosed_reason(&self) -> Option<String> {
        self.inner.unclosed_reason()
    }

    fn filter_matches(
        &self,
        concept: Concept,
        filter: &crate::provider::filter::Filter,
    ) -> Result<bool, ProviderError> {
        if self.supplements_property(filter)
            && let Some(passes) = Self::satisfies(&self.additions(concept)?, filter)
        {
            return Ok(passes);
        }
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
