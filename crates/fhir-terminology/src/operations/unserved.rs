// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `not-found` answer for a code system the server does not serve, as
//! `$validate-code` reports it.

use super::{Issue, MessageId};

/// A system the server does not serve, as `x-caused-by-unknown-system` names it.
///
/// Returns the canonical (`url` or `url|version`) and the issue that says so;
/// the wording is the ecosystem's
/// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>, the
/// `$validate-code` return parameters).
#[must_use]
pub fn system(
    url: &str,
    version: Option<&str>,
    expression: Option<String>,
    valid: &[String],
) -> (String, Issue) {
    let (canonical, message, text) = match version {
        Some(version) => (
            format!("{url}|{version}"),
            MessageId::UnknownCodeSystemVersion,
            format!(
                "A definition for CodeSystem '{url}' version '{version}' could not be found, so the code cannot be validated{}",
                super::valid_versions(valid)
            ),
        ),
        None => (
            url.to_owned(),
            MessageId::UnknownCodeSystem,
            format!(
                "A definition for CodeSystem '{url}' could not be found, so the code cannot be validated"
            ),
        ),
    };
    (
        canonical,
        Issue {
            severity: "error",
            code: "not-found",
            kind: "not-found",
            message,
            text,
            expression,
        },
    )
}

/// A coding that names a version of a system the server serves no version of:
/// the `not-found` issue names the version asked for, and the canonical the
/// answer reports unknown is the system's.
#[must_use]
pub fn version_none(url: &str, version: &str, expression: Option<String>) -> (String, Issue) {
    (
        url.to_owned(),
        Issue {
            severity: "error",
            code: "not-found",
            kind: "not-found",
            message: MessageId::UnknownCodeSystemVersionNone,
            text: format!(
                "A definition for CodeSystem '{url}' version '{version}' could not be found, so the code cannot be validated. No version of this code system is served"
            ),
            expression,
        },
    )
}
