// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ValueSet.compose` as the seam hands it across.
//!
//! An include selects and an exclude takes away
//! (<https://hl7.org/fhir/R4B/valueset.html#compositions>); the compose layer
//! evaluates it above every provider.

use super::filter::Filter;

/// A system and optional version an include names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemRef {
    /// The system URI.
    pub url: String,
    /// The version, or the registry default.
    pub version: Option<String>,
}

/// One enumerated concept of an include.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptRef {
    /// The code.
    pub code: String,
    /// A display to use instead of the system's.
    pub display: Option<String>,
    /// Whether the value set marks the concept deprecated
    /// (`valueset-deprecated`).
    pub deprecated: bool,
}

/// `ValueSet.compose.include` (and `exclude`, which has the same shape).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Include {
    /// The system, when codes are selected from one.
    pub system: Option<SystemRef>,
    /// Enumerated concepts; empty means every code the filters admit.
    pub concepts: Vec<ConceptRef>,
    /// Filters, all of which apply.
    pub filters: Vec<Filter>,
    /// Value sets whose expansions the selection must be in, all of them.
    pub value_sets: Vec<String>,
}

/// `ValueSet.compose`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Compose {
    /// The includes; their union.
    pub include: Vec<Include>,
    /// The excludes; subtracted.
    pub exclude: Vec<Include>,
    /// `compose.inactive`: `Some(false)` keeps inactive codes out of every expansion.
    pub inactive: Option<bool>,
}

impl Compose {
    /// Where the first filter on `property` whose value is `value` sits,
    /// searching the includes before the excludes.
    #[must_use]
    pub fn filter_at(&self, property: &str, value: &str) -> Option<FilterAt> {
        let find = |side: Side, rules: &[Include]| {
            rules.iter().enumerate().find_map(|(rule, include)| {
                include
                    .filters
                    .iter()
                    .position(|filter| filter.property == property && filter.value == value)
                    .map(|filter| FilterAt { side, rule, filter })
            })
        };
        find(Side::Include, &self.include).or_else(|| find(Side::Exclude, &self.exclude))
    }
}

/// Which list of `ValueSet.compose` a rule sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// `compose.include`.
    Include,
    /// `compose.exclude`.
    Exclude,
}

/// Where a filter sits in a compose: `compose.include[rule].filter[filter]`,
/// or the same under `compose.exclude`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterAt {
    /// The list the rule is in.
    pub side: Side,
    /// The index of the rule in that list.
    pub rule: usize,
    /// The index of the filter in the rule.
    pub filter: usize,
}
