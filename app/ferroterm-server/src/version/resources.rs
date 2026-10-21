// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `tx-resource` parameters of one version, converted to the engine's models.

macro_rules! resources {
    ($fhir:ident) => {
        pub mod resources {
            //! `tx-resource` parameters split off a `Parameters` and converted to
            //! the models the request scope layers
            //! (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).

            use fhir_types::$fhir::parameters::Parameters;
            use fhir_types::$fhir::resource::Resource;
            use fhir_terminology::{conceptmap, fhir_codesystem, valueset};
            use http::StatusCode;

            use crate::outcome::Failure;
            use crate::scope::{Loaded, TX_RESOURCE, UUID, Unusable};

            /// Splits the `tx-resource` parameters off `parameters` and converts
            /// each carried resource; a `uuid` parameter is dropped.
            ///
            /// # Errors
            ///
            /// A `tx-resource` without a resource, with a resource of another type
            /// than `CodeSystem`, `ValueSet`, or `ConceptMap`, or with one the
            /// engine cannot serve is a 400.
            pub fn split_resources(
                parameters: Parameters,
            ) -> Result<(Parameters, Vec<Loaded>), Failure> {
                let mut own = Vec::with_capacity(parameters.parameter.len());
                let mut resources = Vec::new();
                for parameter in parameters.parameter {
                    if parameter.name.value.as_deref() == Some(UUID) {
                        continue;
                    }
                    if parameter.name.value.as_deref() == Some(TX_RESOURCE) {
                        let resource = parameter.resource.ok_or_else(|| {
                            Failure::new(
                                StatusCode::BAD_REQUEST,
                                "invalid",
                                format!("`{TX_RESOURCE}` must carry a resource"),
                            )
                        })?;
                        resources.push(loaded(resource)?);
                    } else {
                        own.push(parameter);
                    }
                }
                Ok((
                    Parameters {
                        parameter: own,
                        ..parameters
                    },
                    resources,
                ))
            }

            /// Splits the `tx-resource` parameters off the JSON object of a
            /// `Parameters` and reads each carried resource; a `uuid` parameter is
            /// dropped.
            ///
            /// A supplied resource is read leniently, and one the server still
            /// cannot use is recorded rather than refused: cardinality is an aspect
            /// of validating a resource, which a server performs at its discretion
            /// (<https://hl7.org/fhir/R4B/validation.html>), so a defect in a
            /// resource the request never resolves costs the request nothing.
            ///
            /// # Errors
            ///
            /// A `tx-resource` without a resource is a 400, and so is a body that is
            /// not a `Parameters` once the resources are off it.
            pub fn split_supplied(
                mut object: fhir_types::codec::Object,
            ) -> Result<(Parameters, Vec<Loaded>), Failure> {
                let mut resources = Vec::new();
                let mut kept: Option<Vec<fhir_types::codec::Value>> = None;
                if let Some(fhir_types::codec::Value::Array(sent)) = object.get("parameter") {
                    let mut own = Vec::with_capacity(sent.len());
                    for parameter in sent {
                        match parameter.get("name").and_then(fhir_types::codec::Value::as_str) {
                            Some(UUID) => {}
                            Some(TX_RESOURCE) => resources.push(supplied(parameter)?),
                            _ => own.push(parameter.clone()),
                        }
                    }
                    if own.len() != sent.len() {
                        kept = Some(own);
                    }
                }
                if let Some(kept) = kept {
                    // NOTE: FHIR JSON never writes an empty array
                    // (<https://hl7.org/fhir/R4B/json.html>), so a `Parameters` whose
                    // every parameter was a resource states none at all.
                    if kept.is_empty() {
                        object.remove("parameter");
                    } else {
                        object.insert("parameter".to_owned(), fhir_types::codec::Value::Array(kept));
                    }
                }
                Ok((super::parameters::parameters_from_object(&object)?, resources))
            }

            /// One `tx-resource` parameter read as the model it carries, or the
            /// record of why the server cannot use it.
            fn supplied(parameter: &fhir_types::codec::Value) -> Result<Loaded, Failure> {
                let carried = parameter
                    .get("resource")
                    .and_then(fhir_types::codec::Value::as_object)
                    .ok_or_else(|| {
                        Failure::new(
                            StatusCode::BAD_REQUEST,
                            "invalid",
                            format!("`{TX_RESOURCE}` must carry a resource"),
                        )
                    })?;
                let stated = |name: &str| {
                    carried
                        .get(name)
                        .and_then(fhir_types::codec::Value::as_str)
                        .map(str::to_owned)
                };
                let root = stated("resourceType").unwrap_or_else(|| "Resource".to_owned());
                let mut path = fhir_types::codec::Path::lenient(&root);
                let read = match <Resource as fhir_types::codec::Json>::from_json(carried, &mut path) {
                    // A resource of a type the server never serves is a defect in the
                    // request itself, so it is refused where it is stated.
                    Ok(resource) if !served(&resource) => return Err(unsupported(&resource)),
                    Ok(resource) => loaded(resource),
                    Err(error) => Err(Failure::new(
                        StatusCode::BAD_REQUEST,
                        "invalid",
                        format!("a `{TX_RESOURCE}` cannot be read: {error}"),
                    )
                    .kind("invalid-data")),
                };
                Ok(match read {
                    Ok(loaded) => loaded,
                    Err(failure) => Loaded::Unusable(Unusable {
                        url: stated("url"),
                        failure,
                    }),
                })
            }

            /// The model of a resource given as a JSON object of this version.
            ///
            /// # Errors
            ///
            /// Returns the codec's refusal with its element path, or what the
            /// conversion refused, as text.
            pub fn model_of(
                object: &fhir_types::codec::Object,
            ) -> Result<Loaded, crate::version::ReadError> {
                let resource = resource_of(object).map_err(crate::version::ReadError::Decode)?;
                loaded(resource).map_err(|failure| crate::version::ReadError::Convert(failure.diagnostics))
            }

            /// `object` read as a resource of this version and written back.
            ///
            /// # Errors
            ///
            /// Returns what the codec refused, as text: a resource stored in
            /// another FHIR version can carry an element this one does not define.
            pub fn round_trip(
                object: &fhir_types::codec::Object,
            ) -> Result<fhir_types::codec::Object, String> {
                let resource = resource_of(object).map_err(|error| error.to_string())?;
                fhir_types::codec::Json::to_json(&resource).map_err(|error| error.to_string())
            }

            /// `object` read as a resource of this version.
            ///
            /// # Errors
            ///
            /// Returns what the codec refused, with the element path.
            pub fn resource_of(
                object: &fhir_types::codec::Object,
            ) -> Result<Resource, fhir_types::codec::DecodeError> {
                // The path names the resource type the object states, so an
                // `expression` reads `CodeSystem.date`, not the enum's `Resource`.
                let root = object
                    .get("resourceType")
                    .and_then(fhir_types::codec::Value::as_str)
                    .unwrap_or("Resource");
                let mut path = fhir_types::codec::Path::root(root);
                fhir_types::codec::Json::from_json(object, &mut path)
            }

            fn loaded(resource: Resource) -> Result<Loaded, Failure> {
                match resource {
                    Resource::CodeSystem(code_system) => {
                        fhir_codesystem::convert::$fhir::convert(&code_system)
                            .map(Loaded::CodeSystem)
                            .map_err(invalid)
                    }
                    Resource::ValueSet(value_set) => valueset::convert::$fhir::convert(&value_set)
                        .map(Loaded::ValueSet)
                        .map_err(value_set_refusal),
                    Resource::ConceptMap(concept_map) => {
                        conceptmap::convert::$fhir::convert(&concept_map)
                            .map(Loaded::ConceptMap)
                            .map_err(invalid)
                    }
                    ref other => Err(unsupported(other)),
                }
            }

            /// Whether the server serves resources of this type at all.
            fn served(resource: &Resource) -> bool {
                matches!(
                    resource,
                    Resource::CodeSystem(_) | Resource::ValueSet(_) | Resource::ConceptMap(_)
                )
            }

            /// What a `tx-resource` of a type the server never serves answers with.
            fn unsupported(resource: &Resource) -> Failure {
                Failure::new(
                    StatusCode::BAD_REQUEST,
                    "not-supported",
                    format!(
                        "`{TX_RESOURCE}` carries a {}; only CodeSystem, ValueSet, and ConceptMap resources are accepted",
                        resource_type(resource)
                    ),
                )
            }

            /// What a `ValueSet` the model cannot represent answers with.
            ///
            /// A filter with no value leaves the value set undefined, which the
            /// ecosystem classifies `vs-invalid`
            /// (<https://build.fhir.org/ig/FHIR/fhir-tools-ig/CodeSystem-tx-issue-type.html>);
            /// `422` is the status for a resource that breaks the server's rules
            /// (<https://hl7.org/fhir/R4B/http.html#status-codes>).
            fn value_set_refusal(error: valueset::model::ModelError) -> Failure {
                match error {
                    valueset::model::ModelError::FilterValue { ref expression, .. } => {
                        let expression = expression.clone();
                        Failure::new(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "invalid",
                            error.to_string(),
                        )
                        .kind("vs-invalid")
                        .at(expression)
                    }
                    other => invalid(other),
                }
            }

            fn invalid(error: impl std::fmt::Display) -> Failure {
                Failure::new(
                    StatusCode::BAD_REQUEST,
                    "invalid",
                    format!("a `{TX_RESOURCE}` cannot be served: {error}"),
                )
            }

            fn resource_type(resource: &Resource) -> &'static str {
                match resource {
                    Resource::Bundle(_) => "Bundle",
                    Resource::CapabilityStatement(_) => "CapabilityStatement",
                    Resource::CodeSystem(_) => "CodeSystem",
                    Resource::ConceptMap(_) => "ConceptMap",
                    Resource::OperationOutcome(_) => "OperationOutcome",
                    Resource::Parameters(_) => "Parameters",
                    Resource::TerminologyCapabilities(_) => "TerminologyCapabilities",
                    Resource::ValueSet(_) => "ValueSet",
                    Resource::Unknown(_) => "resource of another type",
                }
            }
        }
    };
}

pub(crate) use resources;
