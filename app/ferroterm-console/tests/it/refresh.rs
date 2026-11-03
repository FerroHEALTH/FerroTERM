// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refresh of a session's access token before the request it arrives
//! with is served (RFC 6749 §6): kept on success, ended on a refusal the
//! provider answered, and kept when the provider does not answer.

use std::time::Duration;

use ferroterm_console::server::ConsoleState;
use http::StatusCode;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::provider::{Provider, TOKEN_PATH, console, form, get, send, sessions_held, set_cookies};
use crate::sign_in::{SESSION, access_token, signed_in_with, start};

/// Signs an operator in at `provider` with an access token that expires at
/// once and the refresh token `refresh`, forgets every answer the provider
/// was given, and returns the session cookie's `name=value` pair.
async fn due(state: &ConsoleState, provider: &Provider, refresh: &str) -> String {
    signed_in_for(state, provider, 0, refresh).await
}

/// Signs an operator in at `provider` with an access token that lasts
/// `expires_in` seconds and the refresh token `refresh`, forgets every
/// answer the provider was given, and returns the session cookie's
/// `name=value` pair.
async fn signed_in_for(
    state: &ConsoleState,
    provider: &Provider,
    expires_in: u64,
    refresh: &str,
) -> String {
    let started = start(state, "/login").await;
    let id_token = provider.id_token(&provider.claims(started.nonce()));
    let answer = json!({
        "access_token": "synthetic-access-1",
        "token_type": "Bearer",
        "expires_in": expires_in,
        "refresh_token": refresh,
        "id_token": id_token,
    });
    let session = signed_in_with(state, provider, &started, &answer).await;
    provider.reset().await;
    session
}

/// Whether `response` tells the browser to drop the session cookie.
fn drops_the_session(response: &http::Response<axum::body::Body>) -> bool {
    set_cookies(response.headers())
        .iter()
        .any(|cookie| cookie.starts_with(&format!("{SESSION}=;")) && cookie.contains("Max-Age=0"))
}

#[tokio::test]
async fn a_due_session_is_refreshed_before_the_request_is_served() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "").await;
    let session = due(&state, &provider, "synthetic-refresh-1").await;
    provider
        .answer_tokens(
            200,
            &json!({"access_token": "synthetic-access-2", "token_type": "Bearer", "expires_in": 3600}),
        )
        .await;
    assert_eq!(
        None,
        access_token(&state, &session),
        "expired until refreshed"
    );
    let response = send(&state, get("/health", std::slice::from_ref(&session))).await;
    assert_eq!(StatusCode::OK, response.status());
    assert!(!drops_the_session(&response));
    assert_eq!(
        Some("synthetic-access-2"),
        access_token(&state, &session).as_deref()
    );
    let requests = provider.requests_at(TOKEN_PATH).await;
    let [request] = requests.as_slice() else {
        panic!("one token request");
    };
    assert_eq!("synthetic-refresh-1", form(request)["refresh_token"]);
}

#[tokio::test]
async fn a_session_whose_refresh_is_refused_is_ended_and_its_cookie_dropped() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "").await;
    let session = due(&state, &provider, "synthetic-refresh-1").await;
    provider
        .answer_tokens(400, &json!({"error": "invalid_grant"}))
        .await;
    let response = send(&state, get("/health", std::slice::from_ref(&session))).await;
    assert_eq!(
        StatusCode::OK,
        response.status(),
        "the request is still served"
    );
    assert!(drops_the_session(&response));
    assert_eq!(0, sessions_held(&state));
}

#[tokio::test]
async fn a_provider_failing_on_its_side_leaves_the_session_for_the_next_request() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "").await;
    let session = due(&state, &provider, "synthetic-refresh-1").await;
    provider
        .answer_tokens(503, &json!({"error": "temporarily_unavailable"}))
        .await;
    let response = send(&state, get("/health", std::slice::from_ref(&session))).await;
    assert_eq!(StatusCode::OK, response.status());
    assert!(!drops_the_session(&response));
    assert_eq!(1, sessions_held(&state));
    send(&state, get("/health", std::slice::from_ref(&session))).await;
    assert_eq!(
        2,
        provider.requests_at(TOKEN_PATH).await.len(),
        "the next request tries again"
    );
}

#[tokio::test]
async fn a_session_that_is_not_due_sends_no_refresh() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "").await;
    let session = signed_in_for(&state, &provider, 3600, "synthetic-refresh-1").await;
    let response = send(&state, get("/health", &[session])).await;
    assert_eq!(StatusCode::OK, response.status());
    assert!(provider.requests_at(TOKEN_PATH).await.is_empty());
}

#[tokio::test]
async fn the_sign_in_routes_are_served_without_a_refresh() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "").await;
    let session = due(&state, &provider, "synthetic-refresh-1").await;
    for uri in ["/login", "/auth/callback?code=c1&state=s1"] {
        send(&state, get(uri, std::slice::from_ref(&session))).await;
    }
    assert!(provider.requests_at(TOKEN_PATH).await.is_empty());
    assert_eq!(1, sessions_held(&state));
}

#[tokio::test]
async fn a_session_cookie_on_a_console_without_sign_in_is_dropped() {
    let state = console(None, "").await;
    let session = format!("{SESSION}=not-a-session");
    let response = send(&state, get("/health", std::slice::from_ref(&session))).await;
    assert_eq!(StatusCode::OK, response.status());
    assert!(drops_the_session(&response));
    assert_eq!(0, sessions_held(&state));
}

#[tokio::test]
async fn concurrent_requests_of_one_session_spend_its_refresh_token_once() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "").await;
    let session = due(&state, &provider, "synthetic-refresh-1").await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "access_token": "synthetic-access-2",
                    "token_type": "Bearer",
                    "expires_in": 3600,
                    "refresh_token": "synthetic-refresh-2",
                }))
                .set_delay(Duration::from_millis(200)),
        )
        .mount(&provider.server)
        .await;
    let cookies = std::slice::from_ref(&session);
    let (first, second, third) = tokio::join!(
        send(&state, get("/health", cookies)),
        send(&state, get("/health", cookies)),
        send(&state, get("/health", cookies)),
    );
    for response in [first, second, third] {
        assert_eq!(StatusCode::OK, response.status());
    }
    assert_eq!(1, provider.requests_at(TOKEN_PATH).await.len());
    assert_eq!(
        Some("synthetic-access-2"),
        access_token(&state, &session).as_deref()
    );
}
