// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The standalone ICF releases, served with their qualifier grammar.
//!
//! `http://hl7.org/fhir/sid/icf` (`2001a` and `2017a`) comes from an
//! operator-supplied `CodeSystem`, and the grammar is the one the terminology
//! ecosystem IG defines (`HL7/fhir-tx-ecosystem-ig` at `9c2bbce`,
//! `input/pagecontent/icf.md`).
//!
//! The resource carries the categories, the headings and the qualifier scales
//! (`q1` to `q6` and their values). A qualified code (`s7300.183`) is "a code
//! in its own right" that no enumeration holds, so it is interned the first
//! time it is met, after the ordinals of the resource's concepts. FerroTERM
//! ships no ICF content: a deployment loads the release it is licensed for.

mod qualifier;

use std::sync::Arc;

use concept_graph::subsumption::Outcome;

use crate::fhir_codesystem::provider::{BuildError, FhirCodeSystem};
use crate::provider::code_system::CodeSystemModel;
use crate::provider::declaration::{Declaration, HierarchyMeaning};
use crate::provider::filter::{Filter, FilterOperator};
use crate::provider::{
    CodeSystemProvider, Concept, ConceptSet, Designation, DisplayStanding, Hierarchy, Identity,
    Located, Property, PropertyValue, Standing, Status, error::ProviderError,
};
use crate::registries::interned::Interned;
use qualifier::{NOT_SPECIFIED, PROPERTIES, Qualified};

/// The system URI (`icf.md`, Summary).
pub const URL: &str = "http://hl7.org/fhir/sid/icf";

/// The `kind` of an ICF category, as against a qualifier scale or value.
const PRIMARY: &str = "primary";

/// A standalone ICF release as a provider.
#[derive(Debug)]
pub struct StandaloneIcf {
    /// The resource's concepts, served as an is-a tree.
    inner: FhirCodeSystem,
    /// The resource as it was published, for reads of the `CodeSystem`.
    model: CodeSystemModel,
    /// The qualified codes met so far.
    qualified: Interned,
    /// The first ordinal past the resource's concepts.
    base: u32,
}

impl StandaloneIcf {
    /// Serves `model`, an ICF release.
    ///
    /// "Hierarchy: A strict tree: component, chapter, block, then categories
    /// at up to four levels. Subsumption follows it" (`icf.md`, Summary), so
    /// the nesting is read as is-a whatever `hierarchyMeaning` the release
    /// states.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError`] when the resource cannot be served.
    pub fn new(model: CodeSystemModel) -> Result<Self, BuildError> {
        let mut tree = model.clone();
        tree.hierarchy_meaning = Some(HierarchyMeaning::IsA);
        let inner = FhirCodeSystem::new(tree)?;
        let base = inner
            .all()
            .map_or(0, |all| all.max().map_or(0, |max| max.saturating_add(1)));
        Ok(Self {
            inner,
            model,
            qualified: Interned::default(),
            base,
        })
    }

    /// The qualified code at `concept`, when it is one.
    fn qualified_code(&self, concept: Concept) -> Option<String> {
        let offset = concept.index().checked_sub(self.base)?;
        self.qualified.code(Concept::new(offset))
    }

    /// The qualified code at `concept` with its category's ordinal.
    fn qualified_at(&self, concept: Concept) -> Result<Option<(String, Concept)>, ProviderError> {
        let Some(code) = self.qualified_code(concept) else {
            return Ok(None);
        };
        let Some(parts) = qualifier::split(&code) else {
            return Ok(None);
        };
        let Some(category) = self.inner.locate(parts.category)? else {
            return Ok(None);
        };
        Ok(Some((code, category.concept)))
    }

    /// Whether `concept` is a selectable category: "only a selectable
    /// category can be qualified - not a component, chapter or block".
    fn qualifiable(&self, concept: Concept) -> Result<bool, ProviderError> {
        if self.inner.status(concept)?.abstract_concept {
            return Ok(false);
        }
        Ok(self.inner.properties(concept)?.iter().any(|property| {
            property.code == "kind" && property.value == PropertyValue::Code(PRIMARY.to_owned())
        }))
    }

    /// Whether every digit of `parts` is a value of its position's scale:
    /// "each digit must be one of the values of its position's scale".
    fn scaled(&self, parts: &Qualified<'_>) -> Result<bool, ProviderError> {
        for (position, digit) in parts.values() {
            if self
                .inner
                .locate(&Qualified::scale_value(position, digit))?
                .is_none()
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The generated display of a qualified code (`icf.md`, Display Terms):
    /// the category's title, a `:`, then each qualifier that is not `8`.
    fn qualified_display(
        &self,
        parts: &Qualified<'_>,
        category: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        let Some(title) = self.inner.display(category, language)? else {
            return Ok(None);
        };
        let mut said = Vec::new();
        for (position, digit) in parts.values() {
            if digit == NOT_SPECIFIED {
                continue;
            }
            let Some(value) = self
                .inner
                .locate(&Qualified::scale_value(position, digit))?
            else {
                continue;
            };
            let Some(meaning) = self.inner.display(value.concept, language)? else {
                continue;
            };
            said.push(match position.label {
                Some(label) => format!("{label} {meaning}"),
                None => meaning,
            });
        }
        if said.is_empty() {
            return Ok(Some(title));
        }
        Ok(Some(format!("{title}: {}", said.join(", "))))
    }

    /// Whether the qualified code at `concept` passes `filter` (`icf.md`,
    /// Filter Properties); `None` when `concept` is no qualified code.
    fn qualified_matches(
        &self,
        concept: Concept,
        filter: &Filter,
    ) -> Result<Option<bool>, ProviderError> {
        let Some((code, category)) = self.qualified_at(concept)? else {
            return Ok(None);
        };
        let Some(parts) = qualifier::split(&code) else {
            return Ok(None);
        };
        let value = filter.value.trim();
        let property = filter.property.as_str();
        Ok(Some(match (property, filter.op) {
            ("qualified", FilterOperator::Equal) => value == "true",
            (name, FilterOperator::Exists) if PROPERTIES.contains(&name) => {
                parts.states(name) == (value == "true")
            }
            // NOTE: "A qualified code is in the results when its category is: `is-a b1`
            // includes `b1100.2`" (`icf.md`, Filter Properties), and it sits one step
            // below its category.
            ("concept" | "code", FilterOperator::IsA | FilterOperator::IsNotA) => {
                self.inner.filter_matches(category, filter)?
            }
            ("concept" | "code", FilterOperator::DescendentOf) => {
                self.inner.code(category)?.as_deref() == Some(value)
                    || self.inner.filter_matches(category, filter)?
            }
            ("concept" | "code", FilterOperator::ChildOf) => {
                self.inner.code(category)?.as_deref() == Some(value)
            }
            ("concept" | "code", FilterOperator::Equal) => code == value,
            ("concept" | "code", FilterOperator::In) => value.split(',').any(|c| c.trim() == code),
            ("concept" | "code", _) => false,
            _ => self.inner.filter_matches(category, filter)?,
        }))
    }
}

impl CodeSystemProvider for StandaloneIcf {
    fn identity(&self) -> &Identity {
        self.inner.identity()
    }

    fn declaration(&self) -> &Declaration {
        self.inner.declaration()
    }

    fn code_system(&self) -> Option<&CodeSystemModel> {
        Some(&self.model)
    }

    fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        if let Some(located) = self.inner.locate(code)? {
            return Ok(Some(located));
        }
        let Some(parts) = qualifier::split(code) else {
            return Ok(None);
        };
        let Some(category) = self.inner.locate(parts.category)? else {
            return Ok(None);
        };
        if !self.qualifiable(category.concept)? || !self.scaled(&parts)? {
            return Ok(None);
        }
        let interned = self.qualified.intern(code)?;
        let Some(ordinal) = self.base.checked_add(interned.index()) else {
            return Ok(None);
        };
        Ok(Some(Located {
            concept: Concept::new(ordinal),
            code: code.to_owned(),
        }))
    }

    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        match self.qualified_code(concept) {
            Some(code) => Ok(Some(code)),
            None => self.inner.code(concept),
        }
    }

    fn display(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        let Some((code, category)) = self.qualified_at(concept)? else {
            return self.inner.display(concept, language);
        };
        let Some(parts) = qualifier::split(&code) else {
            return Ok(None);
        };
        self.qualified_display(&parts, category, language)
    }

    fn display_standing(&self, concept: Concept) -> Result<DisplayStanding, ProviderError> {
        Ok(if self.qualified_code(concept).is_some() {
            DisplayStanding::Generated
        } else {
            DisplayStanding::Defined
        })
    }

    fn definition(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        self.inner.definition(concept)
    }

    fn language(&self) -> Option<&str> {
        self.inner.language()
    }

    fn standing(&self) -> Standing {
        self.inner.standing()
    }

    fn status(&self, concept: Concept) -> Result<Status, ProviderError> {
        match self.qualified_at(concept)? {
            Some((_, category)) => self.inner.status(category),
            None => self.inner.status(concept),
        }
    }

    // NOTE: the category's title "is not a valid display for a qualified code (unless every
    // qualifier is `8`)" (`icf.md`, Display Terms), so a qualified code has no designation.
    fn designations(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Vec<Designation>, ProviderError> {
        if self.qualified_code(concept).is_some() {
            return Ok(Vec::new());
        }
        self.inner.designations(concept, language)
    }

    /// "A qualified code has its category's properties as well, other than
    /// `parent`", its category as `parent`, and each qualifier as its scale
    /// value, `8` included (`icf.md`, Properties).
    fn properties(&self, concept: Concept) -> Result<Vec<Property>, ProviderError> {
        let Some((code, category)) = self.qualified_at(concept)? else {
            return self.inner.properties(concept);
        };
        let mut out: Vec<Property> = self
            .inner
            .properties(category)?
            .into_iter()
            .filter(|property| property.code != "parent" && property.code != "child")
            .collect();
        if let Some(parent) = self.inner.code(category)? {
            out.push(Property {
                code: String::from("parent"),
                value: PropertyValue::Code(parent),
                ..Property::default()
            });
        }
        if let Some(parts) = qualifier::split(&code) {
            for (position, digit) in parts.values() {
                out.push(Property {
                    code: position.property.to_owned(),
                    value: PropertyValue::Code(Qualified::scale_value(position, digit)),
                    ..Property::default()
                });
            }
        }
        Ok(out)
    }

    fn hierarchy(&self) -> Option<&dyn Hierarchy> {
        Some(self)
    }

    fn all(&self) -> Result<ConceptSet, ProviderError> {
        self.inner.all()
    }

    fn search(&self, text: &str, language: Option<&str>) -> Result<ConceptSet, ProviderError> {
        self.inner.search(text, language)
    }

    // NOTE: qualified codes "cannot be enumerated", so a selection holds the plain codes and
    // a qualifier filter that must be stated selects none of them (`icf.md`, Filter Properties).
    fn filter(&self, filter: &Filter) -> Result<ConceptSet, ProviderError> {
        let value = filter.value.trim();
        match (filter.property.as_str(), filter.op) {
            ("qualified", FilterOperator::Equal) => {
                if value == "true" {
                    Ok(ConceptSet::new())
                } else {
                    self.inner.all()
                }
            }
            (name, FilterOperator::Exists) if PROPERTIES.contains(&name) => {
                if value == "true" {
                    Ok(ConceptSet::new())
                } else {
                    self.inner.all()
                }
            }
            _ => self.inner.filter(filter),
        }
    }

    fn filter_matches(&self, concept: Concept, filter: &Filter) -> Result<bool, ProviderError> {
        if let Some(passes) = self.qualified_matches(concept, filter)? {
            return Ok(passes);
        }
        let value = filter.value.trim();
        match (filter.property.as_str(), filter.op) {
            ("qualified", FilterOperator::Equal) => Ok(value == "false"),
            (name, FilterOperator::Exists) if PROPERTIES.contains(&name) => Ok(value == "false"),
            _ => self.inner.filter_matches(concept, filter),
        }
    }

    /// "an include that admits qualified codes - any include without
    /// `qualified = false` - expands to the plain codes it contains, and the
    /// expansion is marked as incomplete" (`icf.md`, Filter Properties).
    fn unclosed(&self, filters: &[Filter]) -> bool {
        !filters.iter().any(|filter| {
            filter.property == "qualified"
                && filter.op == FilterOperator::Equal
                && filter.value.trim() == "false"
        })
    }

    fn subsumes(&self, _a: Concept, _b: Concept) -> Result<Option<Outcome>, ProviderError> {
        Ok(None)
    }
}

impl Hierarchy for StandaloneIcf {
    fn parents(&self, concept: Concept) -> ConceptSet {
        match self.qualified_at(concept) {
            Ok(Some((_, category))) => std::iter::once(category.index()).collect(),
            _ => self
                .inner
                .hierarchy()
                .map(|tree| tree.parents(concept))
                .unwrap_or_default(),
        }
    }

    fn children(&self, concept: Concept) -> ConceptSet {
        if self.qualified_code(concept).is_some() {
            return ConceptSet::new();
        }
        self.inner
            .hierarchy()
            .map(|tree| tree.children(concept))
            .unwrap_or_default()
    }

    fn ancestors(&self, concept: Concept) -> ConceptSet {
        match self.qualified_at(concept) {
            Ok(Some((_, category))) => {
                let mut above = self
                    .inner
                    .hierarchy()
                    .map(|tree| tree.ancestors(category))
                    .unwrap_or_default();
                above.insert(category.index());
                above
            }
            _ => self
                .inner
                .hierarchy()
                .map(|tree| tree.ancestors(concept))
                .unwrap_or_default(),
        }
    }

    fn descendants(&self, concept: Concept) -> ConceptSet {
        if self.qualified_code(concept).is_some() {
            return ConceptSet::new();
        }
        self.inner
            .hierarchy()
            .map(|tree| tree.descendants(concept))
            .unwrap_or_default()
    }
    /// "a qualified code is subsumed by its category" (`icf.md`, Summary), which
    /// no enumeration of descendants holds, so the answer reads the ancestors.
    fn subsumes(&self, a: Concept, b: Concept) -> Outcome {
        if a == b {
            Outcome::Equivalent
        } else if self.ancestors(b).contains(a.index()) {
            Outcome::Subsumes
        } else if self.ancestors(a).contains(b.index()) {
            Outcome::SubsumedBy
        } else {
            Outcome::NotSubsumed
        }
    }
}

/// `model` served by its own provider: a standalone ICF release by
/// [`StandaloneIcf`], every other `CodeSystem` by [`FhirCodeSystem`].
///
/// # Errors
///
/// Returns [`BuildError`] when the resource cannot be served.
pub fn serve(model: CodeSystemModel) -> Result<Arc<dyn CodeSystemProvider>, BuildError> {
    if model.url == URL {
        return Ok(Arc::new(StandaloneIcf::new(model)?));
    }
    Ok(Arc::new(FhirCodeSystem::new(model)?))
}
