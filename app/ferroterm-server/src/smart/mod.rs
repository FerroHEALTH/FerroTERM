// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SMART App Launch: the server as a resource server over `ferrohealth-smart`.
//!
//! The server takes the resource-server role and never the authorization-server
//! one: it validates a presented credential and issues none
//! (<https://hl7.org/fhir/smart-app-launch/conformance.html>). The crate reads
//! each configured issuer's metadata and key set, validates every access token
//! under RFC 9068, checks the Basic users of `[[auth.basic.users]]`, answers
//! the status and challenge of every refusal (RFC 6750 §3.1) and writes the
//! audit line. This module holds what is the server's own: which surfaces need
//! `[auth]`, the audiences each surface accepts, the admin scope, the
//! `OperationOutcome` and admin bodies of a refusal, and the SMART discovery
//! document ([`discovery`]). The route table is [`guard`]'s.
//!
//! The read surface stays open whatever `[auth]` says. A terminology read
//! names a code system and a code, so the gate is on what a caller may change,
//! and the deployment's own gateway remains free to require more.

pub mod discovery;
pub mod guard;

use std::sync::Arc;

use axum::body::Body;
use axum::response::{IntoResponse, Response};
use ferrohealth_smart::basic::PasswordStore;
use ferrohealth_smart::config::ConfigError;
use ferrohealth_smart::extract::HeaderOnly;
use ferrohealth_smart::guard::{Gate, Render};
use ferrohealth_smart::metadata::ServerMetadata;
use ferrohealth_smart::refusal::Refusal;
use ferrohealth_smart::resource::{IssuerSet, ResourceError, ResourceServer, Settings};
use http::request::Parts;
use http::{HeaderMap, StatusCode};

use crate::config::Config;
use crate::outcome::Failure;

/// The `restful-security-service` code a SMART server declares
/// (<http://terminology.hl7.org/CodeSystem/restful-security-service>).
pub const SMART_ON_FHIR: &str = "SMART-on-FHIR";

/// The extension a capability statement states its OAuth endpoints under
/// (<https://hl7.org/fhir/smart-app-launch/1.0.0/conformance/index.html>).
pub const OAUTH_URIS: &str =
    "http://fhir-registry.smarthealthit.org/StructureDefinition/oauth-uris";

/// The media type the discovery document is served as.
///
/// The specification fixes it, whatever the request asks for
/// (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
pub const SMART_CONFIGURATION_JSON: &str = "application/json";

/// The admin scope of a deployment whose `[auth] product_scope` names none.
///
/// SMART scopes cover FHIR resources only, so the admin listener is guarded by
/// a scope of FerroTERM's own, in the form SMART App Launch 2.2.0, Scopes and
/// Launch Context, §Extensibility, gives an extension scope: "any string
/// starting with `__`".
pub const ADMIN_SCOPE: &str = "__ferroterm.admin";

/// The realm of a deployment that names neither `[auth] realm` nor a base URL.
const REALM: &str = "FerroTERM";

/// A start the authorization server or the configuration refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StartError {
    /// A guarded surface runs and `[auth]` names no issuer and is not off.
    #[error(
        "{} need [auth]: set FERROTERM_AUTH__ISSUER and FERROTERM_AUTH__AUDIENCE (or the file FERROTERM_AUTH_FILE), or FERROTERM_AUTH__ENABLED=false on a network only trusted callers reach",
        surfaces.join(" and ")
    )]
    Unconfigured {
        /// The guarded surfaces the deployment turned on, each with the
        /// variable that turned it on.
        surfaces: Vec<&'static str>,
    },
    /// The `[auth]` section breaks a rule.
    #[error("[auth] is refused")]
    Auth {
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
    /// The first issuer publishes no token endpoint, which the discovery
    /// document requires (SMART App Launch 2.2.0, Conformance, §Metadata,
    /// `token_endpoint`: REQUIRED).
    #[error("the authorization server metadata of {issuer} names no token_endpoint")]
    NoTokenEndpoint {
        /// The issuer identifier.
        issuer: String,
    },
}

/// The started resource server: the gates in front of the write routes and
/// the admin listener, and what the discovery documents name.
#[derive(Debug)]
pub struct Smart {
    /// The credentials the write routes admit.
    fhir: Gate,
    /// The credentials the admin listener admits; `None` when the deployment
    /// binds no admin listener, so the admin routes refuse every caller.
    admin: Option<Gate>,
    /// The issuer the discovery document and the capability statement name.
    primary: Arc<ResourceServer>,
    /// The token endpoint that issuer's metadata names.
    token_endpoint: String,
    /// The protection space the challenges name (RFC 6750 §3, RFC 7617 §2).
    realm: String,
    /// The scope the admin listener requires.
    admin_scope: String,
    /// The scopes `[auth] scopes_supported` lists.
    scopes_supported: Vec<String>,
    /// The capabilities `[auth] capabilities` adds.
    capabilities: Vec<String>,
}

impl Smart {
    /// Reads the metadata and the key set of every issuer `[auth]` names, or
    /// answers `None` when the deployment serves without authentication.
    ///
    /// Authentication is off when `[auth] enabled = false`, and when `[auth]`
    /// names no issuer on a server that serves no write and no admin listener:
    /// the read surface is open either way. A server that serves either and
    /// names no issuer refuses to start (no specification governs the
    /// default: our own design, the family's fail-closed rule). Each issuer is
    /// read once here, so an authorization server that cannot be reached
    /// refuses the start rather than the first request.
    ///
    /// # Errors
    ///
    /// Returns [`StartError::Unconfigured`] for a guarded surface without an
    /// issuer, [`StartError::Auth`] for a section that breaks a rule,
    /// [`StartError::Issuer`] for an issuer whose metadata or key set could
    /// not be read, and [`StartError::NoTokenEndpoint`] when the first issuer
    /// names no token endpoint.
    pub async fn start(config: &Config) -> Result<Option<Self>, StartError> {
        let auth = &config.auth;
        if !auth.enabled {
            return Ok(None);
        }
        if auth.issuers().iter().all(|entry| entry.issuer.is_empty()) {
            let surfaces = guarded_surfaces(config);
            if surfaces.is_empty() {
                return Ok(None);
            }
            return Err(StartError::Unconfigured { surfaces });
        }
        let resolved = auth.resolve().map_err(|source| StartError::Auth {
            source: Box::new(source),
        })?;
        let bases = version_bases(config.base_url.as_deref());
        let mut fhir = IssuerSet::new();
        let mut admin = IssuerSet::new();
        let mut primary = None;
        for settings in resolved.issuers {
            let mut widened = settings.clone();
            widened.audience.extend(bases.iter().cloned());
            let server = discover(widened).await?;
            primary.get_or_insert_with(|| Arc::clone(&server));
            fhir = fhir.with(Arc::clone(&server));
            if config.admin_listen.is_some() {
                // NOTE: no specification governs the admin surface: our own design;
                // it serves no FHIR base, so only the configured audiences open it.
                let narrow = if bases.is_empty() {
                    server
                } else {
                    discover(settings).await?
                };
                admin = admin.with(narrow);
            }
        }
        let Some(primary) = primary else {
            return Err(StartError::Auth {
                source: Box::new(ConfigError::Missing { key: "auth.issuer" }),
            });
        };
        let token_endpoint = primary.metadata().token_endpoint.clone().ok_or_else(|| {
            StartError::NoTokenEndpoint {
                issuer: primary.settings().issuer.clone(),
            }
        })?;
        let realm = resolved
            .realm
            .or_else(|| config.base_url.clone())
            .unwrap_or_else(|| String::from(REALM));
        let basic = resolved.basic.map(Arc::new);
        let fhir = gate_of(Arc::new(fhir), basic.as_ref(), &realm);
        let admin = config
            .admin_listen
            .is_some()
            .then(|| gate_of(Arc::new(admin), basic.as_ref(), &realm));
        Ok(Some(Self {
            fhir,
            admin,
            primary,
            token_endpoint,
            realm,
            admin_scope: resolved
                .product_scope
                .unwrap_or_else(|| String::from(ADMIN_SCOPE)),
            scopes_supported: resolved.scopes_supported,
            capabilities: resolved.capabilities,
        }))
    }

    /// The credentials the write routes admit.
    #[must_use]
    pub const fn fhir_gate(&self) -> &Gate {
        &self.fhir
    }

    /// The credentials the admin listener admits, when the deployment binds
    /// one.
    #[must_use]
    pub const fn admin_gate(&self) -> Option<&Gate> {
        self.admin.as_ref()
    }

    /// The scope the admin listener requires.
    #[must_use]
    pub fn admin_scope(&self) -> &str {
        &self.admin_scope
    }

    /// The realm every challenge names.
    #[must_use]
    pub fn realm(&self) -> &str {
        &self.realm
    }

    /// The issuer the discovery documents name, the first one configured.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.primary.settings().issuer
    }

    /// That issuer's metadata.
    #[must_use]
    pub fn metadata(&self) -> &ServerMetadata {
        self.primary.metadata()
    }

    /// The issuer's token endpoint, which a capability statement states.
    #[must_use]
    pub fn token_endpoint(&self) -> &str {
        &self.token_endpoint
    }

    /// The issuer's authorization endpoint, when it runs one.
    #[must_use]
    pub fn authorize_endpoint(&self) -> Option<&str> {
        self.metadata().authorization_endpoint.as_deref()
    }

    /// The issuer's dynamic registration endpoint, when it runs one.
    #[must_use]
    pub fn register_endpoint(&self) -> Option<&str> {
        self.metadata().registration_endpoint.as_deref()
    }

    /// The issuer's introspection endpoint, when it runs one (RFC 7662).
    #[must_use]
    pub fn introspect_endpoint(&self) -> Option<&str> {
        self.metadata().introspection_endpoint.as_deref()
    }

    /// The issuer's revocation endpoint, when it runs one (RFC 7009).
    #[must_use]
    pub fn revoke_endpoint(&self) -> Option<&str> {
        self.metadata().revocation_endpoint.as_deref()
    }

    /// The `.well-known/smart-configuration` document of this deployment.
    #[must_use]
    pub fn configuration(&self) -> serde_json::Value {
        self.primary.smart_configuration(&discovery::options(
            &self.scopes_supported,
            &self.capabilities,
        ))
    }

    /// The renderer of a refusal on the FHIR surface.
    #[must_use]
    pub fn fhir_render(&self) -> Challenge {
        Challenge {
            header: HeaderOnly::with_realm(self.realm.clone()),
        }
    }

    /// The renderer of a refusal on the admin listener.
    #[must_use]
    pub fn admin_render(&self) -> AdminRefusal {
        AdminRefusal {
            header: HeaderOnly::with_realm(self.realm.clone()),
        }
    }
}

/// Reads one issuer's metadata and key set.
async fn discover(settings: Settings) -> Result<Arc<ResourceServer>, StartError> {
    let issuer = settings.issuer.clone();
    ResourceServer::discover(settings)
        .await
        .map(Arc::new)
        .map_err(|source| StartError::Issuer {
            issuer,
            source: Box::new(source),
        })
}

/// The gate admitting the bearer tokens `verifier` checks and the Basic users
/// of `basic`, naming `realm` on every challenge.
fn gate_of(verifier: Arc<IssuerSet>, basic: Option<&Arc<PasswordStore>>, realm: &str) -> Gate {
    let gate = Gate::open().with_bearer(verifier).with_realm(realm);
    match basic {
        Some(store) => gate.with_basic(Arc::clone(store)),
        None => gate,
    }
}

/// The guarded surfaces `config` turns on, each named with its variable.
fn guarded_surfaces(config: &Config) -> Vec<&'static str> {
    let mut surfaces = Vec::new();
    if config.resources.is_some() {
        surfaces.push("the write routes (FERROTERM_RESOURCES)");
    }
    if config.admin_listen.is_some() {
        surfaces.push("the admin listener (FERROTERM_ADMIN_LISTEN)");
    }
    surfaces
}

/// The audiences the write routes accept beside `[auth] audience`: every
/// served version's base under `base_url`, with and without a trailing slash.
///
/// The `aud` of a SMART authorization request names the resource server the
/// app wants FHIR data from (<https://hl7.org/fhir/smart-app-launch/app-launch.html>),
/// and this server publishes one base per version. The set is one for every
/// version, so a token minted for `/r5` is spendable on `/r4b` as well: our own
/// design, one process serving the four bases to a reader in one session.
fn version_bases(base_url: Option<&str>) -> Vec<String> {
    let Some(base) = base_url else {
        return Vec::new();
    };
    let mut bases = Vec::new();
    for segment in crate::version::SEGMENTS {
        let endpoint = crate::version::endpoint(base, segment);
        // NOTE: FHIR fixes a service base with no trailing slash
        // (<https://hl7.org/fhir/R4B/http.html#root>), so the slashed form is
        // our own tolerance and the only one performed.
        bases.push(format!("{endpoint}/"));
        bases.push(endpoint);
    }
    bases
}

/// The `OperationOutcome` issue code and the text of a refusal, which name no
/// claim and no scope.
///
/// No specification requires the omission: our own design, so a refusal
/// teaches a caller nothing about why a forged or stolen token failed.
const fn diagnostics(refusal: &Refusal) -> (&'static str, &'static str) {
    match refusal {
        Refusal::Missing(_) | Refusal::InvalidToken | Refusal::InvalidBasic => (
            "login",
            "the request carries no valid credential for this server",
        ),
        Refusal::InvalidRequest => (
            "security",
            "the Authorization header carries no single credential",
        ),
        Refusal::Forbidden | Refusal::Unclassified => (
            "forbidden",
            "the credential carries no scope that permits this request",
        ),
        Refusal::Unavailable => (
            "transient",
            "the credential could not be checked: the authorization server's keys could not be read",
        ),
        _ => ("security", "the request was refused at the gate"),
    }
}

/// The renderer of a refusal on the FHIR surface: the crate's status and
/// challenge, and an `OperationOutcome` body in the format the request asks
/// for.
#[derive(Debug, Clone)]
pub struct Challenge {
    /// The status and the challenge.
    header: HeaderOnly,
}

impl Render for Challenge {
    fn render(&self, refusal: &Refusal, parts: &Parts) -> Response {
        // NOTE: an `Accept` this server does not speak is a second fault on a
        // request already refused, so the outcome answers in the default format.
        let wire = crate::wire::Wire::negotiate(&[], &parts.headers).unwrap_or_default();
        let (code, text) = diagnostics(refusal);
        let mut answered = Failure::new(refusal.status(), code, text).respond(wire);
        let challenged = self.header.respond(refusal, Body::empty());
        if let Some(challenge) = challenged.headers().get(http::header::WWW_AUTHENTICATE) {
            answered
                .headers_mut()
                .insert(http::header::WWW_AUTHENTICATE, challenge.clone());
        }
        answered
    }
}

/// The renderer of a refusal on the admin listener, which answers plain JSON
/// and never FHIR.
#[derive(Debug, Clone)]
pub struct AdminRefusal {
    /// The status and the challenge.
    header: HeaderOnly,
}

impl Render for AdminRefusal {
    fn render(&self, refusal: &Refusal, _parts: &Parts) -> Response {
        let (_, text) = diagnostics(refusal);
        let body = serde_json::json!({
            "outcome": crate::metrics::Outcome::Failed.as_str(),
            "reason": text,
        });
        let Ok(bytes) = serde_json::to_vec(&body) else {
            return self.header.respond(refusal, Body::empty());
        };
        let mut response = self.header.respond(refusal, Body::from(bytes));
        response.headers_mut().insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        response
    }
}

/// `GET [base]/.well-known/smart-configuration`.
///
/// A deployment that serves without authentication serves nothing here, so the
/// answer is the `not-found` outcome any other unknown path gives.
pub async fn configuration(
    axum::extract::State(smart): axum::extract::State<Option<Arc<Smart>>>,
    headers: HeaderMap,
) -> Response {
    let Some(smart) = smart else {
        return crate::outcome::not_found(headers).await;
    };
    let body = smart.configuration();
    match serde_json::to_vec(&body) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(http::header::CONTENT_TYPE, SMART_CONFIGURATION_JSON)
            .body(Body::from(bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(error) => {
            tracing::error!(%error, "the SMART configuration does not encode");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{guarded_surfaces, version_bases};
    use crate::config::Config;

    // The `aud` a client sends is the FHIR base it is talking to
    // (<https://hl7.org/fhir/smart-app-launch/app-launch.html>), and this
    // server has one per version.
    #[test]
    fn every_served_version_base_is_an_audience_of_the_write_routes() {
        let bases = version_bases(Some("https://tx.example.org"));
        for segment in crate::version::SEGMENTS {
            let base = format!("https://tx.example.org/{segment}");
            assert!(bases.contains(&base), "{base}: {bases:?}");
            assert!(bases.contains(&format!("{base}/")), "{base}/");
        }
        assert!(
            !bases.contains(&String::from("https://tx.example.org")),
            "the root serves no FHIR base: {bases:?}"
        );
        assert!(version_bases(None).is_empty());
    }

    // A base URL with a trailing slash would otherwise double the separator
    // and refuse every real token.
    #[test]
    fn a_base_url_that_ends_in_a_slash_derives_the_same_audiences() {
        let bases = version_bases(Some("https://tx.example.org/"));
        assert!(
            bases.contains(&String::from("https://tx.example.org/r4b")),
            "{bases:?}"
        );
    }

    #[test]
    fn the_writes_and_the_admin_listener_are_the_guarded_surfaces() {
        assert!(guarded_surfaces(&Config::default()).is_empty());
        let both = Config {
            resources: Some(std::path::PathBuf::from("resources.redb")),
            admin_listen: Some(String::from("127.0.0.1:8081")),
            ..Config::default()
        };
        assert_eq!(guarded_surfaces(&both).len(), 2);
    }
}
