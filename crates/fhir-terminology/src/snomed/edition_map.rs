// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The implicit concept map from this edition to an edition it depends on:
//! `[edition]?fhir_cm=edition/[module]`.
//!
//! The tx-ecosystem IG requires the conversion ("Servers SHALL correctly
//! convert edition-specific concepts to the International Edition, producing
//! a post-coordinated expression where the International Edition has no
//! equivalent precoordinated concept. Such an expression SHALL use only
//! concepts found in the International Edition", requirements, SNOMED CT) and
//! specifies neither the map's URI, nor the target version, nor the
//! conversion. Those are our own design:
//!
//! - the target edition is the module `[module]` and every module it depends
//!   on, by the edition's Module Dependency reference set, whose row for the
//!   target module also gives the target version;
//! - a concept of the target translates to itself; an inactive one to what
//!   its `SAME AS` or `REPLACED BY` association names, translated in turn;
//! - a sufficiently defined concept to the target concept with the same
//!   inferred definition, else to the expression of its proximal primitive
//!   supertypes in the target refined by its inferred attributes;
//! - a primitive concept to its nearest supertypes in the target, as wider,
//!   unless they are a top-level concept, which says nothing useful.

use std::collections::{BTreeMap, BTreeSet};

use concept_graph::ordinal::Ordinal;
use concept_graph::refsets::ValueRef;

use super::expression::Terms;
use crate::normal_form;
use crate::provider::concept_map::{ConceptMapModel, Element, Group, Relationship, Target};
use crate::provider::{CodeSystemProvider, Concept, MapSelection, error::ProviderError};
use crate::snomed::{FHIR_CM, SYSTEM, SnomedProvider};
use crate::stored::storage;

/// The query form of the map, before the target module.
pub(super) const FORM: &str = "edition/";

/// `900000000000534007 |Module dependency reference set|`.
const MODULE_DEPENDENCY: u64 = 900_000_000_000_534_007;

/// The canonical of the map from the edition `version` to `module`.
pub(super) fn url(version: &str, module: u64) -> String {
    format!("{version}?{FHIR_CM}={FORM}{module}")
}

/// The module of an edition or version URI, `http://snomed.info/sct/[module]`
/// with an optional `/version/[date]`.
pub(super) fn module_of_uri(uri: &str) -> Option<u64> {
    let rest = uri
        .strip_prefix("http://snomed.info/sct/")
        .or_else(|| uri.strip_prefix("http://snomed.info/xsct/"))?;
    rest.split('/').next()?.parse().ok()
}

/// The target edition: its modules and the version this edition depends on.
#[derive(Debug)]
struct TargetEdition {
    modules: BTreeSet<u64>,
    version: String,
}

/// What one concept translates to.
#[derive(Debug)]
struct Translation {
    code: String,
    display: Option<String>,
    relationship: Relationship,
}

impl SnomedProvider {
    /// The module SCTID of this edition, the last segment of its edition URI.
    fn own_module(&self) -> Option<u64> {
        self.system().edition.rsplit('/').next()?.parse().ok()
    }

    /// The edition `module` heads, by this edition's Module Dependency
    /// reference set; `None` when this edition does not depend on it.
    fn target_edition(&self, module: u64) -> Result<Option<TargetEdition>, ProviderError> {
        let Some(table) = self.member_tables().table(MODULE_DEPENDENCY) else {
            return Ok(None);
        };
        let Some(own) = self.own_module() else {
            return Ok(None);
        };
        let target_time = table.field("targetEffectiveTime");
        let code = |row: usize| -> Result<Option<u64>, ProviderError> {
            let Some(concept) = table.concept(row) else {
                return Ok(None);
            };
            Ok(self
                .stored()
                .store
                .code(concept)
                .map_err(storage)?
                .and_then(|code| code.parse().ok()))
        };
        let mut version = None;
        let mut depends: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for row in 0..table.len() {
            let (Some(source), Some(target)) = (table.module(row), code(row)?) else {
                continue;
            };
            depends.entry(source).or_default().push(target);
            if source == own && target == module {
                version = target_time.and_then(|field| match table.value(row, field) {
                    Some(ValueRef::String(time)) => Some(time.to_owned()),
                    Some(ValueRef::Integer(time)) => Some(time.to_string()),
                    _ => None,
                });
            }
        }
        let Some(time) = version else {
            return Ok(None);
        };
        let mut modules = BTreeSet::from([module]);
        let mut pending = vec![module];
        while let Some(next) = pending.pop() {
            for held in depends.get(&next).into_iter().flatten() {
                if modules.insert(*held) {
                    pending.push(*held);
                }
            }
        }
        Ok(Some(TargetEdition {
            modules,
            version: format!("http://snomed.info/sct/{module}/version/{time}"),
        }))
    }

    /// Whether this edition depends on the edition `module` heads.
    pub(super) fn depends_on(&self, module: u64) -> Result<bool, ProviderError> {
        Ok(self.target_edition(module)?.is_some())
    }

    fn in_target(&self, target: &TargetEdition, concept: Ordinal) -> Result<bool, ProviderError> {
        Ok(self
            .module_of(concept)
            .map_err(storage)?
            .is_some_and(|module| target.modules.contains(&module)))
    }

    /// What `concept` translates to in `target`.
    fn translate_to(
        &self,
        target: &TargetEdition,
        concept: Ordinal,
    ) -> Result<Vec<Translation>, ProviderError> {
        let Some(record) = self.stored().store.concept(concept).map_err(storage)? else {
            return Ok(Vec::new());
        };
        if !record.active {
            let mut out = Vec::new();
            for successor in super::conceptmap::successors(self, Concept::new(concept.index()))? {
                let Some(next) = self
                    .stored()
                    .store
                    .ordinal(&successor.code)
                    .map_err(storage)?
                else {
                    continue;
                };
                let held = self.stored().store.concept(next).map_err(storage)?;
                if held.is_some_and(|held| held.active) {
                    out.extend(self.translate_to(target, next)?);
                }
            }
            return Ok(out);
        }
        if self.in_target(target, concept)? {
            return Ok(self
                .itself(concept, Relationship::Equivalent)?
                .into_iter()
                .collect());
        }
        if self.is_defined(concept)? {
            if let Some(equivalent) = self.equivalent_in(target, concept)? {
                return Ok(self
                    .itself(equivalent, Relationship::Equivalent)?
                    .into_iter()
                    .collect());
            }
            return Ok(self.expression_in(target, concept)?.into_iter().collect());
        }
        let nearest = self.nearest_in(target, concept)?;
        // NOTE: a supertype that is a top-level concept says only which hierarchy the
        // concept is in, so it is no translation (tx-ecosystem IG, `snomed-translate-
        // edition-toplevel`); no specification governs this: our own design.
        if nearest.iter().all(|held| self.is_top_level(*held)) {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for held in nearest {
            out.extend(self.itself(held, Relationship::Wider)?);
        }
        Ok(out)
    }

    /// `concept` as a translation; `None` for an ordinal the store holds no code for.
    fn itself(
        &self,
        concept: Ordinal,
        relationship: Relationship,
    ) -> Result<Option<Translation>, ProviderError> {
        let Some(code) = self.stored().store.code(concept).map_err(storage)? else {
            return Ok(None);
        };
        Ok(Some(Translation {
            code: code.to_owned(),
            display: self.display(Concept::new(concept.index()), None)?,
            relationship,
        }))
    }

    /// Whether `concept` is the root or directly under it.
    fn is_top_level(&self, concept: Ordinal) -> bool {
        let parents = self.stored().tree.graph.is_a.neighbours(concept);
        parents.is_empty()
            || parents.iter().all(|parent| {
                self.stored()
                    .tree
                    .graph
                    .is_a
                    .neighbours(Ordinal::new(*parent))
                    .is_empty()
            })
    }

    /// The groups of a concept's inferred attributes, each as a sorted set,
    /// and its ungrouped ones, for comparing two definitions.
    fn definition(&self, concept: Ordinal) -> BTreeSet<(bool, Vec<(u32, String)>)> {
        let mut groups: BTreeMap<u32, Vec<(u32, String)>> = BTreeMap::new();
        for row in self.system().attributes.rows(concept) {
            let value = match row.value {
                concept_graph::attributes::ValueRef::Concept(target) => target.index().to_string(),
                concept_graph::attributes::ValueRef::Number(number) => format!("#{number}"),
                concept_graph::attributes::ValueRef::String(text) => format!("\"{text}\""),
            };
            groups.entry(row.group).or_default().push((row.kind, value));
        }
        groups
            .into_iter()
            .flat_map(|(group, mut rows)| {
                rows.sort();
                if group == 0 {
                    rows.into_iter()
                        .map(|row| (false, vec![row]))
                        .collect::<Vec<_>>()
                } else {
                    vec![(true, rows)]
                }
            })
            .collect()
    }

    /// The sufficiently defined concept of the target with the same inferred
    /// parents and attributes as `concept`.
    fn equivalent_in(
        &self,
        target: &TargetEdition,
        concept: Ordinal,
    ) -> Result<Option<Ordinal>, ProviderError> {
        let graph = &self.stored().tree.graph;
        let parents: BTreeSet<u32> = graph.is_a.neighbours(concept).iter().copied().collect();
        let Some(first) = parents.first() else {
            return Ok(None);
        };
        let definition = self.definition(concept);
        for candidate in self
            .stored()
            .tree
            .children
            .neighbours(Ordinal::new(*first))
            .iter()
            .copied()
        {
            let candidate = Ordinal::new(candidate);
            if candidate == concept
                || !self.in_target(target, candidate)?
                || !self.is_defined(candidate)?
            {
                continue;
            }
            let held: BTreeSet<u32> = graph.is_a.neighbours(candidate).iter().copied().collect();
            if held == parents && self.definition(candidate) == definition {
                return Ok(Some(candidate));
            }
        }
        Ok(None)
    }

    /// The supertypes of `concept` in the target, each nearer than any other.
    fn nearest_in(
        &self,
        target: &TargetEdition,
        concept: Ordinal,
    ) -> Result<Vec<Ordinal>, ProviderError> {
        let closure = &self.stored().tree.graph.closure;
        let mut held = Vec::new();
        for ancestor in closure.ancestors(concept) {
            let ancestor = Ordinal::new(ancestor);
            if ancestor != concept && self.in_target(target, ancestor)? {
                held.push(ancestor);
            }
        }
        Ok(minimal(closure, &held))
    }

    /// The expression of `concept` over the target: its proximal primitive
    /// supertypes there, refined by its inferred attributes, which must all
    /// name concepts of the target.
    fn expression_in(
        &self,
        target: &TargetEdition,
        concept: Ordinal,
    ) -> Result<Option<Translation>, ProviderError> {
        for row in self.system().attributes.rows(concept) {
            if let concept_graph::attributes::ValueRef::Concept(value) = row.value
                && !self.in_target(target, value)?
            {
                return Ok(None);
            }
        }
        let closure = &self.stored().tree.graph.closure;
        let mut primitive = Vec::new();
        for ancestor in closure.ancestors(concept) {
            let ancestor = Ordinal::new(ancestor);
            if ancestor != concept
                && self.in_target(target, ancestor)?
                && !self.is_defined(ancestor)?
            {
                primitive.push(ancestor);
            }
        }
        let focus = minimal(closure, &primitive);
        if focus.is_empty() {
            return Ok(None);
        }
        let render = |terms: Terms<'_>| -> Result<Option<String>, ProviderError> {
            Ok(normal_form::Expression {
                defined: true,
                focus: self.references(focus.iter().map(|o| o.index()), terms)?,
                attributes: self.inferred_attributes(concept, 0, terms)?,
            }
            .render())
        };
        let Some(code) = render(Terms::None)? else {
            return Ok(None);
        };
        Ok(Some(Translation {
            code,
            display: render(Terms::In(None))?,
            relationship: Relationship::Equivalent,
        }))
    }
}

/// The members of `held` no other member is below.
fn minimal(closure: &concept_graph::closure::Closure, held: &[Ordinal]) -> Vec<Ordinal> {
    held.iter()
        .copied()
        .filter(|candidate| {
            !held.iter().any(|other| {
                other != candidate && closure.ancestors(*other).contains(candidate.index())
            })
        })
        .collect()
}

/// The concept map `url` names, to the edition `module` heads, with the
/// elements `selection` asks for.
///
/// # Errors
///
/// Returns [`ProviderError::UnknownImplicitConceptMap`] when this edition does
/// not depend on `module`, and [`ProviderError`] when the substrate fails.
pub(super) fn concept_map(
    provider: &SnomedProvider,
    url: &str,
    module: u64,
    selection: MapSelection<'_>,
) -> Result<ConceptMapModel, ProviderError> {
    let Some(target) = provider.target_edition(module)? else {
        return Err(ProviderError::UnknownImplicitConceptMap {
            url: url.to_owned(),
        });
    };
    // NOTE: a map built for a whole edition or by its targets would translate every
    // concept the edition adds, so it answers only the source a translation names;
    // no specification governs this: our own design.
    let sources: Vec<Ordinal> = match selection {
        MapSelection::Source(code) => provider
            .stored()
            .store
            .ordinal(code)
            .map_err(storage)?
            .into_iter()
            .collect(),
        MapSelection::Whole | MapSelection::Target(_) => Vec::new(),
    };
    let mut elements = Vec::new();
    for source in sources {
        let translations = provider.translate_to(&target, source)?;
        if translations.is_empty() {
            continue;
        }
        elements.push(Element {
            code: provider
                .stored()
                .store
                .code(source)
                .map_err(storage)?
                .map(ToOwned::to_owned),
            display: provider.display(Concept::new(source.index()), None)?,
            no_map: false,
            comment: None,
            targets: translations
                .into_iter()
                .map(|translation| Target {
                    code: Some(translation.code),
                    display: translation.display,
                    relationship: translation.relationship,
                    comment: None,
                    depends_on: Vec::new(),
                    product: Vec::new(),
                })
                .collect(),
        });
    }
    let version = provider.identity().version.clone();
    Ok(ConceptMapModel {
        id: None,
        url: url.to_owned(),
        version: Some(version.clone()),
        name: Some(format!("SNOMED CT edition {module} Concept Map")),
        title: None,
        status: String::from("active"),
        experimental: None,
        date: None,
        publisher: None,
        description: None,
        source_scope: None,
        target_scope: None,
        groups: vec![Group {
            source: Some(SYSTEM.to_owned()),
            source_version: Some(version),
            target: Some(SYSTEM.to_owned()),
            target_version: Some(target.version),
            elements,
            unmapped: None,
        }],
    })
}
