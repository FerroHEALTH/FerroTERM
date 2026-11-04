// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The LOINC provider: one release read from its artifact directory
//! (`store.redb`, `hierarchy.bin`, `text.bin`, and `manifest.json`).
//!
//! The FHIR LOINC page is the contract (<https://hl7.org/fhir/R4B/loinc.html>):
//! codes compared without case, `LONG_COMMON_NAME` as the display, inactive
//! when `STATUS = DEPRECATED`, every table field as a property and a filter
//! (`=`, `regex`), `copyright`, `parent` and `ancestor` over the multiaxial
//! hierarchy, and the implicit value sets `http://loinc.org/vs`,
//! `http://loinc.org/vs/[LL id]`, and `http://loinc.org/vs/[part code]`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use concept_graph::ordinal::{Ordinal, to_usize};
use concept_store::record;
use designation_index::index::Query;
use regex::Regex;
use serde::Deserialize;

use crate::provider::compose::{Compose, ConceptRef, Include, SystemRef};
use crate::provider::declaration::{
    Capability, Compositional, ContentMode, Declaration, FilterDefinition, HierarchyMeaning,
    ImplicitArgument, ImplicitForm, PropertyDefinition, PropertyKind,
};
use crate::provider::filter::{Filter, FilterOperator};
use crate::provider::source::Source;
use crate::provider::{
    Concept, ConceptSet, Hierarchy, Identity, Located, Property, ProviderError, Status,
};
use crate::stored::artifact::{Artifact, Layout};
use crate::stored::system::{StoredProvider, StoredSystem};
use crate::stored::{self, storage};

/// The system URI.
pub const SYSTEM: &str = "http://loinc.org";
/// The properties the FHIR page names, with their types
/// (<https://hl7.org/fhir/R4B/loinc.html>, "Properties").
const FHIR_PROPERTIES: [(&str, PropertyKind); 12] = [
    ("STATUS", PropertyKind::String),
    ("COMPONENT", PropertyKind::Code),
    ("PROPERTY", PropertyKind::Code),
    ("TIME_ASPCT", PropertyKind::Code),
    ("SYSTEM", PropertyKind::Code),
    ("SCALE_TYP", PropertyKind::Code),
    ("METHOD_TYP", PropertyKind::Code),
    ("CLASS", PropertyKind::String),
    ("CONSUMER_NAME", PropertyKind::String),
    ("CLASSTYPE", PropertyKind::String),
    ("ORDER_OBS", PropertyKind::String),
    ("DOCUMENT_SECTION", PropertyKind::String),
];

/// The filter that selects the answers of an answer list, as tx.fhir.org
/// spells it.
const LIST_FILTER: &str = "LIST";
/// The same, under the other name that server answers to; its value is an
/// answer list or a term whose answers are wanted.
const ANSWERS_FOR_FILTER: &str = "answers-for";

/// The Document Ontology axes: the kebab-case filter name tx.fhir.org answers
/// to, and the `PartTypeName` the release writes and the artifact stores.
///
/// No specification defines the kebab-case spelling, so the mapping is ours.
const DOCUMENT_AXES: [(&str, &str); 5] = [
    ("document-kind", "Document.Kind"),
    ("document-role", "Document.Role"),
    ("document-setting", "Document.Setting"),
    (
        "document-subject-matter-domain",
        "Document.SubjectMatterDomain",
    ),
    ("document-type-of-service", "Document.TypeOfService"),
];

/// The stored property key of a Document Ontology filter name.
fn document_axis(property: &str) -> Option<&'static str> {
    DOCUMENT_AXES
        .iter()
        .find(|(name, _)| *name == property)
        .map(|(_, key)| *key)
}

/// The property keys the build adds (`ferroterm-build`'s LOINC pipeline).
const COPYRIGHT_KEY: &str = "copyright";
const ANSWER_LIST_KEY: &str = "answer-list";
const ANSWERS_KEY: &str = "answers";
const KIND_KEY: &str = "kind";
/// The designation use ordinal of `LONG_COMMON_NAME`.
const LONG_COMMON_NAME: u32 = 0;
/// The designation use ordinal of a part, list, or answer display.
const DISPLAY: u32 = 3;
/// The language of an artifact whose manifest states none.
///
/// A release carries its names in `LoincTable/Loinc.csv` and its translations
/// beside them in `AccessoryFiles/LinguisticVariants`, so the table itself is
/// the English one. No FHIR/SNOMED spec governs this: our own design.
const DEFAULT_LANGUAGE: &str = "en";

/// A failure to open an artifact as LOINC.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// The artifact serves another system.
    #[error("the artifact serves `{0}`, not LOINC")]
    NotLoinc(String),
    /// The artifact's shared files do not read.
    #[error(transparent)]
    Artifact(#[from] stored::OpenError),
}

#[derive(Debug, Deserialize)]
struct Manifest {
    system: String,
    version: String,
    /// The language the release's own tables are in; an artifact built before
    /// the build wrote it has none.
    #[serde(default)]
    language: String,
    #[serde(default)]
    languages: Vec<String>,
}

/// What LOINC decides: the part axes, the answer lists, and their filters.
#[derive(Debug)]
pub struct Loinc {
    /// The language the release's own tables are in.
    base_language: String,
}

/// One LOINC release behind the seam.
pub type LoincProvider = StoredProvider<Loinc>;

impl LoincProvider {
    /// Opens the artifact directory `dir`.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError`] when the manifest, the store, or a side file
    /// does not read, or the artifact is not a LOINC release.
    pub fn open(dir: &Path) -> Result<Self, OpenError> {
        let (artifact, manifest) = Artifact::open(dir, Layout::Tree, |manifest: &Manifest| {
            stored::serves(&manifest.system, SYSTEM, OpenError::NotLoinc)
        })?;
        Ok(Self::new(
            Identity {
                url: SYSTEM.to_owned(),
                title: Some(String::from("LOINC")),
                name: None,
                version_needed: false,
                version: manifest.version.clone(),
            },
            declaration(&artifact.keys, manifest.languages),
            Source::new(dir, &manifest.version),
            artifact,
            Loinc {
                base_language: if manifest.language.is_empty() {
                    String::from(DEFAULT_LANGUAGE)
                } else {
                    manifest.language
                },
            },
        ))
    }

    /// The part codes a `parent`/`ancestor` filter names, as concepts.
    /// The parts a filter value names: by code, or by an English name.
    fn parts_matching(&self, wanted: &[&str]) -> Result<ConceptSet, ProviderError> {
        let mut parts = ConceptSet::new();
        for value in wanted {
            if let Some(ordinal) = self
                .stored()
                .store
                .ordinal(&value.to_ascii_uppercase())
                .map_err(storage)?
            {
                parts.insert(ordinal.index());
            }
            for designation in self.stored().text.matches(&Query {
                text: (*value).to_owned(),
                active_only: true,
                ..Query::default()
            }) {
                let Some(entry) = self.stored().text.entry(designation) else {
                    continue;
                };
                let named = self
                    .stored()
                    .store
                    .designations(entry.concept)
                    .map_err(storage)?
                    .get(to_usize(entry.index))
                    .is_some_and(|d| d.term.eq_ignore_ascii_case(value));
                if named {
                    parts.insert(entry.concept.index());
                }
            }
        }
        Ok(parts)
    }

    /// An axis filter: with `=` or `in`, the terms whose linked part is one
    /// the value names (by code or by name) or whose column text is the
    /// value; with `regex`, the terms whose part name or code, or column text,
    /// matches (the FHIR LOINC page: any `Loinc.csv` field with `=` or
    /// `regex`).
    fn axis_filter(&self, filter: &Filter) -> Result<ConceptSet, ProviderError> {
        let wanted: Vec<&str> = filter
            .value
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .collect();
        let regex = if filter.op == FilterOperator::Regex {
            Some(Regex::new(&filter.value)?)
        } else {
            None
        };
        let parts = if regex.is_some() {
            ConceptSet::new()
        } else {
            self.parts_matching(&wanted)?
        };
        let Some(key) = self.stored().key_of(&filter.property) else {
            return Ok(ConceptSet::new());
        };
        let mut part_names: BTreeMap<u32, String> = BTreeMap::new();
        let mut set = ConceptSet::new();
        for index in 0..self.stored().concepts {
            if self.axis_matches(
                Ordinal::new(index),
                key,
                &wanted,
                &parts,
                regex.as_ref(),
                &mut part_names,
            )? {
                set.insert(index);
            }
        }
        Ok(set)
    }

    /// Whether any value the concept at `ordinal` carries on the axis `key`
    /// satisfies the filter.
    fn axis_matches(
        &self,
        ordinal: Ordinal,
        key: u32,
        wanted: &[&str],
        parts: &ConceptSet,
        regex: Option<&Regex>,
        part_names: &mut BTreeMap<u32, String>,
    ) -> Result<bool, ProviderError> {
        let properties = self.stored().store.properties(ordinal).map_err(storage)?;
        let Some((_, values)) = properties.iter().find(|(k, _)| *k == key) else {
            return Ok(false);
        };
        for value in values {
            if self.axis_value_matches(value, wanted, parts, regex, part_names)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Whether one axis `value` satisfies the filter: a linked part the value
    /// names, or column text the value or the regex matches.
    fn axis_value_matches(
        &self,
        value: &record::PropertyValue,
        wanted: &[&str],
        parts: &ConceptSet,
        regex: Option<&Regex>,
        part_names: &mut BTreeMap<u32, String>,
    ) -> Result<bool, ProviderError> {
        match (value, regex) {
            (record::PropertyValue::Concept(part), None) => Ok(parts.contains(part.index())),
            (record::PropertyValue::Concept(part), Some(regex)) => {
                let name = self.part_name(*part, part_names)?;
                let code = self
                    .stored()
                    .store
                    .concept(*part)
                    .map_err(storage)?
                    .map(|c| c.code)
                    .unwrap_or_default();
                Ok(regex.is_match(&name) || regex.is_match(&code))
            }
            (record::PropertyValue::String(text), None) => Ok(wanted.iter().any(|w| w == text)),
            (record::PropertyValue::String(text), Some(regex)) => Ok(regex.is_match(text)),
            _ => Ok(false),
        }
    }

    /// The display of the part `part`, remembered in `part_names` so a scan of
    /// the whole table reads each part once.
    fn part_name(
        &self,
        part: Ordinal,
        part_names: &mut BTreeMap<u32, String>,
    ) -> Result<String, ProviderError> {
        if let Some(name) = part_names.get(&part.index()) {
            return Ok(name.clone());
        }
        let name = Loinc::display(self, Concept::new(part.index()), None)
            .map_err(|e| ProviderError::Storage(Box::new(e)))?
            .unwrap_or_default();
        part_names.insert(part.index(), name.clone());
        Ok(name)
    }

    fn parts_named(&self, filter: &Filter) -> Result<Vec<Concept>, ProviderError> {
        let mut out = Vec::new();
        for code in filter.value.split(',') {
            let code = code.trim();
            let Some(ordinal) = self
                .stored()
                .store
                .ordinal(&code.to_ascii_uppercase())
                .map_err(storage)?
            else {
                return Err(ProviderError::UnknownCode(code.to_owned()));
            };
            out.push(Concept::new(ordinal.index()));
        }
        Ok(out)
    }
}

impl LoincProvider {
    /// `parent` (the children of the named parts) or `ancestor` (everything
    /// under them).
    fn part_filter(&self, filter: &Filter) -> Result<ConceptSet, ProviderError> {
        let tree = &self.stored().tree;
        let mut set = ConceptSet::new();
        for part in self.parts_named(filter)? {
            if filter.property == "parent" {
                set |= tree.children(part);
            } else {
                set |= tree.descendants(part);
            }
        }
        Ok(set)
    }
}

impl StoredSystem for Loinc {
    /// The language of the displays this provider returns, so a
    /// `displayLanguage` the release carries no variant for is answered as a
    /// language the system has no display in
    /// (`OperationDefinition/CodeSystem-validate-code`: `displayLanguage`
    /// "Specifies the language to be used for description when validating the
    /// display property").
    fn language(provider: &LoincProvider) -> Option<&str> {
        Some(&provider.system().base_language)
    }

    fn locate(provider: &LoincProvider, code: &str) -> Result<Option<Located>, ProviderError> {
        // NOTE: LOINC codes are not case sensitive
        // (<https://hl7.org/fhir/R4B/loinc.html>); the store holds them upper case.
        provider.stored().located(&code.trim().to_ascii_uppercase())
    }

    /// The long common name (or the display of a part, list, or answer) in
    /// the requested language, else in the release's own language, else any.
    fn display(
        provider: &LoincProvider,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        provider.stored().display_in(
            concept,
            &provider.system().base_language,
            language,
            |use_ordinal| use_ordinal == LONG_COMMON_NAME || use_ordinal == DISPLAY,
        )
    }

    fn status(provider: &LoincProvider, concept: Concept) -> Result<Status, ProviderError> {
        let active = provider.stored().active(concept)?;
        Ok(Status {
            active,
            inactive_reason: (!active).then(|| String::from("DEPRECATED")),
            ..Status::default()
        })
    }

    fn properties(
        provider: &LoincProvider,
        concept: Concept,
    ) -> Result<Vec<Property>, ProviderError> {
        provider.stored().listed_properties(concept, true)
    }

    /// `http://loinc.org/vs` (every code), `/vs/LL…` (the answers of a list),
    /// and `/vs/LP…` (everything under a part)
    /// (<https://hl7.org/fhir/R4B/loinc.html>, "Implicit Value Sets").
    fn implicit_value_set(
        provider: &LoincProvider,
        url: &str,
    ) -> Option<Result<Compose, ProviderError>> {
        let rest = url.strip_prefix(SYSTEM)?.strip_prefix("/vs")?;
        let system = SystemRef {
            url: SYSTEM.to_owned(),
            version: None,
        };
        let include = match rest {
            "" => Include {
                system: Some(system),
                ..Include::default()
            },
            other => {
                let code = other.strip_prefix('/')?;
                let upper = code.to_ascii_uppercase();
                // NOTE: <https://hl7.org/fhir/R4B/loinc.html> defines `/vs/[id]` and
                // `/vs/[partcode]` without saying how to tell them apart; splitting on the
                // `LL` and `LP` prefixes is a heuristic no page states.
                if upper.starts_with("LL") {
                    let concepts = match provider.answers_of(&upper) {
                        Ok(Some(codes)) => codes,
                        Ok(None) => {
                            return Some(Err(ProviderError::malformed_implicit_value_set(
                                url,
                                format!("`{code}` is not an answer list"),
                            )));
                        }
                        Err(error) => return Some(Err(error)),
                    };
                    Include {
                        system: Some(system),
                        concepts: concepts
                            .into_iter()
                            .map(|code| ConceptRef {
                                code,
                                ..ConceptRef::default()
                            })
                            .collect(),
                        ..Include::default()
                    }
                } else if upper.starts_with("LP") {
                    Include {
                        system: Some(system),
                        filters: vec![Filter {
                            property: String::from("ancestor"),
                            op: FilterOperator::Equal,
                            value: upper,
                        }],
                        ..Include::default()
                    }
                } else {
                    return Some(Err(ProviderError::malformed_implicit_value_set(
                        url,
                        format!("`{code}` is neither an answer list (LL) nor a part (LP)"),
                    )));
                }
            }
        };
        Some(Ok(Compose {
            include: vec![include],
            ..Compose::default()
        }))
    }

    /// `parent` and `ancestor` over the multiaxial hierarchy; an axis property
    /// (`COMPONENT`, `PROPERTY`, `TIME_ASPCT`, `SYSTEM`, `SCALE_TYP`,
    /// `METHOD_TYP`) with `=` or `in` matches the linked part by its code or
    /// its name, or the column text of a term without a link; every other
    /// filter is the generic evaluation over the stored fields.
    fn filter(
        provider: &LoincProvider,
        filter: &Filter,
    ) -> Option<Result<ConceptSet, ProviderError>> {
        Some(match (filter.property.as_str(), filter.op) {
            // NOTE: no specification defines these; they are what tx.fhir.org answers,
            // and the FHIR LOINC page has carried a `TODO: Document Ontology`
            // placeholder since STU3 (<https://hl7.org/fhir/R4B/loinc.html>, #277).
            (LIST_FILTER | ANSWERS_FOR_FILTER, FilterOperator::Equal) => {
                provider.answers_filter(&filter.value)
            }
            (property, FilterOperator::Equal | FilterOperator::Exists)
                if document_axis(property).is_some() =>
            {
                provider.document_filter(property, filter)
            }
            (
                "COMPONENT" | "PROPERTY" | "TIME_ASPCT" | "SYSTEM" | "SCALE_TYP" | "METHOD_TYP",
                FilterOperator::Equal | FilterOperator::In | FilterOperator::Regex,
            ) => provider.axis_filter(filter),
            ("parent" | "ancestor", FilterOperator::Equal | FilterOperator::In) => {
                provider.part_filter(filter)
            }
            _ => return None,
        })
    }
}

impl LoincProvider {
    /// The answer codes of the list `code`, when it is one.
    fn answers_of(&self, code: &str) -> Result<Option<Vec<String>>, ProviderError> {
        let Some(ordinal) = self.stored().store.ordinal(code).map_err(storage)? else {
            return Ok(None);
        };
        let Some(key) = self.stored().key_of(ANSWERS_KEY) else {
            return Ok(None);
        };
        let properties = self.stored().store.properties(ordinal).map_err(storage)?;
        Ok(properties
            .into_iter()
            .find(|(k, _)| *k == key)
            .map(|(_, values)| {
                values
                    .into_iter()
                    .filter_map(|v| match v {
                        record::PropertyValue::Code(c) => Some(c),
                        _ => None,
                    })
                    .collect()
            }))
    }

    /// The answer codes `value` asks for: the answers of an answer list, or the
    /// answers of the list a term is linked to.
    ///
    /// No specification defines this filter; it is what tx.fhir.org answers
    /// (#277).
    fn answers_filter(&self, value: &str) -> Result<ConceptSet, ProviderError> {
        let mut set = ConceptSet::new();
        for list in self.answer_lists(value)? {
            for answer in self.answers_of(&list)?.unwrap_or_default() {
                if let Some(ordinal) = self.stored().store.ordinal(&answer).map_err(storage)? {
                    set.insert(ordinal.index());
                }
            }
        }
        Ok(set)
    }

    /// The answer lists `value` names: itself when it is one, else the lists
    /// the term is linked to.
    fn answer_lists(&self, value: &str) -> Result<Vec<String>, ProviderError> {
        if self.answers_of(value)?.is_some() {
            return Ok(vec![value.to_owned()]);
        }
        let Some(ordinal) = self.stored().store.ordinal(value).map_err(storage)? else {
            return Ok(Vec::new());
        };
        let Some(key) = self.stored().key_of(ANSWER_LIST_KEY) else {
            return Ok(Vec::new());
        };
        let properties = self.stored().store.properties(ordinal).map_err(storage)?;
        let Some((_, values)) = properties.iter().find(|(k, _)| *k == key) else {
            return Ok(Vec::new());
        };
        Ok(values
            .iter()
            .filter_map(|linked| match linked {
                record::PropertyValue::Code(code) => Some(code.clone()),
                _ => None,
            })
            .collect())
    }

    /// The concepts a Document Ontology filter selects: those with any part on
    /// the axis (`exists`), or those whose part on it the value names (`=`).
    fn document_filter(
        &self,
        property: &str,
        filter: &Filter,
    ) -> Result<ConceptSet, ProviderError> {
        let Some(axis) = document_axis(property) else {
            return Ok(ConceptSet::new());
        };
        let Some(key) = self.stored().key_of(axis) else {
            return Ok(ConceptSet::new());
        };
        if filter.op == FilterOperator::Exists {
            let wanted = crate::provider::filter::boolean(filter)?;
            let mut set = ConceptSet::new();
            for index in 0..self.stored().concepts {
                let ordinal = Ordinal::new(index);
                let properties = self.stored().store.properties(ordinal).map_err(storage)?;
                let has = properties
                    .iter()
                    .any(|(k, values)| *k == key && !values.is_empty());
                if has == wanted {
                    set.insert(index);
                }
            }
            return Ok(set);
        }
        self.axis_filter(&Filter {
            property: axis.to_owned(),
            op: filter.op,
            value: filter.value.clone(),
        })
    }
}

/// What the provider declares: the FHIR page's properties, every table field
/// as a filter, `copyright`, `parent`, and `ancestor`.
fn declaration(keys: &BTreeMap<u32, String>, languages: Vec<String>) -> Declaration {
    // NOTE: a property the loaded release does not carry is not declared: the FHIR
    // page still lists `DOCUMENT_SECTION`, which `Loinc.csv` dropped (#278).
    let mut properties: Vec<PropertyDefinition> = FHIR_PROPERTIES
        .iter()
        .filter(|(code, _)| keys.values().any(|key| key == code))
        .map(|(code, kind)| PropertyDefinition {
            code: (*code).to_owned(),
            uri: None,
            description: None,
            kind: *kind,
        })
        .collect();
    for (code, kind) in [
        ("parent", PropertyKind::Code),
        ("child", PropertyKind::Code),
        (COPYRIGHT_KEY, PropertyKind::Code),
        (ANSWER_LIST_KEY, PropertyKind::Code),
        (ANSWERS_KEY, PropertyKind::Code),
        (KIND_KEY, PropertyKind::Code),
    ] {
        properties.push(PropertyDefinition {
            code: code.to_owned(),
            uri: None,
            description: None,
            kind,
        });
    }
    let mut filters: Vec<FilterDefinition> = keys
        .values()
        .filter(|name| {
            !matches!(
                name.as_str(),
                COPYRIGHT_KEY | ANSWER_LIST_KEY | ANSWERS_KEY | KIND_KEY
            )
        })
        .map(|name| FilterDefinition {
            code: name.clone(),
            description: Some(format!("The `{name}` field of the term table")),
            operators: vec![FilterOperator::Equal, FilterOperator::Regex],
            value: String::from("a field value"),
        })
        .collect();
    filters.push(FilterDefinition {
        code: COPYRIGHT_KEY.to_owned(),
        description: Some(String::from("`LOINC` or `3rdParty`")),
        operators: vec![FilterOperator::Equal],
        value: String::from("LOINC | 3rdParty"),
    });
    for (code, description) in [
        ("parent", "Codes whose immediate parent is the part"),
        ("ancestor", "Codes under the part, transitively"),
    ] {
        filters.push(FilterDefinition {
            code: code.to_owned(),
            description: Some(description.to_owned()),
            operators: vec![FilterOperator::Equal, FilterOperator::In],
            value: String::from("a part code"),
        });
    }
    // NOTE: no specification defines these four; they are what tx.fhir.org
    // answers, and the FHIR LOINC page has carried a `TODO: Document Ontology`
    // placeholder since STU3 (<https://hl7.org/fhir/R4B/loinc.html>, #277).
    for (code, description) in [
        (LIST_FILTER, "The answers of the answer list"),
        (
            ANSWERS_FOR_FILTER,
            "The answers of the answer list, or of the list the term is linked to",
        ),
    ] {
        filters.push(FilterDefinition {
            code: code.to_owned(),
            description: Some(description.to_owned()),
            operators: vec![FilterOperator::Equal],
            value: String::from("an answer list code, or a term code"),
        });
    }
    for (code, key) in DOCUMENT_AXES {
        if keys.values().any(|name| name == key) {
            filters.push(FilterDefinition {
                code: code.to_owned(),
                description: Some(format!("The Document Ontology `{key}` axis of the term")),
                operators: vec![FilterOperator::Equal, FilterOperator::Exists],
                value: String::from("a part code or name, or a boolean for `exists`"),
            });
        }
    }
    Declaration {
        content: ContentMode::NotPresent,
        case_sensitive: false,
        hierarchy_meaning: Some(HierarchyMeaning::IsA),
        compositional: Compositional::None,
        languages,
        properties,
        filters,
        capabilities: BTreeSet::from([Capability::Subsumption, Capability::Enumeration]),
        // NOTE: the `/vs`, `/vs/[id]`, and `/vs/[partcode]` forms of
        // <https://hl7.org/fhir/R4B/loinc.html>, "Implicit Value Sets".
        implicit_forms: vec![
            ImplicitForm::new("http://loinc.org/vs", ImplicitArgument::None),
            ImplicitForm::new("http://loinc.org/vs/[id]", ImplicitArgument::Code),
            ImplicitForm::new("http://loinc.org/vs/[partcode]", ImplicitArgument::Code),
        ],
    }
}
