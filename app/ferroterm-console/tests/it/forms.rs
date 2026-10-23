// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's own form routes: a preference kept, every preference reset,
//! the navigation folded, and the FHIR version chosen for the session, each
//! answering with a redirect to one of the console's own pages.

use axum::body::Body;
use http::header::{CACHE_CONTROL, CONTENT_TYPE};
use http::{Request, StatusCode};

use crate::provider::{console, get, location, send, set_cookies, text};

/// A form post of `body` to `path` from the console's own page.
fn post(path: &str, body: &str) -> Request<Body> {
    Request::post(path)
        .header("sec-fetch-site", "same-origin")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.to_owned()))
        .expect("a request")
}

/// The `Set-Cookie` a response sets for the preference `key`.
fn preference_cookie(response: &http::Response<Body>, key: &str) -> Option<String> {
    let name = format!("__Host-ferroterm_console_{key}=");
    set_cookies(response.headers())
        .into_iter()
        .find(|cookie| cookie.starts_with(&name))
}

#[tokio::test]
async fn a_preference_is_kept_a_year_and_the_browser_returns_to_its_page() {
    let state = console(None, "");
    let response = send(
        &state,
        post("/preference", "theme=dark&back=%2Fsystems%3Fq%3Da"),
    )
    .await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!("/systems?q=a", location(&response));
    assert_eq!(
        Some("no-store"),
        response
            .headers()
            .get(CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
    );
    let cookie = preference_cookie(&response, "theme").expect("a theme cookie");
    assert!(
        cookie.starts_with("__Host-ferroterm_console_theme=dark;"),
        "{cookie}"
    );
    assert!(cookie.contains("Max-Age=31536000"), "{cookie}");
}

#[tokio::test]
async fn a_preference_kept_on_your_preferences_returns_saying_which() {
    let state = console(None, "");
    let response = send(
        &state,
        post("/preference", "page-size=50&back=%2Fpreferences%2Freading"),
    )
    .await;
    assert_eq!("/preferences/reading?saved=page-size", location(&response));
    assert!(
        preference_cookie(&response, "page-size")
            .is_some_and(|cookie| cookie.starts_with("__Host-ferroterm_console_page-size=50;"))
    );
}

#[tokio::test]
async fn a_switch_keeps_the_last_value_it_posts() {
    let state = console(None, "");
    let response = send(
        &state,
        post("/preference", "theme=light&theme=dark&back=%2F"),
    )
    .await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert!(
        preference_cookie(&response, "theme")
            .is_some_and(|cookie| cookie.starts_with("__Host-ferroterm_console_theme=dark;"))
    );
}

#[tokio::test]
async fn a_preference_post_without_a_page_returns_to_the_dashboard() {
    let state = console(None, "");
    let response = send(&state, post("/preference", "theme=dark")).await;
    assert_eq!("/", location(&response));
}

#[tokio::test]
async fn a_preference_post_naming_a_page_off_the_console_returns_to_the_dashboard() {
    let state = console(None, "");
    let response = send(
        &state,
        post(
            "/preference",
            "theme=dark&back=https%3A%2F%2Fevil.example%2F",
        ),
    )
    .await;
    assert_eq!("/", location(&response));
}

#[tokio::test]
async fn a_preference_post_naming_no_preference_is_refused() {
    let state = console(None, "");
    let response = send(&state, post("/preference", "back=%2F")).await;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    assert_eq!(Vec::<String>::new(), set_cookies(response.headers()));
    assert_eq!("The post names no preference.", text(response).await);
}

#[tokio::test]
async fn a_preference_post_naming_two_preferences_is_refused() {
    let state = console(None, "");
    let response = send(&state, post("/preference", "theme=dark&page-size=50")).await;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    assert_eq!("A post keeps one preference.", text(response).await);
}

#[tokio::test]
async fn a_preference_or_value_the_console_does_not_offer_is_refused() {
    let state = console(None, "");
    for body in [
        "theme=purple",
        "colour=red",
        "page-size=0",
        "language=xx-invalid-tag-zzzzzzzzz",
    ] {
        let response = send(&state, post("/preference", body)).await;
        assert_eq!(StatusCode::BAD_REQUEST, response.status(), "{body}");
        assert_eq!(
            "The preference or its value is not one the console offers.",
            text(response).await,
            "{body}"
        );
    }
}

#[tokio::test]
async fn reset_clears_every_preference_cookie_the_session_version_among_them() {
    let state = console(None, "");
    let response = send(&state, post("/preferences/reset", "back=%2Fpreferences")).await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!("/preferences", location(&response));
    let cookies = set_cookies(response.headers());
    for key in [
        "theme",
        "density",
        "motion",
        "week-start",
        "hints",
        "language",
        "fhir",
        "display-language",
        "page-size",
        "session-fhir",
    ] {
        let name = format!("__Host-ferroterm_console_{key}=;");
        assert!(
            cookies
                .iter()
                .any(|cookie| cookie.starts_with(&name) && cookie.contains("Max-Age=0")),
            "{key} is cleared: {cookies:?}"
        );
    }
}

#[tokio::test]
async fn a_reset_without_a_page_is_refused_as_a_malformed_form() {
    let state = console(None, "");
    let response = send(&state, post("/preferences/reset", "")).await;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, response.status());
    assert_eq!(Vec::<String>::new(), set_cookies(response.headers()));
}

#[tokio::test]
async fn the_navigation_is_folded_and_unfolded() {
    let state = console(None, "");
    for sidebar in ["rail", "full"] {
        let response = send(
            &state,
            post("/sidebar", &format!("sidebar={sidebar}&back=%2Fexpand")),
        )
        .await;
        assert_eq!(StatusCode::SEE_OTHER, response.status(), "{sidebar}");
        assert_eq!("/expand", location(&response));
        assert!(
            preference_cookie(&response, "sidebar").is_some_and(|cookie| cookie
                .starts_with(&format!("__Host-ferroterm_console_sidebar={sidebar};"))),
            "{sidebar}"
        );
    }
}

#[tokio::test]
async fn a_navigation_state_that_is_neither_rail_nor_full_is_refused() {
    let state = console(None, "");
    let response = send(&state, post("/sidebar", "sidebar=wide&back=%2F")).await;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    assert_eq!(
        "The navigation is either the rail or full.",
        text(response).await
    );
}

#[tokio::test]
async fn a_fhir_version_is_kept_for_the_session_and_the_page_read_through_it() {
    let state = console(None, "");
    let response = send(
        &state,
        get("/fhir-version?fhir=r5&back=%2Fsystems%3Ffhir%3Dr5", &[]),
    )
    .await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!("/systems?fhir=r5", location(&response));
    let cookie = preference_cookie(&response, "session-fhir").expect("a session version cookie");
    assert!(
        cookie.starts_with("__Host-ferroterm_console_session-fhir=r5;"),
        "{cookie}"
    );
    assert!(
        !cookie.contains("Max-Age"),
        "the choice lasts the browser's session: {cookie}"
    );
}

#[tokio::test]
async fn a_fhir_version_the_server_does_not_serve_is_refused() {
    let state = console(None, "");
    let response = send(&state, get("/fhir-version?fhir=dstu2&back=%2F", &[])).await;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    assert_eq!(Vec::<String>::new(), set_cookies(response.headers()));
    assert_eq!(
        "The FHIR version is not one this server serves.",
        text(response).await
    );
}

#[tokio::test]
async fn a_cookie_that_is_not_secure_carries_no_host_prefix() {
    let state = console(None, "[session]\nsecure_cookie = false\n");
    let response = send(&state, post("/sidebar", "sidebar=rail&back=%2F")).await;
    let cookies = set_cookies(response.headers());
    assert!(
        cookies.iter().any(
            |cookie| cookie.starts_with("ferroterm_console_sidebar=rail;")
                && !cookie.contains("Secure")
        ),
        "{cookies:?}"
    );
}
