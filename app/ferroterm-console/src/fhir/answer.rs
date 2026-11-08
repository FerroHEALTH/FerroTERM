// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How one exchange becomes the answer a screen reads: the address below the
//! server's root, the request sent, and the status and body read back into
//! the resource or the refusal.

use http::StatusCode;
use serde::de::DeserializeOwned;

use super::{BODY_EXCERPT_BYTES, Document, FHIR_JSON, FhirClient};
use crate::fhir::error::FhirError;
use crate::fhir::exchange::{Answer, Exchange, ExchangeError, Method};
use crate::fhir::outcome::Outcome;
use crate::fhir::release::Decodes;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr, tr_with};

impl FhirClient {
    /// The path below the server's root that `url`, one of this client's
    /// addresses, names, as an exchange carries it.
    pub(super) fn path_of(&self, url: &str) -> Result<String, FhirError> {
        let locale = &self.locale;
        url.strip_prefix(self.root.as_str())
            .filter(|path| path.starts_with('/'))
            .map(str::to_owned)
            .ok_or_else(|| FhirError::Transport {
                url: url.to_owned(),
                message: tr(locale, Msg::AddressNotBelowServerSRoot),
            })
    }

    /// Sends a request of `method` to `url` with no body, asking for
    /// `accept`, and answers what came back.
    pub(super) async fn send(
        &self,
        method: Method,
        url: &str,
        accept: &str,
    ) -> Result<Answer, FhirError> {
        self.exchange(Exchange::new(method, self.path_of(url)?, accept))
            .await
            .map_err(|error| transport(url, &error))
    }

    /// Sends a FHIR JSON `GET` and decodes the resource it answers.
    pub(super) async fn get_json<T: DeserializeOwned>(
        &self,
        version: FhirVersion,
        url: &str,
    ) -> Result<T, FhirError> {
        let answer = self.send(Method::Get, url, FHIR_JSON).await?;
        read_json(&self.locale, version, &answer, url)
    }

    /// Sends a FHIR JSON `GET` and decodes the resource it answers through
    /// the `fhir-types` module of `version`, the release that serves it.
    pub(super) async fn get_answer<A: Decodes>(
        &self,
        version: FhirVersion,
        url: &str,
    ) -> Result<A, FhirError> {
        let answer = self.send(Method::Get, url, FHIR_JSON).await?;
        decoded(&self.locale, version, &answer, url)
    }

    /// Sends a FHIR JSON `GET`, decodes the resource it answers at `version`,
    /// and keeps the document as the server sent it.
    pub(super) async fn get_answer_document<A: Decodes>(
        &self,
        version: FhirVersion,
        url: &str,
    ) -> Result<Document<A>, FhirError> {
        let answer = self.send(Method::Get, url, FHIR_JSON).await?;
        let value = decoded(&self.locale, version, &answer, url)?;
        Ok(Document {
            value,
            text: pretty(&answer.body),
        })
    }
}

/// A JSON document indented two spaces a level, for a reader; a body that is
/// no JSON (which `read_json` has already refused) stays as it came.
pub(super) fn pretty(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| body.to_owned())
}

/// Turns an answer into the resource `A` reads, decoded at `version`, the
/// release that served it, or into the refusal.
pub(super) fn decoded<A: Decodes>(
    locale: &ferrohealth_ui::i18n::Locale,
    version: FhirVersion,
    answer: &Answer,
    url: &str,
) -> Result<A, FhirError> {
    let status = status_of(locale, answer, url)?;
    if !status.is_success() {
        return Err(failure(version, answer, status, url));
    }
    version
        .decode(&answer.body)
        .map_err(|error| FhirError::undecoded(url, version, &error))
}

fn read_json<T: DeserializeOwned>(
    locale: &ferrohealth_ui::i18n::Locale,
    version: FhirVersion,
    answer: &Answer,
    url: &str,
) -> Result<T, FhirError> {
    let status = status_of(locale, answer, url)?;
    if !status.is_success() {
        return Err(failure(version, answer, status, url));
    }
    serde_json::from_str(&answer.body).map_err(|error| FhirError::Decode {
        url: url.to_owned(),
        message: error.to_string(),
    })
}

/// The failure of an exchange that produced no answer.
pub(super) fn transport(url: &str, error: &ExchangeError) -> FhirError {
    FhirError::Transport {
        url: url.to_owned(),
        message: error.to_string(),
    }
}

/// Reads the answered status as a [`StatusCode`].
///
/// A status outside 100 to 999 is no HTTP status
/// (<https://www.rfc-editor.org/rfc/rfc9110#section-15>), so the conversion is
/// fallible and says so.
pub(super) fn status_of(
    locale: &ferrohealth_ui::i18n::Locale,
    answer: &Answer,
    url: &str,
) -> Result<StatusCode, FhirError> {
    StatusCode::from_u16(answer.status).map_err(|_invalid| FhirError::Transport {
        url: url.to_owned(),
        message: tr_with(
            locale,
            Msg::ServerAnsweredStatus,
            &[("status", (answer.status).to_string().into())],
        ),
    })
}

/// Turns a non-success answer into the refusal the reader is shown, its
/// `OperationOutcome` decoded at `version`, the release that answered.
pub(super) fn failure(
    version: FhirVersion,
    answer: &Answer,
    status: StatusCode,
    url: &str,
) -> FhirError {
    // A body that is no `OperationOutcome` at `version`, or one that carries
    // no issue, says nothing, so the body itself stays the evidence.
    match version.decode::<Outcome>(&answer.body) {
        Ok(outcome) if !outcome.issues().is_empty() => FhirError::Refused {
            url: url.to_owned(),
            status,
            outcome,
        },
        Ok(_) | Err(_) => unexplained(answer, status, url),
    }
}

/// Turns a non-success answer that carries no refusal into a status, with
/// the start of its body as the evidence.
pub(super) fn unexplained(answer: &Answer, status: StatusCode, url: &str) -> FhirError {
    FhirError::Status {
        url: url.to_owned(),
        status,
        body: excerpt(&answer.body),
    }
}

/// Keeps the first part of a body, cut on a character boundary.
pub(super) fn excerpt(body: &str) -> String {
    body.char_indices()
        .take_while(|(index, _)| *index < BODY_EXCERPT_BYTES)
        .map(|(_, character)| character)
        .collect()
}
