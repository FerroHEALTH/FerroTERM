//! SMART App Launch: the optional bearer gate over the write routes.
//!
//! The server takes the resource-server role and never the authorization-server
//! one: it validates a presented access token and issues none, and every
//! endpoint it publishes is read from the configured issuer's own discovery
//! document (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
//!
//! A deployment that names an OpenID Connect issuer in `FERROTERM_OIDC_ISSUER`
//! turns this on. The server then reads the issuer's discovery document and its
//! JWKS at start, publishes `[base]/.well-known/smart-configuration` derived
//! from that document, declares `SMART-on-FHIR` in its capability statements,
//! and requires a bearer token carrying the SMART scope for every write
//! (<https://hl7.org/fhir/smart-app-launch/conformance.html>). A deployment
//! that names none is unchanged: no token is ever asked for, and the whole
//! surface answers as before.
//!
//! The read surface stays open either way. A terminology read names a code
//! system and a code, so the gate is on what a caller may change, and the
//! deployment's own gateway remains free to require more.

pub mod discovery;
pub mod guard;
pub mod jwks;
pub mod scopes;

use std::sync::Arc;

use axum::body::Body;
use axum::response::{IntoResponse, Response};
use http::{HeaderMap, HeaderValue, StatusCode};
use jsonwebtoken::{Algorithm, Validation, decode, decode_header};

use crate::config::Config;
use crate::outcome::Failure;
use crate::smart::discovery::{FetchError, Http, IssuerMetadata};
use crate::smart::jwks::{KeyError, Keys};
use crate::smart::scopes::Permission;

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

/// The signature algorithms this server verifies a token with.
///
/// Only asymmetric algorithms are accepted: an HMAC algorithm would let a
/// caller sign a token with a key the JWKS publishes, which is the algorithm
/// confusion RFC 8725 §3.1 requires a verifier to close.
const ALGORITHMS: [Algorithm; 9] = [
    Algorithm::RS256,
    Algorithm::RS384,
    Algorithm::RS512,
    Algorithm::PS256,
    Algorithm::PS384,
    Algorithm::PS512,
    Algorithm::ES256,
    Algorithm::ES384,
    Algorithm::EdDSA,
];

/// The grant the authorization code flow of a standalone launch runs on
/// (<https://hl7.org/fhir/smart-app-launch/app-launch.html>).
const AUTHORIZATION_CODE: &str = "authorization_code";

/// The grant types the SMART discovery document names as its options.
const GRANT_TYPES: [&str; 2] = [AUTHORIZATION_CODE, "client_credentials"];

/// The client authentication methods the SMART discovery document names as its
/// options.
const TOKEN_ENDPOINT_AUTH_METHODS: [&str; 3] = [
    "client_secret_post",
    "client_secret_basic",
    "private_key_jwt",
];

/// The PKCE methods the SMART discovery document publishes.
///
/// "SMART servers SHALL support the `S256` `code_challenge_method` and SHALL
/// NOT support the `plain` method", so the list is this one whatever the issuer
/// advertises; `S256` is the SHA-256 challenge of RFC 7636 §4.2
/// (<https://hl7.org/fhir/smart-app-launch/app-launch.html>).
const CODE_CHALLENGE_METHODS: [&str; 1] = ["S256"];

/// The scope an issuer advertises when it mints an identity token
/// (<https://openid.net/specs/openid-connect-core-1_0.html>, §3.1.2.1).
const OPENID: &str = "openid";

/// The members of `advertised` that `options` admits, in the order `options`
/// fixes them.
///
/// The discovery document is this server's own assertion about itself, so a
/// value the specification does not define for a member is dropped rather than
/// passed through from the issuer's document
/// (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
fn only(advertised: &[String], options: &[&str]) -> Vec<String> {
    options
        .iter()
        .filter(|option| advertised.iter().any(|value| value == *option))
        .map(|option| (*option).to_owned())
        .collect()
}

/// The audiences a token may name, by the surface it is spent on.
///
/// The `aud` of a SMART authorization request names the resource server the
/// app wants FHIR data from, which is what keeps a genuine token from reaching
/// a counterfeit one (<https://hl7.org/fhir/smart-app-launch/app-launch.html>). This
/// server publishes one FHIR base per version, so an issuer that mints the
/// claim from that request gives a reader who signed in on `/r5` a different
/// audience from one who signed in on `/r4b`.
#[derive(Debug)]
struct Audiences {
    /// What the FHIR surface accepts: every served version's base under
    /// `FERROTERM_BASE_URL`, and the audience the deployment named.
    ///
    /// The set is one for every version rather than one per version, so a
    /// token minted for `/r5` is spendable on `/r4b` as well. That is our own
    /// design: one process serves the four bases and a reader moves between
    /// them in one session. An issuer that wants a token bounded to one base
    /// says so with `authorization_details`
    /// (<https://hl7.org/fhir/smart-app-launch/app-launch.html>), which this
    /// server does not read.
    fhir: Vec<String>,
    /// What the admin listener accepts: the audience the deployment named.
    admin: Vec<String>,
}

impl Audiences {
    /// The sets this deployment accepts; both empty when it checks none.
    fn of(config: &Config) -> Self {
        let named: Vec<String> = config.oidc_audience.clone().into_iter().collect();
        let mut fhir = Vec::new();
        if let Some(base) = config.base_url.as_deref() {
            for segment in crate::version::SEGMENTS {
                let endpoint = crate::version::endpoint(base, segment);
                // NOTE: FHIR fixes a service base with no trailing slash
                // (<https://hl7.org/fhir/R4B/http.html#root>), so admitting the
                // slashed form is our own tolerance and the only one performed.
                fhir.push(format!("{endpoint}/"));
                fhir.push(endpoint);
            }
        }
        fhir.extend(named.iter().cloned());
        Self { fhir, admin: named }
    }

    /// The audiences a token for `need` may name.
    ///
    /// The admin listener serves no FHIR interaction, so the version bases are
    /// not its audience and only the configured one is: no specification
    /// governs that surface, which is our own design.
    fn accepted(&self, need: Need) -> &[String] {
        match need {
            Need::Write { .. } => &self.fhir,
            Need::Admin => &self.admin,
        }
    }
}

/// A start the issuer refused.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// The discovery document does not arrive or does not parse.
    #[error("the OIDC issuer `{issuer}` does not answer its discovery document")]
    Discovery {
        /// The issuer configured.
        issuer: String,
        /// The cause.
        #[source]
        source: Box<FetchError>,
    },
    /// The issuer's key set does not arrive.
    #[error("the OIDC issuer `{issuer}` does not answer its key set")]
    Keys {
        /// The issuer configured.
        issuer: String,
        /// The cause.
        #[source]
        source: Box<KeyError>,
    },
}

/// What a route requires of the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// A write on one of the stored resource types.
    Write {
        /// The resource type the route writes.
        resource: &'static str,
        /// The interaction it performs.
        permission: Permission,
    },
    /// A route of the admin listener, which the configured admin scope covers.
    Admin,
}

/// The claims this server reads off a validated token.
///
/// The signature and the `iss`, `aud`, `exp`, and `nbf` claims are checked by
/// `jsonwebtoken` against the raw payload, whatever this shape carries
/// (<https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/fn.decode.html>).
#[derive(Debug, serde::Deserialize)]
struct Claims {
    /// RFC 6749 §3.3: the space-delimited grant.
    #[serde(default)]
    scope: Option<String>,
    /// The array form several issuers mint instead of `scope`.
    #[serde(default)]
    scp: Option<Vec<String>>,
}

/// The configured SMART layer: the issuer, its keys, and what it permits.
#[derive(Debug)]
pub struct Smart {
    /// The issuer's discovery document, read at start.
    metadata: IssuerMetadata,
    /// The audiences a token may carry, per surface.
    audiences: Audiences,
    /// The scope the admin listener requires, compared verbatim.
    admin_scope: String,
    /// The protection space the challenge names (RFC 6750 §3).
    realm: String,
    /// The client the issuer is asked with.
    http: Http,
    /// The issuer's signing keys.
    keys: Keys,
}

impl Smart {
    /// Reads the issuer's discovery document and key set, or answers `None`
    /// when the deployment configured no issuer.
    ///
    /// # Errors
    ///
    /// Returns [`StartError`] when the issuer does not answer, which is what
    /// refuses the start: a server that cannot check a token must not serve a
    /// surface it has declared protected.
    pub async fn start(config: &Config) -> Result<Option<Self>, StartError> {
        let Some(issuer) = config.oidc_issuer.clone() else {
            return Ok(None);
        };
        let http = Http::new().map_err(|source| StartError::Discovery {
            issuer: issuer.clone(),
            source: Box::new(source),
        })?;
        let metadata = http
            .discover(&issuer)
            .await
            .map_err(|source| StartError::Discovery {
                issuer: issuer.clone(),
                source: Box::new(source),
            })?;
        let keys = Keys::read(&http, &metadata.jwks_uri)
            .await
            .map_err(|source| StartError::Keys {
                issuer: issuer.clone(),
                source: Box::new(source),
            })?;
        let realm = config
            .base_url
            .clone()
            .unwrap_or_else(|| String::from("FerroTERM"));
        Ok(Some(Self {
            metadata,
            audiences: Audiences::of(config),
            admin_scope: config.oidc_admin_scope.clone(),
            realm,
            http,
            keys,
        }))
    }

    /// The issuer this server accepts tokens from.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.metadata.issuer
    }

    /// The issuer's token endpoint, which a capability statement states.
    #[must_use]
    pub fn token_endpoint(&self) -> &str {
        &self.metadata.token_endpoint
    }

    /// The issuer's authorization endpoint, when it runs one.
    #[must_use]
    pub fn authorize_endpoint(&self) -> Option<&str> {
        self.metadata.authorization_endpoint.as_deref()
    }

    /// The issuer's dynamic registration endpoint, when it runs one.
    #[must_use]
    pub fn register_endpoint(&self) -> Option<&str> {
        self.metadata.registration_endpoint.as_deref()
    }

    /// The issuer's introspection endpoint, when it runs one (RFC 7662).
    #[must_use]
    pub fn introspect_endpoint(&self) -> Option<&str> {
        self.metadata.introspection_endpoint.as_deref()
    }

    /// The issuer's revocation endpoint, when it runs one (RFC 7009).
    #[must_use]
    pub fn revoke_endpoint(&self) -> Option<&str> {
        self.metadata.revocation_endpoint.as_deref()
    }

    /// Whether the issuer runs the authorization code flow a standalone launch
    /// needs (<https://hl7.org/fhir/smart-app-launch/app-launch.html>).
    fn standalone_launch(&self) -> bool {
        self.metadata.authorization_endpoint.is_some()
            && self
                .metadata
                .grant_types_supported
                .iter()
                .any(|grant| grant == AUTHORIZATION_CODE)
    }

    /// Whether the issuer offers the OpenID Connect profile the
    /// `sso-openid-connect` capability rests on.
    fn openid_connect(&self) -> bool {
        self.metadata
            .scopes_supported
            .iter()
            .any(|scope| scope == OPENID)
    }

    /// The SMART capabilities this deployment offers, sorted.
    ///
    /// The scope syntaxes are this server's own; every other capability is read
    /// off the issuer's document and claimed only where the issuer advertises
    /// what it rests on
    /// (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
    #[must_use]
    pub fn capabilities(&self) -> Vec<String> {
        let methods = &self.metadata.token_endpoint_auth_methods_supported;
        // NOTE: `permission-v2` is exemplified only by the search-parameter
        // syntax this server refuses, so withholding it is our own reading
        // (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
        let mut out = vec![
            String::from("permission-v1"),
            String::from("permission-user"),
        ];
        if self.standalone_launch() {
            out.push(String::from("launch-standalone"));
        }
        if self.openid_connect() {
            out.push(String::from("sso-openid-connect"));
        }
        if methods.iter().any(|method| method == "private_key_jwt") {
            out.push(String::from("client-confidential-asymmetric"));
        }
        if methods
            .iter()
            .any(|method| method == "client_secret_basic" || method == "client_secret_post")
        {
            out.push(String::from("client-confidential-symmetric"));
        }
        if methods.iter().any(|method| method == "none") {
            out.push(String::from("client-public"));
        }
        out.sort();
        out
    }

    /// The `.well-known/smart-configuration` document of this deployment.
    ///
    /// The members are the issuer's, with `capabilities` derived from what the
    /// issuer advertises; `token_endpoint`, `grant_types_supported`,
    /// `capabilities`, and `code_challenge_methods_supported` are the required
    /// ones (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
    #[must_use]
    pub fn configuration(&self) -> serde_json::Value {
        let mut document = serde_json::Map::new();
        let mut put = |name: &str, value: serde_json::Value| {
            document.insert(name.to_owned(), value);
        };
        // NOTE: `issuer` is required where `sso-openid-connect` is claimed and
        // omitted otherwise
        // (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
        if self.openid_connect() {
            put("issuer", self.metadata.issuer.as_str().into());
        }
        put("jwks_uri", self.metadata.jwks_uri.as_str().into());
        put(
            "token_endpoint",
            self.metadata.token_endpoint.as_str().into(),
        );
        put(
            "grant_types_supported",
            only(&self.metadata.grant_types_supported, &GRANT_TYPES).into(),
        );
        put("capabilities", self.capabilities().into());
        // NOTE: the member is required and `S256` is a SHALL in it, so the
        // server states its own PKCE support rather than the issuer's list
        // (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
        put(
            "code_challenge_methods_supported",
            CODE_CHALLENGE_METHODS
                .iter()
                .map(|method| (*method).to_owned())
                .collect::<Vec<_>>()
                .into(),
        );
        for (name, endpoint) in [
            ("authorization_endpoint", self.authorize_endpoint()),
            ("registration_endpoint", self.register_endpoint()),
            ("introspection_endpoint", self.introspect_endpoint()),
            ("revocation_endpoint", self.revoke_endpoint()),
        ] {
            if let Some(endpoint) = endpoint {
                put(name, endpoint.into());
            }
        }
        let authentication = only(
            &self.metadata.token_endpoint_auth_methods_supported,
            &TOKEN_ENDPOINT_AUTH_METHODS,
        );
        // NOTE: the server SHALL support every scope it lists here, so a scope
        // the write gate refuses is dropped from the issuer's list
        // (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
        let advertised = self
            .metadata
            .scopes_supported
            .iter()
            .filter(|scope| scopes::serves(scope))
            .cloned()
            .collect();
        for (name, values) in [
            ("scopes_supported", advertised),
            (
                "response_types_supported",
                self.metadata.response_types_supported.clone(),
            ),
            ("token_endpoint_auth_methods_supported", authentication),
        ] {
            if !values.is_empty() {
                put(name, values.into());
            }
        }
        serde_json::Value::Object(document)
    }

    /// Checks the caller's token against what `need` requires.
    ///
    /// # Errors
    ///
    /// Returns the [`Refusal`] to answer with: `401` when no usable token
    /// arrived, `403` when the token carries no scope for the route.
    pub async fn authorize(&self, headers: &HeaderMap, need: Need) -> Result<(), Refusal> {
        let token = bearer(headers).ok_or_else(|| Refusal::unauthenticated(&self.realm))?;
        let header = decode_header(token)
            .map_err(|error| Refusal::invalid_token(&self.realm, &error.to_string()))?;
        if !ALGORITHMS.contains(&header.alg) {
            return Err(Refusal::invalid_token(
                &self.realm,
                &format!(
                    "the token is signed with {:?}, which this server does not accept",
                    header.alg
                ),
            ));
        }
        if let Some(declared) = header.typ.as_deref()
            && !accepted_type(declared)
        {
            return Err(Refusal::invalid_token(
                &self.realm,
                &format!("the token declares the type `{declared}`"),
            ));
        }
        let key = self
            .keys
            .key(&self.http, header.kid.as_deref(), header.alg)
            .await
            .map_err(|error| Refusal::invalid_token(&self.realm, &error.to_string()))?;
        // NOTE: the accepted set is checked above; `Validation` admits one
        // algorithm family at a time, so it names the one the key verifies
        // (<https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/struct.Validation.html>).
        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[self.metadata.issuer.as_str()]);
        // RFC 7519 §4.1.5: the crate does not check `nbf` unless asked.
        validation.validate_nbf = true;
        // RFC 8725 §3.8: a claim the crate checks only when it is present is no
        // check at all, so `iss` is required outright.
        validation.required_spec_claims.insert(String::from("iss"));
        let accepted = self.audiences.accepted(need);
        if accepted.is_empty() {
            // NOTE: RFC 9068 §4 has a resource server check `aud`, so a
            // deployment naming neither a base nor an audience is our own
            // tolerance for the unconfigured case, bounded by the issuer check.
            validation.validate_aud = false;
        } else {
            // NOTE: RFC 7519 §4.1.3 has the recipient identify itself with a
            // value in `aud`, which the crate reads over the string form and
            // the array form alike (<https://docs.rs/jsonwebtoken/11.1.0/jsonwebtoken/struct.Validation.html>).
            validation.set_audience(accepted);
            // RFC 8725 §3.9: a token whose audience is absent is rejected
            // just as one whose audience is another recipient.
            validation.required_spec_claims.insert(String::from("aud"));
        }
        let data = decode::<Claims>(token, &key, &validation)
            .map_err(|error| Refusal::invalid_token(&self.realm, &error.to_string()))?;
        let granted = scopes::granted(data.claims.scope.as_deref(), data.claims.scp.as_deref());
        self.permits(&granted, need)
    }

    /// Whether `granted` covers `need`.
    fn permits(&self, granted: &[String], need: Need) -> Result<(), Refusal> {
        match need {
            Need::Write {
                resource,
                permission,
            } => {
                if granted
                    .iter()
                    .any(|scope| scopes::grants(scope, resource, permission))
                {
                    return Ok(());
                }
                Err(Refusal::forbidden(
                    &self.realm,
                    &permission.scopes_for(resource),
                ))
            }
            Need::Admin => {
                if granted.contains(&self.admin_scope) {
                    return Ok(());
                }
                Err(Refusal::forbidden(
                    &self.realm,
                    std::slice::from_ref(&self.admin_scope),
                ))
            }
        }
    }
}

/// Whether `declared`, the token's `typ` header, names a kind this server
/// spends as an access token.
///
/// RFC 8725 §3.12 asks that the rules for the kinds one issuer mints be
/// mutually exclusive, so an ID token or a registration token typed by its
/// issuer is refused here rather than spent as an access token. A token with no
/// `typ` is judged by its claims alone, which is what RFC 7519 §5.1 leaves open.
fn accepted_type(declared: &str) -> bool {
    let declared = declared.trim();
    // RFC 9068 §2.1 admits the media type with or without its `application/`
    // prefix, and RFC 7519 §5.1 makes the generic `JWT` type legal.
    ["at+jwt", "application/at+jwt", "jwt", "application/jwt"]
        .iter()
        .any(|known| declared.eq_ignore_ascii_case(known))
}

/// The bearer credential of `headers`, when one arrived (RFC 6750 §2.1).
fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(http::header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, credential) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return None;
    }
    let credential = credential.trim();
    (!credential.is_empty()).then_some(credential)
}

/// A request this server will not answer, and the challenge that says why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The status to answer.
    status: StatusCode,
    /// The `OperationOutcome` issue code, of the FHIR issue-type value set
    /// (<https://hl7.org/fhir/R4B/valueset-issue-type.html>).
    code: &'static str,
    /// What the client is told.
    diagnostics: String,
    /// The `WWW-Authenticate` value (RFC 6750 §3).
    challenge: String,
}

impl Refusal {
    /// No credential arrived.
    ///
    /// RFC 6750 §3.1 keeps the error code out of the challenge when the request
    /// carried no authentication at all.
    #[must_use]
    fn unauthenticated(realm: &str) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "login",
            diagnostics: String::from("this request requires an OAuth 2.0 bearer token"),
            challenge: format!("Bearer realm=\"{}\"", quoted(realm)),
        }
    }

    /// A credential arrived and did not verify (RFC 6750 §3.1, `invalid_token`).
    ///
    /// The reason can quote what the token said, so it is bounded and reduced
    /// before it reaches either the header or the body.
    #[must_use]
    fn invalid_token(realm: &str, reason: &str) -> Self {
        let reason = &quoted(reason);
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "login",
            diagnostics: format!("the bearer token is not valid: {reason}"),
            challenge: format!(
                "Bearer realm=\"{}\", error=\"invalid_token\", error_description=\"{}\"",
                quoted(realm),
                quoted(reason)
            ),
        }
    }

    /// The token verified and carries none of the scopes the route accepts
    /// (RFC 6750 §3.1, `insufficient_scope`).
    ///
    /// The optional `scope` attribute is sent only where one scope opens the
    /// route: RFC 6750 §3 reads it as "the required scope of the access token",
    /// a space-delimited set the client would have to hold in full (RFC 6749
    /// §3.3), so alternatives are named in the outcome instead.
    #[must_use]
    fn forbidden(realm: &str, wanted: &[String]) -> Self {
        let named = wanted
            .iter()
            .map(|scope| format!("`{scope}`"))
            .collect::<Vec<_>>()
            .join(" or ");
        let attribute = match wanted {
            [one] => format!(", scope=\"{}\"", quoted(one)),
            _ => String::new(),
        };
        Self {
            status: StatusCode::FORBIDDEN,
            code: "forbidden",
            diagnostics: format!("the bearer token carries no {named} scope"),
            challenge: format!(
                "Bearer realm=\"{}\", error=\"insufficient_scope\"{attribute}",
                quoted(realm)
            ),
        }
    }

    /// The refusal as an `OperationOutcome`, in the format `headers` asks for.
    #[must_use]
    pub fn fhir_response(&self, headers: &HeaderMap) -> Response {
        // NOTE: an `Accept` this server does not speak is a second fault on a
        // request already refused, so the outcome answers in the default format
        // rather than replacing the security refusal with a format one.
        let wire = crate::wire::Wire::negotiate(&[], headers).unwrap_or_default();
        let mut response =
            Failure::new(self.status, self.code, self.diagnostics.clone()).respond(wire);
        self.challenge_header(&mut response);
        response
    }

    /// The refusal in the shape the admin listener answers in, which is plain
    /// JSON and never FHIR.
    #[must_use]
    pub fn admin_response(&self) -> Response {
        let body = serde_json::json!({
            "outcome": crate::metrics::Outcome::Failed.as_str(),
            "reason": self.diagnostics,
        });
        let mut response = match serde_json::to_vec(&body) {
            Ok(bytes) => Response::builder()
                .status(self.status)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(bytes))
                .unwrap_or_else(|_| self.status.into_response()),
            Err(_) => self.status.into_response(),
        };
        self.challenge_header(&mut response);
        response
    }

    /// Adds the `WWW-Authenticate` challenge to `response`.
    ///
    /// RFC 9110 §15.5.2 makes the header mandatory on a `401`, so a challenge
    /// that somehow does not read as a header value falls back to the bare
    /// scheme rather than being dropped.
    fn challenge_header(&self, response: &mut Response) {
        let value = HeaderValue::from_str(&self.challenge)
            .unwrap_or_else(|_| HeaderValue::from_static("Bearer"));
        response
            .headers_mut()
            .insert(http::header::WWW_AUTHENTICATE, value);
    }
}

/// The longest a challenge parameter this server writes may be.
///
/// No specification bounds it: our own design, so a `kid` or an issuer message
/// echoed back cannot grow the response header.
const PARAMETER_LIMIT: usize = 200;

/// `text` reduced to what a challenge `quoted-string` may carry.
///
/// RFC 6750 §3 limits the parameter value to `%x20-21 / %x23-5B / %x5D-7E`,
/// which leaves out the quote and the backslash that would end the value early
/// and every control or non-ASCII byte a header cannot hold.
fn quoted(text: &str) -> String {
    text.chars()
        .filter(|character| matches!(*character, ' '..='!' | '#'..='[' | ']'..='~'))
        .take(PARAMETER_LIMIT)
        .collect()
}

/// `GET [base]/.well-known/smart-configuration`.
///
/// A deployment that configured no issuer serves nothing here, so the answer is
/// the `not-found` outcome any other unknown path gives.
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
    use http::HeaderMap;

    use super::{Audiences, Need, bearer, quoted};
    use crate::config::Config;
    use crate::smart::scopes::Permission;

    /// A write, which is what the FHIR surface asks a token for.
    const WRITE: Need = Need::Write {
        resource: "ValueSet",
        permission: Permission::Update,
    };

    #[test]
    fn a_bearer_credential_is_read_whatever_the_case_of_its_scheme() {
        let mut headers = HeaderMap::new();
        assert_eq!(bearer(&headers), None);
        headers.insert(http::header::AUTHORIZATION, "bearer abc".parse().unwrap());
        assert_eq!(bearer(&headers), Some("abc"));
        headers.insert(http::header::AUTHORIZATION, "Bearer  abc ".parse().unwrap());
        assert_eq!(bearer(&headers), Some("abc"));
        headers.insert(http::header::AUTHORIZATION, "Basic abc".parse().unwrap());
        assert_eq!(bearer(&headers), None, "another scheme is not a token");
        headers.insert(http::header::AUTHORIZATION, "Bearer ".parse().unwrap());
        assert_eq!(bearer(&headers), None, "an empty credential is none");
    }

    #[test]
    fn a_challenge_parameter_cannot_carry_a_quote_or_a_backslash() {
        assert_eq!(quoted(r#"a"b\c"#), "abc");
        assert_eq!(quoted("plain"), "plain");
    }

    // The `aud` a client sends is the FHIR base it is talking to
    // (<https://hl7.org/fhir/smart-app-launch/app-launch.html>), and this
    // server has one per version.
    #[test]
    fn every_served_version_base_is_an_audience_beside_the_configured_one() {
        let audiences = Audiences::of(&Config {
            base_url: Some(String::from("https://tx.example.org")),
            oidc_audience: Some(String::from("ferroterm")),
            ..Config::default()
        });
        let accepted = audiences.accepted(WRITE);
        for segment in crate::version::SEGMENTS {
            let base = format!("https://tx.example.org/{segment}");
            assert!(accepted.contains(&base), "{base}: {accepted:?}");
            assert!(accepted.contains(&format!("{base}/")), "{base}/");
        }
        assert!(
            accepted.contains(&String::from("ferroterm")),
            "the configured audience stays accepted: {accepted:?}"
        );
        assert!(
            !accepted.contains(&String::from("https://tx.example.org")),
            "the root serves no FHIR base: {accepted:?}"
        );
    }

    // The admin listener serves no FHIR interaction, so a base URL is not an
    // audience for it and naming one leaves it as it was.
    #[test]
    fn the_admin_listener_takes_only_the_configured_audience() {
        let audiences = Audiences::of(&Config {
            base_url: Some(String::from("https://tx.example.org")),
            oidc_audience: Some(String::from("ferroterm")),
            ..Config::default()
        });
        assert_eq!(
            audiences.accepted(Need::Admin),
            [String::from("ferroterm")],
            "no version base is an admin audience"
        );
        let base_only = Audiences::of(&Config {
            base_url: Some(String::from("https://tx.example.org")),
            ..Config::default()
        });
        assert!(
            base_only.accepted(Need::Admin).is_empty(),
            "a deployment naming no audience checks none there, as before"
        );
    }

    #[test]
    fn a_deployment_naming_no_base_and_no_audience_checks_none() {
        let none = Audiences::of(&Config::default());
        assert!(none.accepted(WRITE).is_empty());
        assert!(none.accepted(Need::Admin).is_empty());
        let only_configured = Audiences::of(&Config {
            oidc_audience: Some(String::from("ferroterm")),
            ..Config::default()
        });
        assert_eq!(only_configured.accepted(WRITE), [String::from("ferroterm")]);
    }

    // A base URL with a trailing slash would otherwise double the separator
    // and refuse every real token.
    #[test]
    fn a_base_url_that_ends_in_a_slash_derives_the_same_audiences() {
        let audiences = Audiences::of(&Config {
            base_url: Some(String::from("https://tx.example.org/")),
            ..Config::default()
        });
        assert!(
            audiences
                .accepted(WRITE)
                .contains(&String::from("https://tx.example.org/r4b")),
            "{:?}",
            audiences.accepted(WRITE)
        );
    }
}
