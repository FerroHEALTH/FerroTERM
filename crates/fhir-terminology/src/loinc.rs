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

use crate::artifact::MANIFEST_FILE;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use concept_graph::csr::{Csr, CsrError};
use concept_graph::ordinal::{Ordinal, to_usize};
use concept_graph::persist::Hierarchy as GraphHierarchy;
use concept_store::record;
use concept_store::store::{Store, StoreError, Vocabulary};
use concept_store::tables;
use designation_index::index::{Query, TextIndex};
use regex::Regex;
use serde::Deserialize;

use crate::artifact::Source;
use crate::compose::{Compose, ConceptRef, Include, SystemRef};
use crate::filter::{Filter, FilterOperator};
use crate::provider::{
    Capability, CodeSystemProvider, Compositional, Concept, ConceptSet, ContentMode, Declaration,
    Designation, DesignationUse, FilterDefinition, Hierarchy, HierarchyMeaning, Identity,
    ImplicitArgument, ImplicitForm, Located, Property, PropertyDefinition, PropertyKind,
    PropertyValue, ProviderError, Status,
};

/// The system URI.
pub const SYSTEM: &str = "http://loinc.org";
/// The manifest version this provider reads.
pub const MANIFEST_VERSION: u32 = 2;

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
    /// A file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The manifest does not parse.
    #[error("{path} is not an artifact manifest")]
    Manifest {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: serde_json::Error,
    },
    /// The artifact serves another system.
    #[error("the artifact serves `{0}`, not LOINC")]
    NotLoinc(String),
    /// The manifest is of another layout version.
    #[error("the manifest is version {0}; this server reads version {MANIFEST_VERSION}")]
    ManifestVersion(u32),
    /// The store cannot be opened or read.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The hierarchy file does not read.
    #[error("cannot read the hierarchy file")]
    Hierarchy(#[from] concept_graph::persist::PersistError),
    /// The designation index file does not read.
    #[error("cannot read the designation index file")]
    Text(#[from] designation_index::persist::PersistError),
    /// The child adjacency cannot be derived.
    #[error("cannot derive the child adjacency")]
    Transpose(#[from] CsrError),
    /// The concept count meta entry is missing or malformed.
    #[error("the store's concept count is `{0:?}`")]
    ConceptCount(Option<String>),
}

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(rename = "manifest")]
    layout: u32,
    system: String,
    version: String,
    store: String,
    hierarchy: String,
    text: String,
    /// The language the release's own tables are in; an artifact built before
    /// the build wrote it has none.
    #[serde(default)]
    language: String,
    #[serde(default)]
    languages: Vec<String>,
}

/// The multiaxial hierarchy over the artifact's graph.
#[derive(Debug)]
struct LoincHierarchy {
    graph: GraphHierarchy,
    children: Csr,
}

impl Hierarchy for LoincHierarchy {
    fn parents(&self, concept: Concept) -> ConceptSet {
        self.graph
            .is_a
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
        self.graph
            .closure
            .ancestors(Ordinal::new(concept.index()))
            .clone()
    }

    fn descendants(&self, concept: Concept) -> ConceptSet {
        self.graph
            .closure
            .descendants(Ordinal::new(concept.index()))
            .clone()
    }
}

/// One LOINC release behind the seam.
pub struct LoincProvider {
    identity: Identity,
    declaration: Declaration,
    store: Store,
    hierarchy: LoincHierarchy,
    text: TextIndex,
    concepts: u32,
    /// The language the release's own tables are in.
    base_language: String,
    /// The artifact the version was read from.
    source: Source,
    /// Property key ordinal to name.
    keys: BTreeMap<u32, String>,
    /// Designation use ordinal to name.
    uses: BTreeMap<u32, String>,
}

impl std::fmt::Debug for LoincProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoincProvider")
            .field("version", &self.identity.version)
            .field("concepts", &self.concepts)
            .finish_non_exhaustive()
    }
}

fn storage(error: StoreError) -> ProviderError {
    ProviderError::Storage(Box::new(error))
}

fn primary_subtag(language: &str) -> String {
    language
        .split(['-', '_'])
        .next()
        .unwrap_or(language)
        .to_ascii_lowercase()
}

impl LoincProvider {
    /// Opens the artifact directory `dir`.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError`] when the manifest, the store, or a side file
    /// does not read, or the artifact is not a LOINC release.
    pub fn open(dir: &Path) -> Result<Self, OpenError> {
        let manifest_path = dir.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&manifest_path).map_err(|source| OpenError::Io {
            path: manifest_path.clone(),
            source,
        })?;
        let manifest: Manifest =
            serde_json::from_str(&text).map_err(|source| OpenError::Manifest {
                path: manifest_path,
                source,
            })?;
        if manifest.system != SYSTEM {
            return Err(OpenError::NotLoinc(manifest.system));
        }
        if manifest.layout != MANIFEST_VERSION {
            return Err(OpenError::ManifestVersion(manifest.layout));
        }
        let store = Store::open(&dir.join(&manifest.store))?;
        let read = |name: &str| {
            let path = dir.join(name);
            crate::artifact::reader(&path).map_err(|source| OpenError::Io { path, source })
        };
        let graph = GraphHierarchy::read_from(&mut read(&manifest.hierarchy)?)?;
        let children = graph.is_a.transpose()?;
        let text = designation_index::persist::read_from(&mut read(&manifest.text)?)?;
        let concepts = store.meta(tables::META_CONCEPTS)?;
        let concepts: u32 = concepts
            .as_deref()
            .and_then(|c| c.parse().ok())
            .ok_or(OpenError::ConceptCount(concepts))?;
        let mut keys = BTreeMap::new();
        let mut ordinal = 0;
        while let Some(name) = store.vocabulary(Vocabulary::PropertyKeys, ordinal)? {
            keys.insert(ordinal, name);
            ordinal += 1;
        }
        let mut uses = BTreeMap::new();
        let mut ordinal = 0;
        while let Some(name) = store.vocabulary(Vocabulary::DesignationUses, ordinal)? {
            uses.insert(ordinal, name);
            ordinal += 1;
        }
        let source = Source::new(dir, &manifest.version);
        Ok(Self {
            identity: Identity {
                url: SYSTEM.to_owned(),
                version: manifest.version,
                title: Some(String::from("LOINC")),
                name: None,
                version_needed: false,
            },
            declaration: declaration(&keys, manifest.languages),
            store,
            hierarchy: LoincHierarchy { graph, children },
            text,
            concepts,
            base_language: if manifest.language.is_empty() {
                String::from(DEFAULT_LANGUAGE)
            } else {
                manifest.language
            },
            keys,
            uses,
            source,
        })
    }

    fn ordinal(concept: Concept) -> Ordinal {
        Ordinal::new(concept.index())
    }

    fn key_of(&self, name: &str) -> Option<u32> {
        self.keys
            .iter()
            .find(|(_, n)| n.as_str() == name)
            .map(|(k, _)| *k)
    }

    fn use_coding(&self, use_ordinal: u32) -> DesignationUse {
        let code = self
            .uses
            .get(&use_ordinal)
            .cloned()
            .unwrap_or_else(|| use_ordinal.to_string());
        DesignationUse {
            system: SYSTEM.to_owned(),
            display: Some(code.clone()),
            code,
        }
    }

    /// The display: the long common name (or the display of a part, list, or
    /// answer) in the requested language, else in the release's own language,
    /// else any.
    fn choose_display(
        &self,
        ordinal: Ordinal,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        let designations = self.store.designations(ordinal).map_err(storage)?;
        let base = primary_subtag(&self.base_language);
        let wanted = language.map_or_else(|| base.clone(), primary_subtag);
        let pick = |lang: Option<&str>| {
            designations
                .iter()
                .find(|d| {
                    (d.use_ordinal == LONG_COMMON_NAME || d.use_ordinal == DISPLAY)
                        && lang.is_none_or(|l| primary_subtag(&d.language) == l)
                })
                .map(|d| d.term.clone())
        };
        Ok(pick(Some(&wanted))
            .or_else(|| pick(Some(&base)))
            .or_else(|| pick(None)))
    }

    fn codes(
        &self,
        set: impl IntoIterator<Item = u32>,
    ) -> Result<Vec<PropertyValue>, ProviderError> {
        let mut out = Vec::new();
        for index in set {
            if let Some(concept) = self.store.concept(Ordinal::new(index)).map_err(storage)? {
                out.push(PropertyValue::Code(concept.code));
            }
        }
        Ok(out)
    }

    /// The part codes a `parent`/`ancestor` filter names, as concepts.
    /// The parts a filter value names: by code, or by an English name.
    fn parts_matching(&self, wanted: &[&str]) -> Result<ConceptSet, ProviderError> {
        let mut parts = ConceptSet::new();
        for value in wanted {
            if let Some(ordinal) = self
                .store
                .ordinal(&value.to_ascii_uppercase())
                .map_err(storage)?
            {
                parts.insert(ordinal.index());
            }
            for designation in self.text.matches(&Query {
                text: (*value).to_owned(),
                active_only: true,
                ..Query::default()
            }) {
                let Some(entry) = self.text.entry(designation) else {
                    continue;
                };
                let named = self
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
        let Some(key) = self.key_of(&filter.property) else {
            return Ok(ConceptSet::new());
        };
        let mut part_names: BTreeMap<u32, String> = BTreeMap::new();
        let mut set = ConceptSet::new();
        for index in 0..self.concepts {
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
        let properties = self.store.properties(ordinal).map_err(storage)?;
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
        let name = self
            .choose_display(part, None)
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

impl CodeSystemProvider for LoincProvider {
    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    /// The language of the displays this provider returns, so a
    /// `displayLanguage` the release carries no variant for is answered as a
    /// language the system has no display in
    /// (`OperationDefinition/CodeSystem-validate-code`: `displayLanguage`
    /// "Specifies the language to be used for description when validating the
    /// display property").
    fn language(&self) -> Option<&str> {
        Some(&self.base_language)
    }

    fn artifact(&self) -> Option<&Source> {
        Some(&self.source)
    }

    fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        // NOTE: LOINC codes are not case sensitive
        // (<https://hl7.org/fhir/R4B/loinc.html>); the store holds them upper case.
        let Some(ordinal) = self
            .store
            .ordinal(&code.trim().to_ascii_uppercase())
            .map_err(storage)?
        else {
            return Ok(None);
        };
        let stored = self.store.concept(ordinal).map_err(storage)?;
        Ok(stored.map(|c| Located {
            concept: Concept::new(ordinal.index()),
            code: c.code,
        }))
    }

    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        Ok(self
            .store
            .concept(Self::ordinal(concept))
            .map_err(storage)?
            .map(|c| c.code))
    }

    fn display(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        self.choose_display(Self::ordinal(concept), language)
    }

    fn status(&self, concept: Concept) -> Result<Status, ProviderError> {
        let record = self
            .store
            .concept(Self::ordinal(concept))
            .map_err(storage)?;
        let active = record.is_some_and(|c| c.active);
        Ok(Status {
            standards_status: None,
            active,
            inactive_reason: (!active).then(|| String::from("DEPRECATED")),
            abstract_concept: false,
            codeless: false,
        })
    }

    fn designations(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Vec<Designation>, ProviderError> {
        let wanted = language.map(primary_subtag);
        Ok(self
            .store
            .designations(Self::ordinal(concept))
            .map_err(storage)?
            .into_iter()
            .filter(|d| {
                wanted
                    .as_deref()
                    .is_none_or(|w| primary_subtag(&d.language) == w)
            })
            .map(|d| Designation {
                standards_status: None,
                language: Some(d.language.clone()),
                use_: Some(self.use_coding(d.use_ordinal)),
                value: d.term,
            })
            .collect())
    }

    fn properties(&self, concept: Concept) -> Result<Vec<Property>, ProviderError> {
        let ordinal = Self::ordinal(concept);
        let Some(record) = self.store.concept(ordinal).map_err(storage)? else {
            return Ok(Vec::new());
        };
        let mut out = vec![Property {
            code: String::from("inactive"),
            value: PropertyValue::Boolean(!record.active),
            ..Property::default()
        }];
        for (key, values) in self.store.properties(ordinal).map_err(storage)? {
            let Some(name) = self.keys.get(&key) else {
                continue;
            };
            for value in values {
                let value = match value {
                    record::PropertyValue::Concept(target) => {
                        match self.store.concept(target).map_err(storage)? {
                            Some(target) => PropertyValue::Code(target.code),
                            None => continue,
                        }
                    }
                    record::PropertyValue::Code(c) => PropertyValue::Code(c),
                    record::PropertyValue::String(s) => PropertyValue::String(s),
                    record::PropertyValue::Integer(i) => PropertyValue::Integer(i),
                    record::PropertyValue::Boolean(b) => PropertyValue::Boolean(b),
                    record::PropertyValue::Decimal(d) => PropertyValue::Decimal(d),
                    record::PropertyValue::DateTime(d) => PropertyValue::DateTime(d),
                };
                out.push(Property {
                    code: name.clone(),
                    value,
                    ..Property::default()
                });
            }
        }
        for value in self.codes(self.hierarchy.parents(concept))? {
            out.push(Property {
                code: String::from("parent"),
                value,
                ..Property::default()
            });
        }
        for value in self.codes(self.hierarchy.children(concept))? {
            out.push(Property {
                code: String::from("child"),
                value,
                ..Property::default()
            });
        }
        Ok(out)
    }

    fn hierarchy(&self) -> Option<&dyn Hierarchy> {
        Some(&self.hierarchy)
    }

    /// `http://loinc.org/vs` (every code), `/vs/LL…` (the answers of a list),
    /// and `/vs/LP…` (everything under a part)
    /// (<https://hl7.org/fhir/R4B/loinc.html>, "Implicit Value Sets").
    fn implicit_value_set(&self, url: &str) -> Option<Result<Compose, ProviderError>> {
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
                    let concepts = match self.answers_of(&upper) {
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
                                deprecated: false,
                                code,
                                display: None,
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

    fn all(&self) -> Result<ConceptSet, ProviderError> {
        Ok(crate::provider::every(self.concepts))
    }

    fn search(&self, text: &str, language: Option<&str>) -> Result<ConceptSet, ProviderError> {
        let query = Query {
            text: text.to_owned(),
            language: language.map(primary_subtag),
            ..Query::default()
        };
        let mut concepts = ConceptSet::new();
        for designation in self.text.matches(&query) {
            if let Some(entry) = self.text.entry(designation) {
                concepts.insert(entry.concept.index());
            }
        }
        Ok(concepts)
    }

    /// `parent` and `ancestor` over the multiaxial hierarchy; an axis property
    /// (`COMPONENT`, `PROPERTY`, `TIME_ASPCT`, `SYSTEM`, `SCALE_TYP`,
    /// `METHOD_TYP`) with `=` or `in` matches the linked part by its code or
    /// its name, or the column text of a term without a link; every other
    /// filter is the generic evaluation over the stored fields.
    fn filter(&self, filter: &Filter) -> Result<ConceptSet, ProviderError> {
        match (filter.property.as_str(), filter.op) {
            // NOTE: no specification defines these; they are what tx.fhir.org answers,
            // and the FHIR LOINC page has carried a `TODO: Document Ontology`
            // placeholder since STU3 (<https://hl7.org/fhir/R4B/loinc.html>, #277).
            (LIST_FILTER | ANSWERS_FOR_FILTER, FilterOperator::Equal) => {
                self.answers_filter(&filter.value)
            }
            (property, FilterOperator::Equal | FilterOperator::Exists)
                if document_axis(property).is_some() =>
            {
                self.document_filter(property, filter)
            }
            (
                "COMPONENT" | "PROPERTY" | "TIME_ASPCT" | "SYSTEM" | "SCALE_TYP" | "METHOD_TYP",
                FilterOperator::Equal | FilterOperator::In | FilterOperator::Regex,
            ) => self.axis_filter(filter),
            ("parent" | "ancestor", FilterOperator::Equal | FilterOperator::In) => {
                let mut set = ConceptSet::new();
                for part in self.parts_named(filter)? {
                    if filter.property == "parent" {
                        set |= self.hierarchy.children(part);
                    } else {
                        set |= self.hierarchy.descendants(part);
                    }
                }
                Ok(set)
            }
            _ => crate::filter::evaluate(self, filter),
        }
    }
}

impl LoincProvider {
    /// The answer codes of the list `code`, when it is one.
    fn answers_of(&self, code: &str) -> Result<Option<Vec<String>>, ProviderError> {
        let Some(ordinal) = self.store.ordinal(code).map_err(storage)? else {
            return Ok(None);
        };
        let Some(key) = self.key_of(ANSWERS_KEY) else {
            return Ok(None);
        };
        let properties = self.store.properties(ordinal).map_err(storage)?;
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
                if let Some(ordinal) = self.store.ordinal(&answer).map_err(storage)? {
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
        let Some(ordinal) = self.store.ordinal(value).map_err(storage)? else {
            return Ok(Vec::new());
        };
        let Some(key) = self.key_of(ANSWER_LIST_KEY) else {
            return Ok(Vec::new());
        };
        let properties = self.store.properties(ordinal).map_err(storage)?;
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
        let Some(key) = self.key_of(axis) else {
            return Ok(ConceptSet::new());
        };
        if filter.op == FilterOperator::Exists {
            let wanted = crate::filter::boolean(filter)?;
            let mut set = ConceptSet::new();
            for index in 0..self.concepts {
                let ordinal = Ordinal::new(index);
                let properties = self.store.properties(ordinal).map_err(storage)?;
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
