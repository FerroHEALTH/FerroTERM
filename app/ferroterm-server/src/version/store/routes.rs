// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The persisted-resource routes of one version: `store!` builds each
//! version's `store` module, and `store_routes!` the handlers of one resource
//! type in it, over the shared implementation in `crate::version::store`.

/// The six axum handlers of one resource type, over the shared implementation.
///
/// The `store!` macro invokes this by path, once per type: a `macro_rules!`
/// definition nested inside another one cannot spell its own metavariables
/// (<https://doc.rust-lang.org/reference/macros-by-example.html>).
macro_rules! store_routes {
    ($create:ident, $read:ident, $update:ident, $version_read:ident, $delete:ident, $search:ident, $history:ident, $type_history:ident, $kind:expr) => {
        crate::version::store::routes::store_routes!(@fn
            /// `GET {id}/_history`: every version of the resource, newest first.
            $history, [id id uri uri], |state, headers, query, wire| {
            let asked = Asked { query: &query, headers: &headers, uri: &uri };
            history(&state, $kind, Some(&id), &asked, wire)
        });

        crate::version::store::routes::store_routes!(@fn
            /// `GET _history`: every version of every resource of the type,
            /// newest first.
            $type_history, [uri uri], |state, headers, query, wire| {
            let asked = Asked { query: &query, headers: &headers, uri: &uri };
            history(&state, $kind, None, &asked, wire)
        });

        crate::version::store::routes::store_routes!(@fn
            /// `POST`: stores the body under a server-assigned id.
            $create, [uri uri body body], |state, headers, query, wire| {
            let request = request(&state, $kind, &headers, uri.path(), wire);
            crate::version::store::create(&request, &body)
        });

        crate::version::store::routes::store_routes!(@fn
            /// `GET {id}`: the current version of the resource.
            $read, [id id], |state, headers, query, wire| {
            let request = request(&state, $kind, &headers, "", wire);
            crate::elements::Projection::of_query(&query, crate::elements::Interaction::Read)
                .and_then(|projection| crate::version::store::read(&request, &id, &projection))
        });

        crate::version::store::routes::store_routes!(@fn
            /// `PUT {id}`: stores the body as `id`.
            $update, [id id uri uri body body], |state, headers, query, wire| {
            let path = uri
                .path()
                .rsplit_once('/')
                .map_or_else(|| uri.path().to_owned(), |(head, _)| head.to_owned());
            let request = request(&state, $kind, &headers, &path, wire);
            crate::version::store::update(&request, &id, &body)
        });

        crate::version::store::routes::store_routes!(@fn
            /// `GET {id}/_history/{version}`: one version of the resource.
            $version_read, [id id version version], |state, headers, query, wire| {
            let request = request(&state, $kind, &headers, "", wire);
            crate::version::store::version_read(&request, &id, &version)
        });

        crate::version::store::routes::store_routes!(@fn
            /// `DELETE {id}`: removes the current version.
            $delete, [id id], |state, headers, query, wire| {
            let request = request(&state, $kind, &headers, "", wire);
            crate::version::store::delete(&request, &id)
        });

        crate::version::store::routes::store_routes!(@fn
            /// `GET ?url=&version=`: a `searchset` of the resources this server
            /// holds, the loaded ones beside the persisted ones.
            $search, [uri uri], |state, headers, query, wire| {
            search(&state, $kind, &query, &headers, &uri, wire)
        });
    };
    // The extractors of each handler shape, bound to the names the handler
    // passes; the body, when there is one, is the last.
    (@fn $(#[$doc:meta])* $name:ident, [uri $uri:ident], $($rest:tt)*) => {
        crate::version::store::routes::store_routes!(@with $(#[$doc])* $name,
            [axum::extract::OriginalUri($uri): axum::extract::OriginalUri,] [], $($rest)*);
    };
    (@fn $(#[$doc:meta])* $name:ident, [uri $uri:ident body $bytes:ident], $($rest:tt)*) => {
        crate::version::store::routes::store_routes!(@with $(#[$doc])* $name,
            [axum::extract::OriginalUri($uri): axum::extract::OriginalUri,] [$bytes: axum::body::Bytes,], $($rest)*);
    };
    (@fn $(#[$doc:meta])* $name:ident, [id $id:ident], $($rest:tt)*) => {
        crate::version::store::routes::store_routes!(@with $(#[$doc])* $name,
            [axum::extract::Path($id): axum::extract::Path<String>,] [], $($rest)*);
    };
    (@fn $(#[$doc:meta])* $name:ident, [id $id:ident uri $uri:ident], $($rest:tt)*) => {
        crate::version::store::routes::store_routes!(@with $(#[$doc])* $name,
            [axum::extract::Path($id): axum::extract::Path<String>, axum::extract::OriginalUri($uri): axum::extract::OriginalUri,] [], $($rest)*);
    };
    (@fn $(#[$doc:meta])* $name:ident, [id $id:ident uri $uri:ident body $bytes:ident], $($rest:tt)*) => {
        crate::version::store::routes::store_routes!(@with $(#[$doc])* $name,
            [axum::extract::Path($id): axum::extract::Path<String>, axum::extract::OriginalUri($uri): axum::extract::OriginalUri,] [$bytes: axum::body::Bytes,], $($rest)*);
    };
    (@fn $(#[$doc:meta])* $name:ident, [id $id:ident version $version:ident], $($rest:tt)*) => {
        crate::version::store::routes::store_routes!(@with $(#[$doc])* $name,
            [axum::extract::Path(($id, $version)): axum::extract::Path<(String, String)>,] [], $($rest)*);
    };
    // One handler: the format negotiated once, and the answer or failure
    // rendered in it.
    (@with $(#[$doc:meta])* $name:ident, [$($before:tt)*] [$($after:tt)*], |$state:ident, $headers:ident, $query:ident, $wire:ident| $body:block) => {
        $(#[$doc])*
        pub async fn $name(
            axum::extract::State($state): axum::extract::State<std::sync::Arc<crate::state::AppState>>,
            $($before)*
            $headers: http::HeaderMap,
            axum::extract::Query($query): axum::extract::Query<Vec<(String, String)>>,
            $($after)*
        ) -> axum::response::Response {
            match Wire::negotiate(&$query, &$headers) {
                Ok($wire) => finish($body, $wire),
                Err(failure) => failure.into_response(),
            }
        }
    };
}

pub(crate) use store_routes;

macro_rules! store {
    ($fhir:ident) => {
        pub mod store {
            //! The persisted-resource routes of one version, over the shared
            //! implementation in `crate::version::store`.

            use axum::response::{IntoResponse, Response};
            use fhir_types::$fhir::bundle::{
                Bundle, BundleEntry, BundleEntryRequest, BundleEntryResponse, BundleEntrySearch,
                BundleLink,
            };
            use fhir_types::$fhir::codeable_concept::CodeableConcept;
            use fhir_types::$fhir::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
            use fhir_types::$fhir::resource::Resource;
            use http::{HeaderMap, StatusCode};

            use crate::outcome::Failure;
            use crate::persistence::{HistoryEntry, Method, ResourceType};
            use crate::state::AppState;
            use crate::version::store::loaded::{
                base_of, loaded_code_systems, loaded_matching, loaded_supplements,
            };
            use crate::version::store::{Request, Surface};
            use crate::wire::Wire;

            use super::metadata::FHIR_VERSION;
            use super::parameters;

            /// The codec of this version, for the shared implementation.
            fn surface() -> Surface {
                Surface {
                    fhir_version: FHIR_VERSION,
                    segment: stringify!($fhir),
                    schemas: &fhir_types::$fhir::schema::SCHEMAS,
                    round_trip: super::resources::round_trip,
                    render_value_set: |model, id| {
                        fhir_types::codec::Json::to_json(&value_set(model, id))
                            .map_err(|error| error.to_string())
                    },
                    render_code_system: |model, id| {
                        fhir_types::codec::Json::to_json(&code_system(model, id))
                            .map_err(|error| error.to_string())
                    },
                    render_concept_map: |model, id| {
                        fhir_types::codec::Json::to_json(&concept_map(model, id))
                            .map_err(|error| error.to_string())
                    },
                }
            }

            /// A loaded code system as this version's `CodeSystem`, under the
            /// instance id the server addresses it by.
            fn code_system(
                model: &fhir_terminology::provider::code_system::CodeSystemModel,
                id: &str,
            ) -> fhir_types::$fhir::code_system::CodeSystem {
                let mut resource =
                    fhir_terminology::fhir_codesystem::render::$fhir::code_system(model);
                resource.id = Some(id.to_owned());
                resource
            }

            /// A loaded value set as this version's `ValueSet`, under the
            /// instance id the server addresses it by.
            ///
            /// `Resource.id` is the logical id the resource is read by, so a
            /// client can address what a search returned
            /// (<https://hl7.org/fhir/R4B/resource.html#id>).
            fn value_set(
                model: &fhir_terminology::valueset::model::ValueSetModel,
                id: &str,
            ) -> fhir_types::$fhir::value_set::ValueSet {
                let mut resource =
                    fhir_terminology::valueset::render::$fhir::value_set(model, true);
                resource.id = Some(id.to_owned());
                resource
            }

            /// A loaded concept map as this version's `ConceptMap`, under the
            /// instance id the server addresses it by.
            fn concept_map(
                model: &fhir_terminology::provider::concept_map::ConceptMapModel,
                id: &str,
            ) -> fhir_types::$fhir::concept_map::ConceptMap {
                let mut resource = fhir_terminology::conceptmap::render::$fhir::concept_map(model);
                resource.id = Some(id.to_owned());
                resource
            }

            /// One request of the routes of `resource_type` on this version.
            fn request<'a>(
                state: &'a AppState,
                resource_type: ResourceType,
                headers: &'a HeaderMap,
                path: &'a str,
                wire: Wire,
            ) -> Request<'a> {
                Request {
                    state,
                    surface: surface(),
                    resource_type,
                    headers,
                    path,
                    wire,
                }
            }

            fn finish(handled: Result<Response, Failure>, wire: Wire) -> Response {
                match handled {
                    Ok(response) => response,
                    Err(failure) => failure.respond(wire),
                }
            }

            /// One `match` entry of a `searchset`, at the absolute URL of the
            /// resource when the base this server is reached at is known
            /// (<https://hl7.org/fhir/R4B/bundle.html#bundle-unique>).
            fn found(full_url: Option<String>, resource: Resource) -> BundleEntry {
                BundleEntry {
                    full_url: full_url.map(Into::into),
                    resource: Some(resource),
                    search: Some(BundleEntrySearch {
                        mode: Some("match".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            }

            /// The `outcome` entry naming the resources this server holds and
            /// this version has no representation for.
            ///
            /// A searchset may carry an `OperationOutcome` about the search
            /// beside its matches, with no issue of `fatal` or `error`
            /// severity and `search.mode` of `outcome`
            /// (<https://hl7.org/fhir/R4B/http.html#search>). R5 and the R6
            /// ballot also define `Bundle.issues` for this content, and both
            /// keep the entry form on that same page, so one shape serves
            /// every served version.
            fn outcome(issue: Vec<OperationOutcomeIssue>) -> BundleEntry {
                // NOTE: R5 `bdl-15` requires a `fullUrl` on every searchset entry, and a
                // resource with no persistent identity takes a UUID
                // (<https://hl7.org/fhir/R5/bundle.html>).
                let full_url = format!("urn:uuid:{}", uuid::Uuid::new_v4());
                BundleEntry {
                    full_url: Some(full_url.into()),
                    resource: Some(Resource::OperationOutcome(Box::new(OperationOutcome {
                        issue,
                        ..Default::default()
                    }))),
                    search: Some(BundleEntrySearch {
                        mode: Some("outcome".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            }

            /// The `self` link of a searchset: the URL the search was made at.
            ///
            /// R5 `bdl-18` requires it on every searchset
            /// (<https://hl7.org/fhir/R5/bundle.html>), and `self` is the
            /// relation the search page gives that link
            /// (<https://hl7.org/fhir/R4B/http.html#paging>).
            fn self_link(
                base: Option<&String>,
                resource_type: ResourceType,
                uri: &http::Uri,
            ) -> Vec<BundleLink> {
                link_at(
                    base.map(|base| format!("{base}/{}", resource_type.name())),
                    uri,
                )
            }

            /// The `self` link at `path`, with the query the request was made with; none
            /// when the base this server is reached at is not known.
            fn link_at(path: Option<String>, uri: &http::Uri) -> Vec<BundleLink> {
                let Some(path) = path else {
                    return Vec::new();
                };
                let url = match uri.query().filter(|query| !query.is_empty()) {
                    Some(query) => format!("{path}?{query}"),
                    None => path,
                };
                vec![BundleLink {
                    relation: "self".into(),
                    url: url.into(),
                    ..Default::default()
                }]
            }

            /// One issue of that entry: the resource left out, and why.
            fn left_out(text: &str) -> OperationOutcomeIssue {
                OperationOutcomeIssue {
                    severity: "warning".into(),
                    code: "not-supported".into(),
                    details: Some(CodeableConcept {
                        text: Some(text.into()),
                        ..Default::default()
                    }),
                    diagnostics: Some(text.into()),
                    ..Default::default()
                }
            }

            /// The `searchset` of the resources of `resource_type` that
            /// `query` matches, the loaded ones before the persisted ones.
            ///
            /// A persisted resource this version has no representation for is
            /// left out of the matches and named in one `outcome` entry, so a
            /// resource written on another base never fails the whole search
            /// (<https://hl7.org/fhir/R4B/http.html#search>); `total` counts
            /// the matches (<https://hl7.org/fhir/R4B/bundle.html#searchset>).
            ///
            /// # Errors
            ///
            /// A search parameter this server does not answer is a 400, and a
            /// loaded resource this version cannot render is a 500.
            pub fn search(
                state: &AppState,
                resource_type: ResourceType,
                query: &[(String, String)],
                headers: &HeaderMap,
                uri: &http::Uri,
                wire: Wire,
            ) -> Result<Response, Failure> {
                let surface = surface();
                let projection = crate::elements::Projection::of_query(
                    query,
                    crate::elements::Interaction::Search,
                )?;
                let base = base_of(state, surface.segment, headers, uri);
                let at = |id: &str| {
                    base.as_ref()
                        .map(|base| format!("{base}/{}/{id}", resource_type.name()))
                };
                let mut entry = Vec::new();
                let loaded = |instances| loaded_matching(state, resource_type, instances, query);
                if resource_type == ResourceType::CodeSystem {
                    for (id, provider) in loaded_code_systems(state, query)? {
                        let model = fhir_terminology::provider::code_system::described(&*provider);
                        entry.push(found(
                            at(&id),
                            Resource::CodeSystem(Box::new(code_system(&model, &id))),
                        ));
                    }
                    for (id, model) in loaded_supplements(state, query)? {
                        entry.push(found(
                            at(&id),
                            Resource::CodeSystem(Box::new(code_system(&model, &id))),
                        ));
                    }
                }
                if resource_type == ResourceType::ValueSet {
                    for id in loaded(state.value_set_instances())? {
                        let Some(model) = state.value_set_instance(&id) else {
                            continue;
                        };
                        entry.push(found(
                            at(&id),
                            Resource::ValueSet(Box::new(value_set(&model, &id))),
                        ));
                    }
                }
                if resource_type == ResourceType::ConceptMap {
                    for id in loaded(state.concept_map_instances())? {
                        let Some(model) = state.concept_map_instance(&id) else {
                            continue;
                        };
                        entry.push(found(
                            at(&id),
                            Resource::ConceptMap(Box::new(concept_map(&model, &id))),
                        ));
                    }
                }
                let mut unreadable = Vec::new();
                for record in crate::version::store::matches(state, resource_type, query)? {
                    let object = match (surface.round_trip)(&record.resource) {
                        Ok(object) => object,
                        Err(reason) => {
                            unreadable.push(left_out(&crate::version::store::unreadable(
                                &record,
                                surface.fhir_version,
                                &reason,
                            )));
                            continue;
                        }
                    };
                    let resource = super::resources::resource_of(&object)
                        .map_err(|reason| crate::version::store::rendering(&reason.to_string()))?;
                    entry.push(found(at(&record.id), resource));
                }
                let total = u32::try_from(entry.len()).map_err(|_| {
                    Failure::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "too-costly",
                        "too many resources to count",
                    )
                })?;
                // NOTE: `_summary=count` returns the count of the matches without the
                // matches (<https://hl7.org/fhir/R4B/search.html#summary>).
                if projection.is_count() {
                    entry.clear();
                }
                if !unreadable.is_empty() {
                    entry.push(outcome(unreadable));
                }
                let bundle = Bundle {
                    r#type: "searchset".into(),
                    link: self_link(base.as_ref(), resource_type, uri),
                    total: Some(total.into()),
                    entry,
                    ..Default::default()
                };
                if projection.is_whole() || projection.is_count() {
                    return parameters::respond_resource(&bundle, wire);
                }
                // A subset of a resource is not a resource, so it has no typed
                // form to build: the projection is over the wire object, which
                // is also what lets one implementation serve all four versions.
                let mut object = fhir_types::codec::Json::to_json(&bundle)
                    .map_err(|reason| crate::version::store::rendering(&reason.to_string()))?;
                projection.apply_bundle(&mut object, surface.schemas);
                wire.object(StatusCode::OK, &object, surface.schemas)
            }

            /// What a history request asked, beside the resource it names.
            struct Asked<'a> {
                /// The query parameters.
                query: &'a [(String, String)],
                /// The request headers, for the base the client used.
                headers: &'a HeaderMap,
                /// The request URI, for the base and the `self` link.
                uri: &'a http::Uri,
            }

            /// The `history` Bundle of the resource `id` names, or of every
            /// resource of `resource_type` when `id` is `None`.
            ///
            /// Each version is one entry, newest first, with `entry.request`
            /// naming the interaction that made it and `entry.response` its
            /// status, `ETag`, and `lastModified`; a delete carries no resource
            /// (<https://hl7.org/fhir/R4B/http.html#history>). The entries,
            /// the absolute `fullUrl`, and the `self` link are the same shape
            /// on every served version.
            ///
            /// # Errors
            ///
            /// The refusals of [`crate::version::store::history_entries`], and
            /// a 500 for a version this version decodes and cannot encode.
            fn history(
                state: &AppState,
                resource_type: ResourceType,
                id: Option<&str>,
                asked: &Asked<'_>,
                wire: Wire,
            ) -> Result<Response, Failure> {
                let surface = surface();
                let request = Request {
                    state,
                    surface,
                    resource_type,
                    headers: asked.headers,
                    path: "",
                    wire,
                };
                let versions = crate::version::store::history_entries(&request, id, asked.query)?;
                let base = base_of(state, surface.segment, asked.headers, asked.uri);
                let mut entry = Vec::with_capacity(versions.len());
                for version in &versions {
                    entry.push(history_entry(base.as_deref(), resource_type, version)?);
                }
                let total = u32::try_from(entry.len()).map_err(|_| {
                    Failure::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "too-costly",
                        "too many versions to count",
                    )
                })?;
                let bundle = Bundle {
                    r#type: "history".into(),
                    link: history_link(base.as_deref(), resource_type, id, asked.uri),
                    total: Some(total.into()),
                    entry,
                    ..Default::default()
                };
                parameters::respond_resource(&bundle, wire)
            }

            /// One version as a history entry.
            fn history_entry(
                base: Option<&str>,
                resource_type: ResourceType,
                version: &HistoryEntry,
            ) -> Result<BundleEntry, Failure> {
                let surface = surface();
                let name = resource_type.name();
                let status = match (version.method, version.created) {
                    (Method::Delete, _) => "204 No Content",
                    (_, true) => "201 Created",
                    (_, false) => "200 OK",
                };
                let mut response = BundleEntryResponse {
                    status: status.into(),
                    etag: Some(version.etag().into()),
                    last_modified: Some(version.last_modified.as_str().into()),
                    ..Default::default()
                };
                let mut request = Some(BundleEntryRequest {
                    method: version.method.name().into(),
                    url: match version.method {
                        Method::Post => name.to_owned(),
                        Method::Put | Method::Delete => format!("{name}/{}", version.id),
                    }
                    .into(),
                    ..Default::default()
                });
                let mut resource = None;
                if let Some(record) = &version.record {
                    match (surface.round_trip)(&record.resource) {
                        Ok(object) => {
                            resource =
                                Some(super::resources::resource_of(&object).map_err(|reason| {
                                    crate::version::store::rendering(&reason.to_string())
                                })?);
                        }
                        // NOTE: a `PUT` or `POST` entry SHALL carry its resource
                        // (<https://hl7.org/fhir/R4B/http.html#history>), so a version this release cannot read
                        // keeps its `response`, with an `outcome` saying why, and no `request`.
                        Err(reason) => {
                            request = None;
                            response.outcome =
                                Some(Resource::OperationOutcome(Box::new(OperationOutcome {
                                    issue: vec![left_out(&crate::version::store::unreadable(
                                        record,
                                        surface.fhir_version,
                                        &reason,
                                    ))],
                                    ..Default::default()
                                })));
                        }
                    }
                }
                Ok(BundleEntry {
                    full_url: base.map(|base| format!("{base}/{name}/{}", version.id).into()),
                    resource,
                    request,
                    response: Some(response),
                    ..Default::default()
                })
            }

            /// The `self` link of a history Bundle: the URL it was read at
            /// (<https://hl7.org/fhir/R4B/http.html#paging>).
            fn history_link(
                base: Option<&str>,
                resource_type: ResourceType,
                id: Option<&str>,
                uri: &http::Uri,
            ) -> Vec<BundleLink> {
                let path = base.map(|base| match id {
                    Some(id) => format!("{base}/{}/{id}/_history", resource_type.name()),
                    None => format!("{base}/{}/_history", resource_type.name()),
                });
                link_at(path, uri)
            }

            crate::version::store::routes::store_routes!(
                code_system_create,
                code_system_read,
                code_system_update,
                code_system_version_read,
                code_system_delete,
                code_system_search,
                code_system_history,
                code_system_type_history,
                ResourceType::CodeSystem
            );
            crate::version::store::routes::store_routes!(
                value_set_create,
                value_set_read,
                value_set_update,
                value_set_version_read,
                value_set_delete,
                value_set_search,
                value_set_history,
                value_set_type_history,
                ResourceType::ValueSet
            );
            crate::version::store::routes::store_routes!(
                concept_map_create,
                concept_map_read,
                concept_map_update,
                concept_map_version_read,
                concept_map_delete,
                concept_map_search,
                concept_map_history,
                concept_map_type_history,
                ResourceType::ConceptMap
            );
        }
    };
}

pub(crate) use store;
