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
use crate::server::ViewerState;
use crate::session::{COOKIE, PendingSignIn, SIGN_IN_COOKIE, SessionError, challenge};

pub mod exchange;
pub mod logout;
pub mod refresh;

/// The path of the sign-in route.
pub const LOGIN: &str = crate::components::shell::SIGN_IN;

/// The path of the redirection endpoint the provider sends the operator back
/// to.
pub const CALLBACK: &str = "/auth/callback";

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

/// Serves `GET /login`: begins a sign-in and redirects to the provider.
pub async fn login(Extension(state): Extension<ViewerState>) -> Response {
    let Some(oidc) = state.settings().oidc.as_ref() else {
        return plain(
            StatusCode::SERVICE_UNAVAILABLE,
            "sign-in is not configured on this console",
        );
    };
    let pending = match PendingSignIn::mint() {
        Ok(pending) => pending,
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
    Extension(state): Extension<ViewerState>,
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
    state: &ViewerState,
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
        return plain(
            StatusCode::UNAUTHORIZED,
            "the OpenID Provider did not sign the operator in",
        );
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
            return plain(
                StatusCode::UNAUTHORIZED,
                "the OpenID Provider's ID Token did not verify",
            );
        }
        Err(error) => {
            tracing::error!(
                error = %crate::command::chain(&error),
                "a sign-in could not be completed at the OpenID Provider"
            );
            return plain(
                StatusCode::BAD_GATEWAY,
                "the sign-in could not be completed at the OpenID Provider",
            );
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
    headers.insert(LOCATION, HeaderValue::from_static("/"));
    headers.insert(SET_COOKIE, cookie);
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The answer to a session store that failed.
fn refused(error: &SessionError) -> Response {
    match error {
        SessionError::Full => plain(
            StatusCode::SERVICE_UNAVAILABLE,
            "the console holds as many sessions as it may; try again later",
        ),
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
