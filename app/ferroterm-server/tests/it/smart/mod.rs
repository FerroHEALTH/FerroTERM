// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resource server over the write routes and the admin listener:
//! `ferrohealth-smart`, configured by `[auth]`.
//!
//! The issuer is a `wiremock` server that publishes an OpenID Connect discovery
//! document and a JWKS; the RFC 9068 access tokens are minted in the test with
//! a key generated for the run, so no key material is committed. The requirements asserted are
//! SMART App Launch conformance and discovery
//! (<https://hl7.org/fhir/smart-app-launch/conformance.html>), the version 2
//! scope syntax
//! (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>),
//! and the bearer challenge of RFC 6750 §3.

use axum::body::Body;
use base64::Engine;
use http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use http::{Request, StatusCode};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::fixture::{self, Server};

mod admin;
mod audience;
mod basic;
mod discovery;
mod introspection;
mod keys;
mod scopes;
mod tokens;

pub(super) const COLOURS: &str = "http://ferroterm.test/CodeSystem/colours";

pub(super) fn colours() -> Value {
    json!({
        "resourceType": "CodeSystem",
        "url": COLOURS,
        "version": "1.0",
        "status": "active",
        "content": "complete",
        "concept": [{"code": "red", "display": "Red"}]
    })
}

/// One ECDSA P-256 signing key, generated for this test run.
pub(super) struct SigningKey {
    /// The PKCS#8 private key `jsonwebtoken` signs with.
    pkcs8: Vec<u8>,
    /// The uncompressed public point, `0x04 || X || Y` (RFC 5480 §2.2).
    public: Vec<u8>,
    /// The `kid` the JWK and the token header carry (RFC 7517 §4.5).
    kid: String,
}

impl SigningKey {
    pub(super) fn generate(kid: &str) -> Self {
        let rng = SystemRandom::new();
        let document = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng)
            .expect("generates a key");
        let pair =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, document.as_ref(), &rng)
                .expect("reads the key back");
        Self {
            pkcs8: document.as_ref().to_vec(),
            public: pair.public_key().as_ref().to_vec(),
            kid: kid.to_owned(),
        }
    }

    /// The public key as a JWK (RFC 7517 §4, RFC 7518 §6.2.1).
    pub(super) fn jwk(&self) -> Value {
        let url = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let x = self.public.get(1..33).expect("the point carries X");
        let y = self.public.get(33..65).expect("the point carries Y");
        json!({
            "kty": "EC",
            "crv": "P-256",
            "alg": "ES256",
            "use": "sig",
            "kid": self.kid,
            "x": url.encode(x),
            "y": url.encode(y),
        })
    }

    /// `claims` signed as an ES256 token naming this key.
    pub(super) fn sign(&self, claims: &Value) -> String {
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(self.kid.clone());
        // RFC 9068 §2.1: an access token is typed `at+jwt`.
        header.typ = Some(String::from("at+jwt"));
        self.sign_with(&header, claims)
    }

    /// `claims` signed with this key under a header the caller chose.
    pub(super) fn sign_with(&self, header: &Header, claims: &Value) -> String {
        encode(header, claims, &EncodingKey::from_ec_der(&self.pkcs8)).expect("signs")
    }
}

/// The claims of an RFC 9068 access token minted for `issuer`.
///
/// RFC 9068 §2.2 requires `iss`, `exp`, `aud`, `sub`, `client_id`, `iat` and
/// `jti`.
pub(super) fn claims(issuer: &str, scope: &str, expires_in: i64) -> Value {
    let now = jiff::Timestamp::now().as_second();
    json!({
        "iss": issuer,
        "sub": "ferroterm-test-client",
        "client_id": "ferroterm-test-client",
        "jti": format!("jti-{now}"),
        "aud": fixture::AUDIENCE,
        "iat": now,
        "nbf": now - 5,
        "exp": now + expires_in,
        "scope": scope,
    })
}

/// The discovery document of the test issuer.
pub(super) fn discovery(base: &str) -> Value {
    json!({
        "issuer": base,
        "jwks_uri": format!("{base}/jwks"),
        "authorization_endpoint": format!("{base}/authorize"),
        "token_endpoint": format!("{base}/token"),
        "introspection_endpoint": format!("{base}/introspect"),
        // The extra values are what a real issuer publishes and SMART does not
        // define for these members; the served document drops them.
        "grant_types_supported": ["client_credentials", "authorization_code", "refresh_token"],
        "scopes_supported": ["system/CodeSystem.cud", "system/ValueSet.cud"],
        "response_types_supported": ["code"],
        "token_endpoint_auth_methods_supported": ["private_key_jwt", "client_secret_jwt"],
        "code_challenge_methods_supported": ["plain", "S256"],
    })
}

/// The discovery document of an issuer a public client signs in to: no client
/// authentication and the OpenID Connect scope.
pub(super) fn public_discovery(base: &str) -> Value {
    json!({
        "issuer": base,
        "jwks_uri": format!("{base}/jwks"),
        "authorization_endpoint": format!("{base}/authorize"),
        "token_endpoint": format!("{base}/token"),
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "scopes_supported": ["openid", "fhirUser", "user/CodeSystem.cud"],
        "response_types_supported": ["code"],
        "token_endpoint_auth_methods_supported": ["none", "client_secret_basic"],
        "code_challenge_methods_supported": ["plain", "S256"],
    })
}

/// The discovery document of an issuer that omits every optional member and
/// advertises a patient scope this server refuses.
pub(super) fn sparse_discovery(base: &str) -> Value {
    json!({
        "issuer": base,
        "jwks_uri": format!("{base}/jwks"),
        "authorization_endpoint": format!("{base}/authorize"),
        "token_endpoint": format!("{base}/token"),
        "scopes_supported": ["openid", "patient/CodeSystem.cud", "launch/patient", "user/ValueSet.cud"],
    })
}

/// A `wiremock` issuer publishing the key set and the document `build` writes
/// for its own base URL.
pub(super) async fn issuer_publishing(key: &SigningKey, build: fn(&str) -> Value) -> MockServer {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(build(&base)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"keys": [key.jwk()]})))
        .mount(&server)
        .await;
    server
}

/// A `wiremock` issuer publishing the document and the key set.
pub(super) async fn issuer(key: &SigningKey) -> MockServer {
    issuer_publishing(key, discovery).await
}

/// A `PUT` of `body` at `uri` carrying `token`, when one is given.
pub(super) async fn put_with(
    server: &Server,
    uri: &str,
    body: &Value,
    token: Option<&str>,
) -> http::Response<Body> {
    let mut request = Request::put(uri).header(http::header::CONTENT_TYPE, "application/fhir+json");
    if let Some(token) = token {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = request.body(Body::from(body.to_string())).expect("request");
    server.send(request).await
}

/// A `PUT` of the colours code system carrying `token`, when one is given.
pub(super) async fn put_colours(server: &Server, token: Option<&str>) -> http::Response<Body> {
    put_with(server, "/r4b/CodeSystem/colours", &colours(), token).await
}

/// The capability strings of a served SMART configuration document.
pub(super) fn capabilities(document: &Value) -> Vec<String> {
    document["capabilities"]
        .as_array()
        .expect("capabilities are required")
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// The served SMART configuration document of `version`.
pub(super) async fn configuration(server: &Server, version: &str) -> Value {
    let (code, content_type, body) = server
        .get_text(
            &format!("/{version}/.well-known/smart-configuration"),
            Some("application/json"),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{version}: {body}");
    assert_eq!(content_type, "application/json", "{version}");
    serde_json::from_str(&body).expect("json")
}

/// The `WWW-Authenticate` value of a response.
pub(super) fn challenge(response: &http::Response<Body>) -> String {
    fixture::header(response, WWW_AUTHENTICATE).unwrap_or_default()
}

/// A `POST /reload` carrying `authorization` as its credential, when given.
pub(super) fn reload(authorization: Option<String>) -> Request<Body> {
    let mut request = Request::post("/reload");
    if let Some(value) = authorization {
        request = request.header(AUTHORIZATION, value);
    }
    request.body(Body::empty()).expect("request")
}

/// `error` and every cause under it, on one line.
pub(super) fn causes(error: &dyn std::error::Error) -> String {
    let mut out = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        out.push_str(": ");
        out.push_str(&cause.to_string());
        source = cause.source();
    }
    out
}

/// The address a proxy reaches the server at in the audience tests.
pub(super) const PROXIED: &str = "https://tx.example.org";
