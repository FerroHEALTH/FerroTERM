// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's own form routes: a preference kept, every preference reset,
//! the navigation folded, and the FHIR version chosen for the session.
//!
//! Each is a plain form post (the version a plain link) that works with no
//! script: the server checks what was posted against what the
//! preference offers, keeps it in its cookie, and sends the browser back to
//! the page it came from, which it checks is one of the console's own. No
//! specification governs these routes: our own design.

use axum::Extension;
use axum::extract::{Form, Query};
use axum::response::{IntoResponse, Response};
use http::header::{CACHE_CONTROL, LOCATION, SET_COOKIE};
use http::{HeaderValue, StatusCode};
use serde::Deserialize;

use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr};
use crate::prefs::{PREFIX, SIDEBAR};
use crate::routes::BACK_PARAM;
use crate::routes::{SESSION_FHIR, return_path};
use crate::server::ConsoleState;

/// The redirect to `back`, when it is one of the console's own pages, else
/// to the dashboard, setting each of `cookies`.
fn redirect(back: &str, cookies: &[String]) -> Response {
    let page = return_path(back).unwrap_or("/");
    let Ok(location) = HeaderValue::from_str(page) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    let headers = response.headers_mut();
    headers.insert(LOCATION, location);
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    for cookie in cookies {
        match HeaderValue::from_str(cookie) {
            Ok(value) => {
                headers.append(SET_COOKIE, value);
            }
            Err(_invalid) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
    response
}

/// A plain-text refusal of a post, saying `why` in English, the console's
/// fallback, since a refused post carries no page to draw in its locale.
fn bad(why: Msg) -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(CACHE_CONTROL, "no-store")],
        tr(&ferrohealth_ui::i18n::Locale::english(), why),
    )
        .into_response()
}

/// Where a preference post returns: the page `back` names, and on Your
/// preferences the same page saying which preference it kept.
#[must_use]
pub(crate) fn preference_return(back: &str, key: &str) -> String {
    let page = return_path(back).unwrap_or("/");
    let (path, _query) = page.split_once('?').unwrap_or((page, ""));
    if path.starts_with("/preferences") {
        format!("{path}?saved={key}")
    } else {
        page.to_owned()
    }
}

/// Keeps the one preference a plain form post chose in its cookie, and
/// sends the browser back to the page it came from.
///
/// The post carries `back` and one preference under its cookie key; a
/// switch posts `off` before its `on`, and the last value wins.
pub(crate) async fn preference(
    Extension(state): Extension<ConsoleState>,
    Form(fields): Form<Vec<(String, String)>>,
) -> Response {
    let back = fields
        .iter()
        .rev()
        .find(|(name, _)| name == BACK_PARAM)
        .map_or("/", |(_, back)| back.as_str());
    let chosen: Vec<&(String, String)> = fields
        .iter()
        .filter(|(name, _)| name != BACK_PARAM)
        .collect();
    let Some((key, value)) = chosen.last() else {
        return bad(Msg::PostNoPreference);
    };
    if chosen.iter().any(|(other, _)| other != key) {
        return bad(Msg::PostOnePreference);
    }
    let Some(kept) = crate::prefs::kept(key, value, crate::locale::offered) else {
        return bad(Msg::PostNotOffered);
    };
    let cookie = state
        .sessions()
        .preference_cookie(&format!("{PREFIX}{key}"), &kept)
        .to_string();
    redirect(&preference_return(back, key), &[cookie])
}

/// What Reset to defaults posts.
#[derive(Debug, Deserialize)]
pub(crate) struct Reset {
    /// The page to return to.
    back: String,
}

/// Clears every preference cookie, the console's own among them, and sends
/// the browser back to the page it came from.
pub(crate) async fn reset(
    Extension(state): Extension<ConsoleState>,
    Form(reset): Form<Reset>,
) -> Response {
    let cleared: Vec<String> = crate::prefs::every_key()
        .into_iter()
        .chain([SESSION_FHIR])
        .map(|key| {
            state
                .sessions()
                .preference_cookie_removal(&format!("{PREFIX}{key}"))
                .to_string()
        })
        .collect();
    redirect(&reset.back, &cleared)
}

/// What the navigation's Collapse or Expand posts.
#[derive(Debug, Deserialize)]
pub(crate) struct SidebarChoice {
    /// `rail` to fold the navigation, `full` to unfold it.
    sidebar: String,
    /// The page to return to.
    back: String,
}

/// Keeps the navigation's state in its cookie and sends the browser back to
/// the page it came from.
pub(crate) async fn sidebar(
    Extension(state): Extension<ConsoleState>,
    Form(choice): Form<SidebarChoice>,
) -> Response {
    if !matches!(choice.sidebar.as_str(), "rail" | "full") {
        return bad(Msg::PostSidebar);
    }
    let cookie = state
        .sessions()
        .preference_cookie(&format!("{PREFIX}{SIDEBAR}"), &choice.sidebar)
        .to_string();
    redirect(&choice.back, &[cookie])
}

/// What the FHIR version select's link carries.
#[derive(Debug, Deserialize)]
pub(crate) struct VersionChoice {
    /// The version chosen, as its path segment.
    fhir: String,
    /// The page to return to, already naming the version.
    back: String,
}

/// Keeps the FHIR version the top bar chose for the browser's session, so a
/// page whose address names none reads through it, and returns to the page.
pub(crate) async fn fhir_version(
    Extension(state): Extension<ConsoleState>,
    Query(choice): Query<VersionChoice>,
) -> Response {
    let Ok(version) = choice.fhir.parse::<FhirVersion>() else {
        return bad(Msg::PostVersion);
    };
    let cookie = state
        .sessions()
        .session_preference_cookie(&format!("{PREFIX}{SESSION_FHIR}"), version.segment())
        .to_string();
    redirect(&choice.back, &[cookie])
}

#[cfg(test)]
mod tests {
    use super::{preference_return, return_path};

    #[test]
    fn a_return_path_is_one_of_the_consoles_own_pages_and_nothing_else() {
        for path in [
            "/",
            "/about",
            "/expand?fhir=r5&url=http%3A%2F%2Fx",
            "/systems/a%2Fb",
        ] {
            assert_eq!(Some(path), return_path(path), "{path}");
        }
        for path in [
            "",
            "about",
            "//attacker.example.net/",
            "https://attacker.example.net/about",
            "/../about",
            "/\\attacker.example.net",
            "/a b",
            "/about#x",
        ] {
            assert_eq!(None, return_path(path), "{path}");
        }
    }

    #[test]
    fn a_preference_post_returns_saying_what_it_kept_only_on_your_preferences() {
        assert_eq!(
            "/preferences/reading?saved=page-size",
            preference_return("/preferences/reading?saved=fhir", "page-size")
        );
        assert_eq!("/systems?q=a", preference_return("/systems?q=a", "theme"));
        assert_eq!(
            "/",
            preference_return("//evil.example/preferences", "theme")
        );
    }
}
