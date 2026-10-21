// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The system-level routes of one version: `$versions`, `$cache-control`, and `ValueSet` read and search.

macro_rules! system {
    ($fhir:ident) => {
        pub mod system {
            //! The system-level operations and the `ValueSet` read and search
            //! interactions of one version.
            //!
            //! `$versions` is R5's `CapabilityStatement/$versions`
            //! (<https://hl7.org/fhir/R5/capabilitystatement-operation-versions.html>),
            //! served here so a client learns the FHIR version of this base before it
            //! reads the capability statement. `$cache-control` is the terminology
            //! ecosystem's (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>):
            //! `mode=start` stores the request's `tx-resource`s under a new `cache-id`,
            //! `mode=end` releases the cache named by `X-Cache-Id` or `cache-id`.

            use std::sync::Arc;

            use axum::body::Bytes;
            use axum::extract::{Query, State};
            use axum::response::{IntoResponse, Response};
            use fhir_types::$fhir::parameters::{Parameters, ParametersParameter, ParametersParameterValue};
            use http::{HeaderMap, StatusCode};

            use crate::outcome::Failure;
            use super::metadata::FHIR_VERSION;
            use super::parameters;
            use super::resources::split_supplied;
            use crate::scope::{CACHE_ID_HEADER, cache_id};
            use crate::state::AppState;
            use crate::wire::Wire;

            /// The canonical of the `$cache-control` operation, the ecosystem IG's.
            pub const CACHE_CONTROL_URL: &str =
                "http://hl7.org/fhir/uv/tx-ecosystem/OperationDefinition/cache-control";
            /// The canonical of R5's `$versions`.
            pub const VERSIONS_URL: &str =
                "http://hl7.org/fhir/OperationDefinition/CapabilityStatement-versions";

            fn finish(handled: Result<Response, Failure>, wire: Wire) -> Response {
                match handled {
                    Ok(response) => response,
                    Err(failure) => failure.respond(wire),
                }
            }

            /// The negotiated format of a request; a format the server does not speak
            /// is refused (in JSON, at the call site).
            fn negotiated(headers: &HeaderMap, query: &[(String, String)]) -> Result<Wire, Failure> {
                Wire::negotiate(query, headers)
            }

            /// `GET /$versions`: the FHIR versions this base serves, `MAJOR.MINOR`.
            pub async fn versions(
                headers: HeaderMap,
                Query(query): Query<Vec<(String, String)>>,
            ) -> Response {
                let wire = match negotiated(&headers, &query) {
                    Ok(wire) => wire,
                    Err(failure) => return failure.into_response(),
                };
                let major_minor: String = FHIR_VERSION
                    .rsplitn(2, '.')
                    .last()
                    .unwrap_or(FHIR_VERSION)
                    .to_owned();
                finish(parameters::respond(&Parameters {
                    parameter: vec![
                        ParametersParameter {
                            name: "version".into(),
                            value: Some(ParametersParameterValue::Code(major_minor.as_str().into())),
                            ..Default::default()
                        },
                        ParametersParameter {
                            name: "default".into(),
                            value: Some(ParametersParameterValue::Code(major_minor.as_str().into())),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }, wire), wire)
            }

            /// `POST /$cache-control?mode=start|end`.
            pub async fn cache_control(
                State(state): State<Arc<AppState>>,
                headers: HeaderMap,
                Query(query): Query<Vec<(String, String)>>,
                body: Bytes,
            ) -> Response {
                let wire = match negotiated(&headers, &query) {
                    Ok(wire) => wire,
                    Err(failure) => return failure.into_response(),
                };
                finish(run_cache_control(&state, &headers, &query, &body, wire), wire)
            }

            fn run_cache_control(
                state: &AppState,
                headers: &HeaderMap,
                query: &[(String, String)],
                body: &Bytes,
                wire: Wire,
            ) -> Result<Response, Failure> {
                let mode = query
                    .iter()
                    .find(|(name, _)| name == "mode")
                    .map(|(_, value)| value.as_str())
                    .ok_or_else(|| {
                        Failure::new(
                            StatusCode::BAD_REQUEST,
                            "required",
                            "`mode` is required: `start` or `end`",
                        )
                    })?;
                let (parameters, resources) = if body.is_empty() {
                    (Parameters::default(), Vec::new())
                } else {
                    split_supplied(parameters::object_from_body(headers, body)?)?
                };
                match mode {
                    "start" => {
                        let id = state.caches().start(resources);
                        parameters::respond(&Parameters {
                            parameter: vec![ParametersParameter {
                                name: "cache-id".into(),
                                value: Some(ParametersParameterValue::Id(id.as_str().into())),
                                ..Default::default()
                            }],
                            ..Default::default()
                        }, wire)
                    }
                    "end" => {
                        let named = parameters
                            .parameter
                            .iter()
                            .find(|p| p.name.value.as_deref() == Some("cache-id"))
                            .and_then(|p| match &p.value {
                                Some(ParametersParameterValue::Id(id)) => id.value.clone(),
                                Some(ParametersParameterValue::String(id)) => id.value.clone(),
                                _ => None,
                            });
                        let Some(id) = named.or(cache_id(headers)?) else {
                            return Err(Failure::new(
                                StatusCode::BAD_REQUEST,
                                "required",
                                format!(
                                    "`mode=end` needs the cache in `{CACHE_ID_HEADER}` or a `cache-id` parameter"
                                ),
                            ));
                        };
                        state.caches().end(&id)?;
                        parameters::respond(&Parameters::default(), wire)
                    }
                    other => Err(Failure::new(
                        StatusCode::BAD_REQUEST,
                        "value",
                        format!("`mode={other}` is not `start` or `end`"),
                    )),
                }
            }

        }
    };
}

pub(crate) use system;
