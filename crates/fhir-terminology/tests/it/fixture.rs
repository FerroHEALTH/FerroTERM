// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic code system: a small is-a hierarchy with two languages and two
//! properties, in two versions, plus a flat system without a hierarchy.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use fhir_terminology::compose::{Compose, Include, SystemRef};
use fhir_terminology::filter::{Filter, FilterOperator};
use fhir_terminology::provider::{
    Capability, CodeSystemProvider, Compositional, Concept, ConceptSet, ContentMode, Declaration,
    Designation, DesignationUse, Hierarchy, HierarchyMeaning, Identity, Located, Property,
    PropertyDefinition, PropertyKind, PropertyValue, ProviderError, Status,
};
use fhir_terminology::registry::Registry;

pub(crate) const URL: &str = "http://example.org/fixture";
pub(crate) const FLAT_URL: &str = "http://example.org/flat";

#[derive(Debug, Clone)]
struct Row {
    code: &'static str,
    parent: Option<&'static str>,
    en: &'static str,
    nl: &'static str,
    active: bool,
    legs: Option<i64>,
}

const ROWS: [Row; 7] = [
    Row {
        code: "root",
        parent: None,
        en: "Living thing",
        nl: "Levend wezen",
        active: true,
        legs: None,
    },
    Row {
        code: "animal",
        parent: Some("root"),
        en: "Animal",
        nl: "Dier",
        active: true,
        legs: None,
    },
    Row {
        code: "cat",
        parent: Some("animal"),
        en: "Cat",
        nl: "Kat",
        active: true,
        legs: Some(4),
    },
    Row {
        code: "dog",
        parent: Some("animal"),
        en: "Dog",
        nl: "Hond",
        active: true,
        legs: Some(4),
    },
    Row {
        code: "fish",
        parent: Some("animal"),
        en: "Fish",
        nl: "Vis",
        active: false,
        legs: Some(0),
    },
    Row {
        code: "kitten",
        parent: Some("cat"),
        en: "Kitten",
        nl: "Kitten",
        active: true,
        legs: Some(4),
    },
    Row {
        code: "plant",
        parent: Some("root"),
        en: "Plant",
        nl: "Plant",
        active: true,
        legs: None,
    },
];

/// The synthetic hierarchy over the rows.
#[derive(Debug)]
pub(crate) struct Tree {
    parents: BTreeMap<u32, u32>,
}

impl Hierarchy for Tree {
    fn parents(&self, concept: Concept) -> ConceptSet {
        self.parents
            .get(&concept.index())
            .copied()
            .into_iter()
            .collect()
    }

    fn children(&self, concept: Concept) -> ConceptSet {
        self.parents
            .iter()
            .filter(|(_, parent)| **parent == concept.index())
            .map(|(child, _)| *child)
            .collect()
    }

    fn ancestors(&self, concept: Concept) -> ConceptSet {
        let mut out = ConceptSet::new();
        let mut current = concept.index();
        while let Some(parent) = self.parents.get(&current) {
            out.insert(*parent);
            current = *parent;
        }
        out
    }

    fn descendants(&self, concept: Concept) -> ConceptSet {
        let mut out = ConceptSet::new();
        let mut frontier = vec![concept.index()];
        while let Some(node) = frontier.pop() {
            for child in self.children(Concept::new(node)) {
                if out.insert(child) {
                    frontier.push(child);
                }
            }
        }
        out
    }
}

/// The synthetic provider.
#[derive(Debug)]
pub(crate) struct Fixture {
    identity: Identity,
    declaration: Declaration,
    rows: Vec<Row>,
    tree: Option<Tree>,
}

impl Fixture {
    /// The hierarchical system at `version`.
    pub(crate) fn hierarchical(version: &str) -> Self {
        Self::build(URL, version, true)
    }

    /// The flat system (no hierarchy, no subsumption).
    pub(crate) fn flat() -> Self {
        Self::build(FLAT_URL, "1", false)
    }

    /// The flat system's concepts under another `url` and `version`.
    pub(crate) fn flat_at(url: &str, version: &str) -> Self {
        Self::build(url, version, false)
    }

    fn build(url: &str, version: &str, hierarchical: bool) -> Self {
        let rows = ROWS.to_vec();
        let index = |code: &str| rows.iter().position(|row| row.code == code).map(ord);
        let tree = hierarchical.then(|| Tree {
            parents: rows
                .iter()
                .enumerate()
                .filter_map(|(i, row)| row.parent.and_then(index).map(|p| (ord(i), p)))
                .collect(),
        });
        let mut capabilities = BTreeSet::from([Capability::Enumeration]);
        if hierarchical {
            capabilities.insert(Capability::Subsumption);
        }
        Self {
            identity: Identity {
                url: url.to_owned(),
                version: version.to_owned(),
                title: Some(String::from("Fixture")),
                name: None,
                version_needed: false,
            },
            declaration: Declaration {
                content: ContentMode::NotPresent,
                case_sensitive: true,
                hierarchy_meaning: hierarchical.then_some(HierarchyMeaning::IsA),
                compositional: Compositional::None,
                languages: vec![String::from("en"), String::from("nl")],
                properties: vec![
                    PropertyDefinition {
                        code: String::from("legs"),
                        uri: None,
                        description: Some(String::from("Number of legs")),
                        kind: PropertyKind::Integer,
                    },
                    PropertyDefinition {
                        code: String::from("kingdom"),
                        uri: None,
                        description: None,
                        kind: PropertyKind::Code,
                    },
                ],
                filters: Vec::new(),
                capabilities,
                implicit_forms: Vec::new(),
            },
            rows,
            tree,
        }
    }

    fn row(&self, concept: Concept) -> Option<&Row> {
        self.rows
            .get(concept_graph::ordinal::to_usize(concept.index()))
    }
}

fn synonym() -> DesignationUse {
    DesignationUse {
        system: String::from("http://snomed.info/sct"),
        code: String::from("900000000000013009"),
        display: Some(String::from("Synonym")),
    }
}

impl CodeSystemProvider for Fixture {
    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        Ok(self
            .rows
            .iter()
            .position(|row| row.code == code)
            .map(|i| Located {
                concept: Concept::new(ord(i)),
                code: code.to_owned(),
            }))
    }

    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        Ok(self.row(concept).map(|row| row.code.to_owned()))
    }

    fn display(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        Ok(self.row(concept).map(|row| match language {
            Some("nl") => row.nl.to_owned(),
            _ => row.en.to_owned(),
        }))
    }

    fn status(&self, concept: Concept) -> Result<Status, ProviderError> {
        Ok(Status {
            standards_status: None,
            active: self.row(concept).is_some_and(|row| row.active),
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
        let Some(row) = self.row(concept) else {
            return Ok(Vec::new());
        };
        Ok([("en", row.en), ("nl", row.nl)]
            .into_iter()
            .filter(|(lang, _)| language.is_none_or(|wanted| wanted == *lang))
            .map(|(lang, value)| Designation {
                standards_status: None,
                language: Some(lang.to_owned()),
                use_: Some(synonym()),
                value: value.to_owned(),
            })
            .collect())
    }

    fn properties(&self, concept: Concept) -> Result<Vec<Property>, ProviderError> {
        let Some(row) = self.row(concept) else {
            return Ok(Vec::new());
        };
        let mut properties = Vec::new();
        if let Some(legs) = row.legs {
            properties.push(Property {
                code: String::from("legs"),
                value: PropertyValue::Integer(legs),
                ..Property::default()
            });
        }
        if let Some(tree) = &self.tree
            && tree.ancestors(concept).contains(1)
        {
            properties.push(Property {
                code: String::from("kingdom"),
                value: PropertyValue::Code(String::from("animal")),
                ..Property::default()
            });
        }
        Ok(properties)
    }

    fn hierarchy(&self) -> Option<&dyn Hierarchy> {
        match &self.tree {
            Some(tree) => Some(tree),
            None => None,
        }
    }

    /// `{url}?vs=isa/{code}`: the descendants and self of a code.
    fn implicit_value_set(&self, url: &str) -> Option<Result<Compose, ProviderError>> {
        let query = url.strip_prefix(&format!("{}?vs=", self.identity.url))?;
        Some(match query.strip_prefix("isa/") {
            Some(code) => Ok(Compose {
                include: vec![Include {
                    system: Some(SystemRef {
                        url: self.identity.url.clone(),
                        version: Some(self.identity.version.clone()),
                    }),
                    filters: vec![Filter {
                        property: String::from("concept"),
                        op: FilterOperator::IsA,
                        value: code.to_owned(),
                    }],
                    ..Include::default()
                }],
                ..Compose::default()
            }),
            None => Err(ProviderError::malformed_implicit_value_set(
                url,
                "expected `isa/{code}`",
            )),
        })
    }

    fn all(&self) -> Result<ConceptSet, ProviderError> {
        Ok((0..ord(self.rows.len())).collect())
    }

    fn search(&self, text: &str, language: Option<&str>) -> Result<ConceptSet, ProviderError> {
        let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
        let mut hits = ConceptSet::new();
        for i in 0..self.rows.len() {
            let designations = self.designations(Concept::new(ord(i)), language)?;
            let matched = words.iter().all(|word| {
                designations.iter().any(|d| {
                    d.value
                        .split_whitespace()
                        .any(|term| term.to_lowercase().starts_with(word.as_str()))
                })
            });
            if matched {
                hits.insert(ord(i));
            }
        }
        Ok(hits)
    }
}

/// A registry with the hierarchical system in two versions and the flat system.
pub(crate) fn registry() -> Registry {
    let mut registry = Registry::new();
    registry
        .register(Arc::new(Fixture::hierarchical("2024")))
        .expect("registers 2024");
    registry
        .register(Arc::new(Fixture::hierarchical("2025")))
        .expect("registers 2025");
    registry
        .register(Arc::new(Fixture::flat()))
        .expect("registers flat");
    registry
}

/// The codes of a concept set under `provider`, sorted.
pub(crate) fn codes(provider: &dyn CodeSystemProvider, set: &ConceptSet) -> Vec<String> {
    let mut codes: Vec<String> = set
        .iter()
        .filter_map(|index| {
            provider
                .code(Concept::new(index))
                .expect("the fixture never fails")
        })
        .collect();
    codes.sort();
    codes
}

/// A fixture index as a concept ordinal.
fn ord(index: usize) -> u32 {
    u32::try_from(index).expect("the fixture is small")
}
