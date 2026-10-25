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

// The admin listener checks tokens as JWTs only (#881), so `mode =
// "introspection"` refuses the start by name rather than starting a gate that
// admits no token.
#[tokio::test]
async fn introspection_mode_refuses_the_start() {
    let introspecting = AuthSection {
        issuer: IssuerKey::Flat(String::from("https://auth.example.org/realms/tx")),
        audience: vec![String::from("ferroterm-sync")],
        mode: AuthMode::Introspection,
        introspection: Some(IntrospectionSection {
            client: Some(OauthSection {
                client_id: String::from("ferroterm-sync"),
                client_secret: Some(SecretString::from("synthetic-secret")),
                ..OauthSection::default()
            }),
            ..IntrospectionSection::default()
        }),
        ..AuthSection::default()
    };
    let refused = AdminAuth::start(&introspecting).await;
    assert!(
        matches!(refused, Err(AuthError::Introspection)),
        "the refusal names the mode: {refused:?}"
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
