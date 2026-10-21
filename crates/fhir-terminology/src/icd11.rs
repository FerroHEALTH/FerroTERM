// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ICD-11 provider: one of the three WHO code systems (the MMS and ICF
//! linearizations, the Foundation) read from its artifact directory.
//!
//! The contract is the HL7 terminology ecosystem test cases for ICD-11 over
//! THO's identity page (<https://terminology.hl7.org/CodeSystem-ICD11MMS.html>):
//! a code is a short code (`1A00`), an entity URI in the unversioned or the
//! versioned form, or, in a linearization, a postcoordination expression;
//! `id`, `code`, `parent`, and `child` are URI-valued properties; an entity
//! without a short code is `notSelectable`; a stem's postcoordination scales
//! are properties and implicit value sets
//! (`<uri>/postcoordinationScale/<axis>`); an expression's `stem` and
//! `postcoordinationValues` report how its values bound to the axes. The
//! axis binding rule (an unfilled axis first, then required axes, then WHO's
//! order) and the reading of `/` (a value when it fits an unfilled axis of the
//! stem before it, else a new stem) are the suite's, recorded in its
//! documentation; no FHIR specification governs them.

use crate::artifact::MANIFEST_FILE;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{PoisonError, RwLock};

use ::icd11::Linearization;
use ::icd11::expression::{Expression, Token};
use concept_graph::csr::{Csr, CsrError};
use concept_graph::ordinal::Ordinal;
use concept_graph::persist::Hierarchy as GraphHierarchy;
use concept_store::keys::{KeyTable, KeyTableError};
use concept_store::record;
use concept_store::store::{Store, StoreError, Vocabulary};
use concept_store::tables;
use designation_index::index::{Query, TextIndex};
use serde::Deserialize;

use crate::artifact::Source;
use crate::compose::{Compose, Include, SystemRef};
use crate::filter::{Filter, FilterOperator};
use crate::provider::{
    Capability, CodeSystemProvider, Compositional, Concept, ConceptSet, ContentMode, Declaration,
    Designation, DesignationUse, Hierarchy, HierarchyMeaning, Identity, ImplicitArgument,
    ImplicitForm, Located, Property, PropertyDefinition, PropertyKind, PropertyValue,
    ProviderError, Status, Subproperty,
};
use crate::registries::interned::Interned;

/// The manifest `kind` of an artifact this provider opens.
pub const KIND: &str = "icd11";
/// The manifest version this provider reads.
pub const MANIFEST_VERSION: u32 = 2;
/// The designation use ordinal of the title.
const TITLE: u32 = 0;
/// The path segment of a postcoordination scale's implicit value set.
const SCALE_SEGMENT: &str = "/postcoordinationScale/";

/// A failure to open an artifact as ICD-11.
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
    #[error("the artifact is `{0}`, not ICD-11")]
    NotIcd11(String),
    /// The manifest names no ICD-11 code system.
    #[error("`{0}` is not an ICD-11 code system")]
    Linearization(String),
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
    /// The key table does not read.
    #[error("cannot read the key table")]
    Keys(#[from] KeyTableError),
    /// The scales file does not parse.
    #[error("{path} is not a scales file")]
    Scales {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: serde_json::Error,
    },
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
    #[serde(default)]
    linearization: String,
    version: String,
    #[serde(default)]
    title: Option<String>,
    store: String,
    hierarchy: String,
    text: String,
    #[serde(default)]
    keys: String,
    #[serde(default)]
    scales: String,
    #[serde(default)]
    languages: Vec<String>,
}

/// One postcoordination scale of a stem, as stored.
#[derive(Debug, Clone, Deserialize)]
struct Scale {
    stem: u32,
    axis: String,
    required: bool,
    multiple: String,
    entities: Vec<u32>,
}

/// The tree (or the Foundation's polyhierarchy) over the artifact's graph.
#[derive(Debug)]
struct Tree {
    graph: GraphHierarchy,
    children: Csr,
    nodes: u32,
}

impl Tree {
    fn ordinal(&self, concept: Concept) -> Option<Ordinal> {
        (concept.index() < self.nodes).then(|| Ordinal::new(concept.index()))
    }
}

impl Hierarchy for Tree {
    fn parents(&self, concept: Concept) -> ConceptSet {
        self.ordinal(concept)
            .map(|o| self.graph.is_a.neighbours(o).iter().copied().collect())
            .unwrap_or_default()
    }

    fn children(&self, concept: Concept) -> ConceptSet {
        self.ordinal(concept)
            .map(|o| self.children.neighbours(o).iter().copied().collect())
            .unwrap_or_default()
    }

    fn ancestors(&self, concept: Concept) -> ConceptSet {
        self.ordinal(concept)
            .map(|o| self.graph.closure.ancestors(o).clone())
            .unwrap_or_default()
    }

    fn descendants(&self, concept: Concept) -> ConceptSet {
        self.ordinal(concept)
            .map(|o| self.graph.closure.descendants(o).clone())
            .unwrap_or_default()
    }
}

/// One value of a postcoordination expression, bound to an axis.
#[derive(Debug, Clone)]
struct Bound {
    axis: String,
    value: u32,
    /// The value token as written.
    text: String,
}

/// One syntactic member of an expression: a stem with its bound values.
#[derive(Debug, Clone)]
struct Member {
    stem: u32,
    values: Vec<Bound>,
}

/// A validated postcoordination expression.
#[derive(Debug, Clone)]
struct Cluster {
    members: Vec<Member>,
    /// The members that were read as values of an earlier stem, with the
    /// axis they bound to (index into `members`, then the binding).
    joined: Vec<(usize, Bound)>,
}

/// One ICD-11 code system behind the seam.
pub struct Icd11Provider {
    identity: Identity,
    declaration: Declaration,
    linearization: Linearization,
    release: String,
    store: Store,
    tree: Tree,
    text: TextIndex,
    keys: KeyTable,
    /// The scales by stem ordinal, in WHO's declaration order.
    scales: BTreeMap<u32, Vec<Scale>>,
    concepts: u32,
    /// The entity id of every concept by ordinal.
    ids: Vec<String>,
    /// The short code of every concept by ordinal, when it has one.
    codes: Vec<Option<String>>,
    /// Property key ordinal to name.
    key_names: BTreeMap<u32, String>,
    /// Designation use ordinal to name.
    uses: BTreeMap<u32, String>,
    /// The postcoordination expressions met, by their text.
    expressions: Interned,
    /// The validated expressions, by interned index.
    clusters: RwLock<Vec<Cluster>>,
    /// The artifact the version was read from.
    source: Source,
}

impl std::fmt::Debug for Icd11Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Icd11Provider")
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

/// The short axis name of a schema URI (`infectiousAgent`).
fn axis_short(axis: &str) -> &str {
    axis.rsplit('/').next().unwrap_or(axis)
}

/// The `kind` name table of `store`, by ordinal.
fn vocabulary_names(store: &Store, kind: Vocabulary) -> Result<BTreeMap<u32, String>, OpenError> {
    let mut names = BTreeMap::new();
    let mut ordinal = 0;
    while let Some(name) = store.vocabulary(kind, ordinal)? {
        names.insert(ordinal, name);
        ordinal += 1;
    }
    Ok(names)
}

/// The entity id the `id` property of the concept at `ordinal` states, empty
/// when the concept states none.
fn entity_id(
    store: &Store,
    linearization: Linearization,
    ordinal: Ordinal,
    id_key: Option<u32>,
) -> Result<String, OpenError> {
    let Some(key) = id_key else {
        return Ok(String::new());
    };
    let mut id = String::new();
    for (k, values) in store.properties(ordinal)? {
        if k == key
            && let Some(record::PropertyValue::Code(uri)) = values.first()
            && let Some(entity) = linearization.id_of(uri)
        {
            id = entity;
        }
    }
    Ok(id)
}

impl Icd11Provider {
    /// Opens the artifact directory `dir`.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError`] when the manifest, the store, or a side file
    /// does not read, or the artifact is not an ICD-11 code system.
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
            return Err(OpenError::NotIcd11(manifest.kind));
        }
        if manifest.layout != MANIFEST_VERSION {
            return Err(OpenError::ManifestVersion(manifest.layout));
        }
        let linearization = Linearization::parse(&manifest.linearization)
            .ok_or_else(|| OpenError::Linearization(manifest.linearization.clone()))?;
        let store = Store::open(&dir.join(&manifest.store))?;
        let read = |name: &str| {
            let path = dir.join(name);
            crate::artifact::reader(&path).map_err(|source| OpenError::Io { path, source })
        };
        let graph = GraphHierarchy::read_from(&mut read(&manifest.hierarchy)?)?;
        let children = graph.is_a.transpose()?;
        let text = designation_index::persist::read_from(&mut read(&manifest.text)?)?;
        let keys = KeyTable::read_from(&mut read(&manifest.keys)?)?;
        let scales_path = dir.join(&manifest.scales);
        let scales_text =
            std::fs::read_to_string(&scales_path).map_err(|source| OpenError::Io {
                path: scales_path.clone(),
                source,
            })?;
        let stored: Vec<Scale> =
            serde_json::from_str(&scales_text).map_err(|source| OpenError::Scales {
                path: scales_path,
                source,
            })?;
        let mut scales: BTreeMap<u32, Vec<Scale>> = BTreeMap::new();
        for scale in stored {
            scales.entry(scale.stem).or_default().push(scale);
        }
        let concepts = store.meta(tables::META_CONCEPTS)?;
        let concepts: u32 = concepts
            .as_deref()
            .and_then(|c| c.parse().ok())
            .ok_or(OpenError::ConceptCount(concepts))?;
        let key_names = vocabulary_names(&store, Vocabulary::PropertyKeys)?;
        let uses = vocabulary_names(&store, Vocabulary::DesignationUses)?;
        let id_key = key_names
            .iter()
            .find(|(_, n)| n.as_str() == "id")
            .map(|(k, _)| *k);
        let mut ids = Vec::with_capacity(usize::try_from(concepts).unwrap_or_default());
        let mut codes = Vec::with_capacity(usize::try_from(concepts).unwrap_or_default());
        for index in 0..concepts {
            let ordinal = Ordinal::new(index);
            let stored_code = store.concept(ordinal)?.map(|c| c.code).unwrap_or_default();
            let entity = entity_id(&store, linearization, ordinal, id_key)?;
            let id = if entity.is_empty() {
                linearization.id_of(&stored_code).unwrap_or_default()
            } else {
                entity
            };
            let code = (linearization.id_of(&stored_code).is_none()).then_some(stored_code);
            ids.push(id);
            codes.push(code);
        }
        let title = manifest.title.clone();
        let source = Source::new(dir, &manifest.version);
        Ok(Self {
            identity: Identity {
                url: linearization.system().to_owned(),
                version: manifest.version.clone(),
                title,
                name: None,
                version_needed: false,
            },
            declaration: declaration(linearization, manifest.languages),
            linearization,
            release: manifest.version,
            store,
            tree: Tree {
                graph,
                children,
                nodes: concepts,
            },
            text,
            keys,
            scales,
            concepts,
            ids,
            codes,
            key_names,
            uses,
            expressions: Interned::new(),
            clusters: RwLock::new(Vec::new()),
            source,
        })
    }

    /// The code system.
    #[must_use]
    pub fn linearization(&self) -> Linearization {
        self.linearization
    }

    fn at(ordinal: u32) -> Option<usize> {
        usize::try_from(ordinal).ok()
    }

    /// The unversioned URI of the entity at `ordinal`.
    fn uri(&self, ordinal: u32) -> String {
        Self::at(ordinal)
            .and_then(|i| self.ids.get(i))
            .map(|id| self.linearization.uri(id))
            .unwrap_or_default()
    }

    /// The short code of the entity at `ordinal`, else its URI.
    fn code_or_uri(&self, ordinal: u32) -> String {
        Self::at(ordinal)
            .and_then(|i| self.codes.get(i))
            .and_then(Clone::clone)
            .unwrap_or_else(|| self.uri(ordinal))
    }

    /// The entity ordinal a URI names, in either form.
    fn ordinal_of_uri(&self, uri: &str) -> Option<u32> {
        let id = self.linearization.id_of(uri)?;
        self.keys.get(Linearization::key_of(&id)?)
    }

    /// The entity ordinal a token names: a URI in either form, or a short
    /// code (the linearizations only; the Foundation has no short codes).
    fn resolve(&self, token: &Token) -> Result<Option<u32>, ProviderError> {
        if token.uri {
            return Ok(self.ordinal_of_uri(&token.text));
        }
        if self.linearization == Linearization::Foundation {
            return Ok(None);
        }
        Ok(self
            .store
            .ordinal(&token.text)
            .map_err(storage)?
            .map(Ordinal::index))
    }

    /// The language this linearization's own titles are in: the first the
    /// artifact declares, which is the first language bundle it was built from.
    fn base_language(&self) -> Option<&str> {
        self.declaration.languages.first().map(String::as_str)
    }

    /// The title of the entity at `ordinal` in `language`, else in the base
    /// language, else in English, else any.
    fn title(&self, ordinal: u32, language: Option<&str>) -> Result<Option<String>, ProviderError> {
        let designations = self
            .store
            .designations(Ordinal::new(ordinal))
            .map_err(storage)?;
        let wanted = language
            .or_else(|| self.base_language())
            .map(primary_subtag);
        let pick = |lang: Option<&str>| {
            designations
                .iter()
                .find(|d| {
                    d.use_ordinal == TITLE && lang.is_none_or(|l| primary_subtag(&d.language) == l)
                })
                .map(|d| d.term.clone())
        };
        Ok(pick(wanted.as_deref())
            .or_else(|| pick(Some("en")))
            .or_else(|| pick(None)))
    }

    /// Whether `value` is on the scale: one of its entities or under one.
    fn fits(&self, scale: &Scale, value: u32) -> bool {
        scale.entities.iter().any(|&e| {
            e == value
                || self
                    .tree
                    .graph
                    .closure
                    .is_ancestor(Ordinal::new(e), Ordinal::new(value))
        })
    }

    /// The axis `value` binds to on `stem`, given the axes already filled:
    /// an unfilled axis first, then a required one, then WHO's order.
    fn bind(&self, stem: u32, value: u32, filled: &[String]) -> Option<String> {
        let scales = self.scales.get(&stem)?;
        let candidates: Vec<&Scale> = scales.iter().filter(|s| self.fits(s, value)).collect();
        let pick = |unfilled_only: bool| {
            candidates
                .iter()
                .filter(|s| !unfilled_only || !filled.contains(&s.axis))
                .find(|s| s.required)
                .or_else(|| {
                    candidates
                        .iter()
                        .find(|s| !unfilled_only || !filled.contains(&s.axis))
                })
                .map(|s| s.axis.clone())
        };
        pick(true).or_else(|| pick(false))
    }

    /// Binds `stem` onto an unfilled postcoordination axis of `last`, when one
    /// takes it: the value, recorded on `last` as well.
    fn join(&self, last: &mut Member, stem: u32, text: &str) -> Option<Bound> {
        let filled: Vec<String> = last.values.iter().map(|b| b.axis.clone()).collect();
        let axis = self.bind(last.stem, stem, &filled)?;
        if filled.contains(&axis) {
            return None;
        }
        let bound = Bound {
            axis,
            value: stem,
            text: text.to_owned(),
        };
        last.values.push(bound.clone());
        Some(bound)
    }

    /// The values `member` binds onto the axes of `stem`, in the order written;
    /// `None` when a dotted qualifier names no code.
    fn bindings(
        &self,
        text: &str,
        expression: &Expression,
        member: &::icd11::expression::Member,
        stem: u32,
    ) -> Result<Option<Vec<Bound>>, ProviderError> {
        let invalid = |reason: String| ProviderError::InvalidCode {
            code: text.to_owned(),
            reason,
        };
        let mut values: Vec<Bound> = Vec::new();
        for token in &member.values {
            let Some(value) = self.resolve(token)? else {
                // NOTE: an ICF dotted qualifier that is no code (`d5409.3`) is an unknown
                // code (the ecosystem's `lookup-icf-pc-old`), not a malformed expression.
                if expression.dotted {
                    return Ok(None);
                }
                return Err(invalid(format!("`{}` is not a code", token.text)));
            };
            let filled: Vec<String> = values.iter().map(|b: &Bound| b.axis.clone()).collect();
            let Some(axis) = self.bind(stem, value, &filled) else {
                return Err(invalid(format!(
                    "`{}` is on no postcoordination axis of `{}`",
                    token.text, member.stem.text
                )));
            };
            values.push(Bound {
                axis,
                value,
                text: token.text.clone(),
            });
        }
        Ok(Some(values))
    }

    /// Validates the expression `text`, interning it when it holds.
    fn cluster(
        &self,
        text: &str,
        expression: &Expression,
    ) -> Result<Option<Located>, ProviderError> {
        let mut members: Vec<Member> = Vec::new();
        let mut joined: Vec<(usize, Bound)> = Vec::new();
        let mut last_stem: Option<usize> = None;
        for (position, member) in expression.members.iter().enumerate() {
            let Some(stem) = self.resolve(&member.stem)? else {
                return Ok(None);
            };
            // A `/` member without values that fits an unfilled axis of the
            // stem before it is a value of that stem.
            if position > 0
                && member.values.is_empty()
                && let Some(last) = last_stem.and_then(|i| members.get_mut(i))
                && let Some(bound) = self.join(last, stem, &member.stem.text)
            {
                joined.push((position, bound));
                members.push(Member {
                    stem,
                    values: Vec::new(),
                });
                continue;
            }
            last_stem = Some(members.len());
            let Some(values) = self.bindings(text, expression, member, stem)? else {
                return Ok(None);
            };
            members.push(Member { stem, values });
        }
        let index = self.expressions.intern(text)?.index();
        let mut clusters = self
            .clusters
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = Self::at(index) {
            if slot == clusters.len() {
                clusters.push(Cluster { members, joined });
            } else if let Some(existing) = clusters.get_mut(slot) {
                *existing = Cluster { members, joined };
            }
        }
        Ok(Some(Located {
            concept: Concept::new(self.concepts.saturating_add(index)),
            code: text.to_owned(),
        }))
    }

    fn cluster_of(&self, concept: Concept) -> Option<Cluster> {
        let index = concept.index().checked_sub(self.concepts)?;
        self.clusters
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(Self::at(index)?)
            .cloned()
    }

    /// The display of an expression: the members joined by ` / `, each stem
    /// title followed by its `&` values' titles in brackets.
    fn cluster_display(
        &self,
        cluster: &Cluster,
        language: Option<&str>,
    ) -> Result<String, ProviderError> {
        let mut parts = Vec::new();
        for (position, member) in cluster.members.iter().enumerate() {
            let mut part = self.title(member.stem, language)?.unwrap_or_default();
            let own = cluster.joined.iter().any(|(p, _)| *p == position);
            if !own {
                for bound in &member.values {
                    if cluster
                        .joined
                        .iter()
                        .any(|(_, b)| b.value == bound.value && b.text == bound.text)
                    {
                        continue;
                    }
                    part.push_str(" [");
                    part.push_str(&self.title(bound.value, language)?.unwrap_or_default());
                    part.push(']');
                }
            }
            parts.push(part);
        }
        Ok(parts.join(" / "))
    }

    /// The short-code form of an expression.
    fn cluster_code(&self, cluster: &Cluster) -> String {
        cluster
            .members
            .iter()
            .enumerate()
            .map(|(position, member)| {
                let mut text = self.code_or_uri(member.stem);
                if !cluster.joined.iter().any(|(p, _)| *p == position) {
                    for bound in &member.values {
                        if cluster
                            .joined
                            .iter()
                            .any(|(_, b)| b.value == bound.value && b.text == bound.text)
                        {
                            continue;
                        }
                        text.push('&');
                        text.push_str(&self.code_or_uri(bound.value));
                    }
                }
                text
            })
            .collect::<Vec<_>>()
            .join("/")
    }

    /// The URI form of an expression: the members' URIs, values joined with ` & `.
    fn cluster_uri(&self, cluster: &Cluster) -> String {
        cluster
            .members
            .iter()
            .enumerate()
            .map(|(position, member)| {
                let mut text = self.uri(member.stem);
                if !cluster.joined.iter().any(|(p, _)| *p == position) {
                    for bound in &member.values {
                        if cluster
                            .joined
                            .iter()
                            .any(|(_, b)| b.value == bound.value && b.text == bound.text)
                        {
                            continue;
                        }
                        text.push_str(" & ");
                        text.push_str(&self.uri(bound.value));
                    }
                }
                text
            })
            .collect::<Vec<_>>()
            .join(" / ")
    }

    fn cluster_properties(
        &self,
        cluster: &Cluster,
        language: Option<&str>,
    ) -> Result<Vec<Property>, ProviderError> {
        let mut out = vec![
            Property {
                code: String::from("code"),
                value: PropertyValue::Code(self.cluster_code(cluster)),
                ..Property::default()
            },
            Property {
                code: String::from("id"),
                value: PropertyValue::Code(self.cluster_uri(cluster)),
                ..Property::default()
            },
        ];
        for (position, member) in cluster.members.iter().enumerate() {
            if cluster.joined.iter().any(|(p, _)| *p == position) {
                continue;
            }
            out.push(Property {
                code: String::from("stem"),
                value: PropertyValue::Code(self.code_or_uri(member.stem)),
                subproperties: vec![
                    Subproperty {
                        code: String::from("stemLabel"),
                        value: PropertyValue::String(
                            self.title(member.stem, language)?.unwrap_or_default(),
                        ),
                        description: None,
                    },
                    Subproperty {
                        code: String::from("stemUri"),
                        value: PropertyValue::Uri(self.uri(member.stem)),
                        description: None,
                    },
                ],
                ..Property::default()
            });
            let mut bounds: Vec<&Bound> = member.values.iter().collect();
            bounds.sort_by(|a, b| {
                self.code_or_uri(a.value)
                    .cmp(&self.code_or_uri(b.value))
                    .then_with(|| a.axis.cmp(&b.axis))
            });
            // NOTE: the ecosystem's icd-11 `lookup-pc-simple` keys each bound value's
            // subproperty by the value's code, not by an axis property name.
            for bound in bounds {
                out.push(Property {
                    code: String::from("postcoordinationValues"),
                    value: PropertyValue::Code(bound.axis.clone()),
                    subproperties: vec![Subproperty {
                        code: self.code_or_uri(bound.value),
                        value: PropertyValue::Uri(self.uri(bound.value)),
                        description: self.title(bound.value, language)?,
                    }],
                    ..Property::default()
                });
            }
        }
        Ok(out)
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
}

impl CodeSystemProvider for Icd11Provider {
    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    /// The language of the titles this provider returns, so a `displayLanguage`
    /// the artifact carries no bundle for is answered as a language the system
    /// has no display in rather than satisfied by the base-language title.
    fn language(&self) -> Option<&str> {
        self.base_language()
    }

    fn artifact(&self) -> Option<&Source> {
        Some(&self.source)
    }

    /// A short code, an entity URI in either form, or (in a linearization) a
    /// postcoordination expression; case sensitive throughout.
    fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        let text = code.trim();
        if text.is_empty() {
            return Ok(None);
        }
        let icf = self.linearization == Linearization::Icf;
        let Some(expression) = Expression::parse(text, icf) else {
            return Ok(None);
        };
        if expression.is_simple() {
            let Some(token) = expression.members.first().map(|m| &m.stem) else {
                return Ok(None);
            };
            return Ok(self.resolve(token)?.map(|ordinal| Located {
                concept: Concept::new(ordinal),
                code: self.code_or_uri(ordinal),
            }));
        }
        if self.linearization == Linearization::Foundation {
            return Ok(None);
        }
        self.cluster(text, &expression)
    }

    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        if concept.index() < self.concepts {
            return Ok(Some(self.code_or_uri(concept.index())));
        }
        Ok(self
            .expressions
            .code(Concept::new(concept.index().saturating_sub(self.concepts))))
    }

    fn display(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        if concept.index() < self.concepts {
            return self.title(concept.index(), language);
        }
        match self.cluster_of(concept) {
            Some(cluster) => Ok(Some(self.cluster_display(&cluster, language)?)),
            None => Ok(None),
        }
    }

    fn definition(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        if concept.index() >= self.concepts {
            return Ok(None);
        }
        let Some(key) = self
            .key_names
            .iter()
            .find(|(_, n)| n.as_str() == "definition")
            .map(|(k, _)| *k)
        else {
            return Ok(None);
        };
        Ok(self
            .store
            .properties(Ordinal::new(concept.index()))
            .map_err(storage)?
            .into_iter()
            .find(|(k, _)| *k == key)
            .and_then(|(_, values)| {
                values.into_iter().find_map(|v| match v {
                    record::PropertyValue::String(s) => Some(s),
                    _ => None,
                })
            }))
    }

    fn is_postcoordinated(&self, concept: Concept) -> bool {
        concept.index() >= self.concepts
    }

    fn status(&self, concept: Concept) -> Result<Status, ProviderError> {
        // A grouper without a code of its own is `notSelectable`: abstract in an
        // expansion and refused by `$validate-code` with `abstract = false`.
        let codeless = concept.index() < self.concepts
            && Self::at(concept.index())
                .and_then(|i| self.codes.get(i))
                .is_some_and(Option::is_none);
        Ok(Status {
            standards_status: None,
            active: true,
            inactive_reason: None,
            abstract_concept: codeless,
            codeless,
        })
    }

    fn designations(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Vec<Designation>, ProviderError> {
        if concept.index() >= self.concepts {
            return Ok(Vec::new());
        }
        let wanted = language.map(primary_subtag);
        Ok(self
            .store
            .designations(Ordinal::new(concept.index()))
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
        if concept.index() >= self.concepts {
            return match self.cluster_of(concept) {
                Some(cluster) => self.cluster_properties(&cluster, None),
                None => Ok(Vec::new()),
            };
        }
        let ordinal = concept.index();
        let mut out = Vec::new();
        if let Some(code) = Self::at(ordinal)
            .and_then(|i| self.codes.get(i))
            .and_then(Clone::clone)
        {
            out.push(Property {
                code: String::from("code"),
                value: PropertyValue::Code(code),
                ..Property::default()
            });
        }
        for (key, values) in self
            .store
            .properties(Ordinal::new(ordinal))
            .map_err(storage)?
        {
            let Some(name) = self.key_names.get(&key) else {
                continue;
            };
            if name == "definition" {
                continue;
            }
            for value in values {
                let value = match value {
                    record::PropertyValue::Code(c) => PropertyValue::Code(c),
                    record::PropertyValue::String(s) => PropertyValue::String(s),
                    record::PropertyValue::Boolean(b) => PropertyValue::Boolean(b),
                    record::PropertyValue::Integer(i) => PropertyValue::Integer(i),
                    record::PropertyValue::Decimal(d) => PropertyValue::Decimal(d),
                    record::PropertyValue::DateTime(d) => PropertyValue::DateTime(d),
                    record::PropertyValue::Concept(_) => continue,
                };
                out.push(Property {
                    code: name.clone(),
                    value,
                    ..Property::default()
                });
            }
        }
        for parent in self.tree.parents(concept) {
            out.push(Property {
                code: String::from("parent"),
                value: PropertyValue::Code(self.uri(parent)),
                ..Property::default()
            });
        }
        for child in self.tree.children(concept) {
            out.push(Property {
                code: String::from("child"),
                value: PropertyValue::Code(self.uri(child)),
                ..Property::default()
            });
        }
        for scale in self.scales.get(&ordinal).into_iter().flatten() {
            out.push(Property {
                code: String::from("postcoordinationScale"),
                value: PropertyValue::Code(scale.axis.clone()),
                subproperties: vec![
                    Subproperty {
                        code: String::from("valueSet"),
                        value: PropertyValue::Uri(format!(
                            "{}{SCALE_SEGMENT}{}",
                            self.uri(ordinal),
                            axis_short(&scale.axis)
                        )),
                        description: None,
                    },
                    Subproperty {
                        code: String::from("requiredPostcoordination"),
                        value: PropertyValue::Boolean(scale.required),
                        description: None,
                    },
                    Subproperty {
                        code: String::from("allowMultipleValues"),
                        value: PropertyValue::Code(scale.multiple.clone()),
                        description: None,
                    },
                ],
                ..Property::default()
            });
        }
        Ok(out)
    }

    fn hierarchy(&self) -> Option<&dyn Hierarchy> {
        Some(&self.tree)
    }

    /// `<entity uri>/postcoordinationScale/<axis>`, in either URI form: the
    /// stem's scale as a value set, one `is-a` include per scale entity.
    fn implicit_value_set(&self, url: &str) -> Option<Result<Compose, ProviderError>> {
        let (entity, axis) = url.rsplit_once(SCALE_SEGMENT)?;
        let stem = self.ordinal_of_uri(entity)?;
        let scale = self
            .scales
            .get(&stem)?
            .iter()
            .find(|s| axis_short(&s.axis) == axis)?;
        let include = scale
            .entities
            .iter()
            .map(|&e| Include {
                system: Some(SystemRef {
                    url: self.identity.url.clone(),
                    version: Some(self.release.clone()),
                }),
                filters: vec![Filter {
                    property: String::from("concept"),
                    op: FilterOperator::IsA,
                    value: self.uri(e),
                }],
                ..Include::default()
            })
            .collect();
        Some(Ok(Compose {
            include,
            ..Compose::default()
        }))
    }

    /// A scale's value set carries the release as its version and date, and a
    /// name and title built from the stem and the axis (the WHO ICD-API's
    /// shape, which the ecosystem's `icd-11` cases expect).
    fn implicit_metadata(&self, url: &str) -> crate::provider::ImplicitMetadata {
        let Some((entity, axis)) = url.rsplit_once(SCALE_SEGMENT) else {
            return crate::provider::ImplicitMetadata::default();
        };
        let stem = entity.rsplit('/').next().unwrap_or(entity);
        crate::provider::ImplicitMetadata {
            version: Some(self.release.clone()),
            name: Some(format!("PostcoordinationScale_{stem}_{axis}")),
            title: Some(format!("Postcoordination scale {axis} of {entity}")),
            experimental: Some(false),
            date: Some(self.release.clone()),
            description: None,
            copyright: None,
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

/// What the code system declares.
fn declaration(linearization: Linearization, languages: Vec<String>) -> Declaration {
    let mut properties: Vec<PropertyDefinition> = [
        ("id", PropertyKind::Code, "The entity URI"),
        ("code", PropertyKind::Code, "The short code"),
        ("parent", PropertyKind::Code, "The parent entity's URI"),
        ("child", PropertyKind::Code, "A child entity's URI"),
        (
            "classKind",
            PropertyKind::Code,
            "chapter, block, category, or window",
        ),
        (
            "notSelectable",
            PropertyKind::Boolean,
            "The entity has no short code and cannot be used to code with",
        ),
        ("definition", PropertyKind::String, "The definition"),
        ("exclusion", PropertyKind::String, "An exclusion"),
        ("source", PropertyKind::Code, "The Foundation entity's URI"),
        ("browserUrl", PropertyKind::String, "The ICD-11 browser URL"),
    ]
    .into_iter()
    .map(|(code, kind, description)| PropertyDefinition {
        code: code.to_owned(),
        uri: None,
        description: Some(description.to_owned()),
        kind,
    })
    .collect();
    let capabilities = BTreeSet::from([Capability::Subsumption, Capability::Enumeration]);
    let mut implicit_forms = Vec::new();
    if linearization != Linearization::Foundation {
        for (code, description) in [
            ("postcoordinationScale", "An axis the stem takes values on"),
            ("stem", "A stem of a postcoordination expression"),
            (
                "postcoordinationValues",
                "A value of a postcoordination expression, on its axis",
            ),
        ] {
            properties.push(PropertyDefinition {
                code: code.to_owned(),
                uri: None,
                description: Some(description.to_owned()),
                kind: PropertyKind::Code,
            });
        }
        // NOTE: no FHIR/SNOMED spec governs this: our own design, the WHO
        // ICD-API's scale URI (<https://icd.who.int/icdapi>) with two placeholders.
        implicit_forms.push(ImplicitForm::new(
            "[entity]/postcoordinationScale/[axis]",
            ImplicitArgument::Expression,
        ));
    }
    Declaration {
        content: ContentMode::NotPresent,
        case_sensitive: true,
        hierarchy_meaning: Some(HierarchyMeaning::ClassifiedWith),
        // NOTE: a linearization defines postcoordination clusters
        // (<https://icd.who.int/icdapi>), and this provider validates and
        // locates one; the foundation defines no such grammar.
        // NOTE: WHO states no subsumption of a cluster under its stem's
        // ancestors (<https://icd.who.int/icdapi>), so an expansion of this
        // system lists every member and is not unclosed.
        compositional: if linearization == Linearization::Foundation {
            Compositional::None
        } else {
            Compositional::Supported
        },
        languages,
        properties,
        filters: Vec::new(),
        capabilities,
        implicit_forms,
    }
}
