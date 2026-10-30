// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The session of the request being answered, read from its cookie in the
//! console's session store.
//!
//! A visitor reads the console with no session at all, as the terminology
//! server's read surface asks for no token; a signed-in reader's session
//! carries the access token every request to the server presents.

use secrecy::SecretString;

use crate::auth::Operator;
use crate::server::ConsoleState;
use crate::session::{COOKIE, SessionError};

/// The access token of the live signed-in session the request whose headers
/// are `headers` carries, or `None` for a visitor and for a session that is
/// no longer live.
///
/// # Errors
/// Returns [`SessionError::Poisoned`] when the session store is unusable.
pub fn access_token(
    state: &ConsoleState,
    headers: &http::HeaderMap,
) -> Result<Option<SecretString>, SessionError> {
    let Some(id) = state.sessions().cookie_in(headers, COOKIE) else {
        return Ok(None);
    };
    state.sessions().access_token(&id)
}

/// The signed-in reader of the request whose headers are `headers`, as the
/// page shows them.
///
/// The page is given their name and the scopes their token grants, never the
/// token: `None` for a visitor, and for a session store that cannot be read,
/// which the server logs.
#[must_use]
pub fn operator(state: &ConsoleState, headers: &http::HeaderMap) -> Option<Operator> {
    let id = state.sessions().cookie_in(headers, COOKIE)?;
    let read = || -> Result<Option<Operator>, SessionError> {
        if state.sessions().access_token(&id)?.is_none() {
            return Ok(None);
        }
        let scopes = state.sessions().scopes(&id)?.unwrap_or_default();
        let name = state.sessions().operator(&id)?;
        Ok(Some(Operator { name, scopes }))
    };
    read().unwrap_or_else(|error| {
        tracing::error!(%error, "the session store could not be read");
        None
    })
}

/// What the live signed-in session of the request whose headers are
/// `headers` has left, read without extending it; `None` for a visitor or a
/// session that is gone.
#[must_use]
pub fn left(
    state: &ConsoleState,
    headers: &http::HeaderMap,
) -> Option<ferrohealth_ui::view::session::Left> {
    let id = state.sessions().cookie_in(headers, COOKIE)?;
    state
        .sessions()
        .remaining(&id, false)
        .unwrap_or_else(|error| {
            tracing::error!(%error, "the session store could not be read");
            None
        })
        .map(crate::session::watch::left)
}
