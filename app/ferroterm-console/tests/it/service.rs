// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's HTTP service: the liveness route, FerroUI's assets and the
//! brand's files, the browser security headers every response carries, no
//! answer compressed, and the same-origin rule for every unsafe request.

use axum::body::Body;
use ferrohealth_ui::serve::Assets;
use ferrohealth_ui::style::Brand;
use ferroterm_console::config::Oidc;
use http::header::{
    ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_SECURITY_POLICY, CONTENT_TYPE,
    ORIGIN, REFERER, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
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
    assert_eq!(r#"{"state":"up"}"#, text(response).await);
}

#[tokio::test]
async fn the_health_route_withdraws_readiness_once_the_console_drains() {
    let state = console(None, "");
    state.lifecycle().drain();
    let response = send(&state, get("/health", &[])).await;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, response.status());
    assert_eq!(r#"{"state":"down"}"#, text(response).await);
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
async fn the_policy_takes_scripts_and_styles_from_the_console_alone_with_no_nonce() {
    let state = console(None, "");
    for path in ["/health", "/about"] {
        let response = send(&state, get(path, &[])).await;
        let policy = header(response.headers(), &CONTENT_SECURITY_POLICY).expect("a policy");
        assert_eq!(
            Some("script-src 'self'"),
            directive(policy, "script-src"),
            "{path}: {policy}"
        );
        assert_eq!(
            Some("style-src 'self'"),
            directive(policy, "style-src"),
            "{path}"
        );
        assert_eq!(
            Some("default-src 'self'"),
            directive(policy, "default-src"),
            "{path}"
        );
        assert!(!policy.contains("'nonce-"), "{path}: {policy}");
        assert!(!policy.contains("wasm-unsafe-eval"), "{path}: {policy}");
    }
}

#[tokio::test]
async fn a_page_loads_its_one_script_from_the_library_and_writes_none_inline() {
    let state = console(None, "");
    let response = send(&state, get("/about", &[])).await;
    assert_eq!(StatusCode::OK, response.status());
    let html = text(response).await;
    let loader = Assets::of(Brand::Term).loader();
    assert_eq!(1, html.matches("<script").count(), "{html}");
    assert!(
        html.contains(&format!(
            r#"<script type="module" src="{loader}"></script>"#
        )),
        "the document loads FerroUI's loader: {html}"
    );
    assert!(!html.contains(" style="), "no inline style: {html}");
    assert!(!html.contains("<style"), "no style element: {html}");
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
async fn the_library_s_stylesheet_and_theme_are_served_as_css_for_a_year() {
    let state = console(None, "");
    let assets = Assets::of(Brand::Term);
    for path in [assets.stylesheet(), assets.theme()] {
        let response = send(&state, get(&path, &[])).await;
        assert_eq!(StatusCode::OK, response.status(), "{path}");
        assert!(
            header(response.headers(), &CONTENT_TYPE)
                .is_some_and(|media| media.starts_with("text/css")),
            "{path}: {:?}",
            response.headers()
        );
        assert_eq!(
            Some("public, max-age=31536000, immutable"),
            header(response.headers(), &CACHE_CONTROL),
            "a new pin is a new path: {path}"
        );
    }
    let theme = send(&state, get(&assets.theme(), &[])).await;
    assert_eq!(Brand::Term.theme(), text(theme).await);
}

#[tokio::test]
async fn an_asset_is_never_compressed() {
    let state = console(None, "");
    let request = Request::get(Assets::of(Brand::Term).stylesheet())
        .header(ACCEPT_ENCODING, "br")
        .body(Body::empty())
        .expect("a request");
    let response = send(&state, request).await;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(None, header(response.headers(), &CONTENT_ENCODING));
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

#[tokio::test]
async fn an_asset_the_library_does_not_ship_is_a_plain_not_found() {
    let state = console(None, "");
    let path = format!(
        "{}/{}/scripts/no-such-module.js",
        ferrohealth_ui::serve::PREFIX,
        ferrohealth_ui::serve::VERSION
    );
    let response = send(&state, get(&path, &[])).await;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    assert_eq!(
        Some("no-store"),
        header(response.headers(), &CACHE_CONTROL),
        "a miss is never kept"
    );
    assert!(
        !text(response).await.contains("<html"),
        "no page is drawn for a missing file"
    );
}

#[tokio::test]
async fn a_brand_file_is_served_as_it_stands_and_revalidated() {
    let state = console(None, "");
    let response = send(&state, get("/favicon.svg", &[])).await;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        Some("image/svg+xml"),
        header(response.headers(), &CONTENT_TYPE)
    );
    assert_eq!(Some("no-cache"), header(response.headers(), &CACHE_CONTROL));
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a body");
    assert_eq!(&include_bytes!("../../public/favicon.svg")[..], &*body);
}
