// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener and the scope it requires.

use axum::body::Body;
use ferroterm_server::smart::Smart;
use http::{Request, StatusCode};
use tower::ServiceExt;

use super::{SigningKey, causes, challenge, claims, issuer, reload};
use crate::fixture::{Server, SmartSetup};

#[tokio::test]
async fn the_admin_listener_requires_the_configured_scope() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server =
        Server::start_persisting_with_smart(&mock.uri(), None, Some("__ferroterm.reload")).await;
    let admin = server.admin_router();

    let response = admin.clone().oneshot(reload(None)).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(challenge(&response).starts_with("Bearer realm="));

    let wrong = key.sign(&claims(&mock.uri(), "system/CodeSystem.cud", 300));
    let response = admin
        .clone()
        .oneshot(reload(Some(format!("Bearer {wrong}"))))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    // RFC 6750 §3 makes the `scope` attribute optional, and the refusal names
    // no scope anywhere.
    let sent = challenge(&response);
    assert!(sent.contains("error=\"insufficient_scope\""), "{sent}");
    assert!(!sent.contains("scope="), "{sent}");

    let right = key.sign(&claims(&mock.uri(), "__ferroterm.reload", 300));
    let response = admin
        .clone()
        .oneshot(reload(Some(format!("Bearer {right}"))))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

// A path no admin route matches is answered inside the guard, through the
// stack of `ferrohealth-server` (FerroHEALTH/FerroSYS#199); health stays open.
#[tokio::test]
async fn an_unknown_path_on_the_admin_listener_needs_a_token_and_health_does_not() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let admin = server.admin_router();

    for path in ["/nowhere", "/r4/CodeSystem"] {
        let request = Request::get(path).body(Body::empty()).expect("request");
        let response = admin.clone().oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
        assert!(challenge(&response).starts_with("Bearer realm="), "{path}");
    }

    let request = Request::get("/health")
        .body(Body::empty())
        .expect("request");
    let response = admin.oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

// The admin scope is a product scope, `__ferroterm.admin` when `[auth]` names
// none (SMART App Launch 2.2.0, Scopes and Launch Context, §Extensibility).
#[tokio::test]
async fn the_default_admin_scope_is_the_extension_scope_of_ferroterm() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let admin = server.admin_router();

    let old = key.sign(&claims(&mock.uri(), "ferroterm/admin", 300));
    let response = admin
        .clone()
        .oneshot(reload(Some(format!("Bearer {old}"))))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let token = key.sign(&claims(&mock.uri(), "__ferroterm.admin", 300));
    let response = admin
        .oneshot(reload(Some(format!("Bearer {token}"))))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

// SMART App Launch 2.2.0, Scopes and Launch Context, §Extensibility: a scope
// of a server's own is a full URI or a string starting with `__`.
#[tokio::test]
async fn an_admin_scope_outside_the_extension_forms_refuses_the_start() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let mut setup = SmartSetup::for_issuer(&mock.uri());
    setup.auth.product_scope = String::from("ferroterm/reload");
    let config = Server::smart_config(dir.path(), setup);
    let error = Smart::start(&config)
        .await
        .expect_err("the start is refused");
    assert!(
        causes(&error).contains("ferroterm/reload"),
        "{}",
        causes(&error)
    );
}
