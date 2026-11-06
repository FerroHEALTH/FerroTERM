// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The files every stored system opens, and the reads they answer alike.

use std::collections::BTreeMap;
use std::path::Path;

use concept_graph::closure::Closure;
use concept_graph::csr::{Csr, CsrError};
use concept_graph::ordinal::Ordinal;
use concept_graph::persist::Hierarchy as GraphHierarchy;
use concept_store::record;
use concept_store::store::{Store, StoreError, Vocabulary};
use concept_store::tables;
use designation_index::index::TextIndex;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::{MANIFEST_VERSION, OpenError, primary_subtag, side_file, storage};
use crate::artifact::MANIFEST_FILE;
use crate::provider::{
    Concept, ConceptSet, Designation, DesignationUse, Hierarchy, Located, Property, PropertyValue,
    error::ProviderError,
};

/// Whether an artifact carries a hierarchy file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    /// The manifest names a `hierarchy` file, which is read.
    Tree,
    /// The system has no hierarchy, and the manifest names none.
    Flat,
}

/// The files every manifest names.
#[derive(Debug, Deserialize)]
struct Files {
    #[serde(rename = "manifest")]
    layout: u32,
    store: String,
    text: String,
}

/// The hierarchy file a [`Layout::Tree`] manifest names.
#[derive(Debug, Deserialize)]
struct HierarchyFile {
    hierarchy: String,
}

/// The manifest text `text`, read from `path`, as `T`.
fn manifest<T: DeserializeOwned>(path: &Path, text: &str) -> Result<T, OpenError> {
    serde_json::from_str(text).map_err(|source| OpenError::Manifest {
        path: path.to_path_buf(),
        source,
    })
}

/// The hierarchy of a stored system over the artifact's graph.
///
/// An ordinal at or past the stored concept count (an expression a system
/// composed on request) has no parent and no child here.
#[derive(Debug)]
pub struct Tree {
    /// The child-to-parent adjacency and its closure.
    pub(crate) graph: GraphHierarchy,
    /// The parent-to-child adjacency, transposed when the artifact opens.
    pub(crate) children: Csr,
    nodes: u32,
}

impl Tree {
    /// The tree of a system with no hierarchy: no concept has a parent.
    fn flat() -> Result<Self, CsrError> {
        Ok(Self {
            graph: GraphHierarchy {
                is_a: Csr::build(0, std::iter::empty())?,
                closure: Closure::from_parts(Vec::new(), Vec::new()),
            },
            children: Csr::build(0, std::iter::empty())?,
            nodes: 0,
        })
    }

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

    fn any_parent_in(&self, concept: Concept, set: &ConceptSet) -> bool {
        self.ordinal(concept).is_some_and(|o| {
            self.graph
                .is_a
                .neighbours(o)
                .iter()
                .any(|parent| set.contains(*parent))
        })
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

/// What every stored system opens: the store, the hierarchy, the designation
/// index, and the vocabularies the store names its keys by.
#[derive(Debug)]
pub struct Artifact {
    /// The concept store.
    pub(crate) store: Store,
    /// The hierarchy; flat for a system without one.
    pub(crate) tree: Tree,
    /// The designation index.
    pub(crate) text: TextIndex,
    /// The number of stored concepts.
    pub(crate) concepts: u32,
    /// Property key ordinal to name.
    pub(crate) keys: BTreeMap<u32, String>,
    /// Designation use ordinal to name.
    pub(crate) uses: BTreeMap<u32, String>,
}

impl Artifact {
    /// Opens the artifact directory `dir`, with its manifest read as `M`.
    ///
    /// `accept` checks the manifest is one the system serves before anything
    /// else is read, so a foreign artifact is refused as foreign.
    ///
    /// # Errors
    ///
    /// Returns what `accept` refuses, and [`OpenError`] when the manifest, the
    /// store, or a side file does not read.
    pub(crate) fn open<M, E>(
        dir: &Path,
        layout: Layout,
        accept: impl FnOnce(&M) -> Result<(), E>,
    ) -> Result<(Self, M), E>
    where
        M: DeserializeOwned,
        E: From<OpenError>,
    {
        let path = dir.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&path).map_err(|source| OpenError::Io {
            path: path.clone(),
            source,
        })?;
        let described: M = manifest(&path, &text)?;
        accept(&described)?;
        Ok((Self::read(dir, &path, &text, layout)?, described))
    }

    /// Reads the files the manifest `text` at `path` names.
    fn read(dir: &Path, path: &Path, text: &str, layout: Layout) -> Result<Self, OpenError> {
        let files: Files = manifest(path, text)?;
        if files.layout != MANIFEST_VERSION {
            return Err(OpenError::ManifestVersion(files.layout));
        }
        let store = Store::open(&dir.join(&files.store))?;
        let graph = match layout {
            Layout::Tree => {
                let named: HierarchyFile = manifest(path, text)?;
                Some(GraphHierarchy::read_from(&mut side_file(
                    dir,
                    &named.hierarchy,
                )?)?)
            }
            Layout::Flat => None,
        };
        let index = designation_index::persist::read_from(&mut side_file(dir, &files.text)?)?;
        let concepts = store.meta(tables::META_CONCEPTS)?;
        let concepts: u32 = concepts
            .as_deref()
            .and_then(|c| c.parse().ok())
            .ok_or(OpenError::ConceptCount(concepts))?;
        let tree = match graph {
            Some(graph) => Tree {
                children: graph.is_a.transpose()?,
                graph,
                nodes: concepts,
            },
            None => Tree::flat()?,
        };
        let keys = Self::vocabulary(&store, Vocabulary::PropertyKeys)?;
        let uses = Self::vocabulary(&store, Vocabulary::DesignationUses)?;
        Ok(Self {
            store,
            tree,
            text: index,
            concepts,
            keys,
            uses,
        })
    }

    /// The `kind` name table of `store`, by ordinal.
    fn vocabulary(store: &Store, kind: Vocabulary) -> Result<BTreeMap<u32, String>, StoreError> {
        let mut names = BTreeMap::new();
        let mut ordinal = 0_u32;
        while let Some(name) = store.vocabulary(kind, ordinal)? {
            names.insert(ordinal, name);
            ordinal = ordinal.saturating_add(1);
        }
        Ok(names)
    }

    /// The property key ordinal of `name`, when the store has the key.
    pub(crate) fn key_of(&self, name: &str) -> Option<u32> {
        self.keys
            .iter()
            .find(|(_, n)| n.as_str() == name)
            .map(|(k, _)| *k)
    }

    /// The concept the store holds under `code`, spelled as the store spells
    /// it; `code` is already in the system's normal form.
    pub(crate) fn located(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        let Some(ordinal) = self.store.ordinal(code).map_err(storage)? else {
            return Ok(None);
        };
        Ok(self
            .store
            .concept(ordinal)
            .map_err(storage)?
            .map(|c| Located {
                concept: Concept::new(ordinal.index()),
                code: c.code,
            }))
    }

    /// The stored code of `concept`.
    pub(crate) fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        Ok(self
            .store
            .code(Ordinal::new(concept.index()))
            .map_err(storage)?
            .map(ToOwned::to_owned))
    }

    /// Whether `concept` is stored and active.
    pub(crate) fn active(&self, concept: Concept) -> Result<bool, ProviderError> {
        Ok(self
            .store
            .concept(Ordinal::new(concept.index()))
            .map_err(storage)?
            .is_some_and(|c| c.active))
    }

    /// The codes of `set`, sliced out of the concept column: a concept with
    /// hundreds of children costs one string per child and no search.
    pub(crate) fn codes(
        &self,
        set: impl IntoIterator<Item = u32>,
    ) -> Result<Vec<PropertyValue>, ProviderError> {
        Ok(self
            .store
            .codes(set.into_iter().map(Ordinal::new))
            .map_err(storage)?
            .into_iter()
            .flatten()
            .map(PropertyValue::Code)
            .collect())
    }

    /// The designation use the ordinal names, as a coding of `system`.
    pub(crate) fn use_coding(&self, system: &str, use_ordinal: u32) -> DesignationUse {
        let code = self
            .uses
            .get(&use_ordinal)
            .cloned()
            .unwrap_or_else(|| use_ordinal.to_string());
        DesignationUse {
            system: system.to_owned(),
            display: Some(code.clone()),
            code,
        }
    }

    /// The first designation of `ordinal` whose use `shown` admits, in the
    /// first of `preference` that has one: each a primary language subtag,
    /// or `None` for any language.
    pub(crate) fn first_term(
        &self,
        ordinal: Ordinal,
        preference: &[Option<&str>],
        shown: impl Fn(u32) -> bool,
    ) -> Result<Option<String>, ProviderError> {
        let designations = self.store.designations(ordinal).map_err(storage)?;
        Ok(preference.iter().find_map(|language| {
            designations
                .iter()
                .find(|d| {
                    shown(d.use_ordinal)
                        && language.is_none_or(|l| primary_subtag(&d.language) == l)
                })
                .map(|d| d.term.clone())
        }))
    }

    /// The display of `concept` by a use `shown` admits: in `language`, else
    /// in the release's own language `base`, else in any.
    pub(crate) fn display_in(
        &self,
        concept: Concept,
        base: &str,
        language: Option<&str>,
        shown: impl Fn(u32) -> bool,
    ) -> Result<Option<String>, ProviderError> {
        let base = primary_subtag(base);
        let wanted = language.map_or_else(|| base.clone(), primary_subtag);
        self.first_term(
            Ordinal::new(concept.index()),
            &[Some(&wanted), Some(&base), None],
            shown,
        )
    }

    /// The stored designations of `concept`, only those in `language` when
    /// one is named, each with the use `coding` gives its ordinal.
    pub(crate) fn designations(
        &self,
        concept: Concept,
        language: Option<&str>,
        coding: impl Fn(u32) -> DesignationUse,
    ) -> Result<Vec<Designation>, ProviderError> {
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
                use_: Some(coding(d.use_ordinal)),
                value: d.term,
            })
            .collect())
    }

    /// A stored property value on the seam; a concept reference is its code,
    /// and `None` when the store holds no concept there.
    pub(crate) fn property_value(
        &self,
        value: record::PropertyValue,
    ) -> Result<Option<PropertyValue>, ProviderError> {
        Ok(Some(match value {
            record::PropertyValue::Concept(target) => {
                match self.store.code(target).map_err(storage)? {
                    Some(code) => PropertyValue::Code(code.to_owned()),
                    None => return Ok(None),
                }
            }
            record::PropertyValue::Code(c) => PropertyValue::Code(c),
            record::PropertyValue::String(s) => PropertyValue::String(s),
            record::PropertyValue::Integer(i) => PropertyValue::Integer(i),
            record::PropertyValue::Boolean(b) => PropertyValue::Boolean(b),
            record::PropertyValue::Decimal(d) => PropertyValue::Decimal(d),
            record::PropertyValue::DateTime(d) => PropertyValue::DateTime(d),
        }))
    }

    /// `inactive` and every stored property of `concept` under its key name;
    /// `None` when the store holds no such concept.
    pub(crate) fn stored_properties(
        &self,
        concept: Concept,
    ) -> Result<Option<Vec<Property>>, ProviderError> {
        let ordinal = Ordinal::new(concept.index());
        let Some(record) = self.store.concept(ordinal).map_err(storage)? else {
            return Ok(None);
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
                if let Some(value) = self.property_value(value)? {
                    out.push(Property {
                        code: name.clone(),
                        value,
                        ..Property::default()
                    });
                }
            }
        }
        Ok(Some(out))
    }

    /// The stored properties of `concept`, with its `parent` and `child`
    /// codes when `relatives` asks; none when the store holds no such concept.
    pub(crate) fn listed_properties(
        &self,
        concept: Concept,
        relatives: bool,
    ) -> Result<Vec<Property>, ProviderError> {
        let Some(mut out) = self.stored_properties(concept)? else {
            return Ok(Vec::new());
        };
        if relatives {
            self.relatives(concept, &mut out)?;
        }
        Ok(out)
    }

    /// Adds the `parent` and `child` codes of `concept` to `out`.
    pub(crate) fn relatives(
        &self,
        concept: Concept,
        out: &mut Vec<Property>,
    ) -> Result<(), ProviderError> {
        for (code, set) in [
            ("parent", self.tree.parents(concept)),
            ("child", self.tree.children(concept)),
        ] {
            for value in self.codes(set)? {
                out.push(Property {
                    code: code.to_owned(),
                    value,
                    ..Property::default()
                });
            }
        }
        Ok(())
    }
}
