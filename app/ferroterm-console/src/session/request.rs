// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The session of the request being rendered or answered, read from the
//! context the server provides to every page and server function.
//!
//! A visitor reads the console with no session at all, as the terminology
//! server's read surface asks for no token; a signed-in reader's session
//! carries the access token every request to the server presents.

use leptos::context::use_context;
use secrecy::SecretString;

use crate::auth::Operator;
use crate::server::ConsoleState;
use crate::session::{COOKIE, SessionError};

/// The access token of the request's live signed-in session, or `None` for
/// a visitor and for a session that is no longer live.
///
/// # Errors
/// Returns [`SessionError::Poisoned`] when the session store is unusable.
pub fn access_token() -> Result<Option<SecretString>, SessionError> {
    let Some(state) = use_context::<ConsoleState>() else {
        return Ok(None);
    };
    let Some(parts) = use_context::<http::request::Parts>() else {
        return Ok(None);
    };
    let Some(id) = state.sessions().cookie_in(&parts.headers, COOKIE) else {
        return Ok(None);
    };
    state.sessions().access_token(&id)
}

/// The signed-in reader of the request, as the page shows them.
///
/// The page is given their name and the scopes their token grants, never the
/// token: `None` for a visitor, and for a session store that cannot be read,
/// which the server logs.
#[must_use]
pub fn operator() -> Option<Operator> {
    let state = use_context::<ConsoleState>()?;
    let parts = use_context::<http::request::Parts>()?;
    let id = state.sessions().cookie_in(&parts.headers, COOKIE)?;
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
