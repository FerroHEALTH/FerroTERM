// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR surface of one served version, instantiated per version.
//!
//! R4 (4.0.1) and R4B (4.3.0) declare the same terminology operation
//! parameters and the same `CapabilityStatement`, `TerminologyCapabilities`,
//! `ValueSet`, and `Parameters` elements this server fills, so one set of
//! macros produces each version's modules from the generated
//! `fhir_types::<version>` types: `surface!(r4b, "R4B", to_r4b)`
//! expands to `parameters`, `resources`, `map`, `metadata`, `system`,
//! `operations`, and `router()`. The `map` module comes from one macro,
//! `map::family_map!`, whose `$flavour` arms state what the R4 family and the
//! R5 family declare differently.

pub(crate) mod batch;
pub(crate) mod closure;
pub(crate) mod map;
pub(crate) mod metadata;
pub(crate) mod operations;
pub(crate) mod parameters;
pub mod r4;
pub mod r4b;
pub mod r5;
pub mod r6;
pub(crate) mod resources;
pub(crate) mod store;
pub(crate) mod system;

macro_rules! surface {
    // A version whose package declares no `ConceptMap/$closure`: the R6 ballot
    // ships no OperationDefinition for it, so this server offers none.
    ($fhir:ident, $label:literal, $capabilities:ident) => {
        crate::version::surface!(@common $fhir, $label, $capabilities);

        /// This version declares no `$closure`, so its router adds no route for it.
        fn closure_route(
            router: axum::Router<crate::reload::Serving>,
        ) -> axum::Router<crate::reload::Serving> {
            router
        }

        /// The canonical of `$closure`, for a version that declares it; none here.
        pub(crate) const CLOSURE_URL: Option<&str> = None;
    };
    ($fhir:ident, $label:literal, $capabilities:ident, closure) => {
        crate::version::surface!(@common $fhir, $label, $capabilities);
        crate::version::closure::closure!($fhir);

        /// The `$closure` route of a version that declares the operation.
        fn closure_route(
            router: axum::Router<crate::reload::Serving>,
        ) -> axum::Router<crate::reload::Serving> {
            router.route("/$closure", axum::routing::post(closure::closure))
        }

        /// The canonical of `$closure`, which this version declares.
        pub(crate) const CLOSURE_URL: Option<&str> = Some(closure::CLOSURE_URL);
    };
    (@common $fhir:ident, $label:literal, $capabilities:ident) => {
        crate::version::parameters::parameters!($fhir);
        crate::version::resources::resources!($fhir);
        crate::version::metadata::metadata!($fhir, $label, $capabilities);
        crate::version::system::system!($fhir);
        crate::version::store::routes::store!($fhir);
        crate::version::batch::batch!($fhir);
        crate::version::operations::operations!($fhir);

        /// The routes of this version, nested under its root by the crate router.
        pub fn router() -> axum::Router<crate::reload::Serving> {
            use axum::routing::{get, post};
            let router = closure_route(axum::Router::new())
                .route("/", post(batch::batch))
                .route("/metadata", get(metadata::metadata))
                // SMART App Launch puts the discovery document at
                // `[base]/.well-known/smart-configuration`
                // (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
                .route(
                    "/.well-known/smart-configuration",
                    get(crate::smart::configuration),
                )
                .route("/$versions", get(system::versions))
                .route("/$cache-control", post(system::cache_control));
            lookup_instance_route(operation_routes(resource_routes(router)))
        }

        /// The RESTful interactions of the three resource types this server
        /// serves (<https://hl7.org/fhir/R4B/http.html>).
        fn resource_routes(
            router: axum::Router<crate::reload::Serving>,
        ) -> axum::Router<crate::reload::Serving> {
            use axum::routing::get;
            router
                .route(
                    "/CodeSystem",
                    get(store::code_system_search).post(store::code_system_create),
                )
                .route(
                    "/CodeSystem/{id}",
                    get(store::code_system_read)
                        .put(store::code_system_update)
                        .delete(store::code_system_delete),
                )
                .route("/CodeSystem/_history", get(store::code_system_type_history))
                .route("/CodeSystem/{id}/_history", get(store::code_system_history))
                .route(
                    "/CodeSystem/{id}/_history/{version}",
                    get(store::code_system_version_read),
                )
                .route(
                    "/ValueSet",
                    get(store::value_set_search).post(store::value_set_create),
                )
                .route(
                    "/ValueSet/{id}",
                    get(store::value_set_read)
                        .put(store::value_set_update)
                        .delete(store::value_set_delete),
                )
                .route("/ValueSet/_history", get(store::value_set_type_history))
                .route("/ValueSet/{id}/_history", get(store::value_set_history))
                .route(
                    "/ValueSet/{id}/_history/{version}",
                    get(store::value_set_version_read),
                )
                .route(
                    "/ConceptMap",
                    get(store::concept_map_search).post(store::concept_map_create),
                )
                .route(
                    "/ConceptMap/{id}",
                    get(store::concept_map_read)
                        .put(store::concept_map_update)
                        .delete(store::concept_map_delete),
                )
                .route("/ConceptMap/_history", get(store::concept_map_type_history))
                .route("/ConceptMap/{id}/_history", get(store::concept_map_history))
                .route(
                    "/ConceptMap/{id}/_history/{version}",
                    get(store::concept_map_version_read),
                )
        }

        /// The terminology operations, at the levels this version declares.
        ///
        /// The instance form runs the operation on the resource the URL names
        /// (<https://hl7.org/fhir/R4B/operations.html#request>), which every
        /// served version's `OperationDefinition` declares for `$expand` and
        /// `$translate`.
        fn operation_routes(
            router: axum::Router<crate::reload::Serving>,
        ) -> axum::Router<crate::reload::Serving> {
            use axum::routing::{get, post};
            router
                .route(
                    "/CodeSystem/$lookup",
                    get(operations::lookup_get).post(operations::lookup_post),
                )
                .route(
                    "/CodeSystem/$validate-code",
                    get(operations::validate_code_get).post(operations::validate_code_post),
                )
                .route(
                    "/CodeSystem/{id}/$validate-code",
                    get(operations::validate_code_instance_get)
                        .post(operations::validate_code_instance_post),
                )
                .route(
                    "/ValueSet/$expand",
                    get(operations::expand_get).post(operations::expand_post),
                )
                .route(
                    "/ValueSet/{id}/$expand",
                    get(operations::expand_instance_get).post(operations::expand_instance_post),
                )
                .route(
                    "/ValueSet/$validate-code",
                    get(operations::value_set_validate_code_get)
                        .post(operations::value_set_validate_code_post),
                )
                .route(
                    "/ValueSet/{id}/$validate-code",
                    get(operations::value_set_validate_code_instance_get)
                        .post(operations::value_set_validate_code_instance_post),
                )
                .route(
                    "/ValueSet/$batch-validate-code",
                    post(operations::batch_validate_code_post),
                )
                .route(
                    "/CodeSystem/$batch-validate-code",
                    post(operations::batch_validate_code_post),
                )
                .route(
                    "/ConceptMap/$translate",
                    get(operations::translate_get).post(operations::translate_post),
                )
                .route(
                    "/ConceptMap/{id}/$translate",
                    get(operations::translate_instance_get)
                        .post(operations::translate_instance_post),
                )
                .route(
                    "/CodeSystem/$subsumes",
                    get(operations::subsumes_get).post(operations::subsumes_post),
                )
                .route(
                    "/CodeSystem/{id}/$subsumes",
                    get(operations::subsumes_instance_get).post(operations::subsumes_instance_post),
                )
        }

        /// The instance-level `$lookup` route, only where the version's
        /// `OperationDefinition` declares the operation at that level: R5 and
        /// the R6 ballot do, R4 and R4B do not
        /// (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>).
        fn lookup_instance_route(
            router: axum::Router<crate::reload::Serving>,
        ) -> axum::Router<crate::reload::Serving> {
            if fhir_types::$fhir::operations::code_system_lookup::CODE_SYSTEM_LOOKUP.instance {
                router.route(
                    "/CodeSystem/{id}/$lookup",
                    axum::routing::get(operations::lookup_instance_get)
                        .post(operations::lookup_instance_post),
                )
            } else {
                router
            }
        }
    };
}

pub(crate) use surface;
