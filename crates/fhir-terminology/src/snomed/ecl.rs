// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SNOMED provider as the edition the ECL evaluator reads
//! (`sct_ecl::eval::Model`): the closure and the graphs are the
//! artifact's, the concept and description filters are answered from the
//! store and the designation index.

use concept_graph::attributes::Attributes;
use concept_graph::identifiers::Identifiers;
use concept_graph::ordinal::Ordinal;
use concept_graph::refsets::RefsetMembers;
use concept_store::record::{self, Designation};
use concept_store::store::{StoreError, Vocabulary};
use designation_index::index::Query;
use rf2::constants;
use roaring::RoaringBitmap;
use sct_ecl::ast::{Acceptability, Equality, Sctid, TypedSearchTerm};
use sct_ecl::eval::{ConceptPredicate, DescriptionPredicate, EvalError, Model, term_matches};

use super::SnomedProvider;
use crate::provider::{CodeSystemProvider, ProviderError};

fn storage(error: &StoreError) -> EvalError {
    EvalError::Storage(error.to_string())
}

fn provider(error: &ProviderError) -> EvalError {
    EvalError::Storage(error.to_string())
}

impl SnomedProvider {
    /// The concepts with no parent, computed once.
    fn root_set(&self) -> &RoaringBitmap {
        self.roots.get_or_init(|| {
            (0..self.concepts)
                .filter(|&i| {
                    self.hierarchy
                        .graph
                        .is_a
                        .neighbours(Ordinal::new(i))
                        .is_empty()
                })
                .collect()
        })
    }

    /// The concepts with no child, computed once.
    fn leaf_set(&self) -> &RoaringBitmap {
        self.leaves.get_or_init(|| {
            (0..self.concepts)
                .filter(|&i| {
                    self.hierarchy
                        .children
                        .neighbours(Ordinal::new(i))
                        .is_empty()
                })
                .collect()
        })
    }

    /// The sufficiently defined concepts, scanned once from the store.
    fn defined_set(&self) -> Result<&RoaringBitmap, EvalError> {
        if let Some(set) = self.defined.get() {
            return Ok(set);
        }
        let defined = constants::DEFINED.to_string();
        let mut set = RoaringBitmap::new();
        for index in 0..self.concepts {
            let properties = self
                .store
                .properties(Ordinal::new(index))
                .map_err(|e| storage(&e))?;
            let is_defined = properties
                .iter()
                .find(|(k, _)| *k == self.keys.definition_status)
                .is_some_and(|(_, values)| {
                    values
                        .iter()
                        .any(|v| matches!(v, record::PropertyValue::Code(c) if *c == defined))
                });
            if is_defined {
                set.insert(index);
            }
        }
        Ok(self.defined.get_or_init(|| set))
    }

    /// The module SCTID of a concept, from its stored property.
    fn module_of(&self, concept: Ordinal) -> Result<Option<u64>, EvalError> {
        let properties = self.store.properties(concept).map_err(|e| storage(&e))?;
        Ok(properties
            .iter()
            .find(|(k, _)| *k == self.keys.module)
            .and_then(|(_, values)| values.first())
            .and_then(|v| match v {
                record::PropertyValue::Code(code) => code.parse().ok(),
                _ => None,
            }))
    }

    /// The SCTID a designation use ordinal stands for.
    fn use_sctid(&self, use_ordinal: u32) -> Result<Option<u64>, EvalError> {
        Ok(self
            .store
            .vocabulary(Vocabulary::DesignationUses, use_ordinal)
            .map_err(|e| storage(&e))?
            .and_then(|name| name.parse().ok()))
    }

    /// Whether one designation satisfies every predicate.
    fn designation_passes(
        &self,
        concept: Ordinal,
        index: u32,
        designation: &Designation,
        predicates: &[DescriptionPredicate],
    ) -> Result<bool, EvalError> {
        let mut active_asked = false;
        for predicate in predicates {
            let passes = match predicate {
                DescriptionPredicate::Term { operator, terms } => {
                    terms.iter().any(|t| term_matches(t, &designation.term))
                        == (*operator == Equality::Equal)
                }
                DescriptionPredicate::Language { operator, codes } => {
                    let primary = super::primary_subtag(&designation.language);
                    codes.iter().any(|c| c.eq_ignore_ascii_case(&primary))
                        == (*operator == Equality::Equal)
                }
                DescriptionPredicate::Type { operator, types } => {
                    let actual = self.use_sctid(designation.use_ordinal)?;
                    actual.is_some_and(|a| types.contains(&a)) == (*operator == Equality::Equal)
                }
                DescriptionPredicate::Dialect { operator, dialects } => {
                    let hit = self.dialect_hit(concept, index, dialects)?;
                    hit == (*operator == Equality::Equal)
                }
                DescriptionPredicate::Active(active) => {
                    active_asked = true;
                    designation.active == *active
                }
                DescriptionPredicate::Id { operator, ids } => {
                    let actual = designation
                        .id
                        .as_deref()
                        .and_then(|id| id.parse::<u64>().ok());
                    actual.is_some_and(|a| ids.contains(&a)) == (*operator == Equality::Equal)
                }
            };
            if !passes {
                return Ok(false);
            }
        }
        // NOTE: without an `active` filter only active descriptions match, as
        // the reference servers answer; the specification's filters describe
        // the active description set (ECL, "Description filters").
        Ok(active_asked || designation.active)
    }

    /// Whether the designation is acceptable in one of the language reference
    /// sets, at one of the acceptabilities that set admits.
    fn dialect_hit(
        &self,
        concept: Ordinal,
        index: u32,
        dialects: &[(u64, Vec<Acceptability>)],
    ) -> Result<bool, EvalError> {
        for (refset, allowed) in dialects {
            let Some((ordinal, _)) = self
                .keys
                .refsets
                .iter()
                .find(|(_, sctid)| sctid.parse::<u64>().ok() == Some(*refset))
            else {
                continue;
            };
            let Some(acceptability) = self
                .store
                .acceptability(concept, index, *ordinal)
                .map_err(|e| storage(&e))?
            else {
                continue;
            };
            let name = self
                .store
                .vocabulary(Vocabulary::Acceptabilities, acceptability)
                .map_err(|e| storage(&e))?;
            let actual = if name.as_deref() == Some(&constants::PREFERRED.to_string()) {
                Acceptability::Preferred
            } else {
                Acceptability::Acceptable
            };
            if allowed.is_empty() || allowed.contains(&actual) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The designations the index narrows a description filter to, when a
    /// term filter has a word-prefix search term to start from.
    fn indexed_candidates(&self, predicates: &[DescriptionPredicate]) -> Option<RoaringBitmap> {
        for predicate in predicates {
            if let DescriptionPredicate::Term {
                operator: Equality::Equal,
                terms,
            } = predicate
                && terms.iter().all(|t| matches!(t, TypedSearchTerm::Match(_)))
            {
                let active_only = !predicates
                    .iter()
                    .any(|p| matches!(p, DescriptionPredicate::Active(false)));
                let mut out = RoaringBitmap::new();
                for term in terms {
                    if let TypedSearchTerm::Match(words) = term {
                        out |= self.text.matches(&Query {
                            text: words.join(" "),
                            active_only,
                            ..Query::default()
                        });
                    }
                }
                return Some(out);
            }
        }
        None
    }

    /// The concepts of `within` a candidate designation of the index carries,
    /// each candidate re-checked against every predicate.
    fn matching_indexed(
        &self,
        within: &RoaringBitmap,
        candidates: &RoaringBitmap,
        predicates: &[DescriptionPredicate],
    ) -> Result<RoaringBitmap, EvalError> {
        let mut out = RoaringBitmap::new();
        for designation in candidates {
            let Some(entry) = self.text.entry(designation) else {
                continue;
            };
            if !within.contains(entry.concept.index()) || out.contains(entry.concept.index()) {
                continue;
            }
            let designations = self
                .store
                .designations(entry.concept)
                .map_err(|e| storage(&e))?;
            let Some(record) = designations.get(concept_graph::ordinal::to_usize(entry.index))
            else {
                continue;
            };
            if self.designation_passes(entry.concept, entry.index, record, predicates)? {
                out.insert(entry.concept.index());
            }
        }
        Ok(out)
    }

    /// The concepts of `within` with a designation satisfying every predicate,
    /// read from the store one concept at a time.
    fn matching_scanned(
        &self,
        within: &RoaringBitmap,
        predicates: &[DescriptionPredicate],
    ) -> Result<RoaringBitmap, EvalError> {
        let mut out = RoaringBitmap::new();
        for concept in within {
            let ordinal = Ordinal::new(concept);
            for (index, record) in self
                .store
                .designations(ordinal)
                .map_err(|e| storage(&e))?
                .iter()
                .enumerate()
            {
                let index = u32::try_from(index).unwrap_or(u32::MAX);
                if self.designation_passes(ordinal, index, record, predicates)? {
                    out.insert(concept);
                    break;
                }
            }
        }
        Ok(out)
    }
}

impl Model for SnomedProvider {
    fn concept(&self, id: Sctid) -> Result<Option<Ordinal>, EvalError> {
        self.store
            .ordinal(&id.0.to_string())
            .map_err(|e| storage(&e))
    }

    fn sctid(&self, concept: Ordinal) -> Result<Option<Sctid>, EvalError> {
        Ok(self
            .store
            .concept(concept)
            .map_err(|e| storage(&e))?
            .and_then(|record| record.code.parse().ok().map(Sctid)))
    }

    fn all(&self) -> RoaringBitmap {
        (0..self.concepts).collect()
    }

    fn roots(&self) -> RoaringBitmap {
        self.root_set().clone()
    }

    fn leaves(&self) -> RoaringBitmap {
        self.leaf_set().clone()
    }

    fn descendants(&self, concept: Ordinal) -> &RoaringBitmap {
        self.hierarchy.graph.closure.descendants(concept)
    }

    fn ancestors(&self, concept: Ordinal) -> &RoaringBitmap {
        self.hierarchy.graph.closure.ancestors(concept)
    }

    fn children(&self, concept: Ordinal) -> RoaringBitmap {
        self.hierarchy
            .children
            .neighbours(concept)
            .iter()
            .copied()
            .collect()
    }

    fn parents(&self, concept: Ordinal) -> RoaringBitmap {
        self.hierarchy
            .graph
            .is_a
            .neighbours(concept)
            .iter()
            .copied()
            .collect()
    }

    fn attributes(&self) -> &Attributes {
        &self.attributes
    }

    fn members(&self) -> &RefsetMembers {
        &self.member_tables
    }

    fn identifiers(&self) -> &Identifiers {
        &self.identifiers
    }

    /// An alias is an active term of one of the scheme concepts the
    /// identifier file names, compared without case. No specification
    /// defines the alias table: our own design.
    fn scheme(&self, alias: &str) -> Result<Option<u64>, EvalError> {
        for scheme in self.identifiers.schemes() {
            let Some(ordinal) = self.concept(Sctid(scheme))? else {
                continue;
            };
            let named = self
                .store
                .designations(ordinal)
                .map_err(|e| storage(&e))?
                .iter()
                .any(|d| d.active && d.term.eq_ignore_ascii_case(alias));
            if named {
                return Ok(Some(scheme));
            }
        }
        Ok(None)
    }

    fn filter_concepts(
        &self,
        within: &RoaringBitmap,
        predicates: &[ConceptPredicate],
    ) -> Result<RoaringBitmap, EvalError> {
        let mut set = within.clone();
        for predicate in predicates {
            set = match predicate {
                ConceptPredicate::Active(active) => self.by_activity(&set, *active)?,
                ConceptPredicate::DefinitionStatus { defined, primitive } => {
                    self.by_definition_status(&set, *defined, *primitive)?
                }
                ConceptPredicate::Module { modules, negated } => {
                    self.by_module(&set, modules, *negated)?
                }
                ConceptPredicate::EffectiveTime { operator, values } => {
                    self.by_effective_time(&set, *operator, values)?
                }
            };
        }
        Ok(set)
    }

    fn filter_descriptions(
        &self,
        within: &RoaringBitmap,
        predicates: &[DescriptionPredicate],
    ) -> Result<RoaringBitmap, EvalError> {
        match self.indexed_candidates(predicates) {
            Some(candidates) => self.matching_indexed(within, &candidates, predicates),
            None => self.matching_scanned(within, predicates),
        }
    }
}

/// The concept filters of `{{ C … }}`, one method each: the predicates a
/// filter states are applied in turn, and each narrows the set on its own
/// terms (ECL 2.2 §Concept filter constraints).
impl SnomedProvider {
    /// The concepts of `set` that are active, or the inactive ones.
    fn by_activity(&self, set: &RoaringBitmap, active: bool) -> Result<RoaringBitmap, EvalError> {
        let inactive = self.inactive().map_err(|e| provider(&e))?;
        Ok(if active {
            set - inactive
        } else {
            set & inactive
        })
    }

    /// The concepts of `set` whose definition status the filter admits; a
    /// filter that admits both keeps every concept.
    fn by_definition_status(
        &self,
        set: &RoaringBitmap,
        defined: bool,
        primitive: bool,
    ) -> Result<RoaringBitmap, EvalError> {
        let defined_set = self.defined_set()?;
        let mut keep = RoaringBitmap::new();
        if defined {
            keep |= set & defined_set;
        }
        if primitive {
            keep |= set - defined_set;
        }
        Ok(keep)
    }

    /// The concepts of `set` a module names, or the ones it does not.
    fn by_module(
        &self,
        set: &RoaringBitmap,
        modules: &[u64],
        negated: bool,
    ) -> Result<RoaringBitmap, EvalError> {
        let mut keep = RoaringBitmap::new();
        for concept in set {
            let module = self.module_of(Ordinal::new(concept))?;
            if module.is_some_and(|m| modules.contains(&m)) != negated {
                keep.insert(concept);
            }
        }
        Ok(keep)
    }

    /// The concepts of `set` whose effective time the comparison admits.
    fn by_effective_time(
        &self,
        set: &RoaringBitmap,
        operator: sct_ecl::ast::Comparison,
        values: &[u32],
    ) -> Result<RoaringBitmap, EvalError> {
        let mut keep = RoaringBitmap::new();
        for concept in set {
            let time: u32 = self
                .store
                .concept(Ordinal::new(concept))
                .map_err(|e| storage(&e))?
                .and_then(|c| c.effective_time)
                .and_then(|t| t.parse().ok())
                .unwrap_or_default();
            let hit = match &operator {
                sct_ecl::ast::Comparison::NotEqual => values.iter().all(|v| time != *v),
                sct_ecl::ast::Comparison::Equal => values.contains(&time),
                sct_ecl::ast::Comparison::Less => values.iter().any(|v| time < *v),
                sct_ecl::ast::Comparison::LessOrEqual => values.iter().any(|v| time <= *v),
                sct_ecl::ast::Comparison::Greater => values.iter().any(|v| time > *v),
                sct_ecl::ast::Comparison::GreaterOrEqual => values.iter().any(|v| time >= *v),
            };
            if hit {
                keep.insert(concept);
            }
        }
        Ok(keep)
    }
}
