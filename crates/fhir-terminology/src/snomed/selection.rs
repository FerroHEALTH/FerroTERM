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

/// Whether a stored `definitionStatusId` names the defined status.
pub(super) fn states_defined(status: &[record::PropertyValueRef<'_>]) -> bool {
    status.iter().any(|value| {
        matches!(value, record::PropertyValueRef::Code(code)
            if code.parse::<u64>() == Ok(constants::DEFINED.value()))
    })
}

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

    /// The concepts a `concept in` filter lists, each reference set among them
    /// standing for its members.
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
            // NOTE: `in` selects the codes the value lists ("the set of codes or concepts
            // specified", R4B filter-operator), and a reference set among them stands for
            // its members (<https://hl7.org/fhir/R4B/snomedct.html>, "Filter Properties").
            if let Some(members) = self.system().memberships.members(refset.value()) {
                set |= members;
                continue;
            }
            let concept = self
                .stored()
                .store
                .ordinal(value)
                .map_err(storage)?
                .ok_or_else(|| ProviderError::UnknownCode(value.to_owned()))?;
            set.insert(concept.index());
        }
        Ok(set)
    }

    /// Whether the expression `record` passes `filter`: an expression
    /// constraint is tested against the expression itself, since it defines
    /// "a bounded set of clinical meanings represented by either precoordinated
    /// or postcoordinated expressions" (ECL 1 Introduction); any other filter
    /// through its focus concepts, which subsume it.
    ///
    /// "The meaning of the expression is a subtype of all the focus concepts"
    /// (Compositional Grammar §5, Informative Comments), so `is-a` and
    /// `descendent-of` on the concept, which hold every subtype of a member,
    /// admit it when one focus concept is a member. Every other filter needs
    /// all of them.
    pub(super) fn expression_passes(
        &self,
        record: &super::expression::Registered,
        filter: &Filter,
    ) -> Result<bool, ProviderError> {
        if filter.op == FilterOperator::Equal && filter.property == EXPRESSIONS {
            return Ok(filter.value.trim() == "true");
        }
        if filter.op == FilterOperator::Equal && filter.property == "constraint" {
            let parsed = self.parsed_constraint(&filter.value)?;
            let candidate = self.candidate(record)?;
            return sct_ecl::eval::expression::holds(self, &parsed, &candidate)
                .map_err(|error| super::ecl::refusal(error, &filter.value));
        }
        let selected = self.filter(filter)?;
        let mut focus = record.focus_concepts().iter();
        let subtype_closed = matches!(filter.property.as_str(), "concept" | "code")
            && matches!(
                filter.op,
                FilterOperator::IsA | FilterOperator::DescendentOf
            );
        Ok(if subtype_closed {
            focus.any(|concept| selected.contains(*concept))
        } else {
            focus.all(|concept| selected.contains(*concept))
        })
    }

    /// Whether the concept is sufficiently defined by its conditions.
    ///
    /// The RF2 `definitionStatusId` decides it, which is the same field the
    /// `sufficientlyDefined` property is derived from
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "SNOMED CT Properties").
    pub(super) fn is_defined(&self, ordinal: Ordinal) -> Result<bool, ProviderError> {
        let status = self
            .stored()
            .store
            .property(ordinal, self.system().keys.definition_status)
            .map_err(storage)?;
        Ok(states_defined(&status))
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
