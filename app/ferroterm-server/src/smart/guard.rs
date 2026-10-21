// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The route table: which requests need a credential, and what it must grant.
//!
//! One `ferrohealth-smart` layer sits inside each version's router, so the path
//! it classifies is the one under the version prefix, and one sits on the
//! admin listener. A route the table does not name passes untouched and writes
//! no audit line: the reads and the operations stay open.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use ferrohealth_smart::guard::{AuthLayer, Gate, Render, Requirement, Target};
use ferrohealth_smart::refusal::Refusal;
use ferrohealth_smart::resource::Access;
use ferrohealth_smart::scope::Permission;
use http::{Extensions, Method, Uri};

use crate::smart::{Challenge, Smart};

/// The resource types a client writes through the REST API.
const WRITTEN: [&str; 3] = ["CodeSystem", "ValueSet", "ConceptMap"];

/// The system-level route of `ConceptMap/$closure`.
const CLOSURE: &str = "$closure";

/// What a write route requires: a SMART permission on a resource type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Need {
    /// The resource type the route writes.
    pub resource: &'static str,
    /// The interaction it performs, as a SMART permission.
    pub permission: Permission,
    /// The FHIR interaction the audit line records.
    pub interaction: &'static str,
    /// The route template the audit line records, under the version prefix.
    pub route: &'static str,
}

/// What `method` on `path` requires, or `None` when the route reads.
///
/// `POST` on a resource type creates, `PUT` on an instance updates, and
/// `DELETE` on an instance deletes; `$closure` maintains a stored table and is
/// gated with them. Every other route, the operations included, reads and stays
/// open (<https://hl7.org/fhir/R4B/http.html>). `path` is the one under the
/// version prefix, which is what a router nested by `axum` sees
/// (<https://docs.rs/axum/0.8/axum/struct.Router.html#method.nest>).
#[must_use]
pub fn needed(method: &Method, path: &str) -> Option<Need> {
    let mut segments = path.trim_matches('/').split('/');
    let named = segments.next()?;
    let instance = segments.next();
    if named == CLOSURE && instance.is_none() && method == Method::POST {
        // NOTE: `$closure` keeps a closure table in the resource database
        // (<https://hl7.org/fhir/R4B/conceptmap-operation-closure.html>), and no
        // SMART scope names it, so this mapping is our own design.
        return Some(Need {
            resource: "ConceptMap",
            permission: Permission::Update,
            interaction: "operation",
            route: "/$closure",
        });
    }
    let resource = WRITTEN.into_iter().find(|known| *known == named)?;
    if segments.next().is_some() {
        return None;
    }
    let (permission, interaction, route) = match (method, instance) {
        (&Method::POST, None) => (Permission::Create, "create", "/{type}"),
        // An instance segment starting with `$` is an operation, never an id.
        (&Method::PUT, Some(id)) if !id.starts_with('$') => {
            (Permission::Update, "update", "/{type}/{id}")
        }
        (&Method::DELETE, Some(id)) if !id.starts_with('$') => {
            (Permission::Delete, "delete", "/{type}/{id}")
        }
        _ => return None,
    };
    Some(Need {
        resource,
        permission,
        interaction,
        route,
    })
}

/// Guards the write routes of the version served under `base` with the gate
/// of `smart`.
///
/// The crate's layer admits a credential whose SMART scope grants the
/// permission on the type; the `compartment` middleware refuses a grant that
/// reaches only one patient's compartment; [`Challenge`] writes the body
/// of a refusal in the format the request asked for.
pub fn fhir<S>(router: Router<S>, smart: &Smart, base: &'static str) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let render = smart.fhir_render();
    let classify = move |method: &Method, uri: &Uri, _: &Extensions| {
        let need = needed(method, uri.path())?;
        let requirement = Requirement::Fhir {
            permission: need.permission,
            resource_type: need.resource.to_owned(),
        };
        Some(
            Target::new(need.interaction, requirement)
                .route(format!("{base}{}", need.route))
                .target(uri.path().trim_matches('/')),
        )
    };
    router
        .layer(axum::middleware::from_fn_with_state(
            Arc::new(render.clone()),
            compartment,
        ))
        .layer(AuthLayer::new(smart.fhir_gate().clone(), classify).with_render(render))
}

/// Refuses a write whose grant reaches only a patient's compartment.
///
/// A written `CodeSystem`, `ValueSet` or `ConceptMap` belongs to no patient,
/// so a `patient/` scope selects nothing on it
/// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>);
/// a request the layer admitted with one answers `403`.
async fn compartment(
    State(render): State<Arc<Challenge>>,
    request: Request,
    next: Next,
) -> Response {
    let reached = needed(request.method(), request.uri().path()).map(|need| {
        request.extensions().get::<Access>().is_none_or(|access| {
            access
                .reach(need.permission, need.resource)
                .is_some_and(|compartment| compartment.admits(None))
        })
    });
    if reached == Some(false) {
        let (parts, _) = request.into_parts();
        return render.render(&Refusal::Forbidden, &parts);
    }
    next.run(request).await
}

/// Guards every route of the admin listener with the gate of `smart`.
///
/// No specification governs the admin surface: our own design, so every route
/// requires the product scope `[auth] product_scope` names. A deployment that
/// binds no admin listener has no admin gate, and the routes refuse every
/// caller.
pub fn admin<S>(router: Router<S>, smart: &Smart) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let gate = smart.admin_gate().cloned().unwrap_or_else(Gate::open);
    let requirement = if smart.admin_gate().is_some() {
        Requirement::Scope(smart.admin_scope().to_owned())
    } else {
        Requirement::Refused
    };
    let classify = move |_: &Method, uri: &Uri, _: &Extensions| {
        let interaction = if uri.path() == "/reload" {
            "reload"
        } else {
            "admin"
        };
        Some(
            Target::new(interaction, requirement.clone())
                .route(uri.path().to_owned())
                .target(uri.path().to_owned()),
        )
    };
    router.layer(AuthLayer::new(gate, classify).with_render(smart.admin_render()))
}

#[cfg(test)]
mod tests {
    use ferrohealth_smart::scope::Permission;
    use http::Method;

    use super::needed;

    fn written(method: &Method, path: &str) -> Option<(&'static str, Permission)> {
        needed(method, path).map(|need| (need.resource, need.permission))
    }

    #[test]
    fn the_three_write_interactions_are_the_gated_ones() {
        assert_eq!(
            written(&Method::POST, "/CodeSystem"),
            Some(("CodeSystem", Permission::Create))
        );
        assert_eq!(
            written(&Method::PUT, "/ValueSet/vs-1"),
            Some(("ValueSet", Permission::Update))
        );
        assert_eq!(
            written(&Method::DELETE, "/ConceptMap/cm-1"),
            Some(("ConceptMap", Permission::Delete))
        );
    }

    // `$closure` maintains a stored closure table
    // (<https://hl7.org/fhir/R4B/conceptmap-operation-closure.html>).
    #[test]
    fn closure_is_gated_as_a_concept_map_update() {
        assert_eq!(
            written(&Method::POST, "/$closure"),
            Some(("ConceptMap", Permission::Update))
        );
        assert_eq!(written(&Method::GET, "/$closure"), None);
    }

    #[test]
    fn every_read_and_every_operation_stays_open() {
        // NOTE: `$cache-control` holds supplied resources for a run of reads and
        // stores no content, so it stays open
        // (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
        for (method, path) in [
            (Method::POST, "/$cache-control"),
            (Method::GET, "/CodeSystem"),
            (Method::GET, "/CodeSystem/cs-1"),
            (Method::GET, "/CodeSystem/cs-1/_history/1"),
            (Method::POST, "/CodeSystem/$lookup"),
            (Method::POST, "/CodeSystem/cs-1/$validate-code"),
            (Method::POST, "/ValueSet/$expand"),
            (Method::POST, "/ConceptMap/$translate"),
            (Method::POST, "/"),
            (Method::GET, "/metadata"),
            (Method::GET, "/.well-known/smart-configuration"),
            (Method::POST, "/Patient"),
        ] {
            assert_eq!(needed(&method, path), None, "{method} {path}");
        }
    }
}
