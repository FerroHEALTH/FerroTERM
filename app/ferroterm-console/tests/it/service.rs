// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's HTTP service: the liveness route, the stylesheets, the
//! browser security headers every response carries, the compression of the
//! site bundle alone, and the same-origin rule for every unsafe request.

use axum::body::Body;
use ferroterm_console::config::Oidc;
use ferroterm_console::server::THEME;
use http::header::{
    ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_SECURITY_POLICY, CONTENT_TYPE,
    ORIGIN, REFERER, REFERRER_POLICY, VARY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use http::{HeaderMap, Request, StatusCode};

use crate::provider::{Provider, console, get, send, text};

/// The value of `name` in `headers`, as text.
fn header<'h>(headers: &'h HeaderMap, name: &http::HeaderName) -> Option<&'h str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// The directive `name` of a Content-Security-Policy.
fn directive<'p>(policy: &'p str, name: &str) -> Option<&'p str> {
    policy
        .split(';')
        .map(str::trim)
        .find(|directive| directive.starts_with(name))
}

/// The script nonce a Content-Security-Policy names.
fn nonce_of(headers: &HeaderMap) -> String {
    let policy = header(headers, &CONTENT_SECURITY_POLICY).expect("a policy");
    let script = directive(policy, "script-src").expect("a script-src");
    script
        .split_whitespace()
        .find_map(|source| source.strip_prefix("'nonce-"))
        .and_then(|nonce| nonce.strip_suffix('\''))
        .expect("a nonce source")
        .to_owned()
}

/// A `POST` of a preference form to `/preference`, with `headers` added.
fn preference_post(headers: &[(http::HeaderName, &str)]) -> Request<Body> {
    let mut request =
        Request::post("/preference").header(CONTENT_TYPE, "application/x-www-form-urlencoded");
    for (name, value) in headers {
        request = request.header(name, *value);
    }
    request
        .body(Body::from("theme=dark&back=%2F"))
        .expect("a request")
}

#[tokio::test]
async fn the_liveness_route_answers_up() {
    let state = console(None, "");
    let response = send(&state, get("/health", &[])).await;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(r#"{"status":"up"}"#, text(response).await);
}

#[tokio::test]
async fn every_response_carries_the_browser_security_headers() {
    let state = console(None, "");
    let response = send(&state, get("/health", &[])).await;
    let headers = response.headers();
    assert_eq!(Some("nosniff"), header(headers, &X_CONTENT_TYPE_OPTIONS));
    assert_eq!(Some("DENY"), header(headers, &X_FRAME_OPTIONS));
    assert_eq!(Some("no-referrer"), header(headers, &REFERRER_POLICY));
    assert_eq!(Some("no-store"), header(headers, &CACHE_CONTROL));
    let policy = header(headers, &CONTENT_SECURITY_POLICY).expect("a policy");
    assert_eq!(
        Some("frame-ancestors 'none'"),
        directive(policy, "frame-ancestors")
    );
    assert_eq!(Some("object-src 'none'"), directive(policy, "object-src"));
    assert_eq!(Some("form-action 'self'"), directive(policy, "form-action"));
}

#[tokio::test]
async fn every_response_carries_a_nonce_of_its_own() {
    let state = console(None, "");
    let first = send(&state, get("/health", &[])).await;
    let second = send(&state, get("/health", &[])).await;
    assert_ne!(nonce_of(first.headers()), nonce_of(second.headers()));
}

#[tokio::test]
async fn a_page_s_scripts_carry_the_nonce_its_policy_names() {
    let state = console(None, "");
    let response = send(&state, get("/about", &[])).await;
    assert_eq!(StatusCode::OK, response.status());
    let nonce = nonce_of(response.headers());
    let html = text(response).await;
    let scripts = html.matches("<script").count();
    assert!(scripts > 0, "the document loads its islands: {html}");
    assert_eq!(
        scripts,
        html.matches(&format!("nonce=\"{nonce}\"")).count(),
        "every script carries the response's nonce"
    );
}

#[tokio::test]
async fn the_policy_admits_a_form_redirect_to_the_end_session_endpoint_s_origin() {
    let provider = Provider::start().await;
    let state = console(
        Some(Oidc {
            end_session_endpoint: Some(String::from("https://idp.example:8443/logout?x=1")),
            ..provider.oidc()
        }),
        "",
    );
    let response = send(&state, get("/health", &[])).await;
    let policy = header(response.headers(), &CONTENT_SECURITY_POLICY).expect("a policy");
    assert_eq!(
        Some("form-action 'self' https://idp.example:8443"),
        directive(policy, "form-action")
    );
}

#[tokio::test]
async fn the_stylesheets_are_served_as_css_and_revalidated() {
    let state = console(None, "");
    for path in ["/ferrohealth-ui.css", "/theme.css"] {
        let response = send(&state, get(path, &[])).await;
        assert_eq!(StatusCode::OK, response.status(), "{path}");
        assert_eq!(
            Some("text/css; charset=utf-8"),
            header(response.headers(), &CONTENT_TYPE),
            "{path}"
        );
        assert_eq!(
            Some("no-cache"),
            header(response.headers(), &CACHE_CONTROL),
            "a rebuilt bundle is fetched again: {path}"
        );
    }
    let theme = send(&state, get("/theme.css", &[])).await;
    assert_eq!(THEME, text(theme).await);
}

#[tokio::test]
async fn the_site_bundle_is_compressed_as_the_request_asks() {
    let state = console(None, "");
    let request = Request::get("/ferrohealth-ui.css")
        .header(ACCEPT_ENCODING, "br")
        .body(Body::empty())
        .expect("a request");
    let response = send(&state, request).await;
    assert_eq!(Some("br"), header(response.headers(), &CONTENT_ENCODING));
    assert!(
        header(response.headers(), &VARY)
            .is_some_and(|vary| vary.to_ascii_lowercase().contains("accept-encoding")),
        "{:?}",
        response.headers()
    );
}

#[tokio::test]
async fn a_document_is_never_compressed() {
    let state = console(None, "");
    let request = Request::get("/about")
        .header(ACCEPT_ENCODING, "gzip, br")
        .body(Body::empty())
        .expect("a request");
    let response = send(&state, request).await;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(None, header(response.headers(), &CONTENT_ENCODING));
}

#[tokio::test]
async fn an_unsafe_request_from_the_console_s_own_page_passes_by_fetch_metadata() {
    let state = console(None, "");
    let response = send(
        &state,
        preference_post(&[(
            http::HeaderName::from_static("sec-fetch-site"),
            "same-origin",
        )]),
    )
    .await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
}

#[tokio::test]
async fn an_unsafe_request_another_site_sent_is_refused() {
    let state = console(None, "");
    for site in ["cross-site", "same-site", "none"] {
        let response = send(
            &state,
            preference_post(&[(http::HeaderName::from_static("sec-fetch-site"), site)]),
        )
        .await;
        assert_eq!(StatusCode::FORBIDDEN, response.status(), "{site}");
        assert_eq!(
            "the console takes this request only from its own pages",
            text(response).await
        );
    }
}

#[tokio::test]
async fn without_fetch_metadata_the_origin_header_decides() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let own = send(
        &state,
        preference_post(&[(ORIGIN, "http://127.0.0.1:3000")]),
    )
    .await;
    assert_eq!(StatusCode::SEE_OTHER, own.status());
    let other = send(&state, preference_post(&[(ORIGIN, "https://evil.example")])).await;
    assert_eq!(StatusCode::FORBIDDEN, other.status());
    let null = send(&state, preference_post(&[(ORIGIN, "null")])).await;
    assert_eq!(StatusCode::FORBIDDEN, null.status());
}

#[tokio::test]
async fn without_fetch_metadata_or_origin_the_referer_s_origin_decides() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let own = send(
        &state,
        preference_post(&[(REFERER, "http://127.0.0.1:3000/preferences?x=1")]),
    )
    .await;
    assert_eq!(StatusCode::SEE_OTHER, own.status());
    for referer in ["https://evil.example/", "not a url"] {
        let response = send(&state, preference_post(&[(REFERER, referer)])).await;
        assert_eq!(StatusCode::FORBIDDEN, response.status(), "{referer}");
    }
}

#[tokio::test]
async fn an_unsafe_request_that_shows_no_origin_is_refused() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let response = send(&state, preference_post(&[])).await;
    assert_eq!(StatusCode::FORBIDDEN, response.status());
}

#[tokio::test]
async fn a_console_with_no_known_origin_takes_no_unsafe_request_on_its_origin_header() {
    let state = console(None, "");
    let response = send(
        &state,
        preference_post(&[(ORIGIN, "http://127.0.0.1:3000")]),
    )
    .await;
    assert_eq!(
        StatusCode::FORBIDDEN,
        response.status(),
        "with no sign-in configured the console knows no origin of its own"
    );
}

#[tokio::test]
async fn a_posted_form_over_the_limit_is_refused_as_too_large() {
    let state = console(None, "");
    let request = Request::post("/compose")
        .header("sec-fetch-site", "same-origin")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(vec![b'a'; 4 * 1024 * 1024 + 1]))
        .expect("a request");
    let response = send(&state, request).await;
    assert_eq!(StatusCode::PAYLOAD_TOO_LARGE, response.status());
    assert_eq!(
        "The form is larger than the console takes.",
        text(response).await
    );
}
