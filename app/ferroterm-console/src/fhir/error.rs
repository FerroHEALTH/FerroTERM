// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The typed failure every FHIR request can return.

use http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::fhir::outcome::OperationOutcome;

/// Why a FHIR request did not produce the resource the caller asked for.
///
/// A caller that branches on the outcome reads a variant. The status is a
/// [`StatusCode`] rather than a number so a comparison cannot be written
/// against a literal, and the server's own `OperationOutcome` travels as data
/// so a screen can render its wording verbatim.
#[derive(Clone, Debug, thiserror::Error, Serialize, Deserialize)]
pub(crate) enum FhirError {
    /// The request could not be sent or the answer read.
    #[error("{url} could not be reached: {message}")]
    Transport {
        /// The URL that was attempted.
        url: String,
        /// What the browser reported.
        message: String,
    },
    /// The server refused, and said why in an `OperationOutcome`.
    #[error("{url} answered {status}")]
    Refused {
        /// The URL that was requested.
        url: String,
        /// The HTTP status the server answered with.
        #[serde(with = "status_code")]
        status: StatusCode,
        /// The refusal, as the server wrote it.
        outcome: OperationOutcome,
    },
    /// The server answered a failure that carried no readable
    /// `OperationOutcome`, so the body is kept as the evidence.
    #[error("{url} answered {status} without an OperationOutcome")]
    Status {
        /// The URL that was requested.
        url: String,
        /// The HTTP status the server answered with.
        #[serde(with = "status_code")]
        status: StatusCode,
        /// The first part of the body, so a reader sees what arrived.
        body: String,
    },
    /// The answer was a success but did not parse as the expected resource.
    #[error("the answer from {url} did not parse: {message}")]
    Decode {
        /// The URL that was requested.
        url: String,
        /// What the decoder reported.
        message: String,
    },
}

impl FhirError {
    /// The URL that produced this failure, for the reader to retry by hand.
    pub(crate) fn url(&self) -> &str {
        match self {
            Self::Transport { url, .. }
            | Self::Refused { url, .. }
            | Self::Status { url, .. }
            | Self::Decode { url, .. } => url,
        }
    }

    /// The status the server answered, when it answered at all.
    pub(crate) fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Refused { status, .. } | Self::Status { status, .. } => Some(*status),
            Self::Transport { .. } | Self::Decode { .. } => None,
        }
    }

    /// The refusal the server wrote, when it wrote one.
    pub(crate) fn outcome(&self) -> Option<&OperationOutcome> {
        match self {
            Self::Refused { outcome, .. } => Some(outcome),
            Self::Transport { .. } | Self::Status { .. } | Self::Decode { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_carries_the_status_as_a_status_code() {
        let error = FhirError::Refused {
            url: "/r4b/metadata".to_owned(),
            status: StatusCode::NOT_FOUND,
            outcome: OperationOutcome::default(),
        };
        assert_eq!(
            error.status(),
            Some(StatusCode::NOT_FOUND),
            "a caller compares a StatusCode, never a number"
        );
        assert!(
            error.outcome().is_some(),
            "the server's own refusal travels with the error"
        );
    }

    #[test]
    fn a_transport_failure_has_no_status_and_still_names_its_url() {
        let error = FhirError::Transport {
            url: "/health".to_owned(),
            message: "network error".to_owned(),
        };
        assert_eq!(
            error.status(),
            None,
            "nothing answered, so there is no status"
        );
        assert_eq!(
            error.url(),
            "/health",
            "the reader can retry the URL by hand"
        );
    }

    #[test]
    fn the_display_text_names_the_url_and_the_status() {
        let error = FhirError::Status {
            url: "/r6/metadata".to_owned(),
            status: StatusCode::BAD_GATEWAY,
            body: "<html>".to_owned(),
        };
        assert_eq!(
            error.to_string(),
            "/r6/metadata answered 502 Bad Gateway without an OperationOutcome",
            "the message says what was asked and what came back"
        );
    }
}

/// A [`StatusCode`] as the number a page carries from the server to the
/// browser half (serde, `with`).
mod status_code {
    use http::StatusCode;
    use serde::{Deserialize as _, Deserializer, Serializer};

    /// Writes `status` as its number.
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's `with` calls the function with a reference to the field"
    )]
    pub(super) fn serialize<S: Serializer>(status: &StatusCode, out: S) -> Result<S::Ok, S::Error> {
        out.serialize_u16(status.as_u16())
    }

    /// Reads a status from its number, refusing one that is no HTTP status.
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<StatusCode, D::Error> {
        let code = u16::deserialize(input)?;
        StatusCode::from_u16(code).map_err(serde::de::Error::custom)
    }
}
