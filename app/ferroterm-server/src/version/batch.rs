// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `POST [base]` with a `Bundle`: the batch interaction.
//!
//! A `batch` Bundle carries one entry per request, each with the method and the
//! relative URL it would have used on its own, and the server answers with a
//! `batch-response` holding one entry per request in the same order
//! (<https://hl7.org/fhir/R4B/http.html#transaction>). The entries of a batch
//! are independent: one that fails takes only its own entry down, and the batch
//! itself still answers `200`.
//!
//! A `transaction` Bundle needs all-or-nothing processing across entries, which
//! this server does not do, so it is refused with `not-supported`.

/// The path and the query parameters of a `Bundle` entry's relative URL.
///
/// The url is relative to the server base and carries the operation's inputs in
/// its query for a `GET` entry
/// (<https://hl7.org/fhir/R4B/http.html#transaction>).
pub(crate) fn split_url(url: &str) -> (&str, Vec<(String, String)>) {
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    (path.trim_matches('/'), parameters_of(query))
}

/// The `name=value` pairs of a query string, percent-decoded.
fn parameters_of(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(name), decode(value))
        })
        .collect()
}

/// One query component, with `+` read as a space and `%XX` decoded
/// (<https://www.rfc-editor.org/rfc/rfc3986#section-2.1>).
///
/// A byte sequence that is not a valid escape stays as it was written: a query
/// this server cannot decode is still a query the operation may refuse for its
/// own reasons, and losing it would hide that.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        match byte {
            b'+' => {
                out.push(b' ');
                index = index.saturating_add(1);
            }
            b'%' => {
                let escape = index
                    .checked_add(1)
                    .zip(index.checked_add(3))
                    .and_then(|(from, to)| bytes.get(from..to))
                    .and_then(|hex| std::str::from_utf8(hex).ok())
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok());
                if let Some(decoded) = escape {
                    out.push(decoded);
                    index = index.saturating_add(3);
                } else {
                    out.push(byte);
                    index = index.saturating_add(1);
                }
            }
            other => {
                out.push(other);
                index = index.saturating_add(1);
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
}

/// The `entry.response.status` of an HTTP status: the code and its reason.
///
/// The status "SHALL start with a 3 digit HTTP code and may contain the standard
/// HTTP description associated with the status code"
/// (<https://hl7.org/fhir/R4B/bundle-definitions.html#Bundle.entry.response.status>),
/// so a reader keys on the digits and this server writes both.
pub(crate) fn status_text(status: http::StatusCode) -> String {
    match status.canonical_reason() {
        Some(reason) => format!("{} {reason}", status.as_u16()),
        None => status.as_u16().to_string(),
    }
}

macro_rules! batch {
    ($fhir:ident) => {
        pub mod batch {
            //! The batch interaction of one version.

            use std::sync::Arc;

            use axum::body::Bytes;
            use axum::extract::{Query, State};
            use axum::response::{IntoResponse, Response};
            use fhir_types::codec::Json;
            use fhir_types::$fhir::bundle::{Bundle, BundleEntry, BundleEntryResponse};
            use fhir_types::$fhir::parameters::Parameters;
            use fhir_types::$fhir::resource::Resource;
            use http::{HeaderMap, StatusCode};

            use crate::outcome::Failure;
            use crate::state::AppState;
            use crate::version::batch::{split_url, status_text};
            use crate::wire::Wire;

            use super::{operations, parameters};

            /// `POST [base]`: runs a `batch` Bundle and answers a `batch-response`.
            pub async fn batch(
                State(state): State<Arc<AppState>>,
                headers: HeaderMap,
                Query(query): Query<Vec<(String, String)>>,
                body: Bytes,
            ) -> Response {
                let wire = match Wire::negotiate(&query, &headers) {
                    Ok(wire) => wire,
                    Err(failure) => return failure.into_response(),
                };
                match run(&state, &headers, &body) {
                    Ok(bundle) => match parameters::respond_resource(&bundle, wire) {
                        Ok(response) => response,
                        Err(failure) => failure.respond(wire),
                    },
                    Err(failure) => failure.respond(wire),
                }
            }

            /// The `batch-response` for the Bundle in `body`.
            fn run(state: &AppState, headers: &HeaderMap, body: &Bytes) -> Result<Bundle, Failure> {
                let sent = bundle_of(headers, body)?;
                let kind = sent.r#type.value.as_deref().unwrap_or_default();
                if kind == "transaction" {
                    // NOTE: a server that does not support transactions "SHOULD return an HTTP
                    // 400 error and MAY include an OperationOutcome"
                    // (<https://hl7.org/fhir/R4B/http.html#transaction>); the issue code is ours.
                    return Err(Failure::new(
                        StatusCode::BAD_REQUEST,
                        "not-supported",
                        "this server does not process a `transaction` Bundle; send a `batch`",
                    )
                    .kind("not-supported"));
                }
                if kind != "batch" {
                    return Err(Failure::new(
                        StatusCode::BAD_REQUEST,
                        "value",
                        format!("a Bundle posted to the server root is a `batch`, not a `{kind}`"),
                    ));
                }
                let entry = sent
                    .entry
                    .into_iter()
                    .map(|held| answered(state, headers, held))
                    .collect();
                Ok(Bundle {
                    r#type: "batch-response".into(),
                    entry,
                    ..Default::default()
                })
            }

            /// The Bundle the request body carries.
            fn bundle_of(headers: &HeaderMap, body: &Bytes) -> Result<Bundle, Failure> {
                let structure =
                    |text: String| Failure::new(StatusCode::BAD_REQUEST, "structure", text);
                let object = match Wire::of_body(headers)? {
                    Wire::Json => {
                        let value: fhir_types::codec::Value = serde_json::from_slice(body)
                            .map_err(|error| structure(format!("the body is not JSON: {error}")))?;
                        let path = fhir_types::codec::Path::root("Bundle");
                        fhir_types::codec::expect_object(&value, &path)
                            .map_err(|error| structure(error.to_string()))?
                            .clone()
                    }
                    Wire::Xml => {
                        let text = std::str::from_utf8(body).map_err(|error| {
                            structure(format!("the body is not UTF-8: {error}"))
                        })?;
                        fhir_types::xml::from_xml(&fhir_types::$fhir::schema::SCHEMAS, text)
                            .map_err(|error| structure(error.to_string()))?
                    }
                };
                let mut path = fhir_types::codec::Path::root("Bundle");
                Json::from_json(&object, &mut path).map_err(|error| structure(error.to_string()))
            }

            /// The response entry for one request entry.
            fn answered(state: &AppState, headers: &HeaderMap, sent: BundleEntry) -> BundleEntry {
                let full_url = sent.full_url.clone();
                match entry(state, headers, sent) {
                    Ok(answer) => BundleEntry {
                        full_url,
                        resource: Some(answer.resource()),
                        response: Some(BundleEntryResponse {
                            status: status_text(StatusCode::OK).as_str().into(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    Err(failure) => refused(full_url, &failure),
                }
            }

            /// The response entry of an entry that failed.
            ///
            /// The outcome is the entry's `resource`, the body the same request would
            /// have answered on its own: `response.outcome` is "not used for error
            /// responses in batch/transaction, only for hints and warnings"
            /// (<https://hl7.org/fhir/R4B/bundle-definitions.html#Bundle.entry.response.outcome>),
            /// and the specification names no other slot for the error.
            fn refused(
                full_url: Option<fhir_types::$fhir::primitives::Uri>,
                failure: &Failure,
            ) -> BundleEntry {
                let resource = failure.outcome().to_json().ok().and_then(|object| {
                    Resource::from_json(
                        &object,
                        &mut fhir_types::codec::Path::root("OperationOutcome"),
                    )
                    .ok()
                });
                BundleEntry {
                    full_url,
                    resource,
                    response: Some(BundleEntryResponse {
                        status: status_text(failure.status).as_str().into(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            }

            /// What one entry's request produced.
            fn entry(
                state: &AppState,
                headers: &HeaderMap,
                sent: BundleEntry,
            ) -> Result<operations::Answer, Failure> {
                let request = sent.request.ok_or_else(|| {
                    Failure::new(
                        StatusCode::BAD_REQUEST,
                        "required",
                        "every entry of a batch carries `request`",
                    )
                })?;
                let method = request.method.value.as_deref().unwrap_or_default();
                let url = request.url.value.as_deref().unwrap_or_default();
                let (path, query) = split_url(url);
                match method {
                    "GET" => operations::invoke(state, headers, path, &query, None),
                    "POST" => {
                        let sent = match sent.resource {
                            Some(Resource::Parameters(held)) => Some(*held),
                            None => Some(Parameters::default()),
                            Some(_) => {
                                return Err(Failure::new(
                                    StatusCode::BAD_REQUEST,
                                    "invalid",
                                    format!("a `POST {url}` entry carries a `Parameters` resource"),
                                ));
                            }
                        };
                        operations::invoke(state, headers, path, &query, sent)
                    }
                    other => Err(Failure::new(
                        StatusCode::METHOD_NOT_ALLOWED,
                        "not-supported",
                        format!("`{other} {url}` is not an interaction of this server"),
                    )),
                }
            }
        }
    };
}

pub(crate) use batch;

#[cfg(test)]
mod tests {
    use super::{decode, split_url, status_text};

    #[test]
    fn a_relative_url_splits_into_a_path_and_its_query() {
        let (path, query) = split_url("CodeSystem/$lookup?system=http://x&code=a");
        assert_eq!(path, "CodeSystem/$lookup");
        assert_eq!(
            query,
            [
                (String::from("system"), String::from("http://x")),
                (String::from("code"), String::from("a"))
            ]
        );
        assert_eq!(split_url("ValueSet/$expand").0, "ValueSet/$expand");
        assert_eq!(
            split_url("/ValueSet/$expand/").0,
            "ValueSet/$expand",
            "a leading or trailing slash is not a segment"
        );
        assert!(split_url("ValueSet/$expand?").1.is_empty());
    }

    #[test]
    fn a_query_component_decodes_its_escapes() {
        assert_eq!(decode("a%20b"), "a b");
        assert_eq!(decode("a+b"), "a b");
        assert_eq!(decode("%3C%3C%20404684003"), "<< 404684003");
        assert_eq!(decode("100%"), "100%", "a stray percent stays as written");
        assert_eq!(decode("%zz"), "%zz");
    }

    #[test]
    fn a_status_carries_its_code_and_reason() {
        assert_eq!(status_text(http::StatusCode::OK), "200 OK");
        assert_eq!(status_text(http::StatusCode::NOT_FOUND), "404 Not Found");
    }
}
