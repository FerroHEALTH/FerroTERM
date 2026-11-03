// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ValueSet` the composer opens: what it reads of the resource the
//! server holds, decoded at the release that served it, and the draft it
//! opens on.

use serde_json::Value;

use super::{Draft, NEW_STATUS};

/// The `ValueSet` the composer reads back off the server, read from the
/// `ValueSet` its release decoded: the elements the composer authors and the
/// `meta.versionId` an update states.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct StoredValueSet {
    /// `ValueSet.id`.
    id: Option<String>,
    /// `meta.versionId`.
    version_id: Option<String>,
    /// `ValueSet.url`.
    url: Option<String>,
    /// `ValueSet.version`.
    version: Option<String>,
    /// `ValueSet.name`.
    name: Option<String>,
    /// `ValueSet.title`.
    title: Option<String>,
    /// `ValueSet.status`.
    status: Option<String>,
    /// `compose.inactive`.
    inactive: Option<bool>,
    /// The clauses of `compose`, includes before excludes.
    clauses: Vec<StoredClause>,
}

/// One clause of `compose`, as the composer opens it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct StoredClause {
    /// Whether it includes or excludes.
    included: bool,
    /// `system`.
    system: Option<String>,
    /// `version`.
    version: Option<String>,
    /// `valueSet`.
    value_sets: Vec<String>,
    /// `concept`, each its code and its display.
    concepts: Vec<(String, Option<String>)>,
    /// `filter`, each its property, operator and value.
    filters: Vec<(String, String, String)>,
}

impl StoredValueSet {
    /// Whether this server holds a writable record of the resource.
    ///
    /// The server stamps `meta.versionId` on every resource written through
    /// its own API and raises it on each update, and a resource served out of
    /// the loaded content carries none, so a read that answers no version is
    /// content with no write path behind it and the screen opens read-only
    /// (<https://hl7.org/fhir/R4B/http.html#concurrency>).
    pub(crate) fn writable(&self) -> bool {
        self.version_id
            .as_deref()
            .is_some_and(|version| !version.is_empty())
    }

    /// This resource as a draft to edit.
    ///
    /// `resource` is the document the server sent, which the draft carries so
    /// a save writes over it rather than replacing it with the elements the
    /// form draws (<https://hl7.org/fhir/R4B/http.html#update>).
    pub(crate) fn draft(&self, resource: &Value) -> Draft {
        let mut draft = Draft {
            id: self.id.clone().unwrap_or_default(),
            version_id: self.version_id.clone().unwrap_or_default(),
            url: self.url.clone().unwrap_or_default(),
            name: self.name.clone().unwrap_or_default(),
            title: self.title.clone().unwrap_or_default(),
            version: self.version.clone().unwrap_or_default(),
            status: self
                .status
                .clone()
                .unwrap_or_else(|| String::from(NEW_STATUS)),
            inactive: self.inactive,
            clauses: Vec::new(),
            next_key: 0,
            base: resource.clone(),
        };
        for set in &self.clauses {
            let key = draft.add_clause(set.included);
            for canonical in &set.value_sets {
                draft.add_value_set(key, canonical);
            }
            for (code, display) in &set.concepts {
                draft.add_concept(key, code, display.as_deref().unwrap_or_default());
                if let Some(display) = display.clone()
                    && let Some(row) = draft
                        .clause_mut(key)
                        .and_then(|clause| clause.concepts.last_mut())
                {
                    row.display = display;
                }
            }
            for (property, op, value) in &set.filters {
                draft.add_filter(key, property, op);
                if let Some(row) = draft
                    .clause_mut(key)
                    .and_then(|clause| clause.filters.last_mut())
                {
                    row.value.clone_from(value);
                }
            }
            if let Some(clause) = draft.clause_mut(key) {
                clause.system = set.system.clone().unwrap_or_default();
                clause.system_version = set.version.clone().unwrap_or_default();
            }
        }
        if draft.clauses.is_empty() {
            draft.add_clause(true);
        }
        draft
    }
}

/// Reads the `ValueSet` of the `fhir-types` module `$module` as the composer
/// opens it; `$value` reads a filter's `value`, which R4 and R4B require and
/// R5 and R6 make optional.
macro_rules! reads_stored_value_set {
    ($module:ident, $value:expr) => {
        impl crate::fhir::release::Reads<StoredValueSet>
            for fhir_types::$module::value_set::ValueSet
        {
            fn read(&self) -> StoredValueSet {
                use fhir_types::$module::value_set::{
                    ValueSetComposeInclude, ValueSetComposeIncludeFilter,
                };
                let filter_value: fn(&ValueSetComposeIncludeFilter) -> Option<String> = $value;
                let text = |value: Option<&fhir_types::$module::primitives::String>| {
                    value.and_then(|value| value.value.clone())
                };
                let clause = |set: &ValueSetComposeInclude, included: bool| StoredClause {
                    included,
                    system: set.system.as_ref().and_then(|system| system.value.clone()),
                    version: text(set.version.as_ref()),
                    value_sets: set
                        .value_set
                        .iter()
                        .filter_map(|url| url.value.clone())
                        .collect(),
                    concepts: set
                        .concept
                        .iter()
                        .map(|concept| {
                            (
                                concept.code.value.clone().unwrap_or_default(),
                                text(concept.display.as_ref()),
                            )
                        })
                        .collect(),
                    filters: set
                        .filter
                        .iter()
                        .map(|filter| {
                            (
                                filter.property.value.clone().unwrap_or_default(),
                                filter.op.value.clone().unwrap_or_default(),
                                filter_value(filter).unwrap_or_default(),
                            )
                        })
                        .collect(),
                };
                let compose = self.compose.as_ref();
                StoredValueSet {
                    id: self.id.clone(),
                    version_id: self
                        .meta
                        .as_ref()
                        .and_then(|meta| meta.version_id.as_ref())
                        .and_then(|id| id.value.clone()),
                    url: self.url.as_ref().and_then(|url| url.value.clone()),
                    version: text(self.version.as_ref()),
                    name: text(self.name.as_ref()),
                    title: text(self.title.as_ref()),
                    status: self.status.value.clone(),
                    inactive: compose
                        .and_then(|compose| compose.inactive.as_ref())
                        .and_then(|flag| flag.value),
                    clauses: compose
                        .map(|compose| {
                            compose
                                .include
                                .iter()
                                .map(|set| clause(set, true))
                                .chain(compose.exclude.iter().map(|set| clause(set, false)))
                                .collect()
                        })
                        .unwrap_or_default(),
                }
            }
        }
    };
}

reads_stored_value_set!(r4, |filter| filter.value.value.clone());
reads_stored_value_set!(r4b, |filter| filter.value.value.clone());
reads_stored_value_set!(r5, |filter| filter.value.value.clone());
reads_stored_value_set!(r6, |filter| filter
    .value
    .as_ref()
    .and_then(|value| value.value.clone()));
