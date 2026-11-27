// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Serving a [`CodeSystemModel`] through the seam.

use std::collections::{BTreeMap, BTreeSet};

use concept_graph::closure::Closure;
use concept_graph::csr::Csr;
use concept_graph::ordinal::Ordinal;
use roaring::RoaringBitmap;

use crate::provider::code_system::{
    CHILD, CodeSystemModel, ConceptEntry, DEPRECATED, DEPRECATION_DATE, INACTIVE, NOT_SELECTABLE,
    PARENT, RETIRED, RETIREMENT_DATE, STATUS,
};
use crate::provider::declaration::{
    Capability, Compositional, ContentMode, Declaration, HierarchyMeaning, PropertyDefinition,
    PropertyKind,
};
use crate::provider::filter::Filter;
use crate::provider::{
    CodeSystemProvider, Concept, ConceptSet, Designation, DisplayStanding, Hierarchy, Identity,
    Located, Property, PropertyValue, Status, error::ProviderError,
};
use crate::text_match;

/// A failure to build the provider from a model.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// The concept hierarchy has a cycle.
    #[error("the hierarchy of `{url}` is not acyclic")]
    Cycle {
        /// The system.
        url: String,
        /// The cause.
        #[source]
        source: concept_graph::closure::ClosureError,
    },
    /// The concept hierarchy does not fit the graph arrays.
    #[error("the hierarchy of `{url}` cannot be built")]
    Hierarchy {
        /// The system.
        url: String,
        /// The cause.
        #[source]
        source: concept_graph::csr::CsrError,
    },
    /// Too many concepts for a `u32` ordinal.
    #[error("`{url}` has more concepts than the provider can number")]
    TooMany {
        /// The system.
        url: String,
    },
    /// A reserved code space states a pattern that is no regular expression.
    #[error("a reserved code space of `{url}` is no regular expression")]
    ReservedCodeSpace {
        /// The system.
        url: String,
        /// The cause.
        #[source]
        source: regex::Error,
    },
}

/// The hierarchy of a system whose `hierarchyMeaning` is `is-a`.
#[derive(Debug)]
struct ModelHierarchy {
    parents: Csr,
    children: Csr,
    closure: Closure,
}

impl Hierarchy for ModelHierarchy {
    fn parents(&self, concept: Concept) -> ConceptSet {
        self.parents
            .neighbours(Ordinal::new(concept.index()))
            .iter()
            .copied()
            .collect()
    }

    fn children(&self, concept: Concept) -> ConceptSet {
        self.children
            .neighbours(Ordinal::new(concept.index()))
            .iter()
            .copied()
            .collect()
    }

    fn ancestors(&self, concept: Concept) -> ConceptSet {
        self.closure
            .ancestors(Ordinal::new(concept.index()))
            .clone()
    }

    fn descendants(&self, concept: Concept) -> ConceptSet {
        self.closure
            .descendants(Ordinal::new(concept.index()))
            .clone()
    }
}

/// A code system served from a `CodeSystem` resource.
#[derive(Debug)]
pub struct FhirCodeSystem {
    identity: Identity,
    declaration: Declaration,
    model: CodeSystemModel,
    /// The comparison key of each code (lowercased when case-insensitive) to its ordinal.
    codes: BTreeMap<String, u32>,
    hierarchy: Option<ModelHierarchy>,
    all: RoaringBitmap,
    reserved: super::reserved::Reserved,
}

impl FhirCodeSystem {
    /// Builds the provider; the hierarchy exists when `hierarchyMeaning` is
    /// `is-a` or absent, with no edges when no concept names a parent.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError`] for a cyclic hierarchy or too many concepts.
    pub fn new(mut model: CodeSystemModel) -> Result<Self, BuildError> {
        // NOTE: no FHIR spec fixes an expansion order; concepts are numbered in code
        // order so paging over ordinals is paging over codes, as for every built system.
        model.concepts.sort_by(|a, b| a.code.cmp(&b.code));
        // The error names the system; the conversion error adds nothing to it.
        let Ok(count) = u32::try_from(model.concepts.len()) else {
            return Err(BuildError::TooMany {
                url: model.url.clone(),
            });
        };
        let key = |code: &str| {
            if model.case_sensitive {
                code.to_owned()
            } else {
                code.to_lowercase()
            }
        };
        let mut codes = BTreeMap::new();
        for (i, concept) in model.concepts.iter().enumerate() {
            codes.insert(key(&concept.code), u32::try_from(i).unwrap_or(u32::MAX));
        }
        let hierarchy = build_hierarchy(&model, count)?;
        let languages = languages_of(&model);
        let properties = with_standard_properties(&model.properties);
        let mut capabilities = BTreeSet::new();
        if hierarchy.is_some() {
            capabilities.insert(Capability::Subsumption);
        }
        if matches!(model.content, ContentMode::Complete | ContentMode::Fragment) {
            capabilities.insert(Capability::Enumeration);
        }
        let identity = Identity {
            url: model.url.clone(),
            version: model.version.clone(),
            name: model.name.clone(),
            title: model.title.clone(),
            version_needed: model.version_needed,
        };
        let declaration = Declaration {
            content: model.content,
            case_sensitive: model.case_sensitive,
            hierarchy_meaning: model.hierarchy_meaning,
            // NOTE: this provider serves the concepts a resource enumerates and
            // evaluates no grammar, so a declared grammar is `Defined`: the code
            // system has it, this server does not support it.
            compositional: if model.compositional {
                Compositional::Defined
            } else {
                Compositional::None
            },
            languages,
            properties,
            filters: model.filters.clone(),
            capabilities,
            implicit_forms: Vec::new(),
        };
        let mut all = RoaringBitmap::new();
        all.insert_range(0..count);
        let reserved = super::reserved::Reserved::new(&model.reserved_code_spaces, count).map_err(
            |source| BuildError::ReservedCodeSpace {
                url: model.url.clone(),
                source,
            },
        )?;
        Ok(Self {
            identity,
            declaration,
            model,
            codes,
            hierarchy,
            all,
            reserved,
        })
    }

    /// The model this provider serves.
    #[must_use]
    pub fn model(&self) -> &CodeSystemModel {
        &self.model
    }

    fn entry(&self, concept: Concept) -> Option<&ConceptEntry> {
        self.model
            .concepts
            .get(usize::try_from(concept.index()).ok()?)
    }

    /// Refuses a read on a system whose content the resource does not carry.
    ///
    /// `not-present` defines no codes at all and `example` a handful with "no
    /// useful intent" (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>),
    /// so neither can say whether a code is valid.
    fn complete_enough(&self) -> Result<(), ProviderError> {
        match self.model.content {
            ContentMode::NotPresent | ContentMode::Example => {
                Err(ProviderError::IncompleteContent {
                    system: self.model.url.clone(),
                    content: self.model.content.code(),
                })
            }
            ContentMode::Fragment | ContentMode::Complete | ContentMode::Supplement => Ok(()),
        }
    }

    /// Whether `code` is the standard `notSelectable` property: by the URI its
    /// declaration names, or by its code when the declaration names no URI
    /// (<https://hl7.org/fhir/R4B/codesystem-concept-properties.html>).
    fn abstract_property(&self, code: &str) -> bool {
        // NOTE: `CodeSystem.property.uri` is "the formal meaning of the property"
        // (<https://hl7.org/fhir/R4B/codesystem-definitions.html#CodeSystem.property.uri>),
        // so a code declared under another URI is not the standard property.
        let declared = self.model.properties.iter().find(|p| p.code == code);
        match declared.and_then(|p| p.uri.as_deref()) {
            Some(uri) => uri == "http://hl7.org/fhir/concept-properties#notSelectable",
            None => code == NOT_SELECTABLE,
        }
    }

    /// The concept's standing, read from the standard status properties.
    ///
    /// `inactive = true` says so outright, and "the status property may also
    /// be used to indicate that a concept is inactive", with `retired` the
    /// value that ends a concept's life
    /// (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>).
    fn status_of(&self, entry: &ConceptEntry) -> Status {
        let mut abstract_concept = false;
        let mut flagged = false;
        let mut stated_retired = false;
        let mut retired_by_date = false;
        let mut deprecated = false;
        let mut at: Option<jiff::Timestamp> = None;
        for property in &entry.properties {
            match (property.code.as_str(), &property.value) {
                (INACTIVE, PropertyValue::Boolean(true)) => flagged = true,
                // NOTE: a concept "deprecated but not inactive can still be
                // used", so no deprecation marker retires one
                // (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>).
                (STATUS, PropertyValue::Code(status)) if status == RETIRED => stated_retired = true,
                (STATUS, PropertyValue::Code(status)) if status == DEPRECATED => deprecated = true,
                (DEPRECATED | DEPRECATION_DATE, PropertyValue::DateTime(date)) => {
                    let now = *at.get_or_insert_with(crate::clock::now);
                    deprecated = deprecated || super::date_time::retires_at(date, now);
                }
                (RETIREMENT_DATE, PropertyValue::DateTime(date)) => {
                    // NOTE: no FHIR/SNOMED spec governs this: our own design
                    // reads the date as at the request, once per concept, and
                    // a value that is no `dateTime` as a retirement in force.
                    let now = *at.get_or_insert_with(crate::clock::now);
                    retired_by_date = retired_by_date || super::date_time::retires_at(date, now);
                }
                (code, PropertyValue::Boolean(true)) if self.abstract_property(code) => {
                    abstract_concept = true;
                }
                _ => {}
            }
        }
        // NOTE: no FHIR/SNOMED spec governs this: our own design is that any
        // marker saying inactive wins, and that a `status` the specification
        // does not list as ending a concept's life, such as `withdrawn`, does not.
        let reason = if stated_retired {
            Some(RETIRED.to_owned())
        } else if flagged || retired_by_date {
            // NOTE: the ecosystem's `status` output is the concept's status
            // "when its code system states one", so a date alone names none
            // (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
            Some(String::from(INACTIVE))
        } else {
            None
        };
        // NOTE: a deprecation stated through the standard properties earns the
        // standards-status extension's note and nothing more, the concept stays
        // active (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>).
        let standards_status = entry
            .standards_status
            .clone()
            .or_else(|| (deprecated && reason.is_none()).then(|| DEPRECATED.to_owned()));
        Status {
            standards_status,
            active: reason.is_none(),
            inactive_reason: reason,
            abstract_concept,
            codeless: false,
        }
    }

    /// The `parent` and `child` properties the hierarchy states for `concept`.
    fn relation_properties(&self, concept: Concept) -> Result<Vec<Property>, ProviderError> {
        let Some(hierarchy) = &self.hierarchy else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for (code, related) in [
            (PARENT, hierarchy.parents(concept)),
            (CHILD, hierarchy.children(concept)),
        ] {
            for index in related {
                if let Some(related_code) = self.code(Concept::new(index))? {
                    out.push(Property {
                        code: code.to_owned(),
                        value: PropertyValue::Code(related_code),
                        ..Property::default()
                    });
                }
            }
        }
        Ok(out)
    }
}

/// Whether `code` names a property `$lookup` renders from the concept's status
/// or hierarchy instead of its stated properties.
fn rendered_elsewhere(code: &str, abstract_concept: bool) -> bool {
    code == INACTIVE
        || (code == NOT_SELECTABLE && abstract_concept)
        || code == PARENT
        || code == CHILD
}

impl CodeSystemProvider for FhirCodeSystem {
    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn code_system(&self) -> Option<&CodeSystemModel> {
        Some(&self.model)
    }

    fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        self.complete_enough()?;
        let key = if self.model.case_sensitive {
            code.to_owned()
        } else {
            code.to_lowercase()
        };
        let Some(&ordinal) = self.codes.get(&key) else {
            return self.reserved.locate(code);
        };
        Ok(Some(Located {
            concept: Concept::new(ordinal),
            code: self
                .model
                .concepts
                .get(usize::try_from(ordinal).unwrap_or(usize::MAX))
                .map_or_else(|| code.to_owned(), |c| c.code.clone()),
        }))
    }

    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        Ok(self
            .entry(concept)
            .map(|c| c.code.clone())
            .or_else(|| self.reserved.code(concept)))
    }

    fn display_standing(&self, concept: Concept) -> Result<DisplayStanding, ProviderError> {
        Ok(if self.reserved.code(concept).is_some() {
            DisplayStanding::Placeholder
        } else {
            DisplayStanding::Defined
        })
    }

    fn display(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        let Some(entry) = self.entry(concept) else {
            return Ok(self.reserved.display(concept));
        };
        // NOTE: a concept's display is already in `CodeSystem.language`, so
        // a request for that language reads it, not a designation sharing it
        // (<https://hl7.org/fhir/R5/codesystem-definitions.html#CodeSystem.language>).
        let asked_for_its_own = language.is_some_and(|asked| {
            self.model
                .language
                .as_deref()
                .is_some_and(|own| text_match::same_language(own, asked))
        });
        if let Some(language) = language
            && !(asked_for_its_own && entry.display.is_some())
            && let Some(designation) = entry.designations.iter().find(|d| {
                d.language
                    .as_deref()
                    .is_some_and(|l| text_match::same_language(l, language))
            })
        {
            return Ok(Some(designation.value.clone()));
        }
        Ok(entry
            .display
            .clone()
            .or_else(|| entry.designations.first().map(|d| d.value.clone())))
    }

    fn definition(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        Ok(self.entry(concept).and_then(|c| c.definition.clone()))
    }

    fn language(&self) -> Option<&str> {
        self.model.language.as_deref()
    }

    fn standing(&self) -> crate::provider::Standing {
        crate::provider::Standing {
            status: self.model.status.clone(),
            experimental: self.model.experimental.unwrap_or(false),
            standards_status: self.model.standards_status.clone(),
        }
    }

    fn status(&self, concept: Concept) -> Result<Status, ProviderError> {
        Ok(match self.entry(concept) {
            Some(entry) => self.status_of(entry),
            // NOTE: a reserved code is "a valid code in the code system"
            // (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>), so it is active.
            None => Status {
                active: self.reserved.code(concept).is_some(),
                ..Status::default()
            },
        })
    }

    fn designations(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Vec<Designation>, ProviderError> {
        Ok(self
            .entry(concept)
            .map(|c| {
                c.designations
                    .iter()
                    .filter(|d| {
                        language.is_none_or(|wanted| {
                            d.language
                                .as_deref()
                                .is_some_and(|l| text_match::same_language(l, wanted))
                        })
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    fn properties(&self, concept: Concept) -> Result<Vec<Property>, ProviderError> {
        let Some(entry) = self.entry(concept) else {
            return Ok(self
                .reserved
                .code(concept)
                .map(|_| Property {
                    code: INACTIVE.to_owned(),
                    value: PropertyValue::Boolean(false),
                    ..Property::default()
                })
                .into_iter()
                .collect());
        };
        let status = self.status_of(entry);
        // NOTE: no FHIR/SNOMED spec governs this: our own design answers the
        // derived `inactive`, so a read of the resource still shows the flag
        // the publisher wrote where another marker contradicts it.
        let mut out = vec![Property {
            code: INACTIVE.to_owned(),
            value: PropertyValue::Boolean(!status.active),
            ..Property::default()
        }];
        if status.abstract_concept {
            out.push(Property {
                code: NOT_SELECTABLE.to_owned(),
                value: PropertyValue::Boolean(true),
                ..Property::default()
            });
        }
        out.extend(
            entry
                .properties
                .iter()
                .filter(|property| !rendered_elsewhere(&property.code, status.abstract_concept))
                .cloned(),
        );
        out.extend(self.relation_properties(concept)?);
        Ok(out)
    }

    fn hierarchy(&self) -> Option<&dyn Hierarchy> {
        match &self.hierarchy {
            Some(hierarchy) => Some(hierarchy),
            None => None,
        }
    }

    /// A `fragment` resource holds "a subset of the code system", so a
    /// selection over it leaves out the codes the system defines outside the
    /// resource (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>); a
    /// `complete` one holds every code the system defines.
    ///
    /// A system with reserved code spaces holds codes no expansion can list,
    /// so an include of all of it is unclosed too; a filter never selects a
    /// reserved code, so a filtered include is not
    /// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>, `CodeSystem` supplements).
    fn unclosed(&self, filters: &[Filter]) -> bool {
        matches!(self.model.content, ContentMode::Fragment)
            || (filters.is_empty() && !self.reserved.is_empty())
    }

    // NOTE: "a filter never selects a code in a reserved code space - not even a
    // `concept in` or `code in` filter that lists it"
    // (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>), so a selection keeps defined concepts only.
    fn filter(&self, filter: &Filter) -> Result<ConceptSet, ProviderError> {
        Ok(crate::provider::filter::evaluate(self, filter)? & &self.all)
    }

    fn all(&self) -> Result<ConceptSet, ProviderError> {
        if !self
            .declaration
            .capabilities
            .contains(&Capability::Enumeration)
        {
            return Err(ProviderError::NotEnumerable);
        }
        Ok(self.all.clone())
    }

    fn search(&self, text: &str, language: Option<&str>) -> Result<ConceptSet, ProviderError> {
        self.complete_enough()?;
        let words = designation_index::tokenize::prefixes(text);
        let own = self.model.language.as_deref();
        let mut hits = ConceptSet::new();
        for (i, entry) in self.model.concepts.iter().enumerate() {
            let mut terms: Vec<&str> = Vec::new();
            if language.is_none() {
                terms.extend(entry.display.as_deref());
            }
            // NOTE: a text filter searches one language, the code system's own when
            // the request asks for none (terminology ecosystem IG test cases 1.11.0,
            // `search-all-de-no-language`).
            terms.extend(
                entry
                    .designations
                    .iter()
                    .filter(|d| match (language, d.language.as_deref()) {
                        (Some(wanted), Some(l)) => text_match::same_language(l, wanted),
                        (Some(_), None) => false,
                        (None, Some(l)) => own.is_some_and(|own| text_match::same_language(l, own)),
                        (None, None) => true,
                    })
                    .map(|d| d.value.as_str()),
            );
            if text_match::matches_all(&words, &terms) {
                hits.insert(u32::try_from(i).unwrap_or(u32::MAX));
            }
        }
        Ok(hits)
    }
}

/// The is-a hierarchy of `model`, when `hierarchyMeaning` is `is-a` or absent;
/// a system whose concepts name no parent gets one with no edges.
fn build_hierarchy(
    model: &CodeSystemModel,
    count: u32,
) -> Result<Option<ModelHierarchy>, BuildError> {
    // NOTE: no version defines the hierarchy when `hierarchyMeaning` is absent
    // (<https://hl7.org/fhir/R4B/codesystem.html>); the ecosystem suite reads nested
    // concepts as is-a, so only another stated meaning withholds subsumption.
    let is_a = model
        .hierarchy_meaning
        .is_none_or(|meaning| meaning == HierarchyMeaning::IsA);
    if !is_a {
        return Ok(None);
    }
    // NOTE: `is-not-a` selects a code that "does not have an is-a relationship with the
    // provided value" (<https://hl7.org/fhir/R4/codesystem-filter-operator.html>), so a
    // flat system is an is-a hierarchy with no edges, never a refusal.
    let by_code: BTreeMap<&str, u32> = model
        .concepts
        .iter()
        .enumerate()
        .map(|(i, c)| (c.code.as_str(), u32::try_from(i).unwrap_or(u32::MAX)))
        .collect();
    let edges: Vec<(Ordinal, Ordinal)> = model
        .concepts
        .iter()
        .enumerate()
        .flat_map(|(i, c)| {
            let child = Ordinal::new(u32::try_from(i).unwrap_or(u32::MAX));
            c.parents
                .iter()
                .filter_map(|p| by_code.get(p.as_str()).map(|&o| (child, Ordinal::new(o))))
                .collect::<Vec<_>>()
        })
        .collect();
    let hierarchy = |source| BuildError::Hierarchy {
        url: model.url.clone(),
        source,
    };
    let parents = Csr::build(count, edges).map_err(hierarchy)?;
    let closure = Closure::compute(&parents).map_err(|source| BuildError::Cycle {
        url: model.url.clone(),
        source,
    })?;
    let children = parents.transpose().map_err(hierarchy)?;
    Ok(Some(ModelHierarchy {
        parents,
        children,
        closure,
    }))
}

/// The distinct languages of `model`, its own and its designations', sorted.
fn languages_of(model: &CodeSystemModel) -> Vec<String> {
    let mut languages: BTreeSet<String> = BTreeSet::new();
    languages.extend(model.language.clone());
    for concept in &model.concepts {
        for designation in &concept.designations {
            if let Some(language) = &designation.language {
                languages.insert(language.clone());
            }
        }
    }
    languages.into_iter().collect()
}

/// `declared` plus the standard properties every provider answers.
///
/// The URIs are the standard concept properties
/// (<https://hl7.org/fhir/R4B/codesystem-concept-properties.html>).
fn with_standard_properties(declared: &[PropertyDefinition]) -> Vec<PropertyDefinition> {
    let mut properties = declared.to_vec();
    for (code, kind) in [
        (INACTIVE, PropertyKind::Boolean),
        (NOT_SELECTABLE, PropertyKind::Boolean),
        (PARENT, PropertyKind::Code),
        (CHILD, PropertyKind::Code),
    ] {
        if !properties.iter().any(|p| p.code == code) {
            properties.push(PropertyDefinition {
                code: code.to_owned(),
                uri: Some(format!("http://hl7.org/fhir/concept-properties#{code}")),
                description: None,
                kind,
            });
        }
    }
    properties
}
