// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR versions this server serves, and how a resource of each reads as
//! the engine's model.
//!
//! A persisted resource records the version it arrived in, so a rebuild of
//! the served layer reads it with that version's own codec and converter
//! ([`read`]); the version surfaces read their `tx-resource` parameters the
//! same way. No specification governs this layering: our own design.

use std::sync::LazyLock;

use fhir_terminology::fhir_codesystem::load::FhirVersion;

use crate::artefact::{Loaded, ReadError};

/// The path segment each served version answers under, in the order the
/// crate router nests them.
///
/// FHIR fixes no path for a version, so the prefix is this server's own
/// design; what it produces is one FHIR base per version
/// (<https://hl7.org/fhir/R4B/http.html#root>).
pub const SEGMENTS: [&str; 4] = ["r4", "r4b", "r5", "r6"];

/// The base path of each served version, `/` and its segment: the crate
/// router mounts each, and the boot lines name them.
pub static BASES: LazyLock<[String; 4]> =
    LazyLock::new(|| SEGMENTS.map(|segment| format!("/{segment}")));

/// The terminology each served version defines, by the FHIR version its
/// surface reports.
///
/// A code system the FHIR specification defines belongs to the version that
/// defines it (<https://hl7.org/fhir/R4B/terminologies-systems.html>), so
/// each surface answers over its own package's content.
pub const CORE: [(&str, FhirVersion); 4] = [
    (r4::FHIR_VERSION, FhirVersion::R4),
    (r4b::FHIR_VERSION, FhirVersion::R4B),
    (r5::FHIR_VERSION, FhirVersion::R5),
    (r6::FHIR_VERSION, FhirVersion::R6),
];

/// The FHIR base a client reaches the version served under `segment` at.
///
/// This is the one derivation of a version's base: a capability statement
/// publishes it as `implementation.url`
/// (<https://hl7.org/fhir/R4B/capabilitystatement-definitions.html#CapabilityStatement.implementation.url>),
/// and the SMART gate accepts it as a token audience. A service base carries
/// no trailing slash (<https://hl7.org/fhir/R4B/http.html#root>), so one on
/// `base` is dropped rather than doubled.
#[must_use]
pub fn endpoint(base: &str, segment: &str) -> String {
    let base = base.trim_end_matches('/');
    format!("{base}/{segment}")
}

/// The model of a resource stored as a JSON object of `fhir_version`.
///
/// # Errors
///
/// Returns the codec's refusal with its element path, what the conversion
/// refused as text, and says so when `fhir_version` is not one this server
/// serves.
pub fn read(fhir_version: &str, object: &fhir_types::codec::Object) -> Result<Loaded, ReadError> {
    if fhir_version == r4::FHIR_VERSION {
        r4::model_of(object)
    } else if fhir_version == r4b::FHIR_VERSION {
        r4b::model_of(object)
    } else if fhir_version == r5::FHIR_VERSION {
        r5::model_of(object)
    } else if fhir_version == r6::FHIR_VERSION {
        r6::model_of(object)
    } else {
        Err(ReadError::Convert(format!(
            "`{fhir_version}` is not a FHIR version this server serves"
        )))
    }
}

macro_rules! reading {
    ($fhir:ident, $fhir_version:literal) => {
        pub mod $fhir {
            //! A resource of this version read as the engine's model.

            use fhir_terminology::{conceptmap, fhir_codesystem, valueset};
            use fhir_types::$fhir::resource::Resource;
            use http::StatusCode;

            use crate::artefact::{Loaded, ReadError, TX_RESOURCE};
            use crate::outcome::Failure;

            /// The FHIR version of this module.
            pub const FHIR_VERSION: &str = $fhir_version;

            /// The model of a resource given as a JSON object of this version.
            ///
            /// # Errors
            ///
            /// Returns the codec's refusal with its element path, or what the
            /// conversion refused, as text.
            pub fn model_of(
                object: &fhir_types::codec::Object,
            ) -> Result<Loaded, ReadError> {
                let resource = resource_of(object).map_err(ReadError::Decode)?;
                loaded(resource).map_err(|failure| ReadError::Convert(failure.diagnostics))
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

            /// `resource` as the model the engine serves.
            ///
            /// # Errors
            ///
            /// A resource of a type the server does not serve, or one the
            /// engine's model cannot represent.
            pub fn loaded(resource: Resource) -> Result<Loaded, Failure> {
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
            #[must_use]
            pub fn served(resource: &Resource) -> bool {
                matches!(
                    resource,
                    Resource::CodeSystem(_) | Resource::ValueSet(_) | Resource::ConceptMap(_)
                )
            }

            /// What a `tx-resource` of a type the server never serves answers with.
            #[must_use]
            pub fn unsupported(resource: &Resource) -> Failure {
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

reading!(r4, "4.0.1");
reading!(r4b, "4.3.0");
reading!(r5, "5.0.0");
reading!(r6, "6.0.0-ballot5");
