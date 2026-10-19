// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Operator sign-out at `POST /logout`.
//!
//! It ends the server-side session, tells the provider's revocation endpoint
//! the session's token is finished with when one is configured (RFC 7009
//! §2.1), removes the session cookie, and sends the operator to the
//! provider's end-session endpoint when one is configured (OpenID Connect
//! RP-Initiated Logout 1.0 §2).
//!
//! A sign-out is a `POST`, which the server takes only from the console's own
//! pages, as it takes every request that is not a safe method
//! ([`crate::server::same_origin`]).

use axum::Extension;
use axum::response::{IntoResponse, Response};
use http::header::{CACHE_CONTROL, LOCATION, SET_COOKIE};
use http::{HeaderMap, HeaderValue, StatusCode};
use secrecy::{ExposeSecret as _, SecretString};
use url::Url;

use crate::config::settings::OidcSettings;
use crate::server::ViewerState;
use crate::session::{COOKIE, Ended};

/// The path of the sign-out route.
pub const LOGOUT: &str = crate::components::shell::SIGN_OUT;

/// The provider's end-session request for `oidc`.
///
/// It is the endpoint with the ID Token hint, the console's client id, and
/// the registered post-logout redirect, appended to whatever query the
/// endpoint already carries (OpenID Connect RP-Initiated Logout 1.0 §2).
///
/// Returns `None` when the provider publishes no end-session endpoint.
#[must_use]
pub fn end_session_request(oidc: &OidcSettings, id_token: Option<&SecretString>) -> Option<Url> {
    let mut url = oidc.end_session_endpoint.clone()?;
    {
        let mut query = url.query_pairs_mut();
        if let Some(id_token) = id_token {
            query.append_pair("id_token_hint", id_token.expose_secret());
        }
        query.append_pair("client_id", &oidc.client_id);
        if let Some(redirect) = &oidc.post_logout_redirect_uri {
            query.append_pair("post_logout_redirect_uri", redirect.as_str());
        }
    }
    Some(url)
}

/// Serves `POST /logout`: ends the session the browser holds, removes its
/// cookie, and redirects to the provider's end-session endpoint or to the
/// landing page.
pub async fn logout(Extension(state): Extension<ViewerState>, headers: HeaderMap) -> Response {
    let Some(oidc) = state.settings().oidc.as_ref() else {
        return super::plain(
            StatusCode::SERVICE_UNAVAILABLE,
            "sign-in is not configured on this console",
        );
    };
    let sessions = state.sessions();
    let held = sessions.cookie_in(&headers, COOKIE);
    let ended = match held.map(|id| sessions.end(&id)).transpose() {
        Ok(ended) => ended.flatten(),
        Err(error) => {
            tracing::error!(%error, "a session could not be ended");
            return super::plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the session could not be ended",
            );
        }
    };
    let id_token = match ended {
        Some(Ended {
            id_token,
            revocable: (token, hint),
        }) => {
            revoke(state.provider(), oidc, &token, hint.as_str()).await;
            id_token
        }
        None => None,
    };
    let location = end_session_request(oidc, id_token.as_ref())
        .map_or_else(|| String::from("/"), String::from);
    let removal = sessions.session_cookie_removal().to_string();
    let (Ok(location), Ok(removal)) = (
        HeaderValue::from_str(&location),
        HeaderValue::from_str(&removal),
    ) else {
        tracing::error!("the sign-out redirect could not be written as headers");
        return super::plain(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the sign-out redirect could not be written",
        );
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    let headers = response.headers_mut();
    headers.insert(LOCATION, location);
    headers.insert(SET_COOKIE, removal);
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Tells the provider's revocation endpoint, when `oidc` names one, that
/// `token` is finished with (RFC 7009 §2.1).
///
/// The session is already ended here, so a refused or unreachable revocation
/// leaves the token valid at the provider until it expires and is logged with
/// the provider's status; the reader is signed out of the console either way.
async fn revoke(client: &reqwest::Client, oidc: &OidcSettings, token: &SecretString, hint: &str) {
    let Some(endpoint) = oidc.revocation_endpoint.clone() else {
        return;
    };
    let grant = [("token", token.expose_secret()), ("token_type_hint", hint)];
    match super::exchange::client_post(client, oidc, endpoint, &grant).await {
        // NOTE: RFC 7009 §2.2: the provider answers 200 whether or not it knew
        // the token, so 200 is the one success.
        Ok(status) if status == StatusCode::OK => {}
        Ok(status) => {
            tracing::warn!(%status, "the OpenID Provider refused to revoke a signed-out token");
        }
        Err(error) => {
            tracing::warn!(%error, "the OpenID Provider could not be reached to revoke a signed-out token");
        }
    }
}
