// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener behind `ferrohealth-smart`: every route but
//! `GET /health` needs a credential carrying the product scope, and an
//! `[auth]` that admits no credential refuses the start.
//!
//! No specification governs the admin surface: our own design.

use argon2::Argon2;
use argon2::password_hash::PasswordHasher as _;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use ferrohealth_smart::config::{
    AuthMode, AuthSection, BasicSection, BasicUser, IntrospectionSection, IssuerKey, OauthSection,
};
use ferroterm_sync::auth::{AdminAuth, AuthError, SYNC_SCOPE};
use secrecy::SecretString;
use tower::ServiceExt as _;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{Harness, TestClock};

/// The password of every test user, synthetic.
const PASSWORD: &str = "a-synthetic-password";

/// An Argon2id PHC string of [`PASSWORD`], salted with a fixed test salt.
fn phc() -> SecretString {
    let hash = Argon2::default()
        .hash_password_with_salt(PASSWORD.as_bytes(), b"ferroterm-sync-t")
        .expect("hashes");
    SecretString::from(hash.to_string())
}

/// `Basic` credentials of `user` (RFC 7617 §2).
fn basic(user: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{PASSWORD}"));
    format!("Basic {encoded}")
}

/// An `[auth]` admitting two Basic users: `operator` with the product scope
/// and `reader` with an unrelated one.
fn section() -> AuthSection {
    AuthSection {
        basic: Some(BasicSection {
            users: vec![
                BasicUser {
                    user: String::from("operator"),
                    password_hash: Some(phc()),
                    scopes: vec![String::from(SYNC_SCOPE)],
                    ..BasicUser::default()
                },
                BasicUser {
                    user: String::from("reader"),
                    password_hash: Some(phc()),
                    scopes: vec![String::from("system/CodeSystem.rs")],
                    ..BasicUser::default()
                },
            ],
            ..BasicSection::default()
        }),
        ..AuthSection::default()
    }
}

/// The guarded admin router over a service that has run nothing.
async fn guarded() -> Router {
    let harness = Harness::new().await;
    let service = harness.service(Vec::new(), TestClock::new("2026-10-01T03:00:00Z", 0));
    let auth = AdminAuth::start(&section())
        .await
        .expect("the section starts")
        .expect("authentication is on");
    auth.guard(ferroterm_sync::admin::router(service))
}

/// The status `method` on `path` answers with `authorization`.
async fn status(
    router: &Router,
    method: &str,
    path: &str,
    authorization: Option<&str>,
) -> (StatusCode, Option<String>) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(value) = authorization {
        request = request.header("authorization", value);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::empty()).expect("a request"))
        .await
        .expect("the listener answers");
    let challenge = response
        .headers()
        .get("www-authenticate")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    (response.status(), challenge)
}

#[tokio::test]
async fn the_health_probe_needs_no_credential() {
    let router = guarded().await;
    assert_eq!(
        status(&router, "GET", "/health", None).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn the_liveness_and_readiness_probes_need_no_credential() {
    let router = guarded().await;
    for path in ["/health/liveness", "/health/readiness"] {
        assert_eq!(
            status(&router, "GET", path, None).await.0,
            StatusCode::OK,
            "{path}"
        );
    }
}

#[tokio::test]
async fn every_other_route_refuses_a_request_without_a_credential() {
    let router = guarded().await;
    for (method, path) in [
        ("POST", "/run"),
        ("POST", "/activate"),
        ("GET", "/runs"),
        ("GET", "/runs/2026-10-01T03-00-00Z"),
        ("GET", "/metrics"),
        ("GET", "/elsewhere"),
    ] {
        let (answered, challenge) = status(&router, method, path, None).await;
        assert_eq!(answered, StatusCode::UNAUTHORIZED, "{method} {path}");
        assert!(
            challenge.is_some_and(|sent| sent.contains("Basic realm=\"ferroterm-sync\"")),
            "{method} {path} names the realm in its challenge"
        );
    }
}

#[tokio::test]
async fn a_credential_without_the_product_scope_is_forbidden() {
    let router = guarded().await;
    let reader = basic("reader");
    for (method, path) in [("GET", "/runs"), ("GET", "/metrics"), ("POST", "/activate")] {
        let (answered, _) = status(&router, method, path, Some(&reader)).await;
        assert_eq!(answered, StatusCode::FORBIDDEN, "{method} {path}");
    }
}

#[tokio::test]
async fn a_wrong_password_is_refused() {
    let router = guarded().await;
    let encoded = base64::engine::general_purpose::STANDARD.encode("operator:not-the-password");
    let (answered, _) = status(&router, "GET", "/runs", Some(&format!("Basic {encoded}"))).await;
    assert_eq!(answered, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_product_scope_reaches_the_records_and_the_metrics() {
    let router = guarded().await;
    let operator = basic("operator");
    assert_eq!(
        status(&router, "GET", "/runs", Some(&operator)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        status(&router, "GET", "/metrics", Some(&operator)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        status(&router, "GET", "/runs/no-such-run", Some(&operator))
            .await
            .0,
        StatusCode::NOT_FOUND,
        "past the gate, the route answers as it would unguarded"
    );
}

#[tokio::test]
async fn a_section_admitting_no_credential_refuses_the_start() {
    let refused = AdminAuth::start(&AuthSection::default()).await;
    assert!(
        matches!(refused, Err(AuthError::Config { .. })),
        "an enabled [auth] with no issuer and no Basic user fails closed: {refused:?}"
    );
}

/// The audience the introspection tests name.
const AUDIENCE: &str = "ferroterm-sync";

/// A `wiremock` issuer whose metadata names an introspection endpoint
/// (RFC 8414 §2, `introspection_endpoint`), and nothing mounted there yet.
async fn introspecting_issuer() -> MockServer {
    let server = MockServer::start().await;
    let base = server.uri();
    let metadata = format!(
        r#"{{"issuer":"{base}","token_endpoint":"{base}/token","introspection_endpoint":"{base}/introspect","jwks_uri":"{base}/jwks"}}"#
    );
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(metadata, "application/json"))
        .mount(&server)
        .await;
    server
}

/// An `[auth]` introspecting the tokens of `issuer` under a client secret.
fn introspecting(issuer: &str) -> AuthSection {
    AuthSection {
        issuer: IssuerKey::Flat(issuer.to_owned()),
        audience: vec![String::from(AUDIENCE)],
        mode: AuthMode::Introspection,
        introspection: Some(IntrospectionSection {
            client: Some(OauthSection {
                client_id: String::from("ferroterm-sync"),
                client_secret: Some(SecretString::from("a-synthetic-secret")),
                ..OauthSection::default()
            }),
            ..IntrospectionSection::default()
        }),
        ..AuthSection::default()
    }
}

/// The endpoint answers `body` with `status` for the token `token`.
async fn answers(issuer: &MockServer, token: &str, status: u16, body: String) {
    Mock::given(method("POST"))
        .and(path("/introspect"))
        .and(body_string_contains(format!("token={token}")))
        .respond_with(ResponseTemplate::new(status).set_body_raw(body, "application/json"))
        .mount(issuer)
        .await;
}

/// An active introspection response (RFC 7662 §2.2) for `issuer` carrying
/// `scope`, with the members SMART App Launch 2.2.0, Token Introspection,
/// requires.
fn active(issuer: &str, scope: &str) -> String {
    let exp = jiff::Timestamp::now().as_second() + 300;
    format!(
        r#"{{"active":true,"iss":"{issuer}","aud":"{AUDIENCE}","sub":"operator","client_id":"operator","scope":"{scope}","exp":{exp}}}"#
    )
}

/// The guarded admin router over an introspecting `[auth]`.
async fn introspected(issuer: &MockServer) -> Router {
    let harness = Harness::new().await;
    let service = harness.service(Vec::new(), TestClock::new("2026-10-01T03:00:00Z", 0));
    let auth = AdminAuth::start(&introspecting(&issuer.uri()))
        .await
        .expect("the introspecting section starts")
        .expect("authentication is on");
    auth.guard(ferroterm_sync::admin::router(service))
}

// RFC 7662 §2.2: an active response admits the token with the scope it
// names, so the product scope opens the records and another scope does not.
#[tokio::test]
async fn an_active_introspected_token_is_admitted_with_its_scopes() {
    let issuer = introspecting_issuer().await;
    answers(
        &issuer,
        "operator-token",
        200,
        active(&issuer.uri(), SYNC_SCOPE),
    )
    .await;
    answers(
        &issuer,
        "reader-token",
        200,
        active(&issuer.uri(), "system/CodeSystem.rs"),
    )
    .await;
    let router = introspected(&issuer).await;
    assert_eq!(
        status(&router, "GET", "/runs", Some("Bearer operator-token"))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        status(&router, "GET", "/runs", Some("Bearer reader-token"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}

// RFC 7662 §2.2: a response whose `active` is false refuses the token, and
// RFC 6750 §3.1 answers it `invalid_token`.
#[tokio::test]
async fn an_inactive_introspected_token_is_refused() {
    let issuer = introspecting_issuer().await;
    answers(
        &issuer,
        "revoked-token",
        200,
        String::from(r#"{"active":false}"#),
    )
    .await;
    let router = introspected(&issuer).await;
    let (answered, challenge) = status(&router, "GET", "/runs", Some("Bearer revoked-token")).await;
    assert_eq!(answered, StatusCode::UNAUTHORIZED);
    assert!(
        challenge.is_some_and(|sent| sent.contains("error=\"invalid_token\"")),
        "the challenge names invalid_token"
    );
}

// An endpoint that cannot answer is an upstream failure, never a refusal of
// the token: our own design over the crate's `503`.
#[tokio::test]
async fn a_failing_introspection_endpoint_answers_503() {
    let issuer = introspecting_issuer().await;
    answers(&issuer, "any-token", 500, String::from("{}")).await;
    let router = introspected(&issuer).await;
    assert_eq!(
        status(&router, "GET", "/runs", Some("Bearer any-token"))
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

// The introspecting issuer is read at the start, so one whose metadata names
// no introspection endpoint refuses the start by name.
#[tokio::test]
async fn an_issuer_without_an_introspection_endpoint_refuses_the_start() {
    let issuer = MockServer::start().await;
    let base = issuer.uri();
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"{{"issuer":"{base}","token_endpoint":"{base}/token","jwks_uri":"{base}/jwks"}}"#
            ),
            "application/json",
        ))
        .mount(&issuer)
        .await;
    let refused = AdminAuth::start(&introspecting(&base)).await;
    assert!(
        matches!(refused, Err(AuthError::Introspector { .. })),
        "the refusal names the introspection endpoint: {refused:?}"
    );
}

#[tokio::test]
async fn enabled_false_turns_the_guard_off() {
    let off = AuthSection {
        enabled: false,
        ..AuthSection::default()
    };
    assert!(AdminAuth::start(&off).await.expect("starts").is_none());
}

#[tokio::test]
async fn a_deployment_names_its_own_product_scope() {
    let named = AuthSection {
        product_scope: String::from("__example.sync"),
        ..section()
    };
    let auth = AdminAuth::start(&named).await.expect("starts").expect("on");
    assert_eq!(auth.scope(), "__example.sync");
}

/// What `method` on `path` answers with `authorization`: the status, the
/// content type and the body read as JSON.
async fn answered(
    router: &Router,
    method: &str,
    path: &str,
    authorization: Option<&str>,
) -> (StatusCode, Option<String>, serde_json::Value) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(value) = authorization {
        request = request.header("authorization", value);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::empty()).expect("a request"))
        .await
        .expect("the listener answers");
    let status = response.status();
    let kind = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("a body");
    (status, kind, serde_json::from_slice(&bytes).expect("JSON"))
}

#[tokio::test]
async fn every_refusal_and_error_is_a_problem_that_quotes_nothing_it_was_sent() {
    let router = guarded().await;
    let mut bodies = String::new();
    let encoded =
        base64::engine::general_purpose::STANDARD.encode("operator:a-sentinel-secret-9137");
    let wrong = format!("Basic {encoded}");
    let (status, kind, body) = answered(&router, "GET", "/runs", Some(&wrong)).await;
    assert_eq!(StatusCode::UNAUTHORIZED, status);
    assert_eq!(Some("application/problem+json"), kind.as_deref());
    assert_eq!(
        "https://ferroterm.eu/docs/operate/problems.html#unauthenticated",
        body["type"]
    );
    bodies.push_str(&body.to_string());
    let (status, _, body) = answered(&router, "GET", "/runs", Some(&basic("reader"))).await;
    assert_eq!(StatusCode::FORBIDDEN, status);
    assert_eq!(
        "https://ferroterm.eu/docs/operate/problems.html#not-permitted",
        body["type"]
    );
    bodies.push_str(&body.to_string());
    let operator = basic("operator");
    let (status, _, body) =
        answered(&router, "GET", "/runs/a-sentinel-run-4471", Some(&operator)).await;
    assert_eq!(StatusCode::NOT_FOUND, status);
    assert_eq!(
        "https://ferroterm.eu/docs/operate/problems.html#run-not-found",
        body["type"]
    );
    bodies.push_str(&body.to_string());
    let (status, _, body) = answered(&router, "GET", "/a-sentinel-path", Some(&operator)).await;
    assert_eq!(StatusCode::NOT_FOUND, status);
    assert_eq!("about:blank", body["type"]);
    bodies.push_str(&body.to_string());
    for sent in [
        "a-sentinel-secret-9137",
        "a-sentinel-run-4471",
        "a-sentinel-path",
    ] {
        assert!(!bodies.contains(sent), "{bodies}");
    }
}
