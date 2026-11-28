// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener as a resource server over `ferrohealth-smart`.
//!
//! The service validates a presented credential and issues none: the crate
//! reads each `[auth]` issuer's metadata and key set, checks every access
//! token under RFC 9068 (or, for an issuer whose `[auth.issuer.<name>]` table says
//! `mode = "introspection"`, at its introspection endpoint, RFC 7662),
//! reads the caller from the token's standard claims, and checks every
//! `[[auth.basic.users]]` credential, a person's alone, and answers
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
use ferrohealth_smart::basic::{PasswordStore, StoreError};
use ferrohealth_smart::config::{AuthSection, BasicGrants, ConfigError, ServerTls};
use ferrohealth_smart::extract::HeaderOnly;
use ferrohealth_smart::guard::{AuthLayer, Gate, Render, Requirement, Target};
use ferrohealth_smart::refusal::{Denied, Refusal};
use ferrohealth_smart::resource::introspection::{IntrospectionError, Introspector};
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

/// The routes that stay open: a probe holds no credential.
const HEALTH: [&str; 3] = [
    ferrohealth_server::health::SUMMARY,
    ferrohealth_server::health::LIVENESS,
    ferrohealth_server::health::READINESS,
];

/// An `[auth]` section the service cannot start with.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// The section breaks a rule, an enabled section naming no issuer and no
    /// Basic user among them.
    #[error(
        "[auth] is refused; the admin listener needs an [auth.issuer.<name>] table with its issuer, or Basic users, or `[auth] enabled = false` on a network only trusted callers reach"
    )]
    Config {
        /// The rule.
        #[source]
        source: Box<ConfigError>,
    },
    /// `[basic_grants]` names a user `[[auth.basic.users]]` does not list,
    /// or grants a Basic user a scope or a role it may not hold.
    #[error("[basic_grants] is refused")]
    BasicGrants {
        /// The rule, naming the user.
        #[source]
        source: StoreError,
    },
    /// An introspecting issuer's metadata could not be read, names no
    /// introspection endpoint, or its `[auth.issuer.introspection]` table
    /// breaks a rule.
    #[error("the introspection endpoint of {issuer} could not be set up")]
    Introspector {
        /// The issuer identifier.
        issuer: String,
        /// What the introspection check reported.
        #[source]
        source: Box<IntrospectionError>,
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
    /// Reads the metadata and the key set of every issuer `section` names and
    /// grants each Basic user its entry of `grants`, or answers `None` when
    /// the section says `enabled = false`. Under `[auth] post_quantum_only`
    /// the section's own check holds every issuer to ML-DSA (RFC 9964 §5).
    ///
    /// Each issuer is read once here, so an authorization server that cannot
    /// be reached refuses the start rather than the first request.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Config`] for a section that breaks a rule, an
    /// enabled one admitting no credential, the removed flat `issuer` and an
    /// algorithm list `post_quantum_only` refuses among them, [`AuthError::BasicGrants`] for a `[basic_grants]` table
    /// naming a user `[[auth.basic.users]]` does not list or granting a
    /// Basic user a `system/` scope, [`AuthError::Introspector`] for an introspecting issuer that cannot be
    /// set up, and
    /// [`AuthError::Issuer`] for an issuer whose metadata or key set could
    /// not be read.
    pub async fn start(
        section: &AuthSection,
        grants: &BasicGrants,
    ) -> Result<Option<Self>, AuthError> {
        if !section.enabled {
            return Ok(None);
        }
        let resolved = section
            .resolve_with_tls(&ServerTls, &[])
            .map_err(|source| AuthError::Config {
                source: Box::new(source),
            })?;
        let realm = resolved.realm.unwrap_or_else(|| String::from(REALM));
        let mut gate = Gate::open().with_realm(realm.clone());
        if !resolved.issuers.is_empty() || !resolved.introspection.is_empty() {
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
            for settings in resolved.introspection {
                let issuer = settings.issuer.clone();
                let checker = Introspector::discover(settings).await.map_err(|source| {
                    AuthError::Introspector {
                        issuer,
                        source: Box::new(source),
                    }
                })?;
                issuers = issuers.with_introspector(Arc::new(checker));
            }
            gate = gate.with_bearer(Arc::new(issuers));
        }
        let basic = match resolved.basic {
            Some(store) => Some(store),
            None if grants.users.is_empty() => None,
            // NOTE: no specification governs this, our own design: a grant
            // naming no listed user is refused, so a misspelt user-id never grants.
            None => Some(PasswordStore::new()),
        };
        if let Some(store) = basic {
            let store = store
                .with_grants(grants)
                .map_err(|source| AuthError::BasicGrants { source })?;
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

    /// `router` with every route but the health routes behind the gate.
    #[must_use = "the guarded router is what the listener serves"]
    pub fn guard(&self, router: Router) -> Router {
        let requirement = Requirement::Scope(self.scope.clone());
        let classify = move |_: &Method, uri: &Uri, _: &Extensions| {
            let path = uri.path();
            if HEALTH.contains(&path) {
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
/// the RFC 9457 problem every other failure of the listener carries.
#[derive(Debug, Clone)]
struct Refused {
    /// The status and the challenge.
    header: HeaderOnly,
}

impl Render for Refused {
    fn render(&self, denied: &Denied, _parts: &Parts) -> Response {
        let refusal = denied.refusal();
        let text = reason(refusal);
        let challenged = self.header.respond(denied, Body::empty());
        let challenge = challenged
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE);
        ferroterm_problem::refused(refusal.status(), text, challenge)
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
