// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reference set membership in an expression constraint: `^` over a reference
//! set, the member rows a field filter selects, and the concepts their columns
//! name.

use super::matching::{compare_numbers, number, time, times, times_match};
use super::{EvalError, Evaluator, Model, REFERENCED_COMPONENT, term_matches};
use crate::ast::{
    ActiveValue, ConceptSet, Equality, FieldValue, FocusConcept, MemberFilter, RefsetFields, Sctid,
};
use concept_graph::ordinal::Ordinal;
use concept_graph::refsets::{Table, ValueRef as FieldRef};
use roaring::RoaringBitmap;

impl<M: Model> Evaluator<'_, M> {
    /// `^`: the referenced components (or the selected fields) of the active
    /// members of the reference sets in `set`, those rows passing the member
    /// filters.
    pub(super) fn member_of(
        &self,
        set: &RoaringBitmap,
        fields: Option<&RefsetFields>,
        filter_groups: &[Vec<MemberFilter>],
    ) -> Result<RoaringBitmap, EvalError> {
        let mut out = RoaringBitmap::new();
        for concept in set {
            let Some(id) = self.model.sctid(Ordinal::new(concept))? else {
                continue;
            };
            let Some(table) = self.model.members().table(id.0) else {
                return Err(EvalError::NotAReferenceSet(id));
            };
            out |= self.table_members(id, table, fields, filter_groups)?;
        }
        Ok(out)
    }

    /// `^R`: the reference sets with an active member whose referenced
    /// component is in `set` and that passes the member filters (ECL 2.3,
    /// "Refset Containing").
    pub(super) fn containing_any(
        &self,
        set: &RoaringBitmap,
        filter_groups: &[Vec<MemberFilter>],
    ) -> Result<RoaringBitmap, EvalError> {
        let members = self.model.members();
        let mut out = RoaringBitmap::new();
        for refset in members.refsets() {
            let Some(table) = members.table(refset) else {
                continue;
            };
            if table.members().is_disjoint(set) {
                continue;
            }
            let mut contains = filter_groups.is_empty();
            for row in 0..table.len() {
                if contains {
                    break;
                }
                let referenced = table.concept(row).is_some_and(|c| set.contains(c.index()));
                contains = referenced && self.row_passes(table, row, filter_groups)?;
            }
            // NOTE: the answer is a set of the edition's concepts, so a reference
            // set whose own concept the edition lacks cannot be in it.
            if contains && let Some(ordinal) = self.model.concept(Sctid(refset))? {
                out.insert(ordinal.index());
            }
        }
        Ok(out)
    }

    /// The concepts one reference set contributes: the selected columns of
    /// every row that passes the member filters.
    pub(super) fn table_members(
        &self,
        refset: Sctid,
        table: &Table,
        fields: Option<&RefsetFields>,
        filter_groups: &[Vec<MemberFilter>],
    ) -> Result<RoaringBitmap, EvalError> {
        let columns = Self::selected_columns(refset, table, fields)?;
        let mut out = RoaringBitmap::new();
        for row in 0..table.len() {
            if !self.row_passes(table, row, filter_groups)? {
                continue;
            }
            for column in &columns {
                if let Some(value) = Self::column_concept(table, row, *column) {
                    out.insert(value.index());
                }
            }
        }
        Ok(out)
    }

    /// The concept one selected column of a row holds: the referenced
    /// component for `None`, else the field's concept value.
    pub(super) fn column_concept(
        table: &Table,
        row: usize,
        column: Option<usize>,
    ) -> Option<Ordinal> {
        match column {
            None => table.concept(row),
            Some(field) => match table.value(row, field) {
                Some(FieldRef::Concept(value)) => Some(value),
                _ => None,
            },
        }
    }

    /// The columns a field selection names: `None` is the referenced
    /// component, `Some(i)` a field; `[*]` is every concept-valued column.
    pub(super) fn selected_columns(
        refset: Sctid,
        table: &Table,
        fields: Option<&RefsetFields>,
    ) -> Result<Vec<Option<usize>>, EvalError> {
        Ok(match fields {
            None => vec![None],
            Some(RefsetFields::Any) => {
                let mut columns = vec![None];
                columns.extend(
                    table
                        .kinds()
                        .iter()
                        .enumerate()
                        .filter(|(_, kind)| **kind == concept_graph::refsets::FieldKind::Component)
                        .map(|(i, _)| Some(i)),
                );
                columns
            }
            Some(RefsetFields::Names(names)) => {
                let mut columns = Vec::new();
                for name in names {
                    if name.eq_ignore_ascii_case(REFERENCED_COMPONENT) {
                        columns.push(None);
                    } else {
                        columns.push(Some(table.field(name).ok_or_else(|| {
                            EvalError::UnknownField {
                                refset,
                                field: name.clone(),
                            }
                        })?));
                    }
                }
                columns
            }
        })
    }

    /// Whether row `row` of `table` satisfies every member filter.
    pub(super) fn row_passes(
        &self,
        table: &Table,
        row: usize,
        filter_groups: &[Vec<MemberFilter>],
    ) -> Result<bool, EvalError> {
        for filter in filter_groups.iter().flatten() {
            let passes = match filter {
                MemberFilter::Active { operator, value } => match (operator, value) {
                    (Equality::Equal, ActiveValue::Active)
                    | (Equality::NotEqual, ActiveValue::Inactive) => true,
                    (Equality::NotEqual, ActiveValue::Any) => false,
                    _ => {
                        return Err(EvalError::Unsupported(
                            "a member filter on inactive members: the tables hold active members",
                        ));
                    }
                },
                MemberFilter::EffectiveTime { operator, values } => {
                    let actual = table.effective_time(row).ok_or_else(|| {
                        EvalError::Storage(format!("reference set row {row} has no effective time"))
                    })?;
                    times_match(*operator, &times(values)?, actual)
                }
                MemberFilter::Module { operator, value } => {
                    let modules = self.sctids_of(value)?;
                    let inside = table.module(row).is_some_and(|m| modules.contains(&m));
                    inside == (*operator == Equality::Equal)
                }
                MemberFilter::Field { name, value } => {
                    let Some(column) = table.field(name) else {
                        return Ok(false);
                    };
                    self.field_passes(table.value(row, column), value)?
                }
            };
            if !passes {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn field_passes(
        &self,
        actual: Option<FieldRef<'_>>,
        expected: &FieldValue,
    ) -> Result<bool, EvalError> {
        Ok(match (expected, actual) {
            (FieldValue::Expression { operator, value }, Some(FieldRef::Concept(concept))) => {
                let set = self.sub(value)?;
                set.contains(concept.index()) == (*operator == Equality::Equal)
            }
            (FieldValue::Expression { operator, .. }, _) => *operator == Equality::NotEqual,
            (FieldValue::Numeric { operator, value }, Some(FieldRef::Integer(actual))) => {
                let expected = number(&value.0)?;
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_precision_loss,
                    reason = "reference set integers are small"
                )]
                let actual = actual as f64;
                compare_numbers(*operator, actual, expected)
            }
            (FieldValue::String { operator, terms }, Some(FieldRef::String(text))) => {
                terms.iter().any(|t| term_matches(t, text)) == (*operator == Equality::Equal)
            }
            (FieldValue::Boolean { operator, value }, Some(FieldRef::String(text))) => {
                let actual = text.eq_ignore_ascii_case("true");
                (actual == *value) == (*operator == Equality::Equal)
            }
            (FieldValue::Time { operator, values }, Some(FieldRef::String(text))) => {
                times_match(*operator, &times(values)?, time(text)?)
            }
            (FieldValue::Time { operator, values }, Some(FieldRef::Integer(actual))) => {
                times_match(*operator, &times(values)?, time(&actual.to_string())?)
            }
            _ => false,
        })
    }

    /// The SCTIDs a concept set names: a plain reference or a reference set
    /// as written (the metadata concepts need not be in the edition), any
    /// other constraint by evaluation.
    pub(super) fn sctids_of(&self, set: &ConceptSet) -> Result<Vec<u64>, EvalError> {
        match set {
            ConceptSet::Set(references) => Ok(references.iter().map(|r| r.id.0).collect()),
            ConceptSet::Expression(sub)
                if sub.operator.is_none()
                    && sub.refset_operator.is_none()
                    && sub.filters.is_empty()
                    && sub.member_filters.is_empty()
                    && sub.history.is_none() =>
            {
                match &sub.focus {
                    FocusConcept::Reference(reference) => Ok(vec![reference.id.0]),
                    _ => self.sctids_of_set(&self.sub(sub)?),
                }
            }
            ConceptSet::Expression(sub) => self.sctids_of_set(&self.sub(sub)?),
        }
    }

    pub(super) fn sctids_of_set(&self, set: &RoaringBitmap) -> Result<Vec<u64>, EvalError> {
        let mut ids = Vec::new();
        for concept in set {
            if let Some(id) = self.model.sctid(Ordinal::new(concept))? {
                ids.push(id.0);
            }
        }
        Ok(ids)
    }
}
