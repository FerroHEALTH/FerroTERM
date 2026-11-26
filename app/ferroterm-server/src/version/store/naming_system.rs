// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `NamingSystem` search: `kind`, `value`, `type`, `id-type` and `status`
//! (<https://hl7.org/fhir/R4/namingsystem.html#search>), read with the search
//! value grammar of `fhir-search` (<https://hl7.org/fhir/R4/search.html>).

use fhir_search::escape::{split, unescape};
use fhir_search::string::StringMatch;
use fhir_search::token::{System, Token};
use fhir_types::codec::{Object, Value};
use http::StatusCode;

use crate::outcome::Failure;

/// The code system `NamingSystem.kind` is bound to (required).
const KIND_SYSTEM: &str = "http://hl7.org/fhir/namingsystem-type";

/// The code system `NamingSystem.status` is bound to (required).
const STATUS_SYSTEM: &str = "http://hl7.org/fhir/publication-status";

/// The code system `NamingSystem.uniqueId.type` is bound to (required).
const ID_TYPE_SYSTEM: &str = "http://hl7.org/fhir/namingsystem-identifier-type";

/// The parameters this search answers, for the refusal of any other.
const ANSWERED: &str = "`kind`, `value`, `type`, `id-type`, `status`, `_elements` or `_summary`";

/// One parameter of a search: the alternatives of its `OR`, of which one must
/// match (<https://hl7.org/fhir/R4/search.html#combining>).
#[derive(Debug)]
enum Test {
    /// `kind`: `NamingSystem.kind`.
    Kind(Vec<Token>),
    /// `status`: `NamingSystem.status`.
    Status(Vec<Token>),
    /// `id-type`: any `NamingSystem.uniqueId.type`.
    IdType(Vec<Token>),
    /// `type`: a coding of `NamingSystem.type`.
    Type(Vec<Token>),
    /// `value`: any `NamingSystem.uniqueId.value`, under the string match the
    /// modifier names.
    Value(StringMatch, Vec<String>),
}

/// What a `NamingSystem` search asks: every named parameter must match.
#[derive(Debug, Default)]
pub(crate) struct Criteria {
    /// The parameters, one per occurrence in the query.
    tests: Vec<Test>,
}

impl Criteria {
    /// Reads the search parameters of `query`.
    ///
    /// `_elements` and `_summary` name what to return rather than what to
    /// match, so they pass through here and are applied to the answer
    /// (<https://hl7.org/fhir/R4B/search.html#summary>).
    ///
    /// # Errors
    ///
    /// A parameter or modifier this server does not answer is a 400
    /// `not-supported`, and a value the search grammar refuses is a 400
    /// `value`.
    pub(crate) fn parse(query: &[(String, String)]) -> Result<Self, Failure> {
        let mut tests = Vec::new();
        for (name, value) in query {
            let (parameter, modifier) = match name.split_once(':') {
                Some((parameter, modifier)) => (parameter, Some(modifier)),
                None => (name.as_str(), None),
            };
            let test = match parameter {
                "_format" | crate::elements::PARAMETER | crate::elements::SUMMARY => continue,
                "value" => {
                    let matching = StringMatch::from_modifier(modifier)
                        .ok_or_else(|| unsupported_modifier(name))?;
                    Test::Value(matching, strings(name, value)?)
                }
                "kind" | "status" | "id-type" | "type" => {
                    if modifier.is_some() {
                        return Err(unsupported_modifier(name));
                    }
                    let tokens = tokens(name, value)?;
                    match parameter {
                        "kind" => Test::Kind(tokens),
                        "status" => Test::Status(tokens),
                        "id-type" => Test::IdType(tokens),
                        _ => Test::Type(tokens),
                    }
                }
                other => {
                    return Err(Failure::new(
                        StatusCode::BAD_REQUEST,
                        "not-supported",
                        format!("search parameter `{other}` is not supported; use {ANSWERED}"),
                    ));
                }
            };
            tests.push(test);
        }
        Ok(Self { tests })
    }

    /// Whether the `NamingSystem` `object` matches every parameter.
    pub(crate) fn matches(&self, object: &Object) -> bool {
        self.tests.iter().all(|test| match test {
            Test::Kind(tokens) => code_matches(tokens, KIND_SYSTEM, text(object, "kind")),
            Test::Status(tokens) => code_matches(tokens, STATUS_SYSTEM, text(object, "status")),
            Test::IdType(tokens) => unique_ids(object)
                .any(|unique| code_matches(tokens, ID_TYPE_SYSTEM, text(unique, "type"))),
            Test::Type(tokens) => object
                .get("type")
                .and_then(Value::as_object)
                .is_some_and(|concept| concept_matches(tokens, concept)),
            Test::Value(matching, values) => unique_ids(object).any(|unique| {
                text(unique, "value")
                    .is_some_and(|field| values.iter().any(|value| matching.matches(field, value)))
            }),
        })
    }
}

/// The string field `name` of `object`.
fn text<'a>(object: &'a Object, name: &str) -> Option<&'a str> {
    object.get(name).and_then(Value::as_str)
}

/// The `uniqueId` entries of a `NamingSystem`.
fn unique_ids(object: &Object) -> impl Iterator<Item = &Object> {
    object
        .get("uniqueId")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
}

/// Whether a `code` element bound to `system` matches one of `tokens`.
///
/// A `code` carries the system of its required binding implicitly, so
/// `[system]|[code]` matches only under that system and `|[code]` never does
/// (<https://hl7.org/fhir/R4/search.html#token>).
fn code_matches(tokens: &[Token], system: &str, field: Option<&str>) -> bool {
    let Some(field) = field else {
        return false;
    };
    tokens.iter().any(|token| {
        let in_system = match token.system() {
            System::Any => true,
            System::Absent => false,
            System::Is(asked) => asked == system,
        };
        in_system && token.code().is_none_or(|code| code == field)
    })
}

/// Whether a coding of the `CodeableConcept` `concept` matches one of
/// `tokens` (<https://hl7.org/fhir/R4/search.html#token>).
fn concept_matches(tokens: &[Token], concept: &Object) -> bool {
    let mut codings = concept
        .get("coding")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object);
    codings.any(|coding| {
        let system = text(coding, "system");
        let code = text(coding, "code");
        tokens.iter().any(|token| {
            let in_system = match token.system() {
                System::Any => true,
                System::Absent => system.is_none(),
                System::Is(asked) => system == Some(asked),
            };
            in_system && token.code().is_none_or(|asked| code == Some(asked))
        })
    })
}

/// The token alternatives of the parameter `name`.
fn tokens(name: &str, value: &str) -> Result<Vec<Token>, Failure> {
    split(value, ',')
        .iter()
        .map(|part| Token::parse(part).map_err(|error| refused(name, &error)))
        .collect()
}

/// The string alternatives of the parameter `name`, unescaped.
fn strings(name: &str, value: &str) -> Result<Vec<String>, Failure> {
    split(value, ',')
        .iter()
        .map(|part| unescape(part).map_err(|error| refused(name, &error)))
        .collect()
}

/// A value of the parameter `name` the search grammar refuses; the value
/// itself is left out, as it may be an identifier.
fn refused(name: &str, error: &dyn std::fmt::Display) -> Failure {
    Failure::new(
        StatusCode::BAD_REQUEST,
        "value",
        format!("the value of `{name}` is not a search value: {error}"),
    )
}

/// A modifier this search does not answer on the parameter `name`.
fn unsupported_modifier(name: &str) -> Failure {
    Failure::new(
        StatusCode::BAD_REQUEST,
        "not-supported",
        format!("the modifier of `{name}` is not supported"),
    )
}
