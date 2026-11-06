// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Subsumption between post-coordinated expressions and the concepts of the
//! edition: each side expanded to its supertypes and its attributes, and every
//! requirement of one met by the other.

use super::{Attribute, Expanded, Subject, Term, Value};
use crate::provider::{Concept, error::ProviderError};
use crate::snomed::SnomedProvider;
use crate::stored::storage;
use concept_graph::attributes::ValueRef;
use concept_graph::ordinal::Ordinal;
use concept_graph::subsumption::Outcome;
use roaring::RoaringBitmap;
use std::collections::BTreeMap;

impl SnomedProvider {
    /// The subsumption between two codes when at least one is an expression.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] when the substrate fails.
    pub(in crate::snomed) fn expression_subsumes(
        &self,
        a: Concept,
        b: Concept,
    ) -> Result<Outcome, ProviderError> {
        let a = self.subject(a)?;
        let b = self.subject(b)?;
        let a_under_b = self.entails(&a, &b)?;
        let b_under_a = self.entails(&b, &a)?;
        Ok(match (a_under_b, b_under_a) {
            (true, true) => Outcome::Equivalent,
            (true, false) => Outcome::SubsumedBy,
            (false, true) => Outcome::Subsumes,
            (false, false) => Outcome::NotSubsumed,
        })
    }

    /// What one side of a subsumption test is.
    pub(super) fn subject(&self, concept: Concept) -> Result<Subject, ProviderError> {
        if let Some(record) = self.registered(concept)? {
            return Ok(Subject::Expression(record));
        }
        let ordinal = Ordinal::new(concept.index());
        // A concept with no code is no concept of this version, and two of
        // them would compare equal by an empty identity, which is a wrong
        // subsumption rather than a missing one.
        let Some(code) = self.stored().store.code(ordinal).map_err(storage)? else {
            return Err(ProviderError::CannotDetermine(format!(
                "concept {} is not in this version",
                concept.index()
            )));
        };
        Ok(Subject::Concept {
            ordinal: concept.index(),
            code: code.to_owned(),
            defined: self.is_defined(ordinal)?,
        })
    }

    /// Whether `sup` subsumes `sub`.
    ///
    /// The test is the classic one over the edition's inferred view: every
    /// focus concept of the subsumer subsumes a focus concept of the subsumee,
    /// and every attribute and attribute group of the subsumer is matched by
    /// one of the subsumee whose type and value are subsumed.
    ///
    /// It is sound and it is not complete. SNOMED International withdrew
    /// normal-form comparison as a subsumption test in the July 2019
    /// International Edition and directs implementers to "a description logic
    /// classifier"
    /// (<https://docs.snomed.org/snomed-international-documents/snomed-ct-glossary/n/normal-form>),
    /// and no reasoner runs here. What this answers `subsumes` or
    /// `subsumed-by` holds in the inferred view; what it answers
    /// `not-subsumed` is what that view does not state.
    pub(super) fn entails(&self, sub: &Subject, sup: &Subject) -> Result<bool, ProviderError> {
        if sub.key() == sup.key() {
            return Ok(true);
        }
        let requirements = self.requirements(sup)?;
        if requirements.is_empty() {
            return Ok(false);
        }
        let expanded = self.expanded(&sub.term())?;
        for requirement in requirements {
            if self.satisfies(&expanded, &requirement, 0)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The conditions a code states that are sufficient for subsumption.
    ///
    /// A concept of the edition is one: being subsumed by it is being under it
    /// in the inferred hierarchy, and a sufficiently defined concept is also
    /// satisfied by its own definition. An expression with the `===` status
    /// states its focus concepts and refinement; one with `<<<` states
    /// conditions that are "necessary but not necessarily sufficient" (the
    /// Compositional Grammar specification, §6.7), so nothing but the same
    /// expression is under it.
    pub(super) fn requirements(&self, sup: &Subject) -> Result<Vec<Term>, ProviderError> {
        match sup {
            Subject::Concept {
                ordinal, defined, ..
            } => {
                let mut out = vec![Term {
                    focus: vec![*ordinal],
                    ..Term::default()
                }];
                if *defined {
                    out.push(self.definition_term(Ordinal::new(*ordinal))?);
                }
                Ok(out)
            }
            Subject::Expression(record) if record.defined => Ok(vec![record.term.clone()]),
            Subject::Expression(_) => Ok(Vec::new()),
        }
    }

    /// The inferred definition of a concept as a term: its `is a` parents as
    /// focus concepts and its other inferred rows as the refinement.
    pub(super) fn definition_term(&self, ordinal: Ordinal) -> Result<Term, ProviderError> {
        let focus = self.stored().tree.graph.is_a.neighbours(ordinal).to_vec();
        let (ungrouped, groups) = self.inferred_term(ordinal)?;
        Ok(Term {
            focus,
            ungrouped,
            groups,
        })
    }

    /// What a term necessarily is: every supertype of its focus concepts, and
    /// every attribute it carries.
    ///
    /// A concept's inferred rows are its whole necessary attribute set,
    /// because "the inferred view is the necessary normal form of the concept
    /// definitions, following classification" (the SNOMED CT Release File
    /// Specification, Appendix D), so no supertype walk gathers more.
    pub(super) fn expanded(&self, term: &Term) -> Result<Expanded, ProviderError> {
        let mut supertypes = RoaringBitmap::new();
        let mut ungrouped = term.ungrouped.clone();
        let mut groups = term.groups.clone();
        for focus in &term.focus {
            let ordinal = Ordinal::new(*focus);
            supertypes |= self.stored().tree.graph.closure.ancestors_or_self(ordinal);
            let (inherited, inherited_groups) = self.inferred_term(ordinal)?;
            ungrouped.extend(inherited);
            groups.extend(inherited_groups);
        }
        Ok(Expanded {
            supertypes,
            ungrouped,
            groups,
        })
    }

    /// The inferred attributes of one concept, split into the ungrouped ones
    /// and one list per role group.
    pub(super) fn inferred_term(
        &self,
        ordinal: Ordinal,
    ) -> Result<(Vec<Attribute>, Vec<Vec<Attribute>>), ProviderError> {
        let mut ungrouped = Vec::new();
        let mut groups: BTreeMap<u32, Vec<Attribute>> = BTreeMap::new();
        for row in self.system().attributes.rows(ordinal) {
            let Some(kind) = self.attribute_ordinal(row.kind)? else {
                continue;
            };
            let value = match row.value {
                ValueRef::Concept(target) => Value::Concept(Box::new(Term {
                    focus: vec![target.index()],
                    ..Term::default()
                })),
                ValueRef::Number(number) => Value::Number(number.to_owned()),
                ValueRef::String(text) => Value::Text(text.to_owned()),
            };
            let attribute = Attribute { kind, value };
            if row.group == 0 {
                ungrouped.push(attribute);
            } else {
                groups.entry(row.group).or_default().push(attribute);
            }
        }
        Ok((ungrouped, groups.into_values().collect()))
    }

    /// The store ordinal of an attribute type, by its index in the attribute
    /// table, resolved once for the edition.
    pub(super) fn attribute_ordinal(&self, kind: u32) -> Result<Option<u32>, ProviderError> {
        let table = if let Some(table) = self.system().attribute_ordinals.get() {
            table
        } else {
            let (store, types) = (&self.stored().store, self.system().attributes.types());
            let mut built = Vec::with_capacity(types.len());
            for sctid in types {
                built.push(
                    store
                        .ordinal(&sctid.to_string())
                        .map_err(storage)?
                        .map(Ordinal::index),
                );
            }
            self.system().attribute_ordinals.get_or_init(|| built)
        };
        let index = usize::try_from(kind).unwrap_or(usize::MAX);
        Ok(table.get(index).copied().flatten())
    }

    /// Whether what a subject necessarily is satisfies a requirement.
    pub(super) fn satisfies(
        &self,
        sub: &Expanded,
        requirement: &Term,
        depth: usize,
    ) -> Result<bool, ProviderError> {
        if depth > sct_scg::front::NESTING_LIMIT {
            return Ok(false);
        }
        if !requirement
            .focus
            .iter()
            .all(|focus| sub.supertypes.contains(*focus))
        {
            return Ok(false);
        }
        // NOTE: "a grouped set of attributes is more specific than the same
        // attributes that are not grouped" (the SNOMED CT Editorial Guide,
        // Relationship Group), so a grouped attribute answers an ungrouped ask.
        for required in &requirement.ungrouped {
            let mut candidates = sub.ungrouped.iter().chain(sub.groups.iter().flatten());
            if !self.any_matches(&mut candidates, required, depth)? {
                return Ok(false);
            }
        }
        for required in &requirement.groups {
            if !self.group_matches(sub, required, depth)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Whether one group of the subject answers every attribute of a required
    /// group.
    pub(super) fn group_matches(
        &self,
        sub: &Expanded,
        required: &[Attribute],
        depth: usize,
    ) -> Result<bool, ProviderError> {
        // NOTE: "Grouping all ungrouped attributes with a relationship type
        // that is allowed to be grouped" (§7.7 Classifying) needs the concept
        // model, so an ungrouped attribute answers no required group.
        for group in &sub.groups {
            let mut all = true;
            for attribute in required {
                if !self.any_matches(&mut group.iter(), attribute, depth)? {
                    all = false;
                    break;
                }
            }
            if all {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Whether any candidate answers the required attribute.
    pub(super) fn any_matches<'a>(
        &self,
        candidates: &mut impl Iterator<Item = &'a Attribute>,
        required: &Attribute,
        depth: usize,
    ) -> Result<bool, ProviderError> {
        for candidate in candidates {
            if self.attribute_matches(candidate, required, depth)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Whether one attribute answers a required one: its type is the required
    /// type or a subtype, and its value is subsumed by the required value.
    pub(super) fn attribute_matches(
        &self,
        candidate: &Attribute,
        required: &Attribute,
        depth: usize,
    ) -> Result<bool, ProviderError> {
        let closure = &self.stored().tree.graph.closure;
        if !closure
            .ancestors_or_self(Ordinal::new(candidate.kind))
            .contains(required.kind)
        {
            return Ok(false);
        }
        match (&candidate.value, &required.value) {
            (Value::Concept(candidate), Value::Concept(required)) => {
                let expanded = self.expanded(candidate)?;
                self.satisfies(&expanded, required, depth.saturating_add(1))
            }
            // NOTE: `attribute = attributeName ws "=" ws attributeValue` (§5)
            // is the grammar's one operator on a value, so a concrete value
            // compares by the release's own lexical form.
            (Value::Number(candidate), Value::Number(required))
            | (Value::Text(candidate), Value::Text(required)) => Ok(candidate == required),
            _ => Ok(false),
        }
    }

    /// The focus concepts with every concept another focus concept is subsumed
    /// by removed, since a conjunction states only its most specific members.
    pub(super) fn minimal_focus(&self, focus: &[u32]) -> Vec<u32> {
        let closure = &self.stored().tree.graph.closure;
        let mut minimal: Vec<u32> = Vec::with_capacity(focus.len());
        for candidate in focus {
            let strictly_narrower = focus.iter().any(|other| {
                other != candidate && closure.ancestors(Ordinal::new(*other)).contains(*candidate)
            });
            if !strictly_narrower && !minimal.contains(candidate) {
                minimal.push(*candidate);
            }
        }
        if minimal.is_empty() {
            minimal.extend_from_slice(focus);
            minimal.dedup();
        }
        minimal
    }
}
