// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the server adds to the request line of `ferrohealth-telemetry`: the
//! system and code a request named, as the crate's product fields.
//!
//! The same pass feeds the usage report window, by matched route template.
//! Only parameters that name a system or a code reach the line; free text
//! never does. No specification governs the line: our own design.

use std::sync::Arc;
use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use ferrohealth_telemetry::request_log::fields::{LineFields, Token};

use crate::usage_report::window::Window;

/// The query parameters that reach the line, and the field each becomes.
const LOGGED: [(&str, &str); 6] = [
    ("system", "system"),
    ("url", "url"),
    ("version", "version"),
    ("code", "code"),
    ("codeA", "code_a"),
    ("codeB", "code_b"),
];

/// The longest value a field carries, in characters.
const MAX_VALUE: usize = 64;

/// Adds the named system and code to the response's line fields, and records
/// the request in the usage window.
pub async fn record(State(usage): State<Arc<Window>>, request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let matched = request
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_owned());
    let path = request.uri().path().to_owned();
    let fields = named_fields(request.uri().query().unwrap_or(""));
    let started = Instant::now();
    let mut response = next.run(request).await;
    usage.record(
        &method,
        matched.as_deref(),
        &path,
        started.elapsed(),
        response.status(),
    );
    if !fields.is_empty() {
        response.extensions_mut().insert(fields);
    }
    response
}

/// The line fields of the logged parameters of `query`, in query order; the
/// value is taken verbatim up to [`MAX_VALUE`] characters.
fn named_fields(query: &str) -> LineFields {
    let mut fields = LineFields::new();
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let Some((_, field)) = LOGGED.iter().find(|(name, _)| *name == key) else {
            continue;
        };
        // NOTE: no specification governs this: our own design. A value the line
        // cannot carry (empty, or not visible ASCII) is legitimately left off it.
        let Ok(token) = Token::new(value.chars().take(MAX_VALUE).collect::<String>()) else {
            continue;
        };
        // NOTE: no specification governs this: our own design. The six names are
        // legal and fewer than the limit, so a refusal only drops a log field.
        if let Err(error) = fields.insert(field, token) {
            tracing::debug!(%error, field, "a request line field was left off");
        }
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::named_fields;

    #[test]
    fn only_system_and_code_parameters_are_logged() {
        let fields =
            named_fields("system=http://snomed.info/sct&code=1&display=Secret%20text&codeA=2");
        assert_eq!(
            fields.iter().collect::<Vec<_>>(),
            [
                ("system", "http://snomed.info/sct"),
                ("code", "1"),
                ("code_a", "2")
            ]
        );
        assert!(named_fields("").is_empty());
    }
}
