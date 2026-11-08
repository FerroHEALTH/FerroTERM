// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology operations, written and read by `ferrobridge-term`.
//!
//! `ferrobridge_term::invocation` writes each request under the names the
//! serving release's own `OperationDefinition` gives its inputs, in the
//! order it declares them, and reads each answer through that release's
//! `out` contract. The console sends what it writes as one [`Exchange`]
//! through its own server, which adds the signed-in reader's token, so its
//! requests and readings are the crate's. A refusal is read as every other
//! refusal the console shows, from the `OperationOutcome` the server sent.
//! No specification governs this split: our own design.

use ferrobridge_term::concept::{Concept as Term, PropertyValue};
use ferrobridge_term::config::{Form, WireVersion};
use ferrobridge_term::invocation::{self, Invocation};
use ferrobridge_term::outcome::BatchOutcome;
use ferrobridge_term::request::Request;
use fhir_types::codec::Value;

use super::answer::{failure, pretty, status_of, transport};
use super::{Document, FHIR_JSON, FhirClient};
use crate::fhir::error::FhirError;
use crate::fhir::exchange::{Answer, Exchange, Method};
use crate::fhir::validation::ValidateRequest;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr};
use crate::url::RequestUrl;

/// One operation, written for the release that serves it.
#[derive(Clone, Debug)]
pub(crate) struct Operation {
    /// What is asked.
    request: Request,
    /// The request as the release writes it.
    invocation: Invocation,
    /// The address a reader would type, below the server's public root.
    url: String,
}

impl Operation {
    /// The address a reader would type to repeat the request.
    pub(crate) fn url(&self) -> &str {
        &self.url
    }
}

impl FhirVersion {
    /// The release `ferrobridge-term` writes and reads this version in.
    pub(crate) const fn wire(self) -> WireVersion {
        match self {
            Self::R4 => WireVersion::R4,
            Self::R4B => WireVersion::R4B,
            Self::R5 => WireVersion::R5,
            Self::R6 => WireVersion::R6,
        }
    }
}

impl FhirClient {
    /// `request`, written in the `GET` form for `version`; an input that is a
    /// resource, such as an inline value set, makes it a `POST`
    /// (<https://hl7.org/fhir/R4B/operations.html#request>).
    ///
    /// # Errors
    ///
    /// Returns [`FhirError::Unwritten`] when the release cannot carry the
    /// request or one of its inputs.
    pub(crate) fn operation(
        &self,
        version: FhirVersion,
        request: Request,
    ) -> Result<Operation, FhirError> {
        let mut address = RequestUrl::default().segment(version.segment());
        for segment in request.segments() {
            address = address.segment(segment);
        }
        let invocation = Invocation::new(version.wire(), Form::Get, &request).map_err(|error| {
            FhirError::Unwritten {
                url: address.render(&self.root),
                message: error.to_string(),
            }
        })?;
        for (name, value) in invocation.query() {
            address = address.query(name, value);
        }
        Ok(Operation {
            request,
            invocation,
            url: address.render(&self.root),
        })
    }

    /// The `$validate-code` run `request` asks for, written for `version`.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError::Unwritten`] for a value set run that names no
    /// system for its code, since "if a code is provided, a system or a
    /// context must be provided"
    /// (<https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>),
    /// and when the release cannot carry the run.
    pub(crate) fn validation(
        &self,
        version: FhirVersion,
        request: &ValidateRequest,
    ) -> Result<Operation, FhirError> {
        let asked = request.request().ok_or_else(|| {
            let mut address = RequestUrl::default()
                .segment(version.segment())
                .segment(request.on.resource_type());
            if request.instance() {
                address = address.segment(&request.id);
            }
            FhirError::Unwritten {
                url: address.segment("$validate-code").render(&self.root),
                message: tr(&self.locale, Msg::CheckNeedsSystem),
            }
        })?;
        self.operation(version, asked)
    }

    /// Sends `operation` and reads its answer at `version`, keeping the
    /// document as the server sent it.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong: a
    /// refusal as [`FhirError::Refused`] carrying the server's own
    /// `OperationOutcome`, and an answer the release's contract does not read
    /// as [`FhirError::Decode`].
    pub(crate) async fn operate(
        &self,
        version: FhirVersion,
        operation: &Operation,
    ) -> Result<Document<BatchOutcome>, FhirError> {
        let url = operation.url.as_str();
        let method = if operation.invocation.method() == http::Method::GET {
            Method::Get
        } else {
            Method::Post
        };
        let mut exchange = Exchange::new(method, self.path_of(url)?, FHIR_JSON);
        if let Some(body) = operation.invocation.body() {
            exchange = exchange.with_body(FHIR_JSON, body.to_owned());
        }
        let answer = self
            .exchange(exchange)
            .await
            .map_err(|error| transport(url, &error))?;
        self.answered(version, &operation.request, url, &answer)
    }

    /// What `request`, sent to `url` at `version`, answered: the refusal the
    /// reader is shown, or the answer the release's contract reads.
    pub(super) fn answered(
        &self,
        version: FhirVersion,
        request: &Request,
        url: &str,
        answer: &Answer,
    ) -> Result<Document<BatchOutcome>, FhirError> {
        let status = status_of(&self.locale, answer, url)?;
        if !status.is_success() {
            return Err(failure(version, answer, status, url));
        }
        let address = url::Url::parse(url).map_err(|error| FhirError::Transport {
            url: url.to_owned(),
            message: error.to_string(),
        })?;
        let value = invocation::answer(version.wire(), request, &address, status, &answer.body)
            .map_err(|error| unread(url, version, &error))?;
        Ok(Document {
            value,
            text: pretty(&answer.body),
        })
    }
}

/// The failure of an answer the release's contract does not read, with every
/// cause the reader gave down to the element path.
fn unread(url: &str, version: FhirVersion, error: &ferrobridge_term::error::Error) -> FhirError {
    let mut message = format!("FHIR {version}: {error}");
    let mut cause = std::error::Error::source(error);
    while let Some(reason) = cause {
        message.push_str(": ");
        message.push_str(&reason.to_string());
        cause = reason.source();
    }
    FhirError::Decode {
        url: url.to_owned(),
        message,
    }
}

/// The failure of an answer read as another operation's, which a release's
/// reader never makes of a request.
pub(crate) fn mismatched(url: &str, version: FhirVersion) -> FhirError {
    FhirError::Decode {
        url: url.to_owned(),
        message: format!("FHIR {version}: the answer is not the operation's"),
    }
}

/// A `Coding` as the screens draw one.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Coding {
    /// `Coding.system`.
    pub(crate) system: Option<String>,
    /// `Coding.version`.
    pub(crate) version: Option<String>,
    /// `Coding.code`.
    pub(crate) code: Option<String>,
    /// `Coding.display`.
    pub(crate) display: Option<String>,
}

/// The `CodeableConcept` an answer echoes, as the screen draws it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ConceptEcho {
    /// Each coding, with the elements the answer carried.
    pub(crate) codings: Vec<Coding>,
    /// The wording the client sent with the concept.
    pub(crate) text: Option<String>,
}

impl Coding {
    /// The coding as one line: the code, its system, and its display.
    pub(crate) fn rendered(&self) -> String {
        let code = match (self.system.as_deref(), self.code.as_deref()) {
            (Some(system), Some(code)) => format!("{system}|{code}"),
            (None, Some(code)) => code.to_owned(),
            (Some(system), None) => system.to_owned(),
            (None, None) => String::new(),
        };
        match self.display.as_deref() {
            Some(display) if code.is_empty() => display.to_owned(),
            Some(display) => format!("{code} ({display})"),
            None => code,
        }
    }
}

impl From<Term> for Coding {
    fn from(term: Term) -> Self {
        Self {
            system: term.system,
            version: term.version,
            code: term.code,
            display: term.display,
        }
    }
}

/// The member of a `Coding` value's JSON.
fn member(value: &Value, name: &str) -> Option<String> {
    value.get(name).and_then(Value::as_str).map(str::to_owned)
}

/// The coding a `valueCoding` carries.
pub(crate) fn coding_of(value: &PropertyValue) -> Option<Coding> {
    (value.member == "valueCoding").then(|| Coding {
        system: member(&value.value, "system"),
        version: member(&value.value, "version"),
        code: member(&value.value, "code"),
        display: member(&value.value, "display"),
    })
}

/// A primitive value as one line of text: a string, a code, a URI, a date
/// and time, a boolean, an integer or a decimal in the lexical form the
/// server wrote.
pub(crate) fn primitive_text(value: &PropertyValue) -> Option<String> {
    match &value.value {
        Value::String(text) => Some(text.clone()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

/// What the readers' tests share: an answer read as the console reads it.
#[cfg(test)]
pub(crate) mod tests {
    use ferrobridge_term::outcome::BatchOutcome;
    use ferrobridge_term::request::Request;

    use crate::fhir::FhirClient;
    use crate::fhir::error::FhirError;
    use crate::fhir::exchange::Answer;
    use crate::fhir::version::FhirVersion;

    /// `json`, answered `200` to `request` at `version`, read as the console
    /// reads it, completed with the `resourceType` the operation answers
    /// when the fixture leaves it out.
    pub(crate) fn read(
        version: FhirVersion,
        request: &Request,
        json: &str,
    ) -> Result<BatchOutcome, FhirError> {
        let mut document: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(json).expect("the fixture is a JSON object");
        let resource_type = match request {
            Request::Expand(_) => "ValueSet",
            _ => "Parameters",
        };
        document
            .entry("resourceType")
            .or_insert_with(|| serde_json::Value::from(resource_type));
        let answer = Answer {
            status: 200,
            etag: None,
            location: None,
            body: serde_json::Value::Object(document).to_string(),
        };
        FhirClient::new("https://tx.example.org".to_owned())
            .answered(version, request, "https://tx.example.org/", &answer)
            .map(|document| document.value)
    }

    /// The query `request` is sent with at `version`, from its `?` on, or
    /// nothing for a request sent with none.
    pub(crate) fn query(version: FhirVersion, request: Request) -> String {
        let operation = FhirClient::new(String::new())
            .operation(version, request)
            .expect("the release writes the request");
        operation
            .url()
            .split_once('?')
            .map_or_else(String::new, |(_, query)| format!("?{query}"))
    }

    /// The `Parameters` body `request` is sent with at `version`, as JSON.
    pub(crate) fn body(version: FhirVersion, request: Request) -> serde_json::Value {
        let operation = FhirClient::new(String::new())
            .operation(version, request)
            .expect("the release writes the request");
        serde_json::from_str(operation.invocation.body().expect("a POST carries a body"))
            .expect("the body is JSON")
    }
}
