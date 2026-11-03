// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A value set's expansion as `$expand` answers it, before it renders as a
//! `ValueSet.expansion` of the requested FHIR version
//! (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>).

use std::sync::Arc;

use super::model::ValueSetModel;
use crate::provider::{Designation, Property};

/// One echoed `expansion.parameter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpansionParameter {
    /// The parameter name.
    pub name: String,
    /// The value.
    pub value: ParameterValue,
}

/// The value of an echoed expansion parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParameterValue {
    /// A `string`.
    String(String),
    /// A `boolean`.
    Boolean(bool),
    /// An `integer`.
    Integer(i64),
    /// A `code`.
    Code(String),
    /// A `uri`.
    Uri(String),
}

/// One concept of the expansion page.
#[expect(
    clippy::struct_field_names,
    reason = "`contains.contains` is the FHIR element's own name"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contains {
    /// The code system URI.
    pub system: String,
    /// The code system version served.
    pub version: String,
    /// The code.
    pub code: String,
    /// The display in the requested language.
    pub display: Option<String>,
    /// Whether the concept is abstract (not selectable).
    pub abstract_concept: bool,
    /// Whether the concept is inactive.
    pub inactive: bool,
    /// The designations asked for.
    pub designations: Vec<Designation>,
    /// The properties asked for with `property`
    /// (<https://hl7.org/fhir/R5/valueset-operation-expand.html>).
    pub properties: Vec<Property>,
    /// The entry's children, when the expansion carries the system's hierarchy
    /// (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.expansion.contains>).
    pub contains: Vec<Contains>,
}

/// One property the expansion returns on its concepts, for
/// `expansion.property` (<https://hl7.org/fhir/R5/valueset.html>).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpansionProperty {
    /// The property code, as `contains.property.code` names it.
    pub code: String,
    /// The formal URI of the property, when the code system declares one.
    pub uri: Option<String>,
}

/// The outcome of `$expand`.
#[derive(Debug, Clone)]
pub struct ExpansionOutcome {
    /// The value set expanded (its definition, for the resource).
    pub model: Arc<ValueSetModel>,
    /// Whether the compose is to be returned (`includeDefinition`).
    pub include_definition: bool,
    /// The expansion identifier (`urn:uuid:…`).
    pub identifier: String,
    /// The timestamp of the expansion.
    pub timestamp: String,
    /// The size of the whole expansion.
    pub total: u64,
    /// The offset, when the client paged.
    pub offset: Option<u64>,
    /// The parameters echoed, in order.
    pub parameters: Vec<ExpansionParameter>,
    /// The page.
    pub contains: Vec<Contains>,
    /// The properties the concepts carry, each once, in order of appearance.
    pub properties: Vec<ExpansionProperty>,
    /// Whether the value set admits codes the expansion does not list, which
    /// the resource states with `valueset-unclosed`
    /// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>, Notes).
    pub unclosed: bool,
    /// Why the expansion is unclosed, one reason per cause, for
    /// `valueset-unclosed-reason`.
    pub unclosed_reasons: Vec<String>,
}
