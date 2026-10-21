// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `Parameters` wire of one version: GET query and POST body decoding, the `Accept-Language` fold, and JSON responses.

macro_rules! parameters {
    ($fhir:ident) => {
        pub mod parameters {
            //! From the wire to a `Parameters` and back
            //! (<https://hl7.org/fhir/R4B/operations.html#3.2.0.6>).
            //!
            //! `GET` carries the primitive parameters as query parameters, a repeated
            //! parameter as a repeated name; `POST` carries a `Parameters` resource as
            //! FHIR JSON or FHIR XML, by its `Content-Type`. A parameter the operation
            //! does not declare, or a complex one on `GET`, is refused. Responses go out
            //! in the format the request negotiated (`crate::wire`).

            use axum::body::Bytes;
            use axum::response::Response;
            use fhir_types::codec::{Json, Path, expect_object};
            use fhir_types::operation::{Operation, ParameterUse, ParametersError};
            use fhir_types::$fhir::parameters::{Parameters, ParametersParameter, ParametersParameterValue};
            use http::{HeaderMap, StatusCode};

            use crate::outcome::Failure;
            use crate::wire::Wire;

            /// Builds the `Parameters` of a `GET` invocation from its query parameters.
            ///
            /// # Errors
            ///
            /// Returns a `400` failure for an undeclared parameter, a complex parameter
            /// (which needs `POST`), or a value that is not of the parameter's type.
            pub fn parameters_from_query(
                operation: &Operation,
                query: &[(String, String)],
            ) -> Result<Parameters, Failure> {
                let mut parameters = Vec::with_capacity(query.len());
                for (name, text) in query {
                    let declared = operation.parameter(ParameterUse::In, name).ok_or_else(|| {
                        Failure::new(
                            StatusCode::BAD_REQUEST,
                            "invalid",
                            format!(
                                "{}/${} does not declare a parameter `{name}`",
                                operation.resource, operation.code
                            ),
                        )
                    })?;
                    let Some(type_code) = declared.type_code else {
                        return Err(complex(operation, name));
                    };
                    let value = primitive(type_code, text)
                        .ok_or_else(|| complex(operation, name))?
                        .map_err(|reason| {
                            Failure::new(
                                StatusCode::BAD_REQUEST,
                                "value",
                                format!("parameter `{name}` is not a valid {type_code}: {reason}"),
                            )
                        })?;
                    parameters.push(ParametersParameter {
                        name: name.as_str().into(),
                        value: Some(value),
                        ..Default::default()
                    });
                }
                Ok(Parameters {
                    parameter: parameters,
                    ..Default::default()
                })
            }

            /// Refuses a parameter the operation does not declare.
            ///
            /// A `GET` is checked while its query is decoded; a `POST` carries a
            /// `Parameters` the codec reads without consulting the operation, so
            /// the same refusal is made here. The two routes have to agree, or a
            /// parameter another version declares is taken by a version that does
            /// not (<https://hl7.org/fhir/R4B/operations.html#3.2.0.6>).
            ///
            /// # Errors
            ///
            /// Returns a `400` failure naming the first parameter the operation
            /// does not declare.
            pub fn refuse_undeclared(
                operation: &Operation,
                also: &[&str],
                parameters: &Parameters,
            ) -> Result<(), Failure> {
                for parameter in &parameters.parameter {
                    let Some(name) = parameter.name.value.as_deref() else {
                        continue;
                    };
                    if !also.contains(&name)
                        && operation.parameter(ParameterUse::In, name).is_none()
                    {
                        return Err(Failure::new(
                            StatusCode::BAD_REQUEST,
                            "invalid",
                            format!(
                                "{}/${} does not declare a parameter `{name}`",
                                operation.resource, operation.code
                            ),
                        ));
                    }
                }
                Ok(())
            }

            /// Adds a `displayLanguage` parameter from the `Accept-Language` header.
            ///
            /// Only when the operation declares the parameter and the request named none:
            /// the terminology ecosystem IG requires both routes
            /// (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/languages.html>), and
            /// the parameter wins when both are given.
            pub fn apply_accept_language(
                operation: &Operation,
                headers: &HeaderMap,
                parameters: &mut Parameters,
            ) {
                if operation
                    .parameter(ParameterUse::In, "displayLanguage")
                    .is_none()
                    || parameters
                        .parameter
                        .iter()
                        .any(|p| p.name.value.as_deref() == Some("displayLanguage"))
                {
                    return;
                }
                let Some(language) = headers
                    .get(http::header::ACCEPT_LANGUAGE)
                    .and_then(|v| v.to_str().ok())
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                else {
                    return;
                };
                parameters.parameter.push(ParametersParameter {
                    name: "displayLanguage".into(),
                    value: Some(ParametersParameterValue::Code(language.into())),
                    ..Default::default()
                });
            }

            fn complex(operation: &Operation, name: &str) -> Failure {
                Failure::new(
                    StatusCode::BAD_REQUEST,
                    "not-supported",
                    format!(
                        "parameter `{name}` of {}/${} is not a primitive; send it in a Parameters resource with POST",
                        operation.resource, operation.code
                    ),
                )
            }

            /// The typed value of a primitive parameter given as text; `None` when the
            /// type is not a primitive the query form carries.
            fn primitive(type_code: &str, text: &str) -> Option<Result<ParametersParameterValue, String>> {
                Some(match type_code {
                    "code" => Ok(ParametersParameterValue::Code(text.into())),
                    "uri" => Ok(ParametersParameterValue::Uri(text.into())),
                    "url" => Ok(ParametersParameterValue::Url(text.into())),
                    "canonical" => Ok(ParametersParameterValue::Canonical(text.into())),
                    "string" => Ok(ParametersParameterValue::String(text.into())),
                    "dateTime" => Ok(ParametersParameterValue::DateTime(text.into())),
                    "date" => Ok(ParametersParameterValue::Date(text.into())),
                    "decimal" => Ok(ParametersParameterValue::Decimal(text.into())),
                    "boolean" => match text {
                        "true" => Ok(ParametersParameterValue::Boolean(true.into())),
                        "false" => Ok(ParametersParameterValue::Boolean(false.into())),
                        other => Err(format!("`{other}` is not `true` or `false`")),
                    },
                    "integer" => text
                        .parse::<i32>()
                        .map(|i| ParametersParameterValue::Integer(i.into()))
                        .map_err(|e| e.to_string()),
                    _ => return None,
                })
            }

            /// Reads the `Parameters` resource of a `POST` invocation, FHIR JSON or
            /// FHIR XML by its `Content-Type`.
            ///
            /// # Errors
            ///
            /// Returns `415` for another media type and `400` for a body that is not a
            /// `Parameters` resource.
            pub fn parameters_from_body(headers: &HeaderMap, body: &Bytes) -> Result<Parameters, Failure> {
                parameters_from_object(&object_from_body(headers, body)?)
            }

            /// The `Parameters` resource of a `POST` invocation as its JSON object.
            ///
            /// # Errors
            ///
            /// Returns `415` for another media type and `400` for a body that is not a
            /// FHIR object.
            pub fn object_from_body(headers: &HeaderMap, body: &Bytes) -> Result<fhir_types::codec::Object, Failure> {
                let structure = |text: String| Failure::new(StatusCode::BAD_REQUEST, "structure", text);
                let path = Path::root("Parameters");
                let object = match Wire::of_body(headers)? {
                    Wire::Json => {
                        let value: fhir_types::codec::Value = serde_json::from_slice(body)
                            .map_err(|e| structure(format!("the body is not JSON: {e}")))?;
                        expect_object(&value, &path)
                            .map_err(|e| structure(e.to_string()))?
                            .clone()
                    }
                    Wire::Xml => {
                        let text = std::str::from_utf8(body)
                            .map_err(|e| structure(format!("the body is not UTF-8: {e}")))?;
                        fhir_types::xml::from_xml(&fhir_types::$fhir::schema::SCHEMAS, text)
                            .map_err(|e| structure(e.to_string()))?
                    }
                };
                Ok(object)
            }

            /// The `Parameters` resource `object` holds.
            ///
            /// # Errors
            ///
            /// Returns `400` for an object that is not a `Parameters` resource.
            pub fn parameters_from_object(object: &fhir_types::codec::Object) -> Result<Parameters, Failure> {
                let mut path = Path::root("Parameters");
                Parameters::from_json(object, &mut path)
                    .map_err(|e| Failure::new(StatusCode::BAD_REQUEST, "structure", e.to_string()))
            }

            /// The `400` failure for a `Parameters` that does not fit the operation.
            #[must_use]
            pub fn parameters_failure(error: &ParametersError) -> Failure {
                let code = match error {
                    ParametersError::Missing { .. } => "required",
                    _ => "invalid",
                };
                Failure::new(StatusCode::BAD_REQUEST, code, error.to_string())
            }

            /// A `200` response carrying `parameters` in `wire`.
            ///
            /// # Errors
            ///
            /// Returns a `500` failure when the resource cannot be encoded.
            pub fn respond(parameters: &Parameters, wire: Wire) -> Result<Response, Failure> {
                respond_resource(parameters, wire)
            }

            /// The JSON object of a FHIR resource, for a response or a `Bundle` entry.
            ///
            /// # Errors
            ///
            /// Returns a `500` failure when the resource cannot be encoded.
            pub fn encode<R: Json>(resource: &R) -> Result<fhir_types::codec::Object, Failure> {
                resource.to_json().map_err(|e| {
                    Failure::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "exception",
                        format!("cannot encode the response: {e}"),
                    )
                })
            }

            /// A `200` response carrying any FHIR resource in `wire`.
            ///
            /// An operation whose only output is a resource named `return` answers with
            /// that resource itself (<https://hl7.org/fhir/R4B/operations.html#response>).
            ///
            /// # Errors
            ///
            /// Returns a `500` failure when the resource cannot be encoded.
            pub fn respond_resource<R: Json + serde::Serialize>(
                resource: &R,
                wire: Wire,
            ) -> Result<Response, Failure> {
                wire.resource(StatusCode::OK, resource, &fhir_types::$fhir::schema::SCHEMAS)
            }
        }
    };
}

pub(crate) use parameters;
