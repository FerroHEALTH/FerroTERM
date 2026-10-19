// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refresh of the operator's access token on the console's server.
//!
//! Before a request is served, a session whose access token expires within
//! [`MARGIN`], or within half its lifetime when that is shorter, and that
//! holds a refresh token sends it to the provider's token endpoint (RFC 6749
//! §6). One refresh of a session runs at a time, so a rotated refresh token
//! is spent once. A rotated refresh token replaces the old one, which is
//! discarded. A refresh the provider refuses (`invalid_grant` among them)
//! ends the session, and the operator signs in again; a provider that does
//! not answer, or fails on its side, leaves the session as it was, so the
//! next request tries again. The idle and the absolute timeouts end a
//! session whatever its tokens say. The refresh token never leaves the
//! server (RFC 6749 §10.4) and is never logged.

use std::time::Duration;

use axum::Extension;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use http::header::SET_COOKIE;
use http::{HeaderValue, StatusCode};
use secrecy::{ExposeSecret as _, SecretString};

use crate::config::settings::OidcSettings;
use crate::oidc::exchange::{ExchangeError, token_request};
use crate::server::ViewerState;
use crate::session::refresh::Refreshed;
use crate::session::{COOKIE, SessionError, SessionId};

/// How long before its access token expires a session refreshes it.
pub const MARGIN: Duration = Duration::from_secs(60);

/// What became of a session a request carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// The session holds an access token that is not due, has none to
    /// refresh, is not held at all, or kept its tokens through a provider
    /// that did not answer.
    Current,
    /// Its refresh was refused, and the session is ended.
    Ended,
}

/// Refreshes the access token of the session `id` when it is due.
///
/// One refresh of a session runs at a time: a request that waited on
/// another's finds the token current and sends nothing.
///
/// # Errors
/// Returns [`SessionError::Poisoned`] when the session store is unusable.
pub async fn keep_fresh(state: &ViewerState, id: &SessionId) -> Result<Freshness, SessionError> {
    let sessions = state.sessions();
    let Some(gate) = sessions.refresh_due(id, MARGIN)? else {
        return Ok(Freshness::Current);
    };
    let _refreshing = gate.lock().await;
    let Some(refresh_token) = sessions.refresh_grant(id, MARGIN)? else {
        return Ok(Freshness::Current);
    };
    let Some(oidc) = state.settings().oidc.as_ref() else {
        sessions.end(id)?;
        return Ok(Freshness::Ended);
    };
    match refresh(state.provider(), oidc, &refresh_token).await {
        Ok(refreshed) => {
            sessions.refreshed(id, refreshed)?;
            Ok(Freshness::Current)
        }
        Err(error) if error.is_answered_refusal() => {
            tracing::warn!(
                error = %error,
                "the OpenID Provider refused to refresh an operator's access token, so the session is ended"
            );
            sessions.end(id)?;
            Ok(Freshness::Ended)
        }
        Err(error) => {
            tracing::error!(
                error = %crate::command::chain(&error),
                "an operator's access token could not be refreshed at the OpenID Provider"
            );
            Ok(Freshness::Current)
        }
    }
}

/// Sends `refresh_token` to the provider's token endpoint for a new access
/// token (RFC 6749 §6), with the scope the operator granted at sign-in.
///
/// # Errors
/// Returns the [`ExchangeError`] of a provider that cannot be reached,
/// refuses the refresh, or answers no bearer token.
pub async fn refresh(
    client: &reqwest::Client,
    oidc: &OidcSettings,
    refresh_token: &SecretString,
) -> Result<Refreshed, ExchangeError> {
    let grant = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token.expose_secret()),
    ];
    let tokens = token_request(client, oidc, &grant).await?;
    // NOTE: OpenID Connect Core 1.0 §12.2: an ID Token a refresh answers is not
    // read; the sign-in's verified one stays the sign-out hint.
    Ok(Refreshed {
        scopes: tokens
            .scope
            .as_deref()
            .map(|granted| granted.split_whitespace().map(str::to_owned).collect()),
        access_token: SecretString::from(tokens.access_token),
        expires_in: tokens.expires_in.map(Duration::from_secs),
        refresh_token: tokens.refresh_token.map(SecretString::from),
    })
}

/// Refreshes the access token of the session a request carries before the
/// request is served, and tells the browser to drop the cookie of a session
/// a refused refresh ended.
///
/// The sign-in, callback and sign-out routes are served as they come.
pub async fn refresh_session(
    Extension(state): Extension<ViewerState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().trim_end_matches('/');
    if [super::LOGIN, super::CALLBACK, super::logout::LOGOUT].contains(&path) {
        return next.run(request).await;
    }
    let Some(id) = state.sessions().cookie_in(request.headers(), COOKIE) else {
        return next.run(request).await;
    };
    match keep_fresh(&state, &id).await {
        Ok(Freshness::Current) => next.run(request).await,
        Ok(Freshness::Ended) => {
            let mut response = next.run(request).await;
            if let Ok(removal) =
                HeaderValue::from_str(&state.sessions().session_cookie_removal().to_string())
            {
                response.headers_mut().append(SET_COOKIE, removal);
            }
            response
        }
        // NOTE: RFC 9110 §15.6.1: the answer stays a 500, and the page drawn
        // for it says why in place of an empty body.
        Err(error) => {
            tracing::error!(%error, "the session store could not be read");
            let mut response = next.run(request).await;
            if !response.status().is_server_error() {
                *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            }
            response
        }
    }
}
