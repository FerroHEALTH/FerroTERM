// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `POST /auth/sign-out`: the session ended, its token revoked (RFC 7009
//! §2.1), the cookie dropped, and the operator sent to the provider's
//! end-session endpoint (OpenID Connect RP-Initiated Logout 1.0 §2).

use axum::body::Body;
use ferroterm_console::config::Oidc;
use ferroterm_console::oidc::logout::end_session_request;
use http::header::{AUTHORIZATION, COOKIE};
use http::{Request, StatusCode};
use secrecy::SecretString;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::provider::{
    CLIENT_ID, END_SESSION_PATH, Provider, REVOCATION_PATH, console, form, location,
    redirect_query, send, set_cookies,
};
use crate::sign_in::{SESSION, access_token, signed_in};

/// The console's post-logout redirect.
const POST_LOGOUT: &str = "http://127.0.0.1:3000/auth/sign-in?state=signed-out";

/// A sign-out posted from the console's own page, carrying `cookies`.
fn sign_out(cookies: &[String], site: &str) -> Request<Body> {
    let mut request = Request::post("/auth/sign-out").header("sec-fetch-site", site);
    for cookie in cookies {
        request = request.header(COOKIE, cookie);
    }
    request.body(Body::empty()).expect("a request")
}

/// The `[oidc]` section with the end-session and revocation endpoints.
fn full(provider: &Provider) -> Oidc {
    Oidc {
        end_session_endpoint: Some(provider.url(END_SESSION_PATH)),
        post_logout_redirect_uri: Some(String::from(POST_LOGOUT)),
        revocation_endpoint: Some(provider.url(REVOCATION_PATH)),
        ..provider.oidc()
    }
}

/// Answers every revocation request with `status`.
async fn revocation_answers(provider: &Provider, status: u16) {
    Mock::given(method("POST"))
        .and(path(REVOCATION_PATH))
        .respond_with(ResponseTemplate::new(status))
        .mount(&provider.server)
        .await;
}

#[tokio::test]
async fn a_sign_out_ends_the_session_revokes_its_refresh_token_and_redirects_to_end_session() {
    let provider = Provider::start().await;
    revocation_answers(&provider, 200).await;
    let state = console(Some(full(&provider)), "");
    let session = signed_in(&state, &provider).await;
    let response = send(
        &state,
        sign_out(std::slice::from_ref(&session), "same-origin"),
    )
    .await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    let (url, query) = redirect_query(&response);
    assert_eq!(
        provider.url(END_SESSION_PATH),
        url[..url::Position::AfterPath]
    );
    assert_eq!(CLIENT_ID, query["client_id"]);
    assert_eq!(POST_LOGOUT, query["post_logout_redirect_uri"]);
    assert_eq!(
        3,
        query["id_token_hint"].split('.').count(),
        "the sign-in's ID Token is the hint"
    );
    assert!(
        set_cookies(response.headers())
            .iter()
            .any(|cookie| cookie.starts_with(&format!("{SESSION}=;"))
                && cookie.contains("Max-Age=0"))
    );
    assert!(access_token(&state, &session).is_none());
    assert_eq!(0, state.sessions().occupancy().expect("usable").sessions);
    let revocations = provider.requests_at(REVOCATION_PATH).await;
    let [revocation] = revocations.as_slice() else {
        panic!("one revocation request");
    };
    let fields = form(revocation);
    assert_eq!("synthetic-refresh-token", fields["token"]);
    assert_eq!("refresh_token", fields["token_type_hint"]);
    assert_eq!(CLIENT_ID, fields["client_id"]);
}

#[tokio::test]
async fn a_confidential_client_authenticates_its_revocation_with_basic() {
    let provider = Provider::start().await;
    revocation_answers(&provider, 200).await;
    let oidc = Oidc {
        client_secret: Some(ferrohealth_secret::secret::Secret::new("secret")),
        ..full(&provider)
    };
    let state = console(Some(oidc), "");
    let session = signed_in(&state, &provider).await;
    send(&state, sign_out(&[session], "same-origin")).await;
    let revocations = provider.requests_at(REVOCATION_PATH).await;
    let [revocation] = revocations.as_slice() else {
        panic!("one revocation request");
    };
    assert!(
        revocation
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("Basic ")),
    );
    assert!(!form(revocation).contains_key("client_id"));
}

#[tokio::test]
async fn a_sign_out_without_an_end_session_endpoint_shows_the_signed_out_page() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let session = signed_in(&state, &provider).await;
    let response = send(
        &state,
        sign_out(std::slice::from_ref(&session), "same-origin"),
    )
    .await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!("/auth/sign-in?state=signed-out", location(&response));
    assert!(access_token(&state, &session).is_none());
    assert!(
        provider.requests_at(REVOCATION_PATH).await.is_empty(),
        "no revocation endpoint, no revocation"
    );
}

#[tokio::test]
async fn a_refused_revocation_still_signs_the_operator_out() {
    let provider = Provider::start().await;
    revocation_answers(&provider, 503).await;
    let state = console(Some(full(&provider)), "");
    let session = signed_in(&state, &provider).await;
    let response = send(
        &state,
        sign_out(std::slice::from_ref(&session), "same-origin"),
    )
    .await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert!(access_token(&state, &session).is_none());
}

#[tokio::test]
async fn an_unreachable_revocation_endpoint_still_signs_the_operator_out() {
    let provider = Provider::start().await;
    let oidc = Oidc {
        revocation_endpoint: Some(String::from("http://127.0.0.1:9/revoke")),
        ..full(&provider)
    };
    let state = console(Some(oidc), "");
    let session = signed_in(&state, &provider).await;
    let response = send(
        &state,
        sign_out(std::slice::from_ref(&session), "same-origin"),
    )
    .await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert!(access_token(&state, &session).is_none());
}

#[tokio::test]
async fn a_sign_out_without_a_session_redirects_and_revokes_nothing() {
    let provider = Provider::start().await;
    let state = console(Some(full(&provider)), "");
    let response = send(&state, sign_out(&[], "same-origin")).await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    let (_, query) = redirect_query(&response);
    assert!(!query.contains_key("id_token_hint"), "{query:?}");
    assert_eq!(CLIENT_ID, query["client_id"]);
    assert!(provider.requests_at(REVOCATION_PATH).await.is_empty());
}

#[tokio::test]
async fn a_cross_site_sign_out_is_refused_and_the_session_kept() {
    let provider = Provider::start().await;
    let state = console(Some(full(&provider)), "");
    let session = signed_in(&state, &provider).await;
    let response = send(
        &state,
        sign_out(std::slice::from_ref(&session), "cross-site"),
    )
    .await;
    assert_eq!(StatusCode::FORBIDDEN, response.status());
    assert!(access_token(&state, &session).is_some());
    assert!(provider.requests_at(REVOCATION_PATH).await.is_empty());
}

#[tokio::test]
async fn a_sign_out_on_a_console_without_sign_in_is_unavailable() {
    let state = console(None, "");
    let response = send(&state, sign_out(&[], "same-origin")).await;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, response.status());
}

#[tokio::test]
async fn the_end_session_request_keeps_the_endpoints_own_query() {
    let provider = Provider::start().await;
    let oidc = Oidc {
        end_session_endpoint: Some(provider.url("/end-session?realm=north")),
        ..provider.oidc()
    };
    let settings = console(Some(oidc), "")
        .settings()
        .oidc
        .clone()
        .expect("configured");
    let url = end_session_request(&settings, Some(&SecretString::from("h.i.nt")))
        .expect("an end-session endpoint");
    let query: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    assert_eq!(
        vec![
            (String::from("realm"), String::from("north")),
            (String::from("id_token_hint"), String::from("h.i.nt")),
            (String::from("client_id"), String::from(CLIENT_ID)),
        ],
        query
    );
}

#[tokio::test]
async fn no_end_session_endpoint_means_no_end_session_request() {
    let provider = Provider::start().await;
    let settings = console(Some(provider.oidc()), "")
        .settings()
        .oidc
        .clone()
        .expect("configured");
    assert!(end_session_request(&settings, None).is_none());
}
