// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `POST [base]/$closure`: the closure table a client maintains.
//!
//! The operation is invoked at the system level and keeps a named transitive
//! closure table for a client: the client registers concepts over time and the
//! server answers with the subsumption relationships that hold between them,
//! as a `ConceptMap` delta whose `version` rises with every change
//! (<https://hl7.org/fhir/R4B/conceptmap-operation-closure.html>). The table
//! lives in the resource database, so it outlives a restart.

macro_rules! closure {
    ($fhir:ident) => {
        pub mod closure {
            //! The closure interaction of one version.

            use std::sync::Arc;

            use axum::body::Bytes;
            use axum::extract::{Query, State};
            use axum::response::{IntoResponse, Response};
            use fhir_terminology::operations::closure::{Edge, Member, relate};
            use fhir_types::$fhir::operations::concept_map_closure::{
                CONCEPT_MAP_CLOSURE, ConceptMapClosureRequest,
            };
            use http::{HeaderMap, StatusCode};

            use crate::outcome::Failure;
            use crate::persistence::{Closure, ClosureEdge, ClosureMember};
            use crate::state::AppState;
            use crate::wire::Wire;

            use super::{map, parameters};

            /// `POST [base]/$closure`.
            ///
            /// The operation changes the server's state, so it is not offered on
            /// `GET` (<https://hl7.org/fhir/R4B/operations.html#executing>).
            pub async fn closure(
                State(state): State<Arc<AppState>>,
                headers: HeaderMap,
                Query(query): Query<Vec<(String, String)>>,
                body: Bytes,
            ) -> Response {
                let wire = match Wire::negotiate(&query, &headers) {
                    Ok(wire) => wire,
                    Err(failure) => return failure.into_response(),
                };
                let handled = run(&state, &headers, &body)
                    .and_then(|map| parameters::respond_resource(&map, wire));
                match handled {
                    Ok(response) => response,
                    Err(failure) => failure.respond(wire),
                }
            }

            /// The `ConceptMap` the call answers with.
            fn run(
                state: &AppState,
                headers: &HeaderMap,
                body: &Bytes,
            ) -> Result<fhir_types::$fhir::concept_map::ConceptMap, Failure> {
                let sent = parameters::parameters_from_body(headers, body)?;
                let request = ConceptMapClosureRequest::from_parameters(&sent)
                    .map_err(|e| parameters::parameters_failure(&e))?;
                let name = request.name.value.as_deref().unwrap_or_default();
                if name.is_empty() {
                    return Err(Failure::new(
                        StatusCode::BAD_REQUEST,
                        "required",
                        "`name` names the closure table and is required",
                    ));
                }
                let added = members(&request)?;
                let resync = request.version.as_ref().and_then(|v| v.value.clone());
                if resync.is_some() && !added.is_empty() {
                    return Err(Failure::new(
                        StatusCode::BAD_REQUEST,
                        "invalid",
                        "`version` asks the server to resend earlier entries; send it without `concept`",
                    ));
                }
                match resync {
                    Some(version) => replay(state, name, &version),
                    None if added.is_empty() => initialise(state, name),
                    None => extend(state, name, &added),
                }
            }

            /// Creates the table `name`, or empties one that already exists.
            ///
            /// Zero concepts is a request to initialise the table, and the answer is
            /// an empty `ConceptMap` at version `0`
            /// (<https://hl7.org/fhir/R4B/terminology-service.html>, "Maintaining a
            /// Closure Table").
            fn initialise(
                state: &AppState,
                name: &str,
            ) -> Result<fhir_types::$fhir::concept_map::ConceptMap, Failure> {
                let closure = Closure {
                    name: name.to_owned(),
                    version: 0,
                    edition: edition(state),
                    members: Vec::new(),
                    edges: Vec::new(),
                };
                state
                    .put_closure(&closure)
                    .map_err(|error| crate::version::store::persist_failure(&error))?;
                Ok(map::closure_map(
                    name,
                    &format!("Closure Table {name} Creation"),
                    0,
                    &[],
                ))
            }

            /// What the loaded code systems are, so a table that was built over
            /// other content can be refused.
            fn edition(state: &AppState) -> Vec<String> {
                let layer = state.layer();
                let mut out: Vec<String> = layer
                    .registry()
                    .systems()
                    .flat_map(|system| {
                        layer
                            .registry()
                            .versions(system)
                            .map(|provider| {
                                let identity = provider.identity();
                                format!("{}|{}", identity.url, identity.version)
                            })
                            .collect::<Vec<String>>()
                    })
                    .collect();
                out.sort();
                out
            }

            /// The concepts the request registers.
            fn members(request: &ConceptMapClosureRequest) -> Result<Vec<Member>, Failure> {
                let mut out = Vec::new();
                for coding in &request.concept {
                    let (Some(system), Some(code)) = (
                        coding.system.as_ref().and_then(|s| s.value.as_deref()),
                        coding.code.as_ref().and_then(|c| c.value.as_deref()),
                    ) else {
                        return Err(Failure::new(
                            StatusCode::BAD_REQUEST,
                            "required",
                            "each `concept` carries `system` and `code`",
                        ));
                    };
                    out.push(Member {
                        system: system.to_owned(),
                        version: coding.version.as_ref().and_then(|v| v.value.clone()),
                        code: code.to_owned(),
                    });
                }
                Ok(out)
            }

            /// Adds `added` to the table `name` and answers the relationships the
            /// client did not have.
            ///
            /// The table must have been initialised: naming one the server does not
            /// hold is a `404`, never a create
            /// (<https://hl7.org/fhir/R4B/terminology-service.html>, "Maintaining a
            /// Closure Table").
            fn extend(
                state: &AppState,
                name: &str,
                added: &[Member],
            ) -> Result<fhir_types::$fhir::concept_map::ConceptMap, Failure> {
                let mut held = table(state, name)?;
                if held.edition != edition(state) {
                    // NOTE: a table built over other content cannot be trusted, and the
                    // client's only move is to initialise and replay its codes
                    // (<https://hl7.org/fhir/R4B/terminology-service.html>).
                    return Err(Failure::new(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "processing",
                        format!("closure \"{name}\" must be reinitialized"),
                    ));
                }
                // NOTE: a concept already in the table brings no new relationship, so
                // registering it twice answers nothing
                // (<https://hl7.org/fhir/R4B/terminology-service.html>).
                let fresh: Vec<Member> = added
                    .iter()
                    .filter(|member| !held.members.iter().any(|kept| same(kept, member)))
                    .cloned()
                    .collect();
                let layer = state.layer();
                let edges = relate(layer.registry(), &held_members(&held), &fresh)?;
                held.version = held.version.saturating_add(1);
                for member in fresh {
                    held.members.push(ClosureMember {
                        system: member.system,
                        version: member.version,
                        code: member.code,
                    });
                }
                for edge in &edges {
                    held.edges.push(recorded(held.version, edge));
                }
                state
                    .put_closure(&held)
                    .map_err(|error| crate::version::store::persist_failure(&error))?;
                Ok(map::closure_map(
                    name,
                    &format!("Updates for Closure Table {name}"),
                    held.version,
                    &edges,
                ))
            }

            /// The table `name`, or the `404` of one that was never initialised.
            fn table(state: &AppState, name: &str) -> Result<Closure, Failure> {
                state
                    .closure(name)
                    .map_err(|error| crate::version::store::persist_failure(&error))?
                    .ok_or_else(|| {
                        Failure::new(
                            StatusCode::NOT_FOUND,
                            "not-found",
                            format!("invalid closure name \"{name}\""),
                        )
                    })
            }

            /// Answers every relationship the table recorded after `version`.
            fn replay(
                state: &AppState,
                name: &str,
                version: &str,
            ) -> Result<fhir_types::$fhir::concept_map::ConceptMap, Failure> {
                let held = table(state, name)?;
                let from = version.parse::<u32>().map_err(|_unparsed| {
                    Failure::new(
                        StatusCode::BAD_REQUEST,
                        "value",
                        format!("`{version}` is not a version this server counted"),
                    )
                })?;
                if from > held.version {
                    return Err(Failure::new(
                        StatusCode::BAD_REQUEST,
                        "value",
                        format!(
                            "version `{from}` is later than the table's own `{}`",
                            held.version
                        ),
                    ));
                }
                let edges: Vec<Edge> = held
                    .edges
                    .iter()
                    .filter(|edge| edge.version > from)
                    .filter_map(restored)
                    .collect();
                Ok(map::closure_map(
                    name,
                    &format!("Updates for Closure Table {name}"),
                    held.version,
                    &edges,
                ))
            }

            /// The members of a stored table, in the engine's terms.
            fn held_members(held: &Closure) -> Vec<Member> {
                held.members
                    .iter()
                    .map(|member| Member {
                        system: member.system.clone(),
                        version: member.version.clone(),
                        code: member.code.clone(),
                    })
                    .collect()
            }

            /// Whether two members name the same concept.
            fn same(held: &ClosureMember, member: &Member) -> bool {
                held.system == member.system
                    && held.code == member.code
                    && held.version == member.version
            }

            /// A relationship as the table stores it.
            fn recorded(version: u32, edge: &Edge) -> ClosureEdge {
                let member = |held: &Member| ClosureMember {
                    system: held.system.clone(),
                    version: held.version.clone(),
                    code: held.code.clone(),
                };
                ClosureEdge {
                    version,
                    source: member(&edge.source),
                    target: member(&edge.target),
                    relationship: edge.relationship.equivalence().to_owned(),
                }
            }

            /// A stored relationship in the engine's terms; one whose code this
            /// build no longer knows is left out rather than guessed.
            fn restored(edge: &ClosureEdge) -> Option<Edge> {
                let member = |held: &ClosureMember| Member {
                    system: held.system.clone(),
                    version: held.version.clone(),
                    code: held.code.clone(),
                };
                Some(Edge {
                    source: member(&edge.source),
                    target: member(&edge.target),
                    relationship:
                        fhir_terminology::conceptmap::model::Relationship::from_equivalence(
                            &edge.relationship,
                        )?,
                })
            }

            /// The canonical of `$closure`, for the capability statement.
            pub const CLOSURE_URL: &str = CONCEPT_MAP_CLOSURE.url;
        }
    };
}

pub(crate) use closure;
