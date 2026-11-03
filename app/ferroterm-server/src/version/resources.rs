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
            use http::StatusCode;

            use crate::artefact::{Loaded, TX_RESOURCE, Unusable};
            use crate::outcome::Failure;
            use crate::scope::UUID;
            use crate::served::$fhir::{loaded, served, unsupported};

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
                        match parameter
                            .get("name")
                            .and_then(fhir_types::codec::Value::as_str)
                        {
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
                        object.insert(
                            "parameter".to_owned(),
                            fhir_types::codec::Value::Array(kept),
                        );
                    }
                }
                Ok((
                    super::parameters::parameters_from_object(&object)?,
                    resources,
                ))
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
                let read =
                    match <Resource as fhir_types::codec::Json>::from_json(carried, &mut path) {
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
        }
    };
}

pub(crate) use resources;
