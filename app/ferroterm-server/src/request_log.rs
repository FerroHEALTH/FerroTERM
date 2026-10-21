// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One log line per HTTP request: method, route, status, latency, the request
//! id, and the system and code the request named. Bodies are never logged.
//!
//! The same pass records the request in the metrics registry and answers with
//! the `X-Request-Id` the client sent or one this server made, so a log line, a
//! metric sample, and a client trace name the same request. No specification
//! governs the header: our own design, and the name every proxy already uses.
//! The pass also feeds the usage report window, by matched route template.

use std::sync::Arc;
use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use http::HeaderValue;

use crate::metrics;
use crate::state::AppState;
use crate::usage_report::window::Window;

/// The header a client sends to name its request, and the server echoes.
pub const REQUEST_ID: &str = "x-request-id";

/// The request id of `headers`, or a fresh one.
///
/// A client id is taken verbatim when it is printable ASCII of at most 128
/// characters, so a header cannot smuggle a newline into a log line.
fn request_id(headers: &http::HeaderMap) -> String {
    headers
        .get(REQUEST_ID)
        .and_then(|value| value.to_str().ok())
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id.chars().all(|c| c.is_ascii_graphic() || c == ' ')
        })
        .map_or_else(|| uuid::Uuid::new_v4().to_string(), ToOwned::to_owned)
}

/// The query parameters worth a log field: they name a system or a code and
/// never carry free text.
const LOGGED_QUERY: [&str; 6] = ["system", "url", "version", "code", "codeA", "codeB"];

/// Logs the request after it completes, records it, and answers with its id.
pub async fn log(
    State(state): State<Arc<AppState>>,
    State(usage): State<Arc<Window>>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let matched = request
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_owned());
    let path = request.uri().path().to_owned();
    let route = matched.clone().unwrap_or_else(|| path.clone());
    let named = named_parameters(request.uri().query().unwrap_or(""));
    let id = request_id(request.headers());
    let started = Instant::now();
    let mut response = next.run(request).await;
    let status = response.status();
    let duration = started.elapsed();
    usage.record(&method, matched.as_deref(), &path, duration, status);
    let elapsed = duration.as_secs_f64();
    let latency_ms = elapsed * 1000.0;
    state.metrics().record(
        &metrics::Request {
            method: metrics::Method::from(&method),
            route: route.clone(),
            status: status.as_u16(),
        },
        elapsed,
    );
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(REQUEST_ID, value);
    }
    let (status_code, method, route, named, request_id) = (
        status.as_u16(),
        method.as_str(),
        route.as_str(),
        named.as_str(),
        id.as_str(),
    );
    if status.is_server_error() {
        tracing::error!(
            method,
            route,
            status = status_code,
            latency_ms,
            named,
            request_id,
            "request"
        );
    } else if status.is_client_error() {
        tracing::warn!(
            method,
            route,
            status = status_code,
            latency_ms,
            named,
            request_id,
            "request"
        );
    } else {
        tracing::info!(
            method,
            route,
            status = status_code,
            latency_ms,
            named,
            request_id,
            "request"
        );
    }
    response
}

/// `key=value` pairs of the logged query parameters, space-separated, in
/// query order; the value is taken verbatim up to 64 characters.
fn named_parameters(query: &str) -> String {
    let mut out = String::new();
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if LOGGED_QUERY.contains(&key) {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(key);
            out.push('=');
            out.extend(value.chars().take(64));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::named_parameters;

    #[test]
    fn only_system_and_code_parameters_are_logged() {
        assert_eq!(
            named_parameters("system=http://snomed.info/sct&code=1&display=Secret%20text&codeA=2"),
            "system=http://snomed.info/sct code=1 codeA=2"
        );
        assert_eq!(named_parameters(""), "");
    }
}
