// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refresh of a session's access token before the request it arrives
//! with is served (RFC 6749 §6): kept on success, ended on a refusal the
//! provider answered, and kept when the provider does not answer.

use std::time::Duration;

use ferroterm_console::server::ConsoleState;
use ferroterm_console::session::SignedIn;
use http::StatusCode;
use secrecy::SecretString;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::provider::{Provider, TOKEN_PATH, console, form, get, send, set_cookies};
use crate::sign_in::{SESSION, access_token};

/// Establishes a session whose access token has expired and that holds the
/// refresh token `refresh`, and returns its cookie's `name=value` pair.
fn due(state: &ConsoleState, refresh: &str) -> String {
    let id = state
        .sessions()
        .establish(SignedIn {
            access_token: SecretString::from("synthetic-access-1"),
            expires_in: Some(Duration::ZERO),
            refresh_token: Some(SecretString::from(refresh)),
            id_token: None,
            operator: None,
            scopes: vec![String::from("openid")],
        })
        .expect("room");
    format!("{SESSION}={}", id.as_str())
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
    provider
        .answer_tokens(
            200,
            &json!({"access_token": "synthetic-access-2", "token_type": "Bearer", "expires_in": 3600}),
        )
        .await;
    let state = console(Some(provider.oidc()), "");
    let session = due(&state, "synthetic-refresh-1");
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
    provider
        .answer_tokens(400, &json!({"error": "invalid_grant"}))
        .await;
    let state = console(Some(provider.oidc()), "");
    let session = due(&state, "synthetic-refresh-1");
    let response = send(&state, get("/health", std::slice::from_ref(&session))).await;
    assert_eq!(
        StatusCode::OK,
        response.status(),
        "the request is still served"
    );
    assert!(drops_the_session(&response));
    assert_eq!(0, state.sessions().occupancy().expect("usable").sessions);
}

#[tokio::test]
async fn a_provider_failing_on_its_side_leaves_the_session_for_the_next_request() {
    let provider = Provider::start().await;
    provider
        .answer_tokens(503, &json!({"error": "temporarily_unavailable"}))
        .await;
    let state = console(Some(provider.oidc()), "");
    let session = due(&state, "synthetic-refresh-1");
    let response = send(&state, get("/health", std::slice::from_ref(&session))).await;
    assert_eq!(StatusCode::OK, response.status());
    assert!(!drops_the_session(&response));
    assert_eq!(1, state.sessions().occupancy().expect("usable").sessions);
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
    let state = console(Some(provider.oidc()), "");
    let id = state
        .sessions()
        .establish(SignedIn {
            access_token: SecretString::from("synthetic-access-1"),
            expires_in: Some(Duration::from_secs(3600)),
            refresh_token: Some(SecretString::from("synthetic-refresh-1")),
            id_token: None,
            operator: None,
            scopes: Vec::new(),
        })
        .expect("room");
    let session = format!("{SESSION}={}", id.as_str());
    let response = send(&state, get("/health", &[session])).await;
    assert_eq!(StatusCode::OK, response.status());
    assert!(provider.requests_at(TOKEN_PATH).await.is_empty());
}

#[tokio::test]
async fn the_sign_in_routes_are_served_without_a_refresh() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let session = due(&state, "synthetic-refresh-1");
    for uri in ["/auth/login", "/auth/callback?code=c1&state=s1"] {
        send(&state, get(uri, std::slice::from_ref(&session))).await;
    }
    assert!(provider.requests_at(TOKEN_PATH).await.is_empty());
    assert_eq!(1, state.sessions().occupancy().expect("usable").sessions);
}

#[tokio::test]
async fn a_due_session_on_a_console_without_sign_in_is_ended() {
    let state = console(None, "");
    let session = due(&state, "synthetic-refresh-1");
    let response = send(&state, get("/health", std::slice::from_ref(&session))).await;
    assert_eq!(StatusCode::OK, response.status());
    assert!(drops_the_session(&response));
    assert_eq!(0, state.sessions().occupancy().expect("usable").sessions);
}

#[tokio::test]
async fn concurrent_requests_of_one_session_spend_its_refresh_token_once() {
    let provider = Provider::start().await;
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
    let state = console(Some(provider.oidc()), "");
    let session = due(&state, "synthetic-refresh-1");
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
