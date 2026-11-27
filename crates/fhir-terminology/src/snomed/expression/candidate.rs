// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An expression as the ECL evaluator tests it against a constraint.

use concept_graph::attributes::{Row, ValueRef};
use concept_graph::ordinal::Ordinal;
use sct_ecl::eval::expression::Candidate;

use super::{Attribute, Registered, Value};
use crate::provider::error::ProviderError;
use crate::snomed::SnomedProvider;
use crate::stored::storage;

impl SnomedProvider {
    /// An expression as the ECL evaluator tests it: its focus concepts, and
    /// its own attribute rows beside the ones its focus concepts carry, each
    /// role group kept apart.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] when the substrate fails.
    pub(in crate::snomed) fn candidate<'r>(
        &'r self,
        record: &'r Registered,
    ) -> Result<Candidate<'r>, ProviderError> {
        let term = &record.term;
        let mut rows: Vec<Row<'r>> = Vec::new();
        let mut next_group = 1_u32;
        for focus in &term.focus {
            let mut last = 0_u32;
            for row in self.system().attributes.rows(Ordinal::new(*focus)) {
                let group = if row.group == 0 {
                    0
                } else {
                    last = last.max(row.group);
                    next_group.saturating_add(row.group)
                };
                rows.push(Row { group, ..row });
            }
            next_group = next_group.saturating_add(last).saturating_add(1);
        }
        for attribute in &term.ungrouped {
            if let Some(row) = self.candidate_row(attribute, 0)? {
                rows.push(row);
            }
        }
        for group in &term.groups {
            for attribute in group {
                if let Some(row) = self.candidate_row(attribute, next_group)? {
                    rows.push(row);
                }
            }
            next_group = next_group.saturating_add(1);
        }
        rows.sort_by_key(|row| row.group);
        Ok(Candidate {
            focus: term
                .focus
                .iter()
                .map(|focus| Ordinal::new(*focus))
                .collect(),
            refined: !term.ungrouped.is_empty() || !term.groups.is_empty(),
            rows,
        })
    }

    /// One attribute an expression states, as a row of the attribute table;
    /// `None` for a type the table does not list, which no attribute name of a
    /// constraint can select.
    fn candidate_row<'r>(
        &self,
        attribute: &'r Attribute,
        group: u32,
    ) -> Result<Option<Row<'r>>, ProviderError> {
        let Some(record) = self
            .stored()
            .store
            .concept(Ordinal::new(attribute.kind))
            .map_err(storage)?
        else {
            return Ok(None);
        };
        let Some(kind) = self
            .system()
            .attributes
            .types()
            .iter()
            .position(|sctid| sctid.to_string() == record.code)
            .and_then(|index| u32::try_from(index).ok())
        else {
            return Ok(None);
        };
        let value = match &attribute.value {
            Value::Concept(term) => match term.focus.first() {
                Some(focus) => ValueRef::Concept(Ordinal::new(*focus)),
                None => return Ok(None),
            },
            Value::Number(number) => ValueRef::Number(number),
            Value::Text(text) => ValueRef::String(text),
        };
        Ok(Some(Row { group, kind, value }))
    }
}
