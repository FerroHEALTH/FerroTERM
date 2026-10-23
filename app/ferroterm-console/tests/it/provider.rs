// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A test OpenID Provider on a `wiremock` server, and the console wired to
//! it.
//!
//! The provider publishes the public half of an ECDSA P-256 key generated
//! for the run as a JWK Set (RFC 7517 §5), signs the ID Tokens a test writes
//! (OpenID Connect Core 1.0 §2), and answers its token and revocation
//! endpoints with whatever a test mounts.

use std::collections::BTreeMap;

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair};
use axum::body::Body;
use ferroterm_console::config::{Config, Oidc};
use ferroterm_console::server::{ConsoleState, router};
use http::header::{COOKIE, LOCATION, SET_COOKIE};
use http::{HeaderMap, Request, Response};
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};
use tower::ServiceExt as _;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The client id the console is registered under.
pub(crate) const CLIENT_ID: &str = "ferroterm-console";

/// The console's redirection endpoint.
pub(crate) const REDIRECT_URI: &str = "http://127.0.0.1:3000/auth/callback";

/// The `kid` the provider's key is published under.
pub(crate) const KID: &str = "k1";

/// The path of the provider's token endpoint.
pub(crate) const TOKEN_PATH: &str = "/token";

/// The path of the provider's JWK Set.
pub(crate) const JWKS_PATH: &str = "/jwks.json";

/// The path of the provider's revocation endpoint.
pub(crate) const REVOCATION_PATH: &str = "/revoke";

/// The path of the provider's end-session endpoint.
pub(crate) const END_SESSION_PATH: &str = "/end-session";

/// One ECDSA P-256 signing key generated for the run.
pub(crate) struct SigningKey {
    /// The private half `jsonwebtoken` signs with.
    pub(crate) encoding: EncodingKey,
    /// The public half as a JWK, published under `kid`.
    pub(crate) jwk: Jwk,
}

impl SigningKey {
    /// A fresh key, published under `kid`.
    pub(crate) fn generate(kid: &str) -> Self {
        let pkcs8 =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
                .expect("a key pair is generated");
        let encoding = EncodingKey::from_ec_der(pkcs8.as_ref());
        let mut jwk = Jwk::from_encoding_key(&encoding, Algorithm::ES256).expect("a JWK");
        jwk.common.key_id = Some(kid.to_owned());
        Self { encoding, jwk }
    }

    /// `claims` signed with this key under `header`.
    pub(crate) fn sign(&self, header: &Header, claims: &Value) -> String {
        jsonwebtoken::encode(header, claims, &self.encoding).expect("the token is signed")
    }
}

/// The test OpenID Provider.
pub(crate) struct Provider {
    /// The server every endpoint answers on.
    pub(crate) server: MockServer,
    /// The key the published JWK Set holds.
    pub(crate) key: SigningKey,
}

impl Provider {
    /// Starts a provider that publishes its key set.
    pub(crate) async fn start() -> Self {
        let provider = Self::bare().await;
        provider
            .publish(std::slice::from_ref(&provider.key.jwk))
            .await;
        provider
    }

    /// Starts a provider that answers nothing yet.
    pub(crate) async fn bare() -> Self {
        Self {
            server: MockServer::start().await,
            key: SigningKey::generate(KID),
        }
    }

    /// Publishes `keys` as the provider's JWK Set.
    pub(crate) async fn publish(&self, keys: &[Jwk]) {
        let set = JwkSet {
            keys: keys.to_vec(),
        };
        Mock::given(method("GET"))
            .and(path(JWKS_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(&set))
            .mount(&self.server)
            .await;
    }

    /// Drops every mounted answer and every recorded request, and publishes
    /// the key set again.
    pub(crate) async fn reset(&self) {
        self.server.reset().await;
        self.publish(std::slice::from_ref(&self.key.jwk)).await;
    }

    /// The issuer, the provider's base URL.
    pub(crate) fn issuer(&self) -> String {
        self.server.uri()
    }

    /// The URL of `path` at the provider.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.server.uri())
    }

    /// The `[oidc]` section of a console signed in at this provider, a public
    /// client with the end-session and revocation endpoints left out.
    pub(crate) fn oidc(&self) -> Oidc {
        Oidc {
            issuer: self.issuer(),
            authorization_endpoint: self.url("/authorize"),
            token_endpoint: self.url(TOKEN_PATH),
            jwks_uri: self.url(JWKS_PATH),
            client_id: String::from(CLIENT_ID),
            redirect_uri: String::from(REDIRECT_URI),
            scopes: vec![String::from("openid"), String::from("user/*.cruds")],
            ..Oidc::default()
        }
    }

    /// The standard claims of an ID Token for the sign-in that sent `nonce`,
    /// valid for five minutes.
    pub(crate) fn claims(&self, nonce: &str) -> Value {
        let now = jiff::Timestamp::now().as_second();
        json!({
            "iss": self.issuer(),
            "sub": "operator-1",
            "aud": CLIENT_ID,
            "iat": now,
            "exp": now + 300,
            "nonce": nonce,
            "name": "Synthetic Operator",
        })
    }

    /// `claims` signed with the published key under its `kid`.
    pub(crate) fn id_token(&self, claims: &Value) -> String {
        self.key.sign(&es256(Some(KID)), claims)
    }

    /// Answers every token request with `status` and the JSON `body`.
    pub(crate) async fn answer_tokens(&self, status: u16, body: &Value) {
        Mock::given(method("POST"))
            .and(path(TOKEN_PATH))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(&self.server)
            .await;
    }

    /// The requests the provider received at `path`.
    pub(crate) async fn requests_at(&self, path: &str) -> Vec<wiremock::Request> {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|request| request.url.path() == path)
            .collect()
    }
}

/// An `ES256` header naming `kid`.
pub(crate) fn es256(kid: Option<&str>) -> Header {
    let mut header = Header::new(Algorithm::ES256);
    header.kid = kid.map(str::to_owned);
    header
}

/// A bearer token answer carrying `id_token`.
pub(crate) fn token_answer(id_token: &str) -> Value {
    json!({
        "access_token": "synthetic-access-token",
        "token_type": "Bearer",
        "expires_in": 3600,
        "refresh_token": "synthetic-refresh-token",
        "id_token": id_token,
    })
}

/// The console's state for `oidc`, or for no sign-in at all, with the
/// session settings `session` writes as TOML.
pub(crate) fn console(oidc: Option<Oidc>, session: &str) -> ConsoleState {
    let mut config = Config::from_sources(Some(session), &BTreeMap::new()).expect("a config");
    config.oidc = oidc;
    let settings = config.resolve().expect("the settings resolve");
    ConsoleState::new(settings).expect("the clients build")
}

/// Sends `request` through the console's whole router over `state`.
pub(crate) async fn send(state: &ConsoleState, request: Request<Body>) -> Response<Body> {
    router(state.clone())
        .oneshot(request)
        .await
        .expect("the router answers")
}

/// A `GET` of `uri` carrying `cookies`.
pub(crate) fn get(uri: &str, cookies: &[String]) -> Request<Body> {
    let mut request = Request::get(uri);
    for cookie in cookies {
        request = request.header(COOKIE, cookie);
    }
    request.body(Body::empty()).expect("a request")
}

/// The `Location` a response redirects to.
pub(crate) fn location(response: &Response<Body>) -> String {
    response
        .headers()
        .get(LOCATION)
        .expect("a Location header")
        .to_str()
        .expect("ASCII")
        .to_owned()
}

/// The `Location` a response redirects to, read as an absolute URL, and its
/// query pairs.
pub(crate) fn redirect_query(response: &Response<Body>) -> (Url, BTreeMap<String, String>) {
    let url = Url::parse(&location(response)).expect("an absolute URL");
    let query = url.query_pairs().into_owned().collect();
    (url, query)
}

/// Every `Set-Cookie` header of `headers`.
pub(crate) fn set_cookies(headers: &HeaderMap) -> Vec<String> {
    headers
        .get_all(SET_COOKIE)
        .iter()
        .map(|value| value.to_str().expect("ASCII").to_owned())
        .collect()
}

/// The `name=value` pair of the cookie `name` that `headers` set with a
/// value.
pub(crate) fn cookie_pair(headers: &HeaderMap, name: &str) -> Option<String> {
    set_cookies(headers).into_iter().find_map(|cookie| {
        let pair = cookie.split(';').next()?.trim().to_owned();
        let (key, value) = pair.split_once('=')?;
        (key == name && !value.is_empty()).then_some(pair)
    })
}

/// The text body of `response`.
pub(crate) async fn text(response: Response<Body>) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a body");
    String::from_utf8(bytes.to_vec()).expect("UTF-8")
}

/// The form fields of a request body.
pub(crate) fn form(request: &wiremock::Request) -> BTreeMap<String, String> {
    url::form_urlencoded::parse(&request.body)
        .into_owned()
        .collect()
}
