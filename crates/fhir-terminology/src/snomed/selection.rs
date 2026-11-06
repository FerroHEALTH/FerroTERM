// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The selections the SNOMED CT filters answer from the edition itself:
//! reference set membership, the inactive concepts, and an expression tested
//! through its focus concepts.

use super::{EXPRESSIONS, SnomedProvider};
use crate::normal_form;
use crate::provider::filter::{Filter, FilterOperator};
use crate::provider::{CodeSystemProvider, Concept, ConceptSet, error::ProviderError};
use crate::stored::storage;
use concept_graph::ordinal::Ordinal;
use concept_store::record;
use rf2::constants;
use rf2::id::ConceptId;

impl SnomedProvider {
    /// The inactive concepts, scanned once from the store and kept.
    pub(super) fn inactive_set(&self) -> Result<ConceptSet, ProviderError> {
        if let Some(set) = self.system().inactive.get() {
            return Ok(set.clone());
        }
        let mut set = ConceptSet::new();
        for index in 0..self.stored().concepts {
            if let Some(record) = self
                .stored()
                .store
                .concept(Ordinal::new(index))
                .map_err(storage)?
                && !record.active
            {
                set.insert(index);
            }
        }
        Ok(self.system().inactive.get_or_init(|| set).clone())
    }

    /// The members of the reference sets a `concept in` filter names.
    pub(super) fn reference_set_members(
        &self,
        filter: &Filter,
    ) -> Result<ConceptSet, ProviderError> {
        let mut set = ConceptSet::new();
        for value in filter
            .value
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            // The error names the value and the reason; the id error adds nothing.
            let Ok(refset) = ConceptId::parse(value) else {
                return Err(ProviderError::invalid_filter_value(
                    &filter.property,
                    value,
                    "not an SCTID",
                ));
            };
            let members = self
                .system()
                .memberships
                .members(refset.value())
                .ok_or_else(|| ProviderError::UnknownCode(value.to_owned()))?;
            set |= members;
        }
        Ok(set)
    }

    /// Whether the expression `record` passes `filter`: through its focus
    /// concepts, which subsume it.
    pub(super) fn expression_passes(
        &self,
        record: &super::expression::Registered,
        filter: &Filter,
    ) -> Result<bool, ProviderError> {
        if filter.op == FilterOperator::Equal && filter.property == EXPRESSIONS {
            return Ok(filter.value.trim() == "true");
        }
        let selected = self.filter(filter)?;
        Ok(record
            .focus_concepts()
            .iter()
            .all(|focus| selected.contains(*focus)))
    }

    /// Whether the concept is sufficiently defined by its conditions.
    ///
    /// The RF2 `definitionStatusId` decides it, which is the same field the
    /// `sufficientlyDefined` property is derived from
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "SNOMED CT Properties").
    pub(super) fn is_defined(&self, ordinal: Ordinal) -> Result<bool, ProviderError> {
        let stored = self.stored().store.properties(ordinal).map_err(storage)?;
        Ok(stored
            .iter()
            .filter(|(key, _)| *key == self.system().keys.definition_status)
            .any(|(_, values)| {
                values.iter().any(|value| {
                    matches!(value, record::PropertyValue::Code(code) if *code == constants::DEFINED.to_string())
                })
            }))
    }

    /// The Necessary Normal Form of `concept`, with terms when `terms` asks.
    ///
    /// The expression is the concept's inferred view rendered, which the
    /// release already holds: the focus concepts are its `is a` parents and
    /// the attributes are its other inferred rows, so this is two point reads
    /// and no walk (`crate::normal_form`).
    ///
    /// Returns `None` for a concept with no inferred parent, which is the root
    /// of the hierarchy and nothing else.
    pub(super) fn normal_form(
        &self,
        concept: Concept,
        terms: bool,
    ) -> Result<Option<String>, ProviderError> {
        let ordinal = Self::ordinal(concept);
        // NOTE: the term is the display this server would answer with, which
        // the grammar admits ("the term from any SNOMED CT description that is
        // associated with the concept", the Compositional Grammar §5).
        let language = if terms {
            super::expression::Terms::In(None)
        } else {
            super::expression::Terms::None
        };
        let parents = self.stored().tree.graph.is_a.neighbours(ordinal);
        Ok(normal_form::Expression {
            defined: self.is_defined(ordinal)?,
            focus: self.references(parents.iter().copied(), language)?,
            attributes: self.inferred_attributes(ordinal, 0, language)?,
        }
        .render())
    }
}
