// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the server read of the request for the page: the reader's
//! preferences, who is signed in, the console's own settings a page names,
//! the locale and a posted form.
//!
//! The server reads it as it draws a page, and it crosses to the browser
//! with what the page was drawn from, so both halves draw the same page and
//! no component reads a cookie, the session or the request on the server
//! alone (FerroUI `docs/consoles.md`, "How a console builds", step 4). It
//! carries the preference cookies and never the session cookie, so no
//! session id reaches a script. The shell asks for it again after each
//! navigation ([`FollowVisit`]). No specification governs this: our own
//! design.
#![expect(
    clippy::same_name_method,
    reason = "leptos::component derives a TypedBuilder whose `builder` shadows a trait method"
)]
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        clippy::unused_async_trait_impl,
        reason = "the server function macro writes the browser half's call as an async trait method; only that target has it"
    )
)]
#![cfg_attr(
    not(target_arch = "wasm32"),
    expect(
        clippy::unused_async,
        reason = "the visit's server function reads the request alone, with nothing to await"
    )
)]

use ferrohealth_ui::i18n::Locale;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::auth::Operator;
use crate::posted::Posted;

/// The admin listeners the console is configured to reach.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Listeners {
    /// The sync service's admin listener.
    pub(crate) sync: bool,
    /// The terminology server's admin listener.
    pub(crate) admin: bool,
}

/// What the server read of the request for the page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Visit {
    /// The request's preference cookies alone, as one `Cookie` header value.
    pub(crate) cookies: String,
    /// The prefix each preference cookie's name takes.
    pub(crate) prefix: String,
    /// Who is signed in, if anyone.
    pub(crate) operator: Option<Operator>,
    /// The issuer of the OpenID Provider the console signs readers in at,
    /// when one is configured.
    pub(crate) issuer: Option<String>,
    /// The admin listeners the console reaches.
    pub(crate) listeners: Listeners,
    /// The address readers reach the terminology server at.
    pub(crate) root: String,
    /// The tag of the locale the page is drawn in.
    pub(crate) locale: String,
    /// The tag of the locale the browser asks for, which Your preferences
    /// names as the browser's own.
    pub(crate) browser: String,
    /// The form the request posted, for a page drawn as a post's answer.
    pub(crate) posted: Option<Posted>,
}

impl Default for Visit {
    fn default() -> Self {
        Self {
            cookies: String::new(),
            prefix: String::from(crate::prefs::PREFIX),
            operator: None,
            issuer: None,
            listeners: Listeners::default(),
            root: String::new(),
            locale: Locale::english().tag(),
            browser: Locale::english().tag(),
            posted: None,
        }
    }
}

impl Visit {
    /// The page's locale, English for a tag no catalog names.
    #[must_use]
    pub(crate) fn locale(&self) -> Locale {
        // NOTE: no specification governs this: our own design; the server writes a
        // tag it parsed, so a tag that does not parse is English, the fallback.
        Locale::parse(&self.locale).unwrap_or_else(|_| Locale::english())
    }

    /// The locale the browser asks for, English for a tag no catalog names.
    #[must_use]
    pub(crate) fn browser(&self) -> Locale {
        Locale::parse(&self.browser).unwrap_or_else(|_| Locale::english())
    }

    /// What the server reads of the request being served.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub(crate) fn requested() -> Self {
        let state = use_context::<crate::server::ConsoleState>();
        let prefix = state.as_ref().map_or_else(
            || String::from(crate::prefs::PREFIX),
            |state| state.sessions().cookie_name(crate::prefs::PREFIX),
        );
        let cookies = use_context::<http::request::Parts>()
            .map(|parts| preference_cookies(&parts.headers, &prefix))
            .unwrap_or_default();
        let prefs = crate::prefs::Prefs::read(&cookies, &prefix);
        let settings = state.as_ref().map(crate::server::ConsoleState::settings);
        Self {
            locale: crate::locale::chosen(&prefs).tag(),
            browser: crate::locale::requested().tag(),
            cookies,
            prefix,
            operator: crate::session::request::operator(),
            issuer: settings
                .and_then(|settings| settings.oidc.as_ref())
                .map(|oidc| oidc.issuer.clone()),
            listeners: Listeners {
                sync: settings.is_some_and(|settings| settings.ferroterm.sync.is_some()),
                admin: settings.is_some_and(|settings| settings.ferroterm.admin.is_some()),
            },
            root: settings
                .map(|settings| settings.ferroterm.shown_root())
                .unwrap_or_default(),
            posted: use_context::<Posted>(),
        }
    }
}

/// The preference cookies of `headers`, each named `prefix` then the key of
/// a preference, joined as one `Cookie` header value; the session's cookie
/// shares the prefix and is never among them.
#[cfg(not(target_arch = "wasm32"))]
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

/// The visit the page knows, which the shell asks for again after each
/// navigation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Visits(pub(crate) RwSignal<Visit>);

/// The visit the page knows ([`Visits`]), or the request's own outside the
/// page tree, read without tracking it.
#[cfg(target_arch = "wasm32")]
#[must_use]
pub(crate) fn visit_untracked() -> Visit {
    use_context::<Visits>().map_or_else(server_visit, |visits| visits.0.get_untracked())
}

/// The visit the server reads of the request; in the browser, which reads
/// the value the server serialized, the default.
#[must_use]
pub(crate) fn server_visit() -> Visit {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Visit::requested()
    }
    #[cfg(target_arch = "wasm32")]
    {
        Visit::default()
    }
}

/// The visit of the request that asks, for the shell after a navigation.
///
/// # Errors
///
/// When the server function cannot be reached or answered.
#[server(endpoint = "visit")]
pub(crate) async fn visit_now() -> Result<Visit, ServerFnError> {
    Ok(Visit::requested())
}

/// Asks the server for the visit again after each navigation and each post
/// drawn in the page, so a preference a post just kept, or a sign-in that
/// ended, reaches the shell drawn in the browser.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type the crate never exports"
)]
pub(crate) fn FollowVisit() -> impl IntoView {
    let location = leptos_router::hooks::use_location();
    let visits = use_context::<Visits>();
    Effect::watch(
        move || {
            (
                location.pathname.get(),
                location.search.get(),
                ferrohealth_ui::widget::submit::posts().get(),
            )
        },
        move |_, _, _| {
            if let Some(Visits(visits)) = visits {
                leptos::task::spawn_local(async move {
                    match visit_now().await {
                        Ok(now) => {
                            follow_root(&now);
                            // NOTE: no specification governs this: our own design; the same
                            // visit redraws nothing, so a page keeps the focus it took.
                            if visits.get_untracked() != now {
                                visits.set(now);
                            }
                        }
                        Err(error) => leptos::logging::warn!("the visit did not come: {error}"),
                    }
                });
            }
        },
        false,
    );
}

/// Sets the root element's theme, density, motion, hints and language from
/// the visit `now`, as the server writes them on a document it draws, so a
/// preference a post just kept takes effect in the page.
fn follow_root(now: &Visit) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(root) = document().document_element() else {
            return;
        };
        let language = now.locale().tag();
        // NOTE: no specification governs this: our own design; the words of a page are
        // drawn in its locale, so a new language loads the page as a document.
        if root
            .get_attribute("lang")
            .is_some_and(|lang| lang != language)
        {
            if window().location().reload().is_err() {
                leptos::logging::warn!("the browser refused to load the page in its language");
            }
            return;
        }
        let family = crate::prefs::Prefs::read(&now.cookies, &now.prefix).family;
        let attributes = [
            ("data-theme", family.theme.key()),
            ("data-density", family.density.key()),
            ("data-motion", family.motion.key()),
            ("data-hints", family.hints.key()),
        ];
        for (name, value) in attributes {
            if root.get_attribute(name).as_deref() != Some(value)
                && root.set_attribute(name, value).is_err()
            {
                leptos::logging::warn!("the browser refused the root's {name}");
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _: &Visit = now;
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
