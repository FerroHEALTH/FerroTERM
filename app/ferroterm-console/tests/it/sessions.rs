// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's session store as `ferrohealth-session` keeps it for the
//! console's `[session]`: the PKCE pair of a sign-in (RFC 7636), the two
//! bounded pools, the lifetimes, the cookies and what no rendering shows.

use std::time::Duration;

use ferrohealth_secret::secret::Secret;
use ferrohealth_session::record::{Held, SignedIn};
use ferrohealth_smart::oidc::PendingSignIn;
use http::header::COOKIE;
use http::{HeaderMap, StatusCode};

use crate::provider::{
    Provider, console, get, held, send, sessions_held, set_cookies, sign_ins_held, text,
};
use crate::sign_in::{SESSION, SIGN_IN, callback, signed_in, start};

/// One second and a little more: the store counts whole seconds.
const PAST_A_SECOND: Duration = Duration::from_millis(1100);

/// The `name=value` pair of the cookie a `Set-Cookie` value sets.
fn pair(set_cookie: &http::HeaderValue) -> String {
    set_cookie
        .to_str()
        .expect("ASCII")
        .split(';')
        .next()
        .expect("a pair")
        .to_owned()
}

#[test]
fn the_s256_challenge_is_the_one_rfc_7636_appendix_b_derives() {
    assert_eq!(
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
        ferrohealth_smart::oidc::challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")
    );
}

#[test]
fn a_minted_sign_in_carries_a_verifier_of_the_length_rfc_7636_admits() {
    let pending = PendingSignIn::mint().expect("the random source works");
    let verifier = pending.verifier().expose().len();
    assert!((43..=128).contains(&verifier), "{verifier}");
    assert_ne!(pending.state(), pending.verifier().expose());
    assert_ne!(pending.state(), pending.nonce());
}

#[tokio::test]
async fn a_full_sign_in_pool_drops_its_oldest_and_never_touches_the_sessions() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "[session]\nmax_sign_ins = 4\n").await;
    let operator = signed_in(&state, &provider).await;
    let first = start(&state, "/login").await;
    let mut last = start(&state, "/login").await;
    for _ in 0..20 {
        last = start(&state, "/login").await;
    }
    assert_eq!(4, sign_ins_held(&state));
    assert_eq!(1, sessions_held(&state));
    let dropped = send(
        &state,
        callback(&first, &format!("code=c1&state={}", first.state())),
    )
    .await;
    assert_eq!(StatusCode::BAD_REQUEST, dropped.status());
    assert_eq!("no sign-in is pending", text(dropped).await);
    let kept = send(&state, callback(&last, "code=c1&state=forged")).await;
    assert_eq!(
        "the sign-in state does not match this session",
        text(kept).await,
        "the newest sign-in is still pending"
    );
    assert!(matches!(held(&state, &operator), Held::SignedIn(_)));
}

#[tokio::test]
async fn an_expired_sign_in_is_refused() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "[session]\nsign_in_timeout_s = 1\n").await;
    let started = start(&state, "/login").await;
    tokio::time::sleep(PAST_A_SECOND).await;
    let response = send(
        &state,
        callback(&started, &format!("code=c1&state={}", started.state())),
    )
    .await;
    assert_eq!(StatusCode::BAD_REQUEST, response.status());
    assert_eq!("no sign-in is pending", text(response).await);
}

#[tokio::test]
async fn a_session_past_its_idle_timeout_is_no_longer_live() {
    let state = console(None, "[session]\nidle_timeout_s = 2\n").await;
    let cookie = state
        .sessions()
        .establish(
            &HeaderMap::new(),
            SignedIn::basic("operator", Secret::new("synthetic-password")),
        )
        .expect("room");
    let session = pair(&cookie);
    assert!(matches!(held(&state, &session), Held::SignedIn(_)));
    tokio::time::sleep(PAST_A_SECOND * 2).await;
    assert!(matches!(held(&state, &session), Held::Ended), "idle");
}

#[tokio::test]
async fn a_session_past_its_absolute_lifetime_is_no_longer_live_whatever_its_activity() {
    let state = console(
        None,
        "[session]\nidle_timeout_s = 3\nabsolute_timeout_s = 3\n",
    )
    .await;
    let cookie = state
        .sessions()
        .establish(
            &HeaderMap::new(),
            SignedIn::basic("operator", Secret::new("synthetic-password")),
        )
        .expect("room");
    let session = pair(&cookie);
    tokio::time::sleep(PAST_A_SECOND).await;
    assert!(
        matches!(held(&state, &session), Held::SignedIn(_)),
        "active"
    );
    tokio::time::sleep(PAST_A_SECOND * 2).await;
    assert!(matches!(held(&state, &session), Held::Ended), "absolute");
    assert_eq!(0, sessions_held(&state));
}

#[tokio::test]
async fn the_session_and_sign_in_cookies_are_host_bound_http_only_lax_and_secure() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "[session]\nsign_in_timeout_s = 60\n").await;
    let login = send(&state, get("/login", &[])).await;
    let sign_in = set_cookies(login.headers())
        .into_iter()
        .find(|cookie| cookie.starts_with(&format!("{SIGN_IN}=")))
        .expect("a sign-in cookie");
    assert!(
        sign_in.ends_with("; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=60"),
        "{sign_in}"
    );
    let session = signed_in(&state, &provider).await;
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, session.parse().expect("a cookie header"));
    let ended = state.sessions().end(&headers).expect("usable");
    let [removal] = ended.cookies.as_slice() else {
        panic!("one removal");
    };
    assert_eq!(
        format!("{SESSION}=; HttpOnly; SameSite=Lax; Secure; Path=/; Max-Age=0"),
        removal.to_str().expect("ASCII")
    );
}

#[tokio::test]
async fn a_cookie_that_is_not_secure_carries_no_host_prefix_a_browser_would_refuse() {
    let state = console(None, "[session]\nsecure_cookie = false\n").await;
    let cookie = state
        .sessions()
        .establish(
            &HeaderMap::new(),
            SignedIn::basic("operator", Secret::new("synthetic-password")),
        )
        .expect("room");
    assert!(
        pair(&cookie).starts_with("ferroterm_console_session="),
        "{cookie:?}"
    );
}

#[tokio::test]
async fn only_the_prefixed_cookie_is_read_and_a_malformed_pair_is_skipped() {
    let state = console(None, "").await;
    let cookie = state
        .sessions()
        .establish(
            &HeaderMap::new(),
            SignedIn::basic("operator", Secret::new("synthetic-password")),
        )
        .expect("room");
    let real = pair(&cookie);
    let id = real.split_once('=').expect("a pair").1;
    let mut headers = HeaderMap::new();
    headers.append(
        COOKIE,
        format!("ferroterm_console_session={id}; =broken")
            .parse()
            .expect("a header"),
    );
    assert!(
        matches!(
            state.sessions().held(&headers).expect("usable").value,
            Held::Absent
        ),
        "a cookie a sibling host could set is not the session"
    );
    headers.append(COOKIE, real.parse().expect("a header"));
    assert!(matches!(
        state.sessions().held(&headers).expect("usable").value,
        Held::SignedIn(_)
    ));
}

#[tokio::test]
async fn no_rendering_shows_a_token_an_id_or_a_sign_in_s_secrets() {
    let provider = Provider::start().await;
    let state = console(Some(provider.oidc()), "").await;
    let session = signed_in(&state, &provider).await;
    let id = session.split_once('=').expect("a pair").1.to_owned();
    let pending = PendingSignIn::mint().expect("the random source works");
    let shown = format!(
        "{:?} {:?} {pending:?} {state:?}",
        held(&state, &session),
        state.sessions()
    );
    for secret in [
        id.as_str(),
        "synthetic-access-token",
        "synthetic-refresh-token",
        pending.state(),
        pending.nonce(),
        pending.verifier().expose(),
    ] {
        assert!(!shown.contains(secret), "{shown}");
    }
}
