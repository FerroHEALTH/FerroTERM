// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a served SNOMED CT edition holds: its accessors, its memory
//! footprint, and the display rule over its language reference sets.

use concept_graph::attributes::Attributes;
use concept_graph::identifiers::Identifiers;
use concept_graph::ordinal::Ordinal;
use concept_graph::refsets::RefsetMembers;

use super::SnomedProvider;
use crate::provider::{Concept, error::ProviderError};
use crate::stored::{primary_subtag, storage};

/// What each structure of a served edition holds in memory, in bytes.
///
/// No FHIR or SNOMED CT specification governs this: our own design, and the
/// figures are what the structures count rather than what a process reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footprint {
    /// The resident columns of the store, by column name.
    pub store_columns: [(&'static str, usize); 4],
    /// The child-to-parent adjacency.
    pub is_a: usize,
    /// The ancestor and descendant bitmaps.
    pub closure: usize,
    /// The parent-to-child adjacency, transposed when the edition opens.
    pub children: usize,
    /// The designation dictionary and its postings.
    pub text: usize,
    /// The reference set member tables.
    pub member_tables: usize,
    /// The attribute rows.
    pub attributes: usize,
    /// The attribute inverted index, derived when the edition opens.
    pub attributes_inverted: usize,
    /// The reference set membership bitmaps.
    pub memberships: usize,
    /// The alternate identifier table.
    pub identifiers: usize,
}

impl SnomedProvider {
    /// The edition URI (`http://snomed.info/sct/{module}`), the version URI
    /// without its date.
    #[must_use]
    pub fn edition_uri(&self) -> &str {
        &self.system().edition
    }

    /// What every structure of this edition holds in memory.
    ///
    /// Each figure is the structure's own count of the heap its allocations
    /// hold, so a served edition's memory is accounted for structure by
    /// structure. The caches an operation fills later
    /// (the root, leaf, defined, and inactive sets, and the parsed expression
    /// constraints) are not here: they are empty until a request asks for one.
    #[must_use]
    pub fn footprint(&self) -> Footprint {
        Footprint {
            store_columns: self.stored().store.column_sizes(),
            is_a: self.stored().tree.graph.is_a.size_in_bytes(),
            closure: self.stored().tree.graph.closure.size_in_bytes(),
            children: self.stored().tree.children.size_in_bytes(),
            text: self.stored().text.size_in_bytes(),
            member_tables: self.system().member_tables.size_in_bytes(),
            attributes: self.system().attributes.rows_size_in_bytes(),
            attributes_inverted: self.system().attributes.inverted_size_in_bytes(),
            memberships: self.system().memberships.size_in_bytes(),
            identifiers: self.system().identifiers.size_in_bytes(),
        }
    }

    /// The attribute relationships of the edition, with their role groups.
    #[expect(
        clippy::same_name_method,
        reason = "the ECL evaluator's trait exposes the same graph part; this is the crate's own accessor"
    )]
    #[must_use]
    pub fn attributes(&self) -> &Attributes {
        &self.system().attributes
    }

    /// The active reference set member rows of the edition, with their fields.
    #[must_use]
    pub fn member_tables(&self) -> &RefsetMembers {
        &self.system().member_tables
    }

    /// The alternate identifiers of the edition.
    #[expect(
        clippy::same_name_method,
        reason = "the ECL evaluator's trait exposes the same graph part; this is the crate's own accessor"
    )]
    #[must_use]
    pub fn identifiers(&self) -> &Identifiers {
        &self.system().identifiers
    }

    /// The language reference sets of the edition, as SCTIDs.
    #[must_use]
    pub fn language_refsets(&self) -> Vec<&str> {
        self.system()
            .keys
            .refsets
            .iter()
            .map(|(_, s)| s.as_str())
            .collect()
    }

    pub(super) fn ordinal(concept: Concept) -> Ordinal {
        Ordinal::new(concept.index())
    }

    /// The preferred synonym of `concept` in `language`, by the first language
    /// reference set (in store order) whose preferred synonym is in that
    /// language.
    fn preferred_in(
        &self,
        ordinal: Ordinal,
        language: &str,
    ) -> Result<Option<String>, ProviderError> {
        let refsets = self.system().keys.refsets.iter().map(|(refset, _)| *refset);
        self.stored()
            .store
            .display(ordinal, refsets, |found| primary_subtag(found) == language)
            .map_err(storage)
    }

    /// The display for `language` (or the base language), by the SNOMED rule:
    /// the preferred term of the language reference set; then, our own fallback
    /// order (no spec governs it): an active synonym in the language, the
    /// preferred term in the base language, the FSN, any designation.
    pub(super) fn choose_display(
        &self,
        ordinal: Ordinal,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        let wanted = language.map_or_else(|| self.system().base_language.clone(), primary_subtag);
        if let Some(term) = self.preferred_in(ordinal, &wanted)? {
            return Ok(Some(term));
        }
        let designations = self.stored().store.designations(ordinal).map_err(storage)?;
        if let Some(synonym) = designations.iter().find(|d| {
            d.active
                && d.use_ordinal == self.system().keys.synonym
                && primary_subtag(&d.language) == wanted
        }) {
            return Ok(Some(synonym.term.clone()));
        }
        if wanted != self.system().base_language
            && let Some(term) = self.preferred_in(ordinal, &self.system().base_language)?
        {
            return Ok(Some(term));
        }
        if let Some(fsn) = designations
            .iter()
            .find(|d| d.active && d.use_ordinal == self.system().keys.fsn)
        {
            return Ok(Some(fsn.term.clone()));
        }
        Ok(designations.first().map(|d| d.term.clone()))
    }
}
