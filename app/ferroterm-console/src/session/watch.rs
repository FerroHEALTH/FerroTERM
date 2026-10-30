// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The session route FerroUI's session watch asks: what the reader's
//! session has left, and the reader's own activity extending it.
//!
//! The wire is FerroUI's (`docs/consoles.md`, "The session's end"): `GET`
//! answers the whole seconds left until the idle timeout and the absolute
//! lifetime and extends nothing, `POST` starts the idle timeout again first,
//! and a session that is gone answers `401`. No specification governs the
//! console's session: our own design, after the OWASP Session Management
//! Cheat Sheet the guide cites.

use axum::Extension;
use axum::response::{IntoResponse, Response};
use ferrohealth_ui::view::session::Left;
use http::header::CACHE_CONTROL;
use http::{HeaderMap, Method, StatusCode};

use crate::server::ConsoleState;
use crate::session::{COOKIE, Remaining};

/// What `remaining` says in whole seconds.
#[must_use]
pub fn left(remaining: Remaining) -> Left {
    let seconds =
        |duration: std::time::Duration| u32::try_from(duration.as_secs()).unwrap_or(u32::MAX);
    Left {
        idle: seconds(remaining.idle),
        absolute: seconds(remaining.absolute),
    }
}

/// Answers the session route: [`Left`] as JSON for a live session, which a
/// `POST` extends first, and `401` for a session that is gone.
pub async fn status(
    Extension(state): Extension<ConsoleState>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    let Some(id) = state.sessions().cookie_in(&headers, COOKIE) else {
        return gone();
    };
    match state.sessions().remaining(&id, method == Method::POST) {
        Ok(Some(remaining)) => {
            ([(CACHE_CONTROL, "no-store")], axum::Json(left(remaining))).into_response()
        }
        Ok(None) => gone(),
        Err(error) => {
            tracing::error!(%error, "the session store could not be read");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// The answer for a session that is gone: an `about:blank` problem of `401`
/// (RFC 9457 §4.2.1), never kept by a cache.
fn gone() -> Response {
    (
        [(CACHE_CONTROL, "no-store")],
        ferrohealth_server::problem::Problem::new(StatusCode::UNAUTHORIZED).with_detail(
            crate::locale::tr_in(
                crate::locale::catalog(),
                &ferrohealth_ui::i18n::Locale::english(),
                crate::locale::Msg::SessionEndedDetail,
            ),
        ),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::left;
    use crate::session::Remaining;

    #[test]
    fn what_is_left_is_said_in_whole_seconds() {
        let said = left(Remaining {
            idle: Duration::from_millis(840_900),
            absolute: Duration::from_mins(450),
        });
        assert_eq!((840, 27_000), (said.idle, said.absolute));
    }
}
