// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The session route FerroUI's session watch asks (FerroUI
//! `docs/consoles.md`, "The session's end").

use axum::body::Body;
use http::header::{CACHE_CONTROL, COOKIE};
use http::{Request, StatusCode};
use serde_json::Value;

use crate::provider::{Provider, console, get, send, text};
use crate::sign_in::{session_id, signed_in};

/// A post of the reader's activity from the console's own page, carrying
/// `cookies`.
fn extend(cookies: &[String]) -> Request<Body> {
    let mut request = Request::post("/auth/session").header("sec-fetch-site", "same-origin");
    for cookie in cookies {
        request = request.header(COOKIE, cookie);
    }
    request.body(Body::empty()).expect("a request")
}

/// What a session route answer says is left: the idle seconds and the
/// absolute seconds.
async fn left(response: http::Response<Body>) -> (u64, u64) {
    let body: Value = serde_json::from_str(&text(response).await).expect("JSON");
    let seconds = |key: &str| body.get(key).and_then(Value::as_u64).expect(key);
    (seconds("idle"), seconds("absolute"))
}

#[tokio::test]
async fn a_live_session_says_what_it_has_left_and_an_extension_restarts_its_idle_timeout() {
    let provider = Provider::start().await;
    let state = console(
        Some(provider.oidc()),
        "[session]\nidle_timeout_s = 900\nabsolute_timeout_s = 28800\n",
    );
    let session = signed_in(&state, &provider).await;
    let read = send(&state, get("/auth/session", std::slice::from_ref(&session))).await;
    assert_eq!(StatusCode::OK, read.status());
    assert_eq!(
        Some("no-store"),
        read.headers()
            .get(CACHE_CONTROL)
            .and_then(|value| value.to_str().ok())
    );
    let (idle, absolute) = left(read).await;
    assert!((890..=900).contains(&idle), "{idle}");
    assert!((28_790..=28_800).contains(&absolute), "{absolute}");
    let extended = send(&state, extend(std::slice::from_ref(&session))).await;
    assert_eq!(StatusCode::OK, extended.status());
    assert_eq!(900, left(extended).await.0, "the idle timeout starts again");
}

#[tokio::test]
async fn a_read_leaves_the_session_s_idle_count_where_it_was() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let session = signed_in(&state, &provider).await;
    let id = session_id(&session);
    let before = state
        .sessions()
        .remaining(&id, false)
        .expect("usable")
        .expect("live");
    std::thread::sleep(std::time::Duration::from_millis(5));
    let read = send(&state, get("/auth/session", std::slice::from_ref(&session))).await;
    assert_eq!(StatusCode::OK, read.status());
    let after = state
        .sessions()
        .remaining(&id, false)
        .expect("usable")
        .expect("live");
    assert!(after.idle < before.idle, "the read extended nothing");
}

#[tokio::test]
async fn a_session_that_is_gone_or_never_was_answers_401() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let none = send(&state, get("/auth/session", &[])).await;
    assert_eq!(StatusCode::UNAUTHORIZED, none.status());
    assert_eq!(
        Some("application/problem+json"),
        none.headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
    );
    let session = signed_in(&state, &provider).await;
    state.sessions().end(&session_id(&session)).expect("usable");
    let gone = send(&state, extend(std::slice::from_ref(&session))).await;
    assert_eq!(StatusCode::UNAUTHORIZED, gone.status());
}

#[tokio::test]
async fn an_extension_from_another_site_is_refused() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let session = signed_in(&state, &provider).await;
    let mut request = extend(std::slice::from_ref(&session));
    request.headers_mut().insert(
        "sec-fetch-site",
        http::HeaderValue::from_static("cross-site"),
    );
    let refused = send(&state, request).await;
    assert_eq!(StatusCode::FORBIDDEN, refused.status());
}

#[tokio::test]
async fn a_signed_in_page_draws_the_session_watch_and_a_visitor_s_does_not() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let session = signed_in(&state, &provider).await;
    let page = text(send(&state, get("/about", std::slice::from_ref(&session))).await).await;
    assert!(
        page.contains("id=\"ferroui-session-warning\""),
        "the signed-in page carries the watch's warning: {page}"
    );
    let visitor = text(send(&state, get("/about", &[])).await).await;
    assert!(
        !visitor.contains("ferroui-session-warning"),
        "a visitor has no session to watch"
    );
}
