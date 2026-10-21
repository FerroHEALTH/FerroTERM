// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener as a resource server over `ferrohealth-smart`.
//!
//! The service validates a presented credential and issues none: the crate
//! reads each `[auth]` issuer's metadata and key set, checks every access
//! token under RFC 9068 and every `[[auth.basic.users]]` credential, answers
//! the status and challenge of a refusal (RFC 6750 §3.1) and writes the audit
//! line. Every route but `GET /health` requires the product scope, which
//! `[auth] product_scope` names and which defaults to [`SYNC_SCOPE`]. A
//! listener with `[auth]` naming no issuer and no Basic user refuses to start
//! unless `[auth] enabled = false`.
//!
//! No specification governs the admin surface: our own design, the family's
//! fail-closed rule.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::request::Parts;
use axum::http::{Extensions, Method, Uri};
use axum::response::Response;
use ferrohealth_smart::config::{AuthSection, ConfigError};
use ferrohealth_smart::extract::HeaderOnly;
use ferrohealth_smart::guard::{AuthLayer, Gate, Render, Requirement, Target};
use ferrohealth_smart::refusal::Refusal;
use ferrohealth_smart::resource::{IssuerSet, ResourceError, ResourceServer};

/// The product scope of a deployment whose `[auth] product_scope` names none.
///
/// SMART scopes cover FHIR resources only, so the admin listener is guarded by
/// a scope of FerroTERM's own, in the form SMART App Launch 2.2.0, Scopes and
/// Launch Context, §Extensibility, gives an extension scope: "any string
/// starting with `__`".
pub const SYNC_SCOPE: &str = "__ferroterm.sync";

/// The realm of a deployment that names no `[auth] realm`.
const REALM: &str = "ferroterm-sync";

/// The one route that stays open: a probe holds no credential.
const HEALTH: &str = "/health";

/// An `[auth]` section the service cannot start with.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// The section breaks a rule, an enabled section naming no issuer and no
    /// Basic user among them.
    #[error(
        "[auth] is refused; the admin listener needs an issuer and an audience, or Basic users, or `[auth] enabled = false` on a network only trusted callers reach"
    )]
    Config {
        /// The rule.
        #[source]
        source: Box<ConfigError>,
    },
    /// An issuer's metadata or key set could not be read, or breaks a rule.
    #[error("the authorization server {issuer} could not be read")]
    Issuer {
        /// The issuer identifier.
        issuer: String,
        /// What the resource server reported.
        #[source]
        source: Box<ResourceError>,
    },
}

/// The started guard of the admin listener.
#[derive(Debug, Clone)]
pub struct AdminAuth {
    /// The credentials the listener admits.
    gate: Gate,
    /// The scope every guarded route requires.
    scope: String,
    /// The protection space the challenges name (RFC 6750 §3, RFC 7617 §2).
    realm: String,
}

impl AdminAuth {
    /// Reads the metadata and the key set of every issuer `section` names, or
    /// answers `None` when the section says `enabled = false`.
    ///
    /// Each issuer is read once here, so an authorization server that cannot
    /// be reached refuses the start rather than the first request.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Config`] for a section that breaks a rule, an
    /// enabled one admitting no credential among them, and
    /// [`AuthError::Issuer`] for an issuer whose metadata or key set could
    /// not be read.
    pub async fn start(section: &AuthSection) -> Result<Option<Self>, AuthError> {
        if !section.enabled {
            return Ok(None);
        }
        let resolved = section.resolve().map_err(|source| AuthError::Config {
            source: Box::new(source),
        })?;
        let realm = resolved.realm.unwrap_or_else(|| String::from(REALM));
        let mut gate = Gate::open().with_realm(realm.clone());
        if !resolved.issuers.is_empty() {
            let mut issuers = IssuerSet::new();
            for settings in resolved.issuers {
                let issuer = settings.issuer.clone();
                let server = ResourceServer::discover(settings).await.map_err(|source| {
                    AuthError::Issuer {
                        issuer,
                        source: Box::new(source),
                    }
                })?;
                issuers = issuers.with(Arc::new(server));
            }
            gate = gate.with_bearer(Arc::new(issuers));
        }
        if let Some(store) = resolved.basic {
            gate = gate.with_basic(Arc::new(store));
        }
        Ok(Some(Self {
            gate,
            scope: resolved
                .product_scope
                .unwrap_or_else(|| String::from(SYNC_SCOPE)),
            realm,
        }))
    }

    /// The scope every guarded route requires.
    #[must_use]
    pub fn scope(&self) -> &str {
        &self.scope
    }

    /// `router` with every route but `GET /health` behind the gate.
    #[must_use = "the guarded router is what the listener serves"]
    pub fn guard(&self, router: Router) -> Router {
        let requirement = Requirement::Scope(self.scope.clone());
        let classify = move |_: &Method, uri: &Uri, _: &Extensions| {
            let path = uri.path();
            if path == HEALTH {
                return None;
            }
            let interaction = match path {
                "/run" => "run",
                "/activate" => "activate",
                "/metrics" => "metrics",
                _ => "admin",
            };
            Some(
                Target::new(interaction, requirement.clone())
                    .route(route_of(path))
                    .target(path.to_owned()),
            )
        };
        let render = Refused {
            header: HeaderOnly::with_realm(self.realm.clone()),
        };
        router.layer(AuthLayer::new(self.gate.clone(), classify).with_render(render))
    }
}

/// The route template the audit line records for `path`.
fn route_of(path: &str) -> String {
    match path.strip_prefix("/runs/") {
        Some(id) if !id.is_empty() && !id.contains('/') => String::from("/runs/{id}"),
        _ => path.to_owned(),
    }
}

/// The text of a refusal, which names no claim and no scope.
///
/// No specification requires the omission: our own design, so a refusal
/// teaches a caller nothing about why a forged or stolen credential failed.
const fn reason(refusal: &Refusal) -> &'static str {
    match refusal {
        Refusal::Missing(_) | Refusal::InvalidToken | Refusal::InvalidBasic => {
            "the request carries no valid credential for this service"
        }
        Refusal::InvalidRequest => "the Authorization header carries no single credential",
        Refusal::Forbidden | Refusal::Unclassified => {
            "the credential carries no scope that permits this request"
        }
        Refusal::Unavailable => {
            "the credential could not be checked: the authorization server's keys could not be read"
        }
        _ => "the request was refused at the gate",
    }
}

/// The answer to a refused request: the crate's status and challenge, with
/// the JSON `error` body every other failure of the listener carries.
#[derive(Debug, Clone)]
struct Refused {
    /// The status and the challenge.
    header: HeaderOnly,
}

impl Render for Refused {
    fn render(&self, refusal: &Refusal, _parts: &Parts) -> Response {
        let body = serde_json::json!({ "error": reason(refusal) });
        let Ok(bytes) = serde_json::to_vec(&body) else {
            return self.header.respond(refusal, Body::empty());
        };
        let mut response = self.header.respond(refusal, Body::from(bytes));
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/json"),
        );
        response
    }
}

#[cfg(test)]
mod tests {
    use super::route_of;

    #[test]
    fn a_run_record_is_audited_under_its_template() {
        assert_eq!(route_of("/runs/2026-10-21T03-00-00Z"), "/runs/{id}");
        assert_eq!(route_of("/runs"), "/runs");
        assert_eq!(route_of("/run"), "/run");
    }
}
