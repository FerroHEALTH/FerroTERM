// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's pages around the sign-in `ferrohealth-session` runs: where
//! a stopped sign-in lands, where a sign-out lands, and the page the
//! operator comes home to.
//!
//! A sign-in the operator can read about lands on the sign-in page in the
//! state that says why, through a `303` to the page's own address, so a
//! reload draws the page again and never repeats the callback, whose code
//! is spent. A redirect the console cannot hold to a pending sign-in is
//! refused in plain text with the status `ferrohealth-session` names, and
//! carries no screen. No specification governs this: our own design.

use std::future::Future;

use axum::response::{IntoResponse as _, Response};
use ferrohealth_session::routes::{Pages, Stopped};
use http::header::{CACHE_CONTROL, LOCATION};
use http::request::Parts;
use http::{HeaderValue, StatusCode};

use crate::locale::{Msg, tr};
use crate::pages::sign_in::SignInState;

/// The pages `ferrohealth-session` sends the operator to.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConsolePages;

impl Pages for ConsolePages {
    fn stopped(&self, _parts: Parts, why: Stopped) -> impl Future<Output = Response> + Send {
        std::future::ready(answer(&why))
    }

    fn sign_in_page(&self) -> &'static str {
        crate::routes::SIGN_IN_PAGE
    }

    fn signed_out(&self) -> &'static str {
        SIGNED_OUT
    }

    fn home(&self) -> &'static str {
        "/"
    }
}

/// Where a sign-out lands when the provider has no end-session endpoint.
const SIGNED_OUT: &str = "/auth/sign-in?state=signed-out";

/// The answer to a sign-in that stopped for `why`.
fn answer(why: &Stopped) -> Response {
    let status = why.status();
    match why {
        Stopped::NotConfigured => to_sign_in(SignInState::Unavailable),
        Stopped::Full => to_sign_in(SignInState::Full),
        Stopped::Rejected(error) if error.oauth_error().is_some() => {
            to_sign_in(SignInState::Refused)
        }
        Stopped::Rejected(_) => to_sign_in(SignInState::Rejected),
        // NOTE: RFC 6749 §5.2: a provider that answered the exchange was reached, so
        // its refusal or its malformed answer is never "unreachable".
        Stopped::NotCompleted(error)
            if error
                .status()
                .is_some_and(|status| status.is_client_error()) =>
        {
            to_sign_in(SignInState::Refused)
        }
        Stopped::NotCompleted(error) if error.is_answered_refusal() => {
            to_sign_in(SignInState::Rejected)
        }
        Stopped::NotCompleted(_) => to_sign_in(SignInState::Unreachable),
        Stopped::NoSignInPending => plain(status, Msg::SignInNoPending),
        Stopped::StateMismatch => plain(status, Msg::SignInStateMismatch),
        Stopped::NoCode => plain(status, Msg::SignInNoCode),
        _ => plain(status, Msg::SignInNotBegun),
    }
}

/// The redirect to the sign-in page saying `state`.
fn to_sign_in(state: SignInState) -> Response {
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

/// A plain-text answer that no cache keeps, saying `why` in English, the
/// console's fallback, since a refused redirect carries no page to draw in
/// its locale.
fn plain(status: StatusCode, why: Msg) -> Response {
    (
        status,
        [(CACHE_CONTROL, "no-store")],
        tr(&ferrohealth_ui::i18n::Locale::english(), why),
    )
        .into_response()
}
