// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET /auth/login` and `GET /auth/callback`: the authorization request
//! with `state`, `nonce` and PKCE (RFC 6749 §4.1.1, RFC 7636 §4.3, OpenID
//! Connect Core 1.0 §3.1.2.1), and the redirect back that is held to it
//! (RFC 6749 §10.12) before a session is begun.

use std::collections::BTreeMap;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferroterm_console::config::Oidc;
use ferroterm_console::server::ConsoleState;
use ferroterm_console::session::{SessionId, SignedIn};
use http::StatusCode;
use http::header::CACHE_CONTROL;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::json;

use crate::provider::{
    CLIENT_ID, Provider, REDIRECT_URI, TOKEN_PATH, console, cookie_pair, form, get, location,
    redirect_query, send, set_cookies, text, token_answer,
};

/// The name of the pending sign-in's cookie.
pub(crate) const SIGN_IN: &str = "__Host-ferroterm_console_sign_in";

/// The name of the session's cookie.
pub(crate) const SESSION: &str = "__Host-ferroterm_console_session";

/// What `GET /auth/login` left the browser: the `state` and the `nonce` it
/// sent the provider, the PKCE challenge, and the sign-in cookie.
pub(crate) struct Started {
    /// The authorization request's query.
    pub(crate) query: BTreeMap<String, String>,
    /// The `name=value` pair of the sign-in cookie.
    pub(crate) cookie: String,
}

impl Started {
    /// The `state` the request carried.
    pub(crate) fn state(&self) -> &str {
        self.query.get("state").expect("a state")
    }

    /// The `nonce` the request carried.
    pub(crate) fn nonce(&self) -> &str {
        self.query.get("nonce").expect("a nonce")
    }
}

/// Begins a sign-in at `uri`.
pub(crate) async fn start(state: &ConsoleState, uri: &str) -> Started {
    let response = send(state, get(uri, &[])).await;
    assert_eq!(
        StatusCode::SEE_OTHER,
        response.status(),
        "a login redirects"
    );
    let cookie = cookie_pair(response.headers(), SIGN_IN).expect("a sign-in cookie");
    let (_, query) = redirect_query(&response);
    Started { query, cookie }
}

/// The callback of `started` with `query`.
pub(crate) fn callback(started: &Started, query: &str) -> http::Request<axum::body::Body> {
    get(
        &format!("/auth/callback?{query}"),
        std::slice::from_ref(&started.cookie),
    )
}

/// Signs an operator in at `provider` and returns the session cookie's
/// `name=value` pair.
pub(crate) async fn signed_in(state: &ConsoleState, provider: &Provider) -> String {
    let started = start(state, "/auth/login").await;
    let id_token = provider.id_token(&provider.claims(started.nonce()));
    provider.answer_tokens(200, &token_answer(&id_token)).await;
    let response = send(
        state,
        callback(&started, &format!("code=c1&state={}", started.state())),
    )
    .await;
    assert_eq!(
        StatusCode::SEE_OTHER,
        response.status(),
        "a sign-in completes"
    );
    cookie_pair(response.headers(), SESSION).expect("a session cookie")
}

/// The session id a `name=value` cookie pair carries.
pub(crate) fn session_id(pair: &str) -> SessionId {
    SessionId::from_cookie(pair.split_once('=').expect("a pair").1)
}

/// The access token the session behind `pair` holds.
pub(crate) fn access_token(state: &ConsoleState, pair: &str) -> Option<String> {
    state
        .sessions()
        .access_token(&session_id(pair))
        .expect("usable")
        .map(|token| token.expose_secret().to_owned())
}

#[tokio::test]
async fn login_redirects_to_the_authorization_endpoint_with_the_code_grant_and_pkce() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let response = send(&state, get("/auth/login", &[])).await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!(
        Some("no-store"),
        response
            .headers()
            .get(CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
    );
    let (url, query) = redirect_query(&response);
    assert_eq!(provider.url("/authorize"), url[..url::Position::AfterPath]);
    assert_eq!("code", query["response_type"]);
    assert_eq!(CLIENT_ID, query["client_id"]);
    assert_eq!(REDIRECT_URI, query["redirect_uri"]);
    assert_eq!("openid user/*.cruds", query["scope"]);
    assert_eq!("S256", query["code_challenge_method"]);
    assert_eq!(
        43,
        query["code_challenge"].len(),
        "the base64url of a SHA-256 digest"
    );
    assert!(query["state"].len() >= 43, "{query:?}");
    assert!(query["nonce"].len() >= 43, "{query:?}");
    assert_ne!(query["state"], query["nonce"]);
    assert!(
        !query.contains_key("prompt"),
        "prompt is sent only with offline_access"
    );
}

#[tokio::test]
async fn login_sets_a_host_bound_http_only_sign_in_cookie_that_expires() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let response = send(&state, get("/auth/login", &[])).await;
    let cookie = set_cookies(response.headers())
        .into_iter()
        .find(|cookie| cookie.starts_with(SIGN_IN))
        .expect("a sign-in cookie");
    for attribute in [
        "HttpOnly",
        "SameSite=Lax",
        "Secure",
        "Path=/",
        "Max-Age=300",
    ] {
        assert!(cookie.contains(attribute), "{attribute} in {cookie}");
    }
    assert_eq!(1, state.sessions().occupancy().expect("usable").sign_ins);
    assert_eq!(0, state.sessions().occupancy().expect("usable").sessions);
}

#[tokio::test]
async fn every_login_mints_its_own_state_and_nonce() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let first = start(&state, "/auth/login").await;
    let second = start(&state, "/auth/login").await;
    assert_ne!(first.state(), second.state());
    assert_ne!(first.nonce(), second.nonce());
    assert_ne!(
        first.query["code_challenge"],
        second.query["code_challenge"]
    );
    assert_ne!(first.cookie, second.cookie);
}

#[tokio::test]
async fn login_keeps_the_query_the_authorization_endpoint_already_carries() {
    let provider = Provider::start().await;
    let oidc = Oidc {
        authorization_endpoint: provider.url("/authorize?tenant=north"),
        ..provider.oidc()
    };
    let state = console(Some(oidc), "");
    let started = start(&state, "/auth/login").await;
    assert_eq!("north", started.query["tenant"]);
    assert_eq!("code", started.query["response_type"]);
}

#[tokio::test]
async fn login_with_offline_access_asks_for_it_with_prompt_consent() {
    let provider = Provider::start().await;
    let oidc = Oidc {
        offline_access: true,
        ..provider.oidc()
    };
    let state = console(Some(oidc), "");
    let started = start(&state, "/auth/login").await;
    assert_eq!("openid user/*.cruds offline_access", started.query["scope"]);
    assert_eq!("consent", started.query["prompt"]);
}

#[tokio::test]
async fn login_on_a_console_without_sign_in_shows_the_unavailable_sign_in_page() {
    let state = console(None, "");
    let response = send(&state, get("/auth/login", &[])).await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!("/auth/sign-in?state=unavailable", location(&response));
    assert!(cookie_pair(response.headers(), SIGN_IN).is_none());
}

#[tokio::test]
async fn the_token_request_carries_the_verifier_whose_s256_challenge_the_login_sent() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let started = start(&state, "/auth/login").await;
    let id_token = provider.id_token(&provider.claims(started.nonce()));
    provider.answer_tokens(200, &token_answer(&id_token)).await;
    let response = send(
        &state,
        callback(
            &started,
            &format!("code=the-code&state={}", started.state()),
        ),
    )
    .await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    let requests = provider.requests_at(TOKEN_PATH).await;
    let [request] = requests.as_slice() else {
        panic!("one token request, got {}", requests.len());
    };
    let fields = form(request);
    assert_eq!("authorization_code", fields["grant_type"]);
    assert_eq!("the-code", fields["code"]);
    assert_eq!(REDIRECT_URI, fields["redirect_uri"]);
    assert_eq!(
        CLIENT_ID, fields["client_id"],
        "a public client names itself"
    );
    // NOTE: RFC 7636 §4.6: the server holds BASE64URL(SHA256(verifier)) to the
    // challenge, so the console's pair must pass that check.
    let digest = aws_lc_rs::digest::digest(
        &aws_lc_rs::digest::SHA256,
        fields["code_verifier"].as_bytes(),
    );
    assert_eq!(
        started.query["code_challenge"],
        URL_SAFE_NO_PAD.encode(digest.as_ref())
    );
}

#[tokio::test]
async fn a_completed_callback_begins_a_session_and_returns_to_the_dashboard() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let started = start(&state, "/auth/login").await;
    let id_token = provider.id_token(&provider.claims(started.nonce()));
    provider.answer_tokens(200, &token_answer(&id_token)).await;
    let response = send(
        &state,
        callback(&started, &format!("code=c1&state={}", started.state())),
    )
    .await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!("/", location(&response));
    let cookies = set_cookies(response.headers());
    let session = cookie_pair(response.headers(), SESSION).expect("a session cookie");
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.starts_with(&format!("{SIGN_IN}=;"))
                && cookie.contains("Max-Age=0")),
        "the spent sign-in cookie is removed: {cookies:?}"
    );
    assert_eq!(
        Some("synthetic-access-token"),
        access_token(&state, &session).as_deref()
    );
    let id = session_id(&session);
    assert_eq!(
        Some(String::from("Synthetic Operator")),
        state.sessions().operator(&id).expect("usable")
    );
    assert_eq!(
        Some(vec![String::from("openid"), String::from("user/*.cruds")]),
        state.sessions().scopes(&id).expect("usable"),
        "an answer that names no scope granted the scopes requested"
    );
    assert_eq!(0, state.sessions().occupancy().expect("usable").sign_ins);
}

#[tokio::test]
async fn a_completed_callback_returns_to_the_page_the_login_named() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let started = start(&state, "/auth/login?back=%2Fsystems%3Ffhir%3Dr4").await;
    let id_token = provider.id_token(&provider.claims(started.nonce()));
    provider.answer_tokens(200, &token_answer(&id_token)).await;
    let response = send(
        &state,
        callback(&started, &format!("code=c1&state={}", started.state())),
    )
    .await;
    assert_eq!("/systems?fhir=r4", location(&response));
}

#[tokio::test]
async fn a_login_naming_a_page_off_the_console_returns_to_the_dashboard() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let started = start(&state, "/auth/login?back=%2F%2Fevil.example%2F").await;
    let id_token = provider.id_token(&provider.claims(started.nonce()));
    provider.answer_tokens(200, &token_answer(&id_token)).await;
    let response = send(
        &state,
        callback(&started, &format!("code=c1&state={}", started.state())),
    )
    .await;
    assert_eq!("/", location(&response));
}

#[tokio::test]
async fn a_callback_without_a_sign_in_cookie_is_refused() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let response = send(&state, get("/auth/callback?code=c1&state=s1", &[])).await;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    assert_eq!("no sign-in is pending", text(response).await);
    assert!(provider.requests_at(TOKEN_PATH).await.is_empty());
}

#[tokio::test]
async fn a_callback_naming_an_unknown_sign_in_is_refused() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let response = send(
        &state,
        get(
            "/auth/callback?code=c1&state=s1",
            &[format!("{SIGN_IN}=not-a-sign-in")],
        ),
    )
    .await;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    assert_eq!("no sign-in is pending", text(response).await);
}

#[tokio::test]
async fn a_callback_whose_state_is_not_the_sign_ins_is_refused_and_spends_it() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let started = start(&state, "/auth/login").await;
    let response = send(&state, callback(&started, "code=c1&state=forged")).await;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    let cookies = set_cookies(response.headers());
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.starts_with(&format!("{SIGN_IN}=;"))),
        "the spent sign-in cookie is removed: {cookies:?}"
    );
    assert_eq!(
        "the sign-in state does not match this session",
        text(response).await
    );
    assert!(
        provider.requests_at(TOKEN_PATH).await.is_empty(),
        "nothing the redirect says is used before its state matches"
    );
    let retried = send(
        &state,
        callback(&started, &format!("code=c1&state={}", started.state())),
    )
    .await;
    assert_eq!(
        StatusCode::BAD_REQUEST,
        retried.status(),
        "a sign-in is taken once, whatever its first callback said"
    );
}

#[tokio::test]
async fn a_callback_without_a_state_is_refused() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let started = start(&state, "/auth/login").await;
    let response = send(&state, callback(&started, "code=c1")).await;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    assert_eq!(
        "the sign-in state does not match this session",
        text(response).await
    );
}

#[tokio::test]
async fn a_provider_error_with_the_right_state_shows_the_refused_sign_in_page() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let started = start(&state, "/auth/login").await;
    let response = send(
        &state,
        callback(
            &started,
            &format!("error=access_denied&state={}", started.state()),
        ),
    )
    .await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!("/auth/sign-in?state=refused", location(&response));
    assert!(cookie_pair(response.headers(), SESSION).is_none());
}

#[tokio::test]
async fn a_provider_error_with_a_forged_state_is_refused_before_it_is_read() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let started = start(&state, "/auth/login").await;
    let response = send(
        &state,
        callback(&started, "error=access_denied&state=forged"),
    )
    .await;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
}

#[tokio::test]
async fn a_callback_without_a_code_is_refused() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let started = start(&state, "/auth/login").await;
    let response = send(
        &state,
        callback(&started, &format!("state={}", started.state())),
    )
    .await;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    assert_eq!("the redirect carries no code", text(response).await);
}

/// The sign-in page a callback lands on when the token endpoint answers
/// `status` with `body`.
async fn landing_after_token_answer(status: u16, body: serde_json::Value) -> String {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let started = start(&state, "/auth/login").await;
    provider.answer_tokens(status, &body).await;
    let response = send(
        &state,
        callback(&started, &format!("code=c1&state={}", started.state())),
    )
    .await;
    assert_eq!(
        StatusCode::SEE_OTHER,
        response.status(),
        "the callback redirects"
    );
    assert!(
        cookie_pair(response.headers(), SESSION).is_none(),
        "no session is begun"
    );
    assert_eq!(
        0,
        state.sessions().occupancy().expect("usable").sessions,
        "no session is held"
    );
    location(&response)
}

#[tokio::test]
async fn a_token_endpoint_that_refuses_the_code_shows_the_refused_sign_in_page() {
    assert_eq!(
        "/auth/sign-in?state=refused",
        landing_after_token_answer(400, json!({"error": "invalid_grant"})).await,
        "the provider answered and refused, so it is not unreachable"
    );
}

#[tokio::test]
async fn a_token_endpoint_that_refuses_the_client_shows_the_refused_sign_in_page() {
    assert_eq!(
        "/auth/sign-in?state=refused",
        landing_after_token_answer(401, json!({"error": "invalid_client"})).await
    );
}

#[tokio::test]
async fn a_token_answer_without_an_id_token_shows_the_rejected_sign_in_page() {
    assert_eq!(
        "/auth/sign-in?state=rejected",
        landing_after_token_answer(200, json!({"access_token": "a", "token_type": "Bearer"})).await,
        "the answer failed the console's checks"
    );
}

#[tokio::test]
async fn a_token_endpoint_failing_on_its_side_shows_the_unreachable_sign_in_page() {
    assert_eq!(
        "/auth/sign-in?state=unreachable",
        landing_after_token_answer(503, json!({"error": "temporarily_unavailable"})).await
    );
}

#[tokio::test]
async fn an_unreachable_token_endpoint_shows_the_unreachable_sign_in_page() {
    let provider = Provider::start().await;
    let oidc = Oidc {
        token_endpoint: String::from("http://127.0.0.1:9/token"),
        ..provider.oidc()
    };
    let state = console(Some(oidc), "");
    let started = start(&state, "/auth/login").await;
    let response = send(
        &state,
        callback(&started, &format!("code=c1&state={}", started.state())),
    )
    .await;
    assert_eq!("/auth/sign-in?state=unreachable", location(&response));
}

#[tokio::test]
async fn an_id_token_with_another_nonce_shows_the_rejected_sign_in_page() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let started = start(&state, "/auth/login").await;
    let other = start(&state, "/auth/login").await;
    let id_token = provider.id_token(&provider.claims(other.nonce()));
    provider.answer_tokens(200, &token_answer(&id_token)).await;
    let response = send(
        &state,
        callback(&started, &format!("code=c1&state={}", started.state())),
    )
    .await;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!("/auth/sign-in?state=rejected", location(&response));
    assert!(cookie_pair(response.headers(), SESSION).is_none());
    assert_eq!(0, state.sessions().occupancy().expect("usable").sessions);
}

#[tokio::test]
async fn signing_in_again_ends_the_session_the_browser_held() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let first = signed_in(&state, &provider).await;
    provider.reset().await;
    let started = start(&state, "/auth/login").await;
    let id_token = provider.id_token(&provider.claims(started.nonce()));
    provider.answer_tokens(200, &token_answer(&id_token)).await;
    let response = send(
        &state,
        get(
            &format!("/auth/callback?code=c2&state={}", started.state()),
            &[started.cookie.clone(), first.clone()],
        ),
    )
    .await;
    let second = cookie_pair(response.headers(), SESSION).expect("a session cookie");
    assert_ne!(first, second);
    assert!(access_token(&state, &first).is_none());
    assert!(access_token(&state, &second).is_some());
    assert_eq!(1, state.sessions().occupancy().expect("usable").sessions);
}

#[tokio::test]
async fn a_full_session_pool_shows_the_full_sign_in_page() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "[session]\nmax_sessions = 1\n");
    state
        .sessions()
        .establish(SignedIn {
            access_token: SecretString::from("another-operator"),
            expires_in: None,
            refresh_token: None,
            id_token: None,
            operator: None,
            scopes: Vec::new(),
        })
        .expect("room for one");
    let started = start(&state, "/auth/login").await;
    let id_token = provider.id_token(&provider.claims(started.nonce()));
    provider.answer_tokens(200, &token_answer(&id_token)).await;
    let response = send(
        &state,
        callback(&started, &format!("code=c1&state={}", started.state())),
    )
    .await;
    assert_eq!("/auth/sign-in?state=full", location(&response));
    assert!(cookie_pair(response.headers(), SESSION).is_none());
}

#[tokio::test]
async fn the_session_cookie_is_host_bound_http_only_lax_and_secure() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "");
    let started = start(&state, "/auth/login").await;
    let id_token = provider.id_token(&provider.claims(started.nonce()));
    provider.answer_tokens(200, &token_answer(&id_token)).await;
    let response = send(
        &state,
        callback(&started, &format!("code=c1&state={}", started.state())),
    )
    .await;
    let cookie = set_cookies(response.headers())
        .into_iter()
        .find(|cookie| cookie.starts_with(&format!("{SESSION}=")))
        .expect("a session cookie");
    for attribute in ["HttpOnly", "SameSite=Lax", "Secure", "Path=/"] {
        assert!(cookie.contains(attribute), "{attribute} in {cookie}");
    }
}
