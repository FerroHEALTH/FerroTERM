// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Whether a post-coordinated expression, which the edition does not
//! enumerate, satisfies an expression constraint.
//!
//! The expression stands as a concept below its focus concepts with the
//! attribute rows it states beside the ones its focus concepts carry, and each
//! part of the constraint is tested against it the way it selects a concept.

use concept_graph::attributes::Row;
use concept_graph::ordinal::Ordinal;
use roaring::RoaringBitmap;

use super::matching::{matching_rows, relationship_groups, set_holds, within};
use super::{EvalError, Evaluator, Model};
use crate::ast::{
    AttributeSet, ConstraintOperator, ExpressionConstraint, FocusConcept, Refinement,
    SubAttributeSet, SubExpressionConstraint, SubRefinement,
};

/// A post-coordinated expression as the evaluator tests it.
#[derive(Debug, Clone)]
pub struct Candidate<'a> {
    /// The focus concepts.
    pub focus: Vec<Ordinal>,
    /// Whether the expression refines its focus concepts, which makes it a
    /// proper subtype of each; one that does not is its focus concept.
    pub refined: bool,
    /// Its attribute rows, sorted by role group: its own and the ones its
    /// focus concepts carry, with the groups kept apart.
    pub rows: Vec<Row<'a>>,
}

/// Whether `candidate` satisfies `constraint`.
///
/// # Errors
///
/// Returns [`EvalError`] when the constraint names something the edition does
/// not have, or a construct the test cannot answer for an expression (a
/// concept or description filter, a history supplement).
pub fn holds<M: Model>(
    model: &M,
    constraint: &ExpressionConstraint,
    candidate: &Candidate<'_>,
) -> Result<bool, EvalError> {
    let evaluator = Evaluator { model };
    if let ([focus], false) = (candidate.focus.as_slice(), candidate.refined) {
        return Ok(evaluator.expression(constraint)?.contains(focus.index()));
    }
    evaluator.holds(constraint, candidate)
}

impl<M: Model> Evaluator<'_, M> {
    fn holds(
        &self,
        constraint: &ExpressionConstraint,
        candidate: &Candidate<'_>,
    ) -> Result<bool, EvalError> {
        match constraint {
            ExpressionConstraint::Sub(sub) => self.holds_sub(sub, candidate),
            ExpressionConstraint::Refined { focus, refinement } => Ok(self
                .holds_sub(focus, candidate)?
                && self.holds_refinement(refinement, &candidate.rows)?),
            ExpressionConstraint::Conjunction(operands) => {
                for operand in operands {
                    if !self.holds_sub(operand, candidate)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            ExpressionConstraint::Disjunction(operands) => {
                for operand in operands {
                    if self.holds_sub(operand, candidate)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            ExpressionConstraint::Exclusion { left, right } => {
                Ok(self.holds_sub(left, candidate)? && !self.holds_sub(right, candidate)?)
            }
            // NOTE: a dotted constraint selects attribute values, and an expression
            // is the value of no relationship of the edition.
            ExpressionConstraint::Dotted { .. } => Ok(false),
        }
    }

    /// One sub-expression constraint: the concepts the expression is below
    /// against the ones its focus and operator reach.
    fn holds_sub(
        &self,
        sub: &SubExpressionConstraint,
        candidate: &Candidate<'_>,
    ) -> Result<bool, EvalError> {
        // NOTE: `^` returns "the set of concepts that are referenced by the given
        // reference set" (ECL 6.1), and no reference set references an expression.
        if sub.refset_operator.is_some() {
            return Ok(false);
        }
        if !sub.filters.is_empty() || sub.history.is_some() {
            return Err(EvalError::Unsupported(
                "a filter or history supplement over a post-coordinated expression",
            ));
        }
        // NOTE: `*` may "represent any concept in the substrate" (ECL 6.1),
        // and an expression is a clinical meaning of it.
        if sub.operator.is_none() && matches!(sub.focus, FocusConcept::Wildcard) {
            return Ok(true);
        }
        let reached = self.focus(&sub.focus)?;
        let above = match sub.operator {
            Some(ConstraintOperator::DescendantOf | ConstraintOperator::DescendantOrSelfOf) => {
                self.supertypes(&candidate.focus)
            }
            // NOTE: a refined expression is a proper subtype of its focus concepts,
            // so it is no concept a constraint names and an ancestor of none; whether
            // a concept of the edition sits between it and a focus concept takes a
            // classifier, so it is not claimed as a child, a top or a bottom either.
            _ => return Ok(false),
        };
        Ok(!above.is_disjoint(&reached))
    }

    /// The focus concepts and every ancestor of one.
    fn supertypes(&self, focus: &[Ordinal]) -> RoaringBitmap {
        let mut out = RoaringBitmap::new();
        for concept in focus {
            out.insert(concept.index());
            out |= self.model.ancestors(*concept);
        }
        out
    }

    fn holds_refinement(
        &self,
        refinement: &Refinement,
        rows: &[Row<'_>],
    ) -> Result<bool, EvalError> {
        match refinement {
            Refinement::Single(one) => self.holds_sub_refinement(one, rows),
            Refinement::Conjunction(items) => {
                for item in items {
                    if !self.holds_sub_refinement(item, rows)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Refinement::Disjunction(items) => {
                for item in items {
                    if self.holds_sub_refinement(item, rows)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    fn holds_sub_refinement(
        &self,
        item: &SubRefinement,
        rows: &[Row<'_>],
    ) -> Result<bool, EvalError> {
        match item {
            SubRefinement::AttributeSet(set) => self.holds_attribute_set(set, rows),
            SubRefinement::Nested(inner) => self.holds_refinement(inner, rows),
            SubRefinement::Group {
                cardinality,
                attributes,
            } => {
                let tests = self.compile_set(attributes)?;
                let mut matching = 0_u32;
                for group in &relationship_groups(rows) {
                    if set_holds(&tests, group)? {
                        matching = matching.saturating_add(1);
                    }
                }
                Ok(within(*cardinality, matching))
            }
        }
    }

    fn holds_attribute_set(&self, set: &AttributeSet, rows: &[Row<'_>]) -> Result<bool, EvalError> {
        let item = |item: &SubAttributeSet| -> Result<bool, EvalError> {
            match item {
                SubAttributeSet::Attribute(attribute) => {
                    // NOTE: a reversed attribute counts the concepts whose rows point
                    // at this one, and no row of the edition points at an expression.
                    if attribute.reverse {
                        return Ok(within(attribute.cardinality, 0));
                    }
                    let test = self.compile(attribute)?;
                    Ok(within(
                        attribute.cardinality,
                        matching_rows(&test, rows.iter())?,
                    ))
                }
                SubAttributeSet::Nested(inner) => self.holds_attribute_set(inner, rows),
            }
        };
        match set {
            AttributeSet::Single(one) => item(one),
            AttributeSet::Conjunction(items) => {
                for one in items {
                    if !item(one)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            AttributeSet::Disjunction(items) => {
                for one in items {
                    if item(one)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }
}
