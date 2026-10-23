// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Operator sign-in at an OpenID Provider: the OAuth 2.0 authorization code
//! grant with PKCE, run by the console's server half.
//!
//! `GET /login` mints the `state`, the `nonce` and the PKCE verifier, keeps
//! them in the bounded pool of pending sign-ins, and redirects the browser to
//! the provider's authorization endpoint (RFC 6749 §4.1.1, RFC 7636 §4.3,
//! OpenID Connect Core 1.0 §3.1.2.1). `GET /auth/callback` is where the
//! provider sends the operator back: it takes the pending sign-in once and
//! holds the returned `state` to it (RFC 6749 §10.12), then [`exchange`]
//! trades the code for the operator's tokens and checks the ID Token, and
//! only then is a signed-in session begun; [`refresh`] renews its access
//! token before it expires, and [`logout`] ends it. The browser never sees a
//! token. No specification of the FHIR server governs sign-in to the console: our
//! own design on RFC 6749, RFC 7636 and OpenID Connect Core 1.0.

use axum::Extension;
use axum::extract::Query;
use axum::response::{IntoResponse, Response};
use http::header::{CACHE_CONTROL, LOCATION, SET_COOKIE};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde::Deserialize;
use url::Url;

use crate::config::settings::{OFFLINE_ACCESS, OidcSettings};
use crate::server::ConsoleState;
use crate::session::{COOKIE, PendingSignIn, SIGN_IN_COOKIE, SessionError, challenge};

pub mod exchange;
pub mod logout;
pub mod refresh;

/// The path of the sign-in route.
pub const LOGIN: &str = crate::routes::LOGIN_ROUTE;

/// The path of the redirection endpoint the provider sends the operator back
/// to.
pub const CALLBACK: &str = "/auth/callback";

/// Why the sign-in page is shown, as its `state` parameter names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignInState {
    /// The reader signed out of the console.
    SignedOut,
    /// The session ended after its idle timeout.
    Expired,
    /// The session ended at its absolute timeout.
    ExpiredAbsolute,
    /// The console holds as many sessions as it may.
    Full,
    /// The provider refused the sign-in.
    Refused,
    /// The provider's answer failed the console's checks.
    Rejected,
    /// The console could not reach the provider.
    Unreachable,
    /// The console offers no sign-in.
    Unavailable,
}

impl SignInState {
    /// Every state, in the order the page knows them.
    pub const ALL: [Self; 8] = [
        Self::SignedOut,
        Self::Expired,
        Self::ExpiredAbsolute,
        Self::Full,
        Self::Refused,
        Self::Rejected,
        Self::Unreachable,
        Self::Unavailable,
    ];

    /// The `state` parameter that names it.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::SignedOut => "signed-out",
            Self::Expired => "expired",
            Self::ExpiredAbsolute => "expired-absolute",
            Self::Full => "full",
            Self::Refused => "refused",
            Self::Rejected => "rejected",
            Self::Unreachable => "unreachable",
            Self::Unavailable => "unavailable",
        }
    }

    /// The state `key` names, or `None` for one the page does not know.
    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.key() == key)
    }
}

/// The redirect to the sign-in page saying `state`.
#[must_use]
pub fn to_sign_in(state: SignInState) -> Response {
    let location = format!("{}?state={}", crate::routes::SIGN_IN_PAGE, state.key());
    let Ok(location) = HeaderValue::from_str(&location) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    let headers = response.headers_mut();
    headers.insert(LOCATION, location);
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Returns the authorization request for `pending` at the provider `oidc`.
///
/// The request is the authorization endpoint with the code grant's
/// parameters and the `S256` PKCE challenge appended to whatever query it
/// already carries, which RFC 6749 §3.1 requires a client to keep.
#[must_use]
pub fn authorization_request(oidc: &OidcSettings, pending: &PendingSignIn) -> Url {
    let mut scopes: Vec<&str> = oidc.scopes.iter().map(String::as_str).collect();
    if oidc.offline_access {
        scopes.push(OFFLINE_ACCESS);
    }
    let mut url = oidc.authorization_endpoint.clone();
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", &oidc.client_id)
            .append_pair("redirect_uri", oidc.redirect_uri.as_str())
            .append_pair("scope", &scopes.join(" "))
            .append_pair("state", pending.state())
            .append_pair("nonce", pending.nonce())
            .append_pair("code_challenge", &challenge(pending.verifier()))
            .append_pair("code_challenge_method", "S256");
        // NOTE: OpenID Connect Core 1.0 §11: a request for `offline_access`
        // MUST carry the `prompt` value `consent`.
        if oidc.offline_access {
            query.append_pair("prompt", "consent");
        }
    }
    url
}

/// What the sign-in route takes: the page to return to.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct LoginQuery {
    /// The console's page the reader returns to once signed in.
    pub back: Option<String>,
}

/// Serves `GET /auth/login`: begins a sign-in and redirects to the
/// provider, remembering the page to return to.
pub async fn login(
    Extension(state): Extension<ConsoleState>,
    Query(query): Query<LoginQuery>,
) -> Response {
    let Some(oidc) = state.settings().oidc.as_ref() else {
        return to_sign_in(SignInState::Unavailable);
    };
    let back = query
        .back
        .as_deref()
        .and_then(crate::post::return_path)
        .map(str::to_owned);
    let pending = match PendingSignIn::mint() {
        Ok(pending) => pending.returning_to(back),
        Err(error) => return refused(&error),
    };
    let location = authorization_request(oidc, &pending);
    let id = match state.sessions().begin(pending) {
        Ok(id) => id,
        Err(error) => return refused(&error),
    };
    let cookie = state.sessions().sign_in_cookie(&id).to_string();
    let (Ok(location), Ok(cookie)) = (
        HeaderValue::from_str(location.as_str()),
        HeaderValue::from_str(&cookie),
    ) else {
        tracing::error!("the sign-in redirect could not be written as headers");
        return plain(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the sign-in redirect could not be written",
        );
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    let headers = response.headers_mut();
    headers.insert(LOCATION, location);
    headers.insert(SET_COOKIE, cookie);
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The query the provider sends the operator back with (RFC 6749 §4.1.2 and
/// §4.1.2.1).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CallbackQuery {
    /// The authorization code.
    pub code: Option<String>,
    /// The `state` the authorization request carried.
    pub state: Option<String>,
    /// The error code of a refused request.
    pub error: Option<String>,
}

/// Serves `GET /auth/callback`: holds the returned `state` to the pending
/// sign-in the browser's cookie names, and completes it.
pub async fn callback(
    Extension(state): Extension<ConsoleState>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let Some(id) = state.sessions().cookie_in(&headers, SIGN_IN_COOKIE) else {
        return plain(StatusCode::BAD_REQUEST, "no sign-in is pending");
    };
    let mut response = match state.sessions().take_pending(&id) {
        Ok(Some(pending)) => redirected(&state, &headers, &pending, &query).await,
        Ok(None) => plain(StatusCode::BAD_REQUEST, "no sign-in is pending"),
        Err(error) => return refused(&error),
    };
    // NOTE: no specification governs this: our own design; the sign-in is
    // spent either way, so the browser is told to drop its cookie.
    if let Ok(removal) =
        HeaderValue::from_str(&state.sessions().sign_in_cookie_removal().to_string())
    {
        response.headers_mut().append(SET_COOKIE, removal);
    }
    response
}

/// The answer to a redirect back for the pending sign-in `pending`: the
/// code exchanged, the ID Token checked, and a signed-in session begun.
async fn redirected(
    state: &ConsoleState,
    headers: &HeaderMap,
    pending: &PendingSignIn,
    query: &CallbackQuery,
) -> Response {
    // NOTE: RFC 6749 §10.12: a redirect whose `state` is not the one the
    // request carried is refused before anything else it says is read.
    if !query
        .state
        .as_deref()
        .is_some_and(|sent| pending.answers(sent))
    {
        return plain(
            StatusCode::BAD_REQUEST,
            "the sign-in state does not match this session",
        );
    }
    if query.error.is_some() {
        return to_sign_in(SignInState::Refused);
    }
    let Some(code) = query.code.as_deref() else {
        return plain(StatusCode::BAD_REQUEST, "the redirect carries no code");
    };
    let Some(oidc) = state.settings().oidc.as_ref() else {
        return plain(
            StatusCode::SERVICE_UNAVAILABLE,
            "sign-in is not configured on this console",
        );
    };
    let signed_in = match exchange::exchange(state.provider(), oidc, pending, code).await {
        Ok(signed_in) => signed_in,
        Err(error) if error.refuses_the_operator() => {
            tracing::warn!(error = %error, "a sign-in's ID Token was refused");
            return to_sign_in(SignInState::Rejected);
        }
        // NOTE: RFC 6749 §5.2: a provider that answered the exchange was
        // reached, so its refusal or its malformed answer is never "unreachable".
        Err(error) if error.is_answered_refusal() => {
            tracing::warn!(error = %error, "the OpenID Provider refused a sign-in's code exchange");
            return to_sign_in(match error {
                exchange::ExchangeError::Refused { .. } => SignInState::Refused,
                _ => SignInState::Rejected,
            });
        }
        Err(error) => {
            tracing::error!(
                error = %crate::command::chain(&error),
                "a sign-in could not be completed at the OpenID Provider"
            );
            return to_sign_in(SignInState::Unreachable);
        }
    };
    // NOTE: no specification governs this: our own design; a new sign-in ends
    // the session the browser held, so signing in again never fills the pool.
    if let Some(replaced) = state.sessions().cookie_in(headers, COOKIE)
        && let Err(error) = state.sessions().end(&replaced)
    {
        return refused(&error);
    }
    let id = match state.sessions().establish(signed_in) {
        Ok(id) => id,
        Err(error) => return refused(&error),
    };
    let Ok(cookie) = HeaderValue::from_str(&state.sessions().session_cookie(&id).to_string())
    else {
        tracing::error!("the session cookie could not be written as a header");
        return plain(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the session could not be begun",
        );
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    let headers = response.headers_mut();
    let back = pending
        .back()
        .and_then(|back| HeaderValue::from_str(back).ok())
        .unwrap_or_else(|| HeaderValue::from_static("/"));
    headers.insert(LOCATION, back);
    headers.insert(SET_COOKIE, cookie);
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The answer to a session store that failed.
fn refused(error: &SessionError) -> Response {
    match error {
        SessionError::Full => to_sign_in(SignInState::Full),
        SessionError::Random | SessionError::Poisoned => {
            tracing::error!(%error, "a sign-in session could not be begun");
            plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                "a sign-in session could not be begun",
            )
        }
    }
}

/// A plain-text answer that no cache keeps.
fn plain(status: StatusCode, text: &'static str) -> Response {
    (status, [(CACHE_CONTROL, "no-store")], text).into_response()
}
