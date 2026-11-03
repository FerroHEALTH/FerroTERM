// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HTTP handlers of the operations of one version: GET and POST, type and
//! instance level, each one route.
//!
//! `operations!` expands [`handlers`] inside the version's `operations`
//! module, so every handler reads that module's runners and imports.

/// The route handlers and the instance runners they share.
macro_rules! handlers {
    () => {
            /// The inputs an instance-level invocation reads: from the query or from
            /// the body.
            type Sent = Result<(Scope, Parameters), Failure>;

            /// Runs a `CodeSystem` operation on the instance `invocation` names, once
            /// it resolves, with the inputs `sent` reads.
            fn on_code_system(
                invocation: Result<Invocation, Failure>,
                sent: impl FnOnce() -> Sent,
                run: impl FnOnce(&Scope, &Invocation, &Parameters) -> Handled,
            ) -> Handled {
                let invocation = invocation?;
                let (scope, parameters) = sent()?;
                run(&scope, &invocation, &parameters)
            }

            /// Runs a `ValueSet` operation on the instance `bound` names, once it
            /// resolves, with the inputs `sent` reads.
            fn on_value_set(
                bound: Result<Bound, Failure>,
                sent: impl FnOnce() -> Sent,
                run: impl FnOnce(&Scope, Option<&Bound>, &Parameters, &Origin) -> Handled,
            ) -> Handled {
                let bound = bound?;
                let (scope, parameters) = sent()?;
                run(&scope, Some(&bound), &parameters, &Origin::Unplaced)
            }

            /// Runs `$translate` on the concept map `bound` names, once it resolves,
            /// with the inputs `sent` reads.
            fn on_concept_map(bound: Result<Bound, Failure>, sent: impl FnOnce() -> Sent) -> Handled {
                let bound = bound?;
                let (scope, parameters) = sent()?;
                run_translate(&scope, Some(&bound), &parameters)
            }

            /// The response of a handled invocation, a failure rendered in `wire`.
            fn finish(handled: Handled, wire: Wire) -> Response {
                match handled {
                    Ok(answer) => answer.respond(wire),
                    Err(failure) => failure.respond(wire),
                }
            }

            /// The response to one request: the format negotiated, `handle` run on the
            /// query less `_format`, and its answer or failure rendered in that format.
            /// A format the server does not speak is refused in JSON.
            fn respond_to(
                headers: &HeaderMap,
                query: &[(String, String)],
                handle: impl FnOnce(&[(String, String)]) -> Handled,
            ) -> Response {
                match Wire::negotiate(query, headers) {
                    Ok(wire) => finish(handle(&without_format(query)), wire),
                    Err(failure) => failure.into_response(),
                }
            }

            $crate::version::operations::handlers::handler!(get
                /// `GET /CodeSystem/$lookup`.
                lookup_get, |state, headers, query| {
                from_query(&state, &CODE_SYSTEM_LOOKUP, &headers, query)
                    .and_then(|(scope, p)| run_lookup(&scope, &Invocation::Type, &p))
            });

            $crate::version::operations::handlers::handler!(post
                /// `POST /CodeSystem/$lookup`.
                lookup_post, |state, headers, body| {
                from_body(&state, &CODE_SYSTEM_LOOKUP, &headers, &body)
                    .and_then(|(scope, p)| run_lookup(&scope, &Invocation::Type, &p))
            });

            $crate::version::operations::handlers::handler!(get_instance
                /// `GET /CodeSystem/{id}/$lookup`, where the version declares it.
                lookup_instance_get, |state, id, headers, query| {
                on_code_system(instance(&state, &id), || from_query(&state, &CODE_SYSTEM_LOOKUP, &headers, query), run_lookup)
            });

            $crate::version::operations::handlers::handler!(post_instance
                /// `POST /CodeSystem/{id}/$lookup`, where the version declares it.
                lookup_instance_post, |state, id, headers, body| {
                on_code_system(instance(&state, &id), || from_body(&state, &CODE_SYSTEM_LOOKUP, &headers, &body), run_lookup)
            });

            $crate::version::operations::handlers::handler!(get
                /// `GET /CodeSystem/$validate-code`.
                validate_code_get, |state, headers, query| {
                from_query(&state, &CODE_SYSTEM_VALIDATE_CODE, &headers, query)
                    .and_then(|(scope, p)| run_validate_code(&scope, &Invocation::Type, &p))
            });

            $crate::version::operations::handlers::handler!(post
                /// `POST /CodeSystem/$validate-code`.
                validate_code_post, |state, headers, body| {
                from_body(&state, &CODE_SYSTEM_VALIDATE_CODE, &headers, &body)
                    .and_then(|(scope, p)| run_validate_code(&scope, &Invocation::Type, &p))
            });

            $crate::version::operations::handlers::handler!(get_instance
                /// `GET /CodeSystem/{id}/$validate-code`, where the version declares it.
                validate_code_instance_get, |state, id, headers, query| {
                on_code_system(instance(&state, &id), || from_query(&state, &CODE_SYSTEM_VALIDATE_CODE, &headers, query), run_validate_code)
            });

            $crate::version::operations::handlers::handler!(post_instance
                /// `POST /CodeSystem/{id}/$validate-code`, where the version declares it.
                validate_code_instance_post, |state, id, headers, body| {
                on_code_system(instance(&state, &id), || from_body(&state, &CODE_SYSTEM_VALIDATE_CODE, &headers, &body), run_validate_code)
            });

            $crate::version::operations::handlers::handler!(get
                /// `GET /CodeSystem/$subsumes`.
                subsumes_get, |state, headers, query| {
                from_query(&state, &CODE_SYSTEM_SUBSUMES, &headers, query)
                    .and_then(|(scope, p)| run_subsumes(&scope, &Invocation::Type, &p))
            });

            $crate::version::operations::handlers::handler!(post
                /// `POST /CodeSystem/$subsumes`.
                subsumes_post, |state, headers, body| {
                from_body(&state, &CODE_SYSTEM_SUBSUMES, &headers, &body)
                    .and_then(|(scope, p)| run_subsumes(&scope, &Invocation::Type, &p))
            });

            $crate::version::operations::handlers::handler!(get_instance
                /// `GET /CodeSystem/{id}/$subsumes`, where the version declares it.
                subsumes_instance_get, |state, id, headers, query| {
                on_code_system(instance(&state, &id), || from_query(&state, &CODE_SYSTEM_SUBSUMES, &headers, query), run_subsumes)
            });

            $crate::version::operations::handlers::handler!(post_instance
                /// `POST /CodeSystem/{id}/$subsumes`, where the version declares it.
                subsumes_instance_post, |state, id, headers, body| {
                on_code_system(instance(&state, &id), || from_body(&state, &CODE_SYSTEM_SUBSUMES, &headers, &body), run_subsumes)
            });

            $crate::version::operations::handlers::handler!(get_raw
                /// `GET /ValueSet/$expand`.
                expand_get, |state, headers, raw, query| {
                let origin = Origin::Query(raw.unwrap_or_default());
                from_query(&state, &VALUE_SET_EXPAND, &headers, query)
                    .and_then(|(scope, p)| run_expand(&scope, None, &p, &origin))
            });

            $crate::version::operations::handlers::handler!(post
                /// `POST /ValueSet/$expand`.
                expand_post, |state, headers, body| {
                from_body_placed(&state, &VALUE_SET_EXPAND, &[], &headers, &body)
                    .and_then(|(scope, p, origin)| run_expand(&scope, None, &p, &origin))
            });

            $crate::version::operations::handlers::handler!(get_instance
                /// `GET /ValueSet/{id}/$expand`.
                expand_instance_get, |state, id, headers, query| {
                on_value_set(value_set_instance(&state, &id), || from_query(&state, &VALUE_SET_EXPAND, &headers, query), run_expand)
            });

            $crate::version::operations::handlers::handler!(post_instance
                /// `POST /ValueSet/{id}/$expand`.
                expand_instance_post, |state, id, headers, body| {
                on_value_set(value_set_instance(&state, &id), || from_body(&state, &VALUE_SET_EXPAND, &headers, &body), run_expand)
            });

            $crate::version::operations::handlers::handler!(get_raw
                /// `GET /ValueSet/$validate-code`.
                value_set_validate_code_get, |state, headers, raw, query| {
                let origin = Origin::Query(raw.unwrap_or_default());
                from_query(&state, &VALUE_SET_VALIDATE_CODE, &headers, query).and_then(
                    |(scope, p)| run_value_set_validate_code(&scope, None, &p, &origin),
                )
            });

            $crate::version::operations::handlers::handler!(post
                /// `POST /ValueSet/$validate-code`.
                value_set_validate_code_post, |state, headers, body| {
                from_body_placed(&state, &VALUE_SET_VALIDATE_CODE, &[], &headers, &body).and_then(
                    |(scope, p, origin)| run_value_set_validate_code(&scope, None, &p, &origin),
                )
            });

            $crate::version::operations::handlers::handler!(get_instance
                /// `GET /ValueSet/{id}/$validate-code`.
                value_set_validate_code_instance_get, |state, id, headers, query| {
                on_value_set(value_set_instance(&state, &id), || from_query(&state, &VALUE_SET_VALIDATE_CODE, &headers, query), run_value_set_validate_code)
            });

            $crate::version::operations::handlers::handler!(post_instance
                /// `POST /ValueSet/{id}/$validate-code`.
                value_set_validate_code_instance_post, |state, id, headers, body| {
                on_value_set(value_set_instance(&state, &id), || from_body(&state, &VALUE_SET_VALIDATE_CODE, &headers, &body), run_value_set_validate_code)
            });

            $crate::version::operations::handlers::handler!(post
                /// `POST /ValueSet/$batch-validate-code` and `POST /CodeSystem/$batch-validate-code`.
                ///
                /// The operation changes nothing, but it carries its validations in a body,
                /// so it is offered on `POST` alone
                /// (<https://hl7.org/fhir/R4B/operations.html#executing>).
                batch_validate_code_post, |state, headers, body| {
                from_body_accepting(&state, &VALUE_SET_VALIDATE_CODE, &[VALIDATION], &headers, &body)
                    .and_then(|(scope, p)| run_batch_validate(&scope, &p))
            });

            $crate::version::operations::handlers::handler!(get
                /// `GET /ConceptMap/$translate`.
                translate_get, |state, headers, query| {
                reverse_refusal(query.iter().map(|(name, _)| name.as_str()))
                    .and_then(|()| from_query(&state, &CONCEPT_MAP_TRANSLATE, &headers, query))
                    .and_then(|(scope, p)| run_translate(&scope, None, &p))
            });

            $crate::version::operations::handlers::handler!(get_instance
                /// `GET /ConceptMap/{id}/$translate`.
                translate_instance_get, |state, id, headers, query| {
                on_concept_map(concept_map_instance(&state, &id), || {
                    reverse_refusal(query.iter().map(|(name, _)| name.as_str()))
                        .and_then(|()| from_query(&state, &CONCEPT_MAP_TRANSLATE, &headers, query))
                })
            });

            // `reverse` reaches `run_translate`, which refuses it with the ecosystem's
            // own wording on a version that does not declare it; the GET route does the
            // same before decoding.
            $crate::version::operations::handlers::handler!(post
                /// `POST /ConceptMap/$translate`.
                translate_post, |state, headers, body| {
                from_body_accepting(&state, &CONCEPT_MAP_TRANSLATE, &["reverse"], &headers, &body)
                    .and_then(|(scope, p)| run_translate(&scope, None, &p))
            });

            $crate::version::operations::handlers::handler!(post_instance
                /// `POST /ConceptMap/{id}/$translate`.
                translate_instance_post, |state, id, headers, body| {
                on_concept_map(concept_map_instance(&state, &id), || {
                    from_body_accepting(&state, &CONCEPT_MAP_TRANSLATE, &["reverse"], &headers, &body)
                })
            });
    };
}

/// One handler of an operation route, by the extractors it takes: `get` and
/// `get_raw` (with the raw query a placed origin keeps), `post`, and their
/// instance forms. Each answers through `respond_to`, so a format is negotiated,
/// and a failure rendered, once.
macro_rules! handler {
    (get $(#[$doc:meta])* $name:ident, |$state:ident, $headers:ident, $query:ident| $body:block) => {
        $crate::version::operations::handlers::handler!(@fn $(#[$doc])* $name, $headers, |$query| $body,
            [State($state): State<Arc<AppState>>, $headers: HeaderMap,] []
        );
    };
    (get_raw $(#[$doc:meta])* $name:ident, |$state:ident, $headers:ident, $raw:ident, $query:ident| $body:block) => {
        $crate::version::operations::handlers::handler!(@fn $(#[$doc])* $name, $headers, |$query| $body,
            [State($state): State<Arc<AppState>>, $headers: HeaderMap, RawQuery($raw): RawQuery,] []
        );
    };
    (post $(#[$doc:meta])* $name:ident, |$state:ident, $headers:ident, $bytes:ident| $body:block) => {
        $crate::version::operations::handlers::handler!(@fn $(#[$doc])* $name, $headers, |_| $body,
            [State($state): State<Arc<AppState>>, $headers: HeaderMap,] [$bytes: Bytes,]
        );
    };
    (get_instance $(#[$doc:meta])* $name:ident, |$state:ident, $id:ident, $headers:ident, $query:ident| $body:block) => {
        $crate::version::operations::handlers::handler!(@fn $(#[$doc])* $name, $headers, |$query| $body,
            [State($state): State<Arc<AppState>>, UrlPath($id): UrlPath<String>, $headers: HeaderMap,] []
        );
    };
    (post_instance $(#[$doc:meta])* $name:ident, |$state:ident, $id:ident, $headers:ident, $bytes:ident| $body:block) => {
        $crate::version::operations::handlers::handler!(@fn $(#[$doc])* $name, $headers, |_| $body,
            [State($state): State<Arc<AppState>>, UrlPath($id): UrlPath<String>, $headers: HeaderMap,]
            [$bytes: Bytes,]
        );
    };
    // Every handler takes the query, which negotiates the format, between the
    // extractors before it and those after it (a body is the last).
    (@fn $(#[$doc:meta])* $name:ident, $headers:ident, |$arg:pat_param| $body:block,
        [$($before:tt)*] [$($after:tt)*]) => {
        $(#[$doc])*
        pub async fn $name(
            $($before)*
            Query(query): Query<Vec<(String, String)>>,
            $($after)*
        ) -> Response {
            respond_to(&$headers, &query, |$arg| $body)
        }
    };
}

pub(crate) use handler;

pub(crate) use handlers;
