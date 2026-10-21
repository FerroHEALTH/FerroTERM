// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Serving a [`CodeSystemModel`] through the seam.

use std::collections::{BTreeMap, BTreeSet};

use concept_graph::closure::Closure;
use concept_graph::csr::Csr;
use concept_graph::ordinal::Ordinal;
use roaring::RoaringBitmap;

use super::model::{
    CHILD, CodeSystemModel, ConceptEntry, DEPRECATED, DEPRECATION_DATE, INACTIVE, NOT_SELECTABLE,
    PARENT, RETIRED, RETIREMENT_DATE, STATUS,
};
use crate::filter::Filter;
use crate::provider::{
    Capability, CodeSystemProvider, Compositional, Concept, ConceptSet, ContentMode, Declaration,
    Designation, Hierarchy, HierarchyMeaning, Identity, Located, Property, PropertyDefinition,
    PropertyKind, PropertyValue, ProviderError, Status,
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
}

impl FhirCodeSystem {
    /// Builds the provider; the hierarchy exists when `hierarchyMeaning` is
    /// `is-a` and at least one concept names a parent.
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
        Ok(Self {
            identity,
            declaration,
            model,
            codes,
            hierarchy,
            all,
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

    /// Whether `code` is the standard `notSelectable` property: by its code, or
    /// by a declaration under the standard URI
    /// (<https://hl7.org/fhir/R4B/codesystem-concept-properties.html>).
    fn abstract_property(&self, code: &str) -> bool {
        code == NOT_SELECTABLE
            || self.model.properties.iter().any(|p| {
                p.code == code
                    && p.uri.as_deref()
                        == Some("http://hl7.org/fhir/concept-properties#notSelectable")
            })
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
                    deprecated = deprecated || retires_at(date, now);
                }
                (RETIREMENT_DATE, PropertyValue::DateTime(date)) => {
                    // NOTE: no FHIR/SNOMED spec governs this: our own design
                    // reads the date as at the request, once per concept, and
                    // a value that is no `dateTime` as a retirement in force.
                    let now = *at.get_or_insert_with(crate::clock::now);
                    retired_by_date = retired_by_date || retires_at(date, now);
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

/// Whether the `retirementDate` in `text` retires the concept as at `now`.
///
/// Only a date still to come leaves the concept active; text that is no
/// `dateTime` states no date to wait for, so the retirement is in force.
fn retires_at(text: &str, now: jiff::Timestamp) -> bool {
    earliest_instant(text).is_none_or(|instant| instant <= now)
}

/// The first instant the FHIR `dateTime` in `text` covers, or `None` when the
/// text is not one.
///
/// `dateTime` is a year, a year and month, a date, or a date and a whole time
/// with a timezone, and nothing else
/// (<https://hl7.org/fhir/R5/datatypes.html#dateTime>).
fn earliest_instant(text: &str) -> Option<jiff::Timestamp> {
    let (date_text, time_text) = match text.split_once('T') {
        Some((date, time)) => (date, Some(time)),
        None => (text, None),
    };
    let date = period_start(date_text)?;
    let Some(time_text) = time_text else {
        // NOTE: no FHIR/SNOMED spec governs this: our own design compares a
        // partial `dateTime` at the first instant of the period it names.
        return date
            .at(0, 0, 0, 0)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .ok()
            .map(|zoned| zoned.timestamp());
    };
    if date_text.split('-').count() != 3 || !is_time_with_zone(time_text) {
        return None;
    }
    text.parse::<jiff::Timestamp>().ok()
}

/// The first day of the period a `YYYY`, `YYYY-MM`, or `YYYY-MM-DD` names.
fn period_start(text: &str) -> Option<jiff::civil::Date> {
    let mut parts = text.split('-');
    let year = i16::try_from(digits(parts.next()?, 4)?).ok()?;
    if year == 0 {
        return None;
    }
    let month = match parts.next() {
        Some(month) => i8::try_from(digits(month, 2)?).ok()?,
        None => 1,
    };
    let day = match parts.next() {
        Some(day) => i8::try_from(digits(day, 2)?).ok()?,
        None => 1,
    };
    if parts.next().is_some() {
        return None;
    }
    jiff::civil::Date::new(year, month, day).ok()
}

/// Whether `text` is `hh:mm:ss` with an optional fraction and a timezone.
fn is_time_with_zone(text: &str) -> bool {
    let Some(start) = text.rfind(['Z', '+', '-']) else {
        return false;
    };
    let Some((time, zone)) = text.split_at_checked(start) else {
        return false;
    };
    is_zone(zone) && is_time(time)
}

/// Whether `text` is `Z` or an offset of `+hh:mm` / `-hh:mm` up to 14:00.
fn is_zone(text: &str) -> bool {
    if text == "Z" {
        return true;
    }
    let mut characters = text.chars();
    if !matches!(characters.next(), Some('+' | '-')) {
        return false;
    }
    let Some((hours, minutes)) = characters.as_str().split_once(':') else {
        return false;
    };
    match (digits(hours, 2), digits(minutes, 2)) {
        (Some(hours), Some(minutes)) => {
            (hours < 14 && minutes < 60) || (hours == 14 && minutes == 0)
        }
        _ => false,
    }
}

/// Whether `text` is `hh:mm:ss` with an optional decimal fraction of a second.
fn is_time(text: &str) -> bool {
    let mut parts = text.split(':');
    let (Some(hours), Some(minutes), Some(rest), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let (seconds, fraction) = match rest.split_once('.') {
        Some((seconds, fraction)) => (seconds, Some(fraction)),
        None => (rest, None),
    };
    if fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit())) {
        return false;
    }
    match (digits(hours, 2), digits(minutes, 2), digits(seconds, 2)) {
        // A leap second is `60` in the lexical form the specification gives.
        (Some(hours), Some(minutes), Some(seconds)) => hours < 24 && minutes < 60 && seconds <= 60,
        _ => false,
    }
}

/// The value of `text` when it is exactly `count` ASCII digits.
fn digits(text: &str, count: usize) -> Option<u32> {
    if text.len() != count || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
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
        Ok(self.codes.get(&key).map(|&ordinal| Located {
            concept: Concept::new(ordinal),
            code: self
                .model
                .concepts
                .get(usize::try_from(ordinal).unwrap_or(usize::MAX))
                .map_or_else(|| code.to_owned(), |c| c.code.clone()),
        }))
    }

    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        Ok(self.entry(concept).map(|c| c.code.clone()))
    }

    fn display(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        let Some(entry) = self.entry(concept) else {
            return Ok(None);
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
        Ok(self
            .entry(concept)
            .map(|entry| self.status_of(entry))
            .unwrap_or_default())
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
            return Ok(Vec::new());
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
    fn unclosed(&self, _filters: &[Filter]) -> bool {
        matches!(self.model.content, ContentMode::Fragment)
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
        let words = text_match::query_words(text);
        let mut hits = ConceptSet::new();
        for (i, entry) in self.model.concepts.iter().enumerate() {
            let mut terms: Vec<&str> = Vec::new();
            if language.is_none() {
                terms.extend(entry.display.as_deref());
            }
            terms.extend(
                entry
                    .designations
                    .iter()
                    .filter(|d| {
                        language.is_none_or(|wanted| {
                            d.language
                                .as_deref()
                                .is_some_and(|l| text_match::same_language(l, wanted))
                        })
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

/// The is-a hierarchy of `model`, when `hierarchyMeaning` is `is-a` and a
/// concept names a parent.
fn build_hierarchy(
    model: &CodeSystemModel,
    count: u32,
) -> Result<Option<ModelHierarchy>, BuildError> {
    let has_parents = model.concepts.iter().any(|c| !c.parents.is_empty());
    // NOTE: no version defines the hierarchy when `hierarchyMeaning` is absent
    // (<https://hl7.org/fhir/R4B/codesystem.html>); the ecosystem suite reads nested
    // concepts as is-a, so only another stated meaning withholds subsumption.
    let is_a = model
        .hierarchy_meaning
        .is_none_or(|meaning| meaning == HierarchyMeaning::IsA);
    if !is_a || !has_parents {
        return Ok(None);
    }
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

#[cfg(test)]
mod tests {
    use super::{earliest_instant, retires_at};

    /// The instant every case is judged against.
    fn now() -> jiff::Timestamp {
        "2026-01-01T00:00:00Z".parse().expect("a fixed timestamp")
    }

    #[test]
    fn every_date_time_form_names_the_first_instant_it_covers() {
        let instant = |text: &str| earliest_instant(text).expect("a dateTime").to_string();
        assert_eq!(instant("2001"), "2001-01-01T00:00:00Z");
        assert_eq!(instant("2001-06"), "2001-06-01T00:00:00Z");
        assert_eq!(instant("2001-06-15"), "2001-06-15T00:00:00Z");
        assert_eq!(instant("2001-06-15T12:30:00Z"), "2001-06-15T12:30:00Z");
        assert_eq!(instant("2001-06-15T12:30:00.5Z"), "2001-06-15T12:30:00.5Z");
        assert_eq!(instant("2001-06-15T12:30:00+02:00"), "2001-06-15T10:30:00Z");
    }

    // The `dateTime` regular expression admits a four-digit non-zero year, a
    // two-digit month and day, and a whole time with a timezone, nothing else
    // (<https://hl7.org/fhir/R5/datatypes.html#dateTime>).
    #[test]
    fn text_the_date_time_form_refuses_names_no_instant() {
        for text in [
            "",
            "retired",
            "0000",
            "1",
            "99",
            "20010615",
            "2001-6",
            "2001-13",
            "2001-02-30",
            "2001-06-15-01",
            "2001-06-15T12",
            "2001-06-15T12:30",
            "2001-06-15T12:30:00",
            "2001-06-15T12:30:00+15:00",
            "2001-06-15T12:30:00+02:00[Europe/Paris]",
            "2001-06T12:30:00Z",
        ] {
            assert_eq!(earliest_instant(text), None, "{text:?}");
        }
    }

    #[test]
    fn only_a_date_still_to_come_leaves_the_concept_active() {
        assert!(retires_at("2001-06-15", now()));
        assert!(!retires_at("2999-01-01", now()));
        assert!(
            retires_at("yesterday", now()),
            "text that states no date to wait for retires the concept"
        );
    }
}
