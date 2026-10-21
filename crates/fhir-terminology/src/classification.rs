// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The classification provider: one `ClaML` or ICD-10-CM release read from its
//! artifact directory (`store.redb`, `hierarchy.bin`, `text.bin`, and
//! `manifest.json` with `kind = classification`).
//!
//! The FHIR ICD page is the contract (<https://hl7.org/fhir/R4B/icd.html>):
//! codes carry the period, no filters and no implicit value sets are defined,
//! and the hierarchy is `classified-with`
//! (<https://terminology.hl7.org/ICD.html>). The provider therefore offers
//! the generic filters over the tree (`concept is-a`, `descendent-of`, ...),
//! `kind`, `usage`, `valid`, and every note kind as properties and filters,
//! and the title in the requested language as the display.

use crate::artifact::MANIFEST_FILE;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use concept_graph::csr::{Csr, CsrError};
use concept_graph::ordinal::Ordinal;
use concept_graph::persist::Hierarchy as GraphHierarchy;
use concept_store::record;
use concept_store::store::{Store, StoreError, Vocabulary};
use concept_store::tables;
use designation_index::index::{Query, TextIndex};
use serde::Deserialize;

use crate::artifact::Source;
use crate::filter::FilterOperator;
use crate::provider::{
    Capability, CodeSystemProvider, Compositional, Concept, ConceptSet, ContentMode, Declaration,
    Designation, DesignationUse, FilterDefinition, Hierarchy, HierarchyMeaning, Identity, Located,
    Property, PropertyDefinition, PropertyKind, PropertyValue, ProviderError, Status,
};

/// The manifest `kind` of an artifact this provider opens.
pub const KIND: &str = "classification";
/// The manifest version this provider reads.
pub const MANIFEST_VERSION: u32 = 2;
/// The designation use ordinal of the title.
const PREFERRED: u32 = 0;
/// The property keys the build writes beside the note kinds.
const KIND_KEY: &str = "kind";
const USAGE_KEY: &str = "usage";
const VALID_KEY: &str = "valid";

/// A failure to open an artifact as a classification.
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
    /// The artifact is of another kind.
    #[error("the artifact is `{0}`, not a classification")]
    NotClassification(String),
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
    #[serde(default)]
    kind: String,
    system: String,
    version: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    language: String,
    /// `classified-with` for a tree; absent for a flat table.
    #[serde(default, rename = "hierarchyMeaning")]
    hierarchy_meaning: Option<String>,
    store: String,
    hierarchy: String,
    text: String,
    #[serde(default)]
    languages: Vec<String>,
}

/// The classification tree over the artifact's graph.
#[derive(Debug)]
struct Tree {
    graph: GraphHierarchy,
    children: Csr,
}

impl Hierarchy for Tree {
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

/// One classification release behind the seam.
pub struct ClassificationProvider {
    identity: Identity,
    declaration: Declaration,
    /// Whether the artifact carries a hierarchy (a flat table has none).
    tree_served: bool,
    store: Store,
    tree: Tree,
    text: TextIndex,
    concepts: u32,
    /// The language the display falls back to.
    language: String,
    /// The artifact the version was read from.
    source: Source,
    /// Property key ordinal to name.
    keys: BTreeMap<u32, String>,
    /// Designation use ordinal to name.
    uses: BTreeMap<u32, String>,
}

impl std::fmt::Debug for ClassificationProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClassificationProvider")
            .field("system", &self.identity.url)
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

impl ClassificationProvider {
    /// Opens the artifact directory `dir`.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError`] when the manifest, the store, or a side file
    /// does not read, or the artifact is not a classification.
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
        if manifest.kind != KIND {
            return Err(OpenError::NotClassification(manifest.kind));
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
                url: manifest.system,
                version: manifest.version,
                title: (!manifest.title.is_empty()).then_some(manifest.title),
                name: None,
                version_needed: false,
            },
            declaration: declaration(
                &keys,
                manifest.languages,
                manifest.hierarchy_meaning.is_some(),
            ),
            tree_served: manifest.hierarchy_meaning.is_some(),
            store,
            tree: Tree { graph, children },
            text,
            concepts,
            language: if manifest.language.is_empty() {
                String::from("en")
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

    fn use_coding(&self, use_ordinal: u32) -> DesignationUse {
        let code = self
            .uses
            .get(&use_ordinal)
            .cloned()
            .unwrap_or_else(|| use_ordinal.to_string());
        DesignationUse {
            system: self.identity.url.clone(),
            display: Some(code.clone()),
            code,
        }
    }

    /// The title in the requested language, else in the classification's own
    /// language, else any.
    fn choose_display(
        &self,
        ordinal: Ordinal,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        let designations = self.store.designations(ordinal).map_err(storage)?;
        let base = primary_subtag(&self.language);
        let wanted = language.map_or_else(|| base.clone(), primary_subtag);
        let pick = |lang: Option<&str>| {
            designations
                .iter()
                .find(|d| {
                    d.use_ordinal == PREFERRED
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
}

impl CodeSystemProvider for ClassificationProvider {
    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    /// The language the release states for itself (the `ClaML` `Meta` element
    /// named `lang`), so a `displayLanguage` it carries no rubric for is
    /// answered as a language the system has no display in
    /// (`OperationDefinition/CodeSystem-validate-code`: `displayLanguage`
    /// "Specifies the language to be used for description when validating the
    /// display property").
    fn language(&self) -> Option<&str> {
        Some(&self.language)
    }

    fn artifact(&self) -> Option<&Source> {
        Some(&self.source)
    }

    fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        // NOTE: ICD codes are represented with the period included
        // (<https://hl7.org/fhir/R4B/icd.html>); a code without it is not the code.
        let Some(ordinal) = self.store.ordinal(code.trim()).map_err(storage)? else {
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
        Ok(Status {
            standards_status: None,
            active: record.is_some_and(|c| c.active),
            inactive_reason: None,
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
        if self.tree_served {
            for value in self.codes(self.tree.parents(concept))? {
                out.push(Property {
                    code: String::from("parent"),
                    value,
                    ..Property::default()
                });
            }
            for value in self.codes(self.tree.children(concept))? {
                out.push(Property {
                    code: String::from("child"),
                    value,
                    ..Property::default()
                });
            }
        }
        Ok(out)
    }

    fn hierarchy(&self) -> Option<&dyn Hierarchy> {
        if self.tree_served {
            Some(&self.tree)
        } else {
            None
        }
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
}

/// What the provider declares: `kind`, `usage`, `valid`, `parent`, `child`,
/// and every note kind as properties; `kind`, `usage`, `valid`, and the note
/// kinds as filters.
fn declaration(keys: &BTreeMap<u32, String>, languages: Vec<String>, tree: bool) -> Declaration {
    let mut properties = if tree {
        vec![
            PropertyDefinition {
                code: String::from("parent"),
                uri: None,
                description: Some(String::from("The class this one is classified under")),
                kind: PropertyKind::Code,
            },
            PropertyDefinition {
                code: String::from("child"),
                uri: None,
                description: Some(String::from("A class classified under this one")),
                kind: PropertyKind::Code,
            },
        ]
    } else {
        Vec::new()
    };
    let mut filters = Vec::new();
    for name in keys.values() {
        let (kind, operators, value, description) = match name.as_str() {
            KIND_KEY => (
                PropertyKind::Code,
                vec![FilterOperator::Equal, FilterOperator::In],
                "chapter | block | category | subcategory",
                "The level of the class",
            ),
            USAGE_KEY => (
                PropertyKind::Code,
                vec![
                    FilterOperator::Equal,
                    FilterOperator::In,
                    FilterOperator::Exists,
                ],
                "dagger | aster",
                "The usage mark of the class",
            ),
            VALID_KEY => (
                PropertyKind::Boolean,
                vec![FilterOperator::Equal],
                "true | false",
                "Whether the code is valid for use as a code",
            ),
            _ => (
                PropertyKind::String,
                vec![
                    FilterOperator::Equal,
                    FilterOperator::Regex,
                    FilterOperator::Exists,
                ],
                "a note text",
                "A note of this kind on the class",
            ),
        };
        properties.push(PropertyDefinition {
            code: name.clone(),
            uri: None,
            description: Some(description.to_owned()),
            kind,
        });
        filters.push(FilterDefinition {
            code: name.clone(),
            description: Some(description.to_owned()),
            operators,
            value: value.to_owned(),
        });
    }
    Declaration {
        content: ContentMode::NotPresent,
        case_sensitive: true,
        hierarchy_meaning: tree.then_some(HierarchyMeaning::ClassifiedWith),
        compositional: Compositional::None,
        languages,
        properties,
        filters,
        capabilities: if tree {
            BTreeSet::from([Capability::Subsumption, Capability::Enumeration])
        } else {
            BTreeSet::from([Capability::Enumeration])
        },
        implicit_forms: Vec::new(),
    }
}
