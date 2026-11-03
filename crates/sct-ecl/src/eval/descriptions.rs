// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The description, dialect and history filters of an expression constraint.

use super::matching::{Activity, activity, times};
use super::{
    ACCEPTABLE, ConceptPredicate, DEFINED, DEFINITION, DescriptionPredicate, EvalError, Evaluator,
    FULLY_SPECIFIED_NAME, HISTORICAL_ASSOCIATION, Model, PARTIALLY_EQUIVALENT_TO, PREFERRED,
    PRIMITIVE, REPLACED_BY, SAME_AS, SYNONYM, TARGET_COMPONENT, WAS_A,
};
use crate::ast::{
    Acceptability, ConceptFilter, ConceptSet, DefinitionStatus, DescriptionFilter, DialectIdValue,
    Equality, HistorySupplement, Sctid, TypeToken,
};
use concept_graph::refsets::{Table, ValueRef as FieldRef};
use roaring::RoaringBitmap;

impl<M: Model> Evaluator<'_, M> {
    pub(super) fn acceptabilities(
        set: Option<&crate::ast::AcceptabilitySet>,
    ) -> Result<Vec<Acceptability>, EvalError> {
        match set {
            None => Ok(Vec::new()),
            Some(crate::ast::AcceptabilitySet::Tokens(tokens)) => Ok(tokens.clone()),
            Some(crate::ast::AcceptabilitySet::Concepts(references)) => references
                .iter()
                .map(|reference| match reference.id.0 {
                    PREFERRED => Ok(Acceptability::Preferred),
                    ACCEPTABLE => Ok(Acceptability::Acceptable),
                    _ => Err(EvalError::Unsupported(
                        "an acceptability concept other than preferred or acceptable",
                    )),
                })
                .collect(),
        }
    }

    /// The `{{ C ... }}` filters with their concept sets resolved; `None`
    /// when one of them matches no concept (`active != *`).
    pub(super) fn concept_predicates(
        &self,
        filters: &[ConceptFilter],
    ) -> Result<Option<Vec<ConceptPredicate>>, EvalError> {
        let mut predicates = Vec::new();
        for filter in filters {
            predicates.push(match filter {
                ConceptFilter::Active { operator, value } => match activity(*operator, *value) {
                    Activity::Is(active) => ConceptPredicate::Active(active),
                    Activity::Either => continue,
                    Activity::Neither => return Ok(None),
                },
                ConceptFilter::DefinitionStatus { operator, tokens } => {
                    let defined = tokens.contains(&DefinitionStatus::Defined);
                    let primitive = tokens.contains(&DefinitionStatus::Primitive);
                    let equal = *operator == Equality::Equal;
                    ConceptPredicate::DefinitionStatus {
                        defined: defined == equal,
                        primitive: primitive == equal,
                    }
                }
                ConceptFilter::DefinitionStatusId { operator, value } => {
                    let ids = self.sctids_of(value)?;
                    let equal = *operator == Equality::Equal;
                    ConceptPredicate::DefinitionStatus {
                        defined: ids.contains(&DEFINED) == equal,
                        primitive: ids.contains(&PRIMITIVE) == equal,
                    }
                }
                ConceptFilter::Module { operator, value } => ConceptPredicate::Module {
                    modules: self.sctids_of(value)?,
                    negated: *operator == Equality::NotEqual,
                },
                ConceptFilter::EffectiveTime { operator, values } => {
                    ConceptPredicate::EffectiveTime {
                        operator: *operator,
                        values: times(values)?,
                    }
                }
            });
        }
        Ok(Some(predicates))
    }

    /// The `{{ D ... }}` filters with their concept sets and aliases resolved;
    /// `None` when one of them matches no description (`active != *`).
    pub(super) fn description_predicates(
        &self,
        filters: &[DescriptionFilter],
    ) -> Result<Option<Vec<DescriptionPredicate>>, EvalError> {
        let mut predicates = Vec::new();
        for filter in filters {
            match self.description_predicate(filter)? {
                Some(predicate) => predicates.push(predicate),
                None => return Ok(None),
            }
        }
        Ok(Some(predicates))
    }

    /// One `{{ D ... }}` filter with its concept sets and aliases resolved;
    /// `None` for a filter no description passes.
    pub(super) fn description_predicate(
        &self,
        filter: &DescriptionFilter,
    ) -> Result<Option<DescriptionPredicate>, EvalError> {
        Ok(Some(match filter {
            DescriptionFilter::Term { operator, terms } => DescriptionPredicate::Term {
                operator: *operator,
                terms: terms.clone(),
            },
            DescriptionFilter::Language { operator, codes } => DescriptionPredicate::Language {
                operator: *operator,
                codes: codes.clone(),
            },
            DescriptionFilter::TypeId { operator, value } => DescriptionPredicate::Type {
                operator: *operator,
                types: self.sctids_of(value)?,
            },
            DescriptionFilter::Type { operator, tokens } => DescriptionPredicate::Type {
                operator: *operator,
                types: tokens
                    .iter()
                    .map(|t| match t {
                        TypeToken::Synonym => SYNONYM,
                        TypeToken::FullySpecifiedName => FULLY_SPECIFIED_NAME,
                        TypeToken::Definition => DEFINITION,
                    })
                    .collect(),
            },
            DescriptionFilter::DialectId {
                operator,
                value,
                acceptability,
            } => DescriptionPredicate::Dialect {
                operator: *operator,
                dialects: self.dialects_by_id(value, acceptability.as_ref())?,
            },
            DescriptionFilter::Dialect {
                operator,
                aliases,
                acceptability,
            } => DescriptionPredicate::Dialect {
                operator: *operator,
                dialects: Self::dialects_by_alias(aliases, acceptability.as_ref())?,
            },
            DescriptionFilter::Module { .. } => {
                return Err(EvalError::Unsupported(
                    "a description module filter: the store keeps no description module",
                ));
            }
            DescriptionFilter::EffectiveTime { .. } => {
                return Err(EvalError::Unsupported(
                    "a description effective time filter: the store keeps no description time",
                ));
            }
            DescriptionFilter::Active { operator, value } => match activity(*operator, *value) {
                Activity::Is(active) => DescriptionPredicate::Active(Some(active)),
                Activity::Either => DescriptionPredicate::Active(None),
                Activity::Neither => return Ok(None),
            },
            DescriptionFilter::Id { operator, ids } => DescriptionPredicate::Id {
                operator: *operator,
                ids: ids.iter().map(|id| id.0).collect(),
            },
        }))
    }

    /// The language reference sets a `dialectId` filter names, each with the
    /// acceptabilities it admits; an entry's own set wins over the shared one.
    pub(super) fn dialects_by_id(
        &self,
        value: &DialectIdValue,
        acceptability: Option<&crate::ast::AcceptabilitySet>,
    ) -> Result<Vec<(u64, Vec<Acceptability>)>, EvalError> {
        let shared = Self::acceptabilities(acceptability)?;
        match value {
            DialectIdValue::Expression(sub) => Ok(self
                .sctids_of(&ConceptSet::Expression(sub.clone()))?
                .into_iter()
                .map(|id| (id, shared.clone()))
                .collect()),
            DialectIdValue::Set(items) => {
                let mut out = Vec::new();
                for (reference, own) in items {
                    let allowed = if own.is_some() {
                        Self::acceptabilities(own.as_ref())?
                    } else {
                        shared.clone()
                    };
                    out.push((reference.id.0, allowed));
                }
                Ok(out)
            }
        }
    }

    /// The language reference sets a `dialect` filter's aliases name, each
    /// with the acceptabilities it admits.
    pub(super) fn dialects_by_alias(
        aliases: &[crate::ast::DialectAlias],
        acceptability: Option<&crate::ast::AcceptabilitySet>,
    ) -> Result<Vec<(u64, Vec<Acceptability>)>, EvalError> {
        let shared = Self::acceptabilities(acceptability)?;
        let mut dialects = Vec::new();
        for alias in aliases {
            let refset = crate::dialects::refset(&alias.alias)
                .ok_or_else(|| EvalError::UnknownDialect(alias.alias.clone()))?;
            let allowed = if alias.acceptability.is_some() {
                Self::acceptabilities(alias.acceptability.as_ref())?
            } else {
                shared.clone()
            };
            dialects.push((refset, allowed));
        }
        Ok(dialects)
    }

    /// The history supplement: the inactive concepts whose association in the
    /// chosen reference sets targets a member of `set`.
    pub(super) fn history(
        &self,
        mut set: RoaringBitmap,
        history: &HistorySupplement,
    ) -> Result<RoaringBitmap, EvalError> {
        let refsets = self.history_refsets(history)?;
        let added = self.associated_members(&set, &refsets);
        set |= added;
        Ok(set)
    }

    /// The association reference sets a history supplement follows.
    pub(super) fn history_refsets(
        &self,
        history: &HistorySupplement,
    ) -> Result<Vec<u64>, EvalError> {
        match history {
            HistorySupplement::Minimum => Ok(vec![SAME_AS]),
            HistorySupplement::Moderate => {
                Ok(vec![SAME_AS, REPLACED_BY, WAS_A, PARTIALLY_EQUIVALENT_TO])
            }
            HistorySupplement::Default | HistorySupplement::Maximum => {
                match self.model.concept(Sctid(HISTORICAL_ASSOCIATION))? {
                    Some(root) => self.sctids_of_set(self.model.descendants(root)),
                    None => Ok(Vec::new()),
                }
            }
            HistorySupplement::Subset(constraint) => {
                self.sctids_of_set(&self.expression(constraint)?)
            }
        }
    }

    /// The members of the association reference sets whose target lies in
    /// `set`.
    pub(super) fn associated_members(&self, set: &RoaringBitmap, refsets: &[u64]) -> RoaringBitmap {
        let mut added = RoaringBitmap::new();
        for refset in refsets {
            let Some(table) = self.model.members().table(*refset) else {
                continue;
            };
            let Some(target) = table.field(TARGET_COMPONENT) else {
                continue;
            };
            added |= Self::members_targeting(table, target, set);
        }
        added
    }

    /// The members of one association table whose target field lies in `set`.
    pub(super) fn members_targeting(
        table: &Table,
        target: usize,
        set: &RoaringBitmap,
    ) -> RoaringBitmap {
        let mut out = RoaringBitmap::new();
        for row in 0..table.len() {
            if let (Some(FieldRef::Concept(value)), Some(member)) =
                (table.value(row, target), table.concept(row))
                && set.contains(value.index())
            {
                out.insert(member.index());
            }
        }
        out
    }
}
