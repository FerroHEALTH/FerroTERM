// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the server read of the request for the page: the reader's
//! preferences, who is signed in, the console's own settings a page names,
//! the locale, a posted form and the way to the terminology server.
//!
//! A handler reads it once from the request and draws the whole page from
//! it, so no screen reads a cookie, the session or the request itself
//! (FerroUI `docs/consoles.md`, "How a console builds"). It keeps the
//! preference cookies apart from the session cookie, so a screen sees what
//! the reader chose and never the session id. No specification governs
//! this: our own design.

use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::view::session::Left;

use crate::auth::Operator;
use crate::posted::Posted;
use crate::server::ConsoleState;
use crate::upstream::Upstream;

/// The admin listeners the console is configured to reach.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Listeners {
    /// The sync service's admin listener.
    pub(crate) sync: bool,
    /// The terminology server's admin listener.
    pub(crate) admin: bool,
}

/// What the server read of the request for the page.
#[derive(Clone, Debug)]
pub(crate) struct Visit {
    /// The request's preference cookies alone, as one `Cookie` header value.
    pub(crate) cookies: String,
    /// The prefix each preference cookie's name takes.
    pub(crate) prefix: String,
    /// Who is signed in, if anyone.
    pub(crate) operator: Option<Operator>,
    /// What the reader's session had left as the server read the request.
    pub(crate) left: Option<Left>,
    /// The issuer of the OpenID Provider the console signs readers in at,
    /// when one is configured.
    pub(crate) issuer: Option<String>,
    /// The admin listeners the console reaches.
    pub(crate) listeners: Listeners,
    /// The address readers reach the terminology server at.
    pub(crate) root: String,
    /// The locale the page is drawn in.
    pub(crate) locale: Locale,
    /// The locale the browser asks for, which Your preferences names as the
    /// browser's own.
    pub(crate) browser: Locale,
    /// The form the request posted, for a page drawn as a post's answer.
    pub(crate) posted: Option<Posted>,
    /// The console's state, which the Configuration page reads its running
    /// settings from; none outside a server.
    pub(crate) state: Option<ConsoleState>,
    /// The way to the terminology server as the request's reader; none
    /// outside a server.
    pub(crate) upstream: Option<Upstream>,
}

impl Default for Visit {
    fn default() -> Self {
        Self {
            cookies: String::new(),
            prefix: String::from(crate::prefs::PREFIX),
            operator: None,
            left: None,
            issuer: None,
            listeners: Listeners::default(),
            root: String::new(),
            locale: Locale::english(),
            browser: Locale::english(),
            posted: None,
            state: None,
            upstream: None,
        }
    }
}

impl Visit {
    /// What the server reads of the request whose head is `parts`, served by
    /// `state`.
    #[must_use]
    pub(crate) fn read(state: &ConsoleState, parts: &http::request::Parts) -> Self {
        let prefix = state.sessions().cookie_name(crate::prefs::PREFIX);
        let cookies = preference_cookies(&parts.headers, &prefix);
        let prefs = crate::prefs::Prefs::read(&cookies, &prefix);
        let settings = state.settings();
        let header = parts
            .headers
            .get(http::header::ACCEPT_LANGUAGE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let browser = crate::locale::requested(header, settings.locale.clone());
        Self {
            locale: crate::locale::chosen(&prefs, browser.clone()),
            browser,
            cookies,
            prefix,
            operator: crate::session::request::operator(state, &parts.headers),
            left: crate::session::request::left(state, &parts.headers),
            issuer: settings.oidc.as_ref().map(|oidc| oidc.issuer.clone()),
            listeners: Listeners {
                sync: settings.ferroterm.sync.is_some(),
                admin: settings.ferroterm.admin.is_some(),
            },
            root: settings.ferroterm.shown_root(),
            posted: None,
            state: Some(state.clone()),
            upstream: Some(Upstream::of(state, &parts.headers)),
        }
    }
}

/// The preference cookies of `headers`, each named `prefix` then the key of
/// a preference, joined as one `Cookie` header value; the session's cookie
/// shares the prefix and is never among them.
fn preference_cookies(headers: &http::HeaderMap, prefix: &str) -> String {
    let names: Vec<String> = crate::prefs::every_key()
        .into_iter()
        .chain([crate::prefs::SIDEBAR, crate::routes::SESSION_FHIR])
        .map(|key| format!("{prefix}{key}"))
        .collect();
    // NOTE: RFC 9113 §8.2.3: a request may split its cookies over several
    // headers, which read as one list joined by `; `.
    headers
        .get_all(http::header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .map(str::trim)
        .filter(|pair| {
            pair.split_once('=')
                .is_some_and(|(name, _value)| names.iter().any(|kept| kept == name))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::preference_cookies;

    #[test]
    fn only_the_preference_cookies_cross_and_never_the_session_s() {
        let mut headers = http::HeaderMap::new();
        headers.append(
            http::header::COOKIE,
            http::HeaderValue::from_static(
                "__Host-ferroterm_console_session=secret; __Host-ferroterm_console_theme=dark",
            ),
        );
        headers.append(
            http::header::COOKIE,
            http::HeaderValue::from_static(
                "__Host-ferroterm_console_session-fhir=r5; other=x; __Host-ferroterm_console_sidebar=rail",
            ),
        );
        let kept = preference_cookies(&headers, "__Host-ferroterm_console_");
        assert_eq!(
            "__Host-ferroterm_console_theme=dark; __Host-ferroterm_console_session-fhir=r5; __Host-ferroterm_console_sidebar=rail",
            kept
        );
    }
}
