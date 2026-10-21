// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The version-neutral `ValueSet`: identity and `compose`
//! (<https://hl7.org/fhir/R4B/valueset.html>).

use crate::compose::Compose;

/// What the compose layer and the operations need of a `ValueSet`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueSetModel {
    /// `id`: the logical id the resource was authored with, when it carries
    /// one (<https://hl7.org/fhir/R4B/resource.html#id>). The server reads the
    /// resource at it.
    pub id: Option<String>,
    /// `url`.
    pub url: String,
    /// `version`.
    pub version: Option<String>,
    /// `language`: the language of the value set's displays, the default
    /// display language of its expansions and validations.
    pub language: Option<String>,
    /// The supplements the value set asks for (`valueset-supplement`
    /// extension, canonicals).
    pub supplements: Vec<String>,
    /// The expansion parameters the value set carries as defaults
    /// (`valueset-expansion-parameter` on the compose).
    pub expansion_parameters: Vec<ExpansionDefault>,
    /// The `structuredefinition-standards-status` extension's code, when set.
    pub standards_status: Option<String>,
    /// `name`.
    pub name: Option<String>,
    /// `title`.
    pub title: Option<String>,
    /// `status`.
    pub status: String,
    /// `experimental`.
    pub experimental: Option<bool>,
    /// `date`.
    pub date: Option<String>,
    /// `publisher`.
    pub publisher: Option<String>,
    /// `description`.
    pub description: Option<String>,
    /// `copyright`: the licence notice for the content the value set draws in.
    pub copyright: Option<String>,
    /// `immutable`.
    pub immutable: Option<bool>,
    /// `compose`, empty when the resource has none.
    pub compose: Compose,
    /// The `ValueSet` resources the resource carries in `contained`, by the
    /// `id` a `#id` canonical reference names them with
    /// (<https://hl7.org/fhir/R4B/references.html#canonical>).
    pub contained: std::collections::BTreeMap<String, ValueSetModel>,
}

/// One default expansion parameter of a value set: the `name` and `value`
/// of a `valueset-expansion-parameter` extension
/// (<https://hl7.org/fhir/R4B/extension-valueset-expansion-parameter.html>).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpansionDefault {
    /// The parameter name (`displayLanguage`, `activeOnly`, ...).
    pub name: String,
    /// The value, as its text.
    pub value: String,
}

impl crate::versioned::Versioned for ValueSetModel {
    fn url(&self) -> &str {
        &self.url
    }

    fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
}

/// A `ValueSet` the model cannot represent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    /// The resource has no `url`.
    #[error("the ValueSet has no url")]
    NoUrl,
    /// A filter names an operator the specification does not define.
    #[error("filter on `{property}` uses unknown operator `{op}`")]
    FilterOperator {
        /// `filter.property`.
        property: String,
        /// `filter.op`.
        op: String,
    },
    // NOTE: `ValueSet.compose.include.filter.value` is 1..1 through R5
    // (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.compose.include.filter.value>),
    // so a filter without one selects nothing the server can name.
    /// A filter states no value, so nothing says what it selects.
    #[error("The system {system} filter with property = {property}, op = {op} has no value")]
    FilterValue {
        /// `compose.include.system`, the system the filter selects from.
        system: String,
        /// `filter.property`.
        property: String,
        /// `filter.op`.
        op: String,
        /// The path of the filter, for `OperationOutcome.issue.expression`.
        expression: String,
    },
}
