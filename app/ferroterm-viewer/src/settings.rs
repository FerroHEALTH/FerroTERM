// SPDX-License-Identifier: BUSL-1.1
//! The reader's preferences, and the server functions that keep them.
//!
//! Each preference lives in a cookie of the console's own, which the server
//! reads while it renders: the theme and the density are written on `<html>`,
//! so the page arrives drawn in them before any script runs, and the browser
//! keeps no storage of its own (the Leptos book, `ssr/22_life_cycle`). A
//! change is a plain form post the server answers with a redirect back to the
//! page, so it works before the page hydrates. No specification governs
//! this: our own design.

use ferrohealth_ui::theme::{Density, Theme};
use leptos::prelude::*;
use leptos::server_fn::codec::JsonEncoding;
use leptos::server_fn::error::{FromServerFnError, ServerFnErrorErr};
use serde::{Deserialize, Serialize};

use crate::fhir::version::FhirVersion;
use crate::paging::MAX_COUNT;

/// The page size a reader who set none reads at.
pub(crate) const DEFAULT_PAGE_SIZE: u32 = 50;

/// What the reader chose, as the request's cookies say.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// The theme, written as `data-theme`.
    #[serde(with = "theme_key")]
    pub theme: Theme,
    /// The density, written as `data-density`.
    #[serde(with = "density_key")]
    pub density: Density,
    /// The FHIR version an address that names none reads.
    pub version: FhirVersion,
    /// The BCP 47 tag sent as `displayLanguage`, empty for the server's own.
    pub language: String,
    /// How many rows a paged screen asks for.
    pub page_size: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            density: Density::default(),
            version: FhirVersion::default(),
            language: String::new(),
            page_size: DEFAULT_PAGE_SIZE,
        }
    }
}

/// A [`Theme`] as its key (serde, `with`).
mod theme_key {
    use ferrohealth_ui::theme::Theme;
    use serde::{Deserialize as _, Deserializer, Serializer};

    /// Writes the theme's key.
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's `with` calls the function with a reference to the field"
    )]
    pub(super) fn serialize<S: Serializer>(theme: &Theme, out: S) -> Result<S::Ok, S::Error> {
        out.serialize_str(theme.key())
    }

    /// Reads a theme from its key.
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<Theme, D::Error> {
        let key = String::deserialize(input)?;
        Theme::parse(&key).ok_or_else(|| serde::de::Error::custom("no theme of the console"))
    }
}

/// A [`Density`] as its key (serde, `with`).
mod density_key {
    use ferrohealth_ui::theme::Density;
    use serde::{Deserialize as _, Deserializer, Serializer};

    /// Writes the density's key.
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's `with` calls the function with a reference to the field"
    )]
    pub(super) fn serialize<S: Serializer>(density: &Density, out: S) -> Result<S::Ok, S::Error> {
        out.serialize_str(density.key())
    }

    /// Reads a density from its key.
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<Density, D::Error> {
        let key = String::deserialize(input)?;
        Density::parse(&key).ok_or_else(|| serde::de::Error::custom("no density of the console"))
    }
}

/// The page size `text` names, when it is a whole number from 1 to
/// [`MAX_COUNT`].
pub(crate) fn parse_page_size(text: &str) -> Option<u32> {
    let size = text.trim().parse::<u32>().ok()?;
    (1..=MAX_COUNT).contains(&size).then_some(size)
}

/// Why a preference was not kept.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum PreferenceError {
    /// A value names no choice of its field.
    #[error("{0} is not one of the choices of its field")]
    Invalid(String),
    /// The console could not answer the post.
    #[error("the console could not keep the preference")]
    Unavailable,
    /// The post could not reach the console's server.
    #[error("the console could not be reached: {0}")]
    Fetch(String),
}

impl FromServerFnError for PreferenceError {
    type Encoder = JsonEncoding;

    fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
        Self::Fetch(value.to_string())
    }
}

/// Keeps the theme `theme` and sends a plain form post back to `back`.
///
/// # Errors
/// Returns [`PreferenceError::Invalid`] for a key that names no theme and
/// [`PreferenceError::Unavailable`] when the console cannot set the cookie.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        clippy::unused_async,
        reason = "the server_fn #[server] macro copies this onto the items it writes, of which only the server body sets a cookie and awaits nothing (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
#[server(endpoint = "theme")]
pub async fn set_theme(
    /// The key of the theme chosen.
    theme: String,
    /// The page a plain form post returns to.
    back: Option<String>,
) -> Result<(), PreferenceError> {
    let chosen = Theme::parse(&theme).ok_or(PreferenceError::Invalid(theme))?;
    server::keep(
        &[(crate::session::THEME_COOKIE, chosen.key().to_owned())],
        back.as_deref(),
    )
}

/// Keeps the FHIR version `fhir` as the one an address that names none
/// reads, and sends a plain form post back to `back` with the version it
/// names replaced, so the address stays one a reader can share.
///
/// # Errors
/// Returns [`PreferenceError::Invalid`] for a name that is no FHIR version
/// this server serves and [`PreferenceError::Unavailable`] when the console
/// cannot set the cookie.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        clippy::unused_async,
        reason = "the server_fn #[server] macro copies this onto the items it writes, of which only the server body sets a cookie and awaits nothing (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
#[server(endpoint = "version")]
pub async fn set_version(
    /// The FHIR version chosen, as its path segment.
    fhir: String,
    /// The page a plain form post returns to.
    back: Option<String>,
) -> Result<(), PreferenceError> {
    let chosen = fhir
        .parse::<FhirVersion>()
        .map_err(|_unknown| PreferenceError::Invalid(fhir.clone()))?;
    let back = back.map(|page| {
        let (path, search) = page.split_once('?').unwrap_or((page.as_str(), ""));
        crate::routes::version_link(path, search, chosen)
    });
    server::keep(
        &[(crate::session::VERSION_COOKIE, chosen.segment().to_owned())],
        back.as_deref(),
    )
}

/// Keeps the preferences of the settings form and sends the post back to
/// `back`.
///
/// # Errors
/// Returns [`PreferenceError::Invalid`] for a value that names no choice of
/// its field and [`PreferenceError::Unavailable`] when the console cannot set
/// the cookies.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        clippy::unused_async,
        reason = "the server_fn #[server] macro copies this onto the items it writes, of which only the server body sets a cookie and awaits nothing (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
#[server(endpoint = "settings")]
pub async fn save_settings(
    /// The key of the theme chosen.
    theme: String,
    /// The key of the density chosen.
    density: String,
    /// The FHIR version an address that names none reads.
    fhir: String,
    /// The display language, empty for the server's own.
    language: String,
    /// The page size, as typed.
    page_size: String,
    /// The page a plain form post returns to.
    back: Option<String>,
) -> Result<(), PreferenceError> {
    let theme = Theme::parse(&theme).ok_or(PreferenceError::Invalid(theme))?;
    let density = Density::parse(&density).ok_or(PreferenceError::Invalid(density))?;
    let version = fhir
        .parse::<FhirVersion>()
        .map_err(|_unknown| PreferenceError::Invalid(fhir.clone()))?;
    let size = parse_page_size(&page_size).ok_or(PreferenceError::Invalid(page_size))?;
    let language = language.trim().to_owned();
    // NOTE: RFC 5646 §2.1: a language tag is ASCII letters, digits and hyphens,
    // so anything else is no tag and never reaches a cookie.
    if !language
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-')
        || language.len() > 64
    {
        return Err(PreferenceError::Invalid(String::from(
            "the display language",
        )));
    }
    server::keep(
        &[
            (crate::session::THEME_COOKIE, theme.key().to_owned()),
            (crate::session::DENSITY_COOKIE, density.key().to_owned()),
            (crate::session::VERSION_COOKIE, version.segment().to_owned()),
            (crate::session::LANGUAGE_COOKIE, language),
            (crate::session::PAGE_SIZE_COOKIE, size.to_string()),
        ],
        back.as_deref(),
    )
}

/// The page `path` names when it is one of the console's own pages, and
/// `None` for anything else, so a post never redirects off the console.
///
/// A page is an absolute path of visible ASCII with a query of its own,
/// never `//`, a backslash or a dot segment.
#[must_use]
pub fn return_path(path: &str) -> Option<&str> {
    let (route, _query) = path.split_once('?').unwrap_or((path, ""));
    let plain = route.starts_with('/')
        && !route.starts_with("//")
        && path
            .chars()
            .all(|c| c.is_ascii_graphic() && c != '\\' && c != '#')
        && !route
            .split('/')
            .any(|segment| segment == "." || segment == "..");
    plain.then_some(path)
}

/// The server half: the cookies the preferences are kept in, and the
/// preferences a request carries.
#[cfg(not(target_arch = "wasm32"))]
pub mod server {
    use ferrohealth_ui::theme::{Density, Theme};
    use http::HeaderValue;
    use http::header::SET_COOKIE;
    use leptos::context::use_context;

    use crate::fhir::version::FhirVersion;
    use crate::server::ViewerState;
    use crate::session::{
        DENSITY_COOKIE, LANGUAGE_COOKIE, PAGE_SIZE_COOKIE, THEME_COOKIE, VERSION_COOKIE,
    };
    use crate::settings::{PreferenceError, Settings, parse_page_size};

    /// Sets each cookie of `kept` on the answer, and sends a plain form post
    /// back to `back` when it is one of the console's own pages.
    ///
    /// # Errors
    /// Returns [`PreferenceError::Unavailable`] when the console cannot set a
    /// cookie.
    pub fn keep(kept: &[(&str, String)], back: Option<&str>) -> Result<(), PreferenceError> {
        let state = use_context::<ViewerState>().ok_or(PreferenceError::Unavailable)?;
        let response =
            use_context::<leptos_axum::ResponseOptions>().ok_or(PreferenceError::Unavailable)?;
        for (name, value) in kept {
            let cookie = state.sessions().preference_cookie(name, value).to_string();
            let value =
                HeaderValue::from_str(&cookie).map_err(|_invalid| PreferenceError::Unavailable)?;
            response.append_header(SET_COOKIE, value);
        }
        // NOTE: no specification governs this: our own design; only a plain form
        // post, which asks for HTML, is redirected, and only to the console's own page.
        let plain = use_context::<http::request::Parts>().is_some_and(|parts| {
            parts
                .headers
                .get(http::header::ACCEPT)
                .and_then(|accept| accept.to_str().ok())
                .is_some_and(|accept| accept.contains("text/html"))
        });
        let page = back
            .and_then(crate::settings::return_path)
            .and_then(|page| HeaderValue::from_str(page).ok());
        if plain && let Some(page) = page {
            response.insert_header(http::header::LOCATION, page);
            response.set_status(http::StatusCode::SEE_OTHER);
        }
        Ok(())
    }

    /// The preferences the request's cookies name, each that names nothing
    /// usable at its default.
    #[must_use]
    pub fn requested() -> Settings {
        let defaults = Settings::default();
        let Some(state) = use_context::<ViewerState>() else {
            return defaults;
        };
        let Some(parts) = use_context::<http::request::Parts>() else {
            return defaults;
        };
        let cookie = |name: &str| {
            state
                .sessions()
                .cookie_in(&parts.headers, name)
                .map(|value| value.as_str().to_owned())
        };
        // NOTE: no specification governs this: our own design; a cookie that
        // names no choice is the choice not made, as a missing one is.
        Settings {
            theme: cookie(THEME_COOKIE)
                .and_then(|key| Theme::parse(&key))
                .unwrap_or(defaults.theme),
            density: cookie(DENSITY_COOKIE)
                .and_then(|key| Density::parse(&key))
                .unwrap_or(defaults.density),
            version: cookie(VERSION_COOKIE)
                .and_then(|key| key.parse::<FhirVersion>().ok())
                .unwrap_or(defaults.version),
            language: cookie(LANGUAGE_COOKIE).unwrap_or(defaults.language),
            page_size: cookie(PAGE_SIZE_COOKIE)
                .and_then(|text| parse_page_size(&text))
                .unwrap_or(defaults.page_size),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_page_size_is_read() {
        assert_eq!(
            parse_page_size(" 25 "),
            Some(25),
            "surrounding space is trimmed"
        );
    }

    #[test]
    fn a_page_size_outside_the_bounds_is_refused_rather_than_clamped() {
        assert_eq!(parse_page_size("0"), None, "a zero-row page never advances");
        assert_eq!(
            parse_page_size("1001"),
            None,
            "the reader is told the value was not taken"
        );
    }

    #[test]
    fn text_that_is_not_a_number_names_no_page_size() {
        assert_eq!(parse_page_size("twenty"), None);
        assert_eq!(
            parse_page_size("-5"),
            None,
            "a negative count is not a size"
        );
        assert_eq!(parse_page_size(""), None);
    }

    #[test]
    fn the_bounds_themselves_are_accepted() {
        assert_eq!(parse_page_size("1"), Some(1));
        assert_eq!(parse_page_size("1000"), Some(MAX_COUNT));
    }

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
    fn the_settings_survive_the_trip_from_the_server_to_the_page() {
        let settings = Settings {
            theme: Theme::Dark,
            density: Density::Compact,
            version: FhirVersion::R5,
            language: String::from("nl-NL"),
            page_size: 25,
        };
        let carried = serde_json::to_string(&settings).expect("serializes");
        assert_eq!(
            settings,
            serde_json::from_str::<Settings>(&carried).expect("reads back")
        );
    }
}
