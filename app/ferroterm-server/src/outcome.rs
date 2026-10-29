// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `OperationOutcome` responses
//! (<https://hl7.org/fhir/R4B/operationoutcome.html>).
//!
//! R4, R4B, and R5 declare the same `issue` elements a failure fills. R6 drops
//! `issue.location` and adds `success` to the severities; a failure never sets
//! `location` and is always an `error`, so one render serves every version.
//!
//! Every failure a client can cause answers with an `OperationOutcome` whose
//! issue carries `severity`, `code` from the issue-type value set, and
//! `diagnostics`; the status is the one the operation layer chose.

use std::sync::Arc;

use axum::body::Body;
use axum::response::{IntoResponse, Response};
use ferrohealth_server::panic::PanicBody;
use ferrohealth_server::refusal::RefusalBody;
use ferrohealth_server::request_id::RequestIds;
use fhir_terminology::compose::Side;
use fhir_terminology::operations::value_set_validate_code::TX_ISSUE_TYPE;
use fhir_terminology::operations::{OperationError, Refused, RefusedInput};
use fhir_terminology::position::{column, encoded_offset};
use fhir_types::r4b::codeable_concept::CodeableConcept;
use fhir_types::r4b::coding::Coding;
use fhir_types::r4b::extension::{Extension, ExtensionValue};
use fhir_types::r4b::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use http::StatusCode;
use http::header::CONTENT_TYPE;

/// The media type of every FHIR response.
pub const FHIR_JSON: &str = crate::wire::FHIR_JSON;

/// A failure to answer, as the wire sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The HTTP status.
    pub status: StatusCode,
    /// The `issue.code`.
    pub code: &'static str,
    /// The `issue.diagnostics` and `issue.details.text`.
    pub diagnostics: String,
    /// The `tx-issue-type` code in `issue.details.coding`, when one applies.
    pub kind: Option<&'static str>,
    /// The path of the element at fault, for `issue.expression`.
    pub expression: Option<String>,
    /// The 1-based column in the value `expression` names where the fault is,
    /// for `operationoutcome-issue-col` (on line 1).
    pub column: Option<usize>,
}

/// The `operationoutcome-issue-line` extension
/// (<https://hl7.org/fhir/extensions/StructureDefinition-operationoutcome-issue-line.html>).
pub const ISSUE_LINE_URL: &str =
    "http://hl7.org/fhir/StructureDefinition/operationoutcome-issue-line";

/// The `operationoutcome-issue-col` extension
/// (<https://hl7.org/fhir/extensions/StructureDefinition-operationoutcome-issue-col.html>).
pub const ISSUE_COL_URL: &str =
    "http://hl7.org/fhir/StructureDefinition/operationoutcome-issue-col";

/// Where a request wrote its inputs, so a refusal can point into them.
///
/// No specification governs this: our own design.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Origin {
    /// The query string, exactly as the client sent it.
    Query(String),
    /// A `Parameters` body: the name of each parameter, in the order the body
    /// carried them.
    Body(Vec<Option<String>>),
    /// Inputs no expression can name: an instance-level invocation, whose
    /// value set is the instance, or an entry of a batch.
    #[default]
    Unplaced,
}

impl Origin {
    /// The `Parameters` body of `object`, by the names of its parameters.
    #[must_use]
    pub fn of_body(object: &fhir_types::codec::Object) -> Self {
        let names = match object.get("parameter") {
            Some(fhir_types::codec::Value::Array(sent)) => sent
                .iter()
                .map(|parameter| {
                    parameter
                        .get("name")
                        .and_then(fhir_types::codec::Value::as_str)
                        .map(str::to_owned)
                })
                .collect(),
            _ => Vec::new(),
        };
        Self::Body(names)
    }

    /// The `issue.expression` naming where the request wrote `refused`, and
    /// the column of its position there.
    ///
    /// The paths are the restricted `FHIRPath` an `issue.expression` is written
    /// in, and `http.url` is its form for a query parameter
    /// (<https://hl7.org/fhir/R4B/operationoutcome.html#expression>,
    /// <https://hl7.org/fhir/R4B/fhirpath.html#simple>).
    fn point(&self, refused: &Refused) -> Option<(String, Option<usize>)> {
        let at = |text: &str, offset: Option<usize>| offset.and_then(|offset| column(text, offset));
        match (&refused.input, self) {
            (RefusedInput::Filter(filter), Self::Body(names)) => {
                let index = Self::index(names, "valueSet")?;
                let side = match filter.side {
                    Side::Include => "include",
                    Side::Exclude => "exclude",
                };
                Some((
                    format!(
                        "Parameters.parameter[{index}].resource.compose.{side}[{}].filter[{}].value",
                        filter.rule, filter.filter
                    ),
                    at(&refused.text, refused.position),
                ))
            }
            (RefusedInput::Url, Self::Body(names)) => Some((
                format!(
                    "Parameters.parameter[{}].valueUri",
                    Self::index(names, "url")?
                ),
                at(&refused.text, refused.position),
            )),
            (RefusedInput::Url, Self::Query(query)) => {
                let sent = query_value(query, "url")?;
                let offset = refused
                    .position
                    .and_then(|offset| encoded_offset(sent, offset));
                Some((String::from("http.url"), at(sent, offset)))
            }
            (RefusedInput::Filter(_), Self::Query(_)) | (_, Self::Unplaced) => None,
        }
    }

    /// The index of the first parameter called `name`.
    fn index(names: &[Option<String>], name: &str) -> Option<usize> {
        names.iter().position(|sent| sent.as_deref() == Some(name))
    }
}

/// The value of the first query parameter called `name`, still encoded as the
/// client sent it.
fn query_value<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let decoded = form_decoded(key)?;
        (decoded == name).then_some(value)
    })
}

/// A query parameter name decoded as `application/x-www-form-urlencoded`;
/// `None` when it is not UTF-8.
fn form_decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0_usize;
    while let Some(&byte) = bytes.get(index) {
        let hex = index
            .checked_add(1)
            .zip(index.checked_add(3))
            .and_then(|(start, end)| bytes.get(start..end))
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (byte, hex) {
            (b'%', Some(decoded)) => {
                out.push(decoded);
                index = index.checked_add(3)?;
            }
            (b'+', _) => {
                out.push(b' ');
                index = index.checked_add(1)?;
            }
            _ => {
                out.push(byte);
                index = index.checked_add(1)?;
            }
        }
    }
    String::from_utf8(out).ok()
}

impl Failure {
    /// A failure with `status`, `code`, and `diagnostics`.
    #[must_use]
    pub fn new(status: StatusCode, code: &'static str, diagnostics: impl Into<String>) -> Self {
        Self {
            status,
            code,
            diagnostics: diagnostics.into(),
            kind: None,
            expression: None,
            column: None,
        }
    }

    /// The failure of `error`, pointing at the value it refused where
    /// `origin` shows the request wrote it.
    #[must_use]
    pub fn placed(error: OperationError, origin: &Origin) -> Self {
        let pointed = error.refused().and_then(|refused| origin.point(refused));
        let mut failure = Self::from(error);
        if let Some((expression, column)) = pointed {
            failure.expression = Some(expression);
            failure.column = column;
        }
        failure
    }

    /// This failure with a `tx-issue-type` coding.
    #[must_use]
    pub fn kind(mut self, kind: &'static str) -> Self {
        self.kind = Some(kind);
        self
    }

    /// This failure pointing at the element at `expression`.
    #[must_use]
    pub fn at(mut self, expression: impl Into<String>) -> Self {
        self.expression = Some(expression.into());
        self
    }

    /// The `OperationOutcome` resource.
    ///
    /// The issue carries `details.text` and, when known, a `tx-issue-type`
    /// coding, the classification validators read
    /// (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
    #[must_use]
    pub fn outcome(&self) -> OperationOutcome {
        OperationOutcome {
            // NOTE: the ecosystem's message ids live on the itemised `issues` of a
            // validation; the outcome of a refused request carries none (its test
            // cases expect a bare issue, <https://hl7.org/fhir/uv/tx-ecosystem/>).
            issue: vec![OperationOutcomeIssue {
                severity: "error".into(),
                code: self.code.into(),
                details: Some(CodeableConcept {
                    coding: self.kind.map(tx_issue_coding).into_iter().collect(),
                    text: Some(self.diagnostics.as_str().into()),
                    ..Default::default()
                }),
                diagnostics: Some(self.diagnostics.as_str().into()),
                expression: self
                    .expression
                    .iter()
                    .map(|expression| expression.as_str().into())
                    .collect(),
                extension: self.column.map(position).unwrap_or_default(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }
}

/// The line and column of a fault inside the value `issue.expression` names.
// NOTE: <https://hl7.org/fhir/extensions/StructureDefinition-operationoutcome-issue-col.html> and <https://hl7.org/fhir/extensions/StructureDefinition-operationoutcome-issue-line.html>
// fix no base or unit, so this is our own design: an expression constraint is line 1, and the column is 1-based in
// Unicode scalar values into the value `expression` names (for `http.url`, the query value as the client sent it).
fn position(column: usize) -> Vec<Extension> {
    vec![
        Extension {
            url: ISSUE_LINE_URL.to_owned(),
            value: Some(ExtensionValue::Integer(1.into())),
            ..Default::default()
        },
        Extension {
            url: ISSUE_COL_URL.to_owned(),
            value: Some(ExtensionValue::String(column.to_string().as_str().into())),
            ..Default::default()
        },
    ]
}

fn tx_issue_coding(kind: &str) -> Coding {
    Coding {
        system: Some(TX_ISSUE_TYPE.into()),
        code: Some(kind.into()),
        ..Default::default()
    }
}

impl From<OperationError> for Failure {
    fn from(error: OperationError) -> Self {
        Self::new(error.status(), error.issue_code(), error.to_string()).kind(error.tx_issue_type())
    }
}

impl Failure {
    /// The response in `wire`: the `OperationOutcome` as FHIR JSON or XML (the
    /// R4B shape, which every served version declares alike).
    #[must_use]
    pub fn respond(&self, wire: crate::wire::Wire) -> Response {
        // NOTE: encoding a hand-built OperationOutcome cannot fail; if the
        // codec ever refuses, a bare status still tells the client the truth.
        wire.resource(
            self.status,
            &self.outcome(),
            &fhir_types::r4b::schema::SCHEMAS,
        )
        .unwrap_or_else(|_| self.status.into_response())
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        self.respond(crate::wire::Wire::Json)
    }
}

/// A FHIR JSON response body with `status`.
pub fn fhir_json(status: StatusCode, value: &fhir_types::codec::Value) -> Response {
    let Ok(body) = serde_json::to_vec(value) else {
        return status.into_response();
    };
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, FHIR_JSON)
        .body(Body::from(body))
        .unwrap_or_else(|_| status.into_response())
}

/// The fallback for a path the server does not define, in the format `Accept`
/// asks for.
#[must_use]
pub fn not_found(headers: &http::HeaderMap) -> Response {
    let wire = crate::wire::Wire::negotiate(&[], headers).unwrap_or_default();
    Failure::new(
        StatusCode::NOT_FOUND,
        "not-found",
        "no such resource or operation on this server",
    )
    .respond(wire)
}

/// The body of a request whose handler panicked.
///
/// It is an `OperationOutcome` with the `exception` issue type naming the
/// server's request id, which the panic line names, and never the panic
/// message (<https://hl7.org/fhir/R4B/valueset-issue-type.html>).
#[must_use]
pub fn panic_body() -> PanicBody {
    Arc::new(|_uri, ids: RequestIds<'_>| {
        Failure::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "exception",
            format!(
                "the server could not answer this request (request id {})",
                ids.server
            ),
        )
        .into_response()
    })
}

/// The body of a refusal the stack of `ferrohealth-server` writes itself.
///
/// A `408`, `413`, `429` or `503` is an `OperationOutcome` carrying the
/// reason, so every error of the FHIR listener has one shape (FHIR R4B
/// `http.html`).
#[must_use]
pub fn refusal_body() -> RefusalBody {
    Arc::new(|_uri, _ids, status, reason| {
        Failure::new(status, refusal_code(status), reason).into_response()
    })
}

/// The issue type of a refusal of `status`
/// (<https://hl7.org/fhir/R4B/valueset-issue-type.html>).
fn refusal_code(status: StatusCode) -> &'static str {
    match status {
        StatusCode::REQUEST_TIMEOUT => "timeout",
        StatusCode::PAYLOAD_TOO_LARGE => "too-long",
        StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE => "throttled",
        _ => "exception",
    }
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use super::{query_value, refusal_code};

    #[test]
    fn each_refusal_of_the_stack_has_its_issue_type() {
        assert_eq!(refusal_code(StatusCode::REQUEST_TIMEOUT), "timeout");
        assert_eq!(refusal_code(StatusCode::PAYLOAD_TOO_LARGE), "too-long");
        assert_eq!(refusal_code(StatusCode::TOO_MANY_REQUESTS), "throttled");
        assert_eq!(refusal_code(StatusCode::SERVICE_UNAVAILABLE), "throttled");
        assert_eq!(refusal_code(StatusCode::BAD_GATEWAY), "exception");
    }

    #[test]
    fn a_query_value_is_found_by_its_decoded_name_and_kept_as_sent() {
        let query = "count=10&u%72l=a%2520b+c&url=second";
        assert_eq!(
            query_value(query, "url"),
            Some("a%2520b+c"),
            "the first `url`, its name decoded and its value untouched"
        );
        assert_eq!(query_value(query, "count"), Some("10"));
        assert_eq!(
            query_value("flag&url=x", "flag"),
            Some(""),
            "a bare name has an empty value"
        );
        assert_eq!(query_value(query, "valueSet"), None);
    }
}
