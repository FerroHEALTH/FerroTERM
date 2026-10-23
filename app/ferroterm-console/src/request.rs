// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! What the server knows of the request it draws a page for, read once at
//! the root of the page tree and provided to every screen.
//!
//! The pages are drawn on the server alone (islands mode), so a screen reads
//! the address, the reader's preferences and the session straight from this
//! value: nothing crosses to the browser but the islands' props. No
//! specification governs this: our own design.

use ferrohealth_ui::i18n::Locale;
use leptos::prelude::*;

use crate::auth::scopes::Letter;
use crate::auth::{Operator, Session};
use crate::fhir::FhirClient;
use crate::fhir::version::FhirVersion;
use crate::prefs::{PREFIX, Prefs};
use crate::routes::VERSION_PARAM;
use crate::server::ConsoleState;

/// The request a page is drawn for.
#[derive(Clone, Debug)]
pub(crate) struct Request {
    /// The path of the page, without its query.
    pub(crate) path: String,
    /// The query of the address, without its `?`, as the browser sent it.
    pub(crate) query: String,
    /// The reader's preferences.
    pub(crate) prefs: Prefs,
    /// The FHIR version the page reads through: the address's `fhir`, else
    /// the reader's default.
    pub(crate) version: FhirVersion,
    /// Whether the address names its FHIR version itself.
    pub(crate) version_named: bool,
    /// Who is signed in, if anyone.
    pub(crate) session: Session,
    /// Whether the console offers a sign-in: an OpenID Provider is
    /// configured.
    pub(crate) sign_in: bool,
    /// The client of the terminology server.
    pub(crate) client: FhirClient,
    /// The address readers reach the terminology server at.
    pub(crate) root: String,
    /// The locale the page is drawn in.
    pub(crate) locale: Locale,
}

impl Request {
    /// The request the server is drawing, read from its context: the
    /// console's state and the request's parts.
    #[must_use]
    pub(crate) fn read() -> Self {
        let state = use_context::<ConsoleState>();
        let parts = use_context::<http::request::Parts>();
        let cookies = parts
            .as_ref()
            .map(|parts| {
                parts
                    .headers
                    .get_all(http::header::COOKIE)
                    .iter()
                    .filter_map(|value| value.to_str().ok())
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default();
        let prefix = state.as_ref().map_or_else(
            || String::from(PREFIX),
            |state| state.sessions().cookie_name(PREFIX),
        );
        let prefs = Prefs::read(&cookies, &prefix);
        let (path, query) = parts.as_ref().map_or_else(
            || (String::from("/"), String::new()),
            |parts| {
                (
                    parts.uri.path().to_owned(),
                    parts.uri.query().unwrap_or_default().to_owned(),
                )
            },
        );
        let named =
            param_in(&query, VERSION_PARAM).and_then(|text| text.parse::<FhirVersion>().ok());
        let operator: Option<Operator> = crate::session::request::operator();
        let root = state
            .as_ref()
            .map(|state| state.settings().ferroterm.shown_root())
            .unwrap_or_default();
        Self {
            locale: crate::locale::chosen(&prefs),
            version: named.or(prefs.session_fhir).unwrap_or(prefs.fhir),
            version_named: named.is_some(),
            prefs,
            path,
            query,
            session: Session::new(operator),
            sign_in: state
                .as_ref()
                .is_some_and(|state| state.settings().oidc.is_some()),
            client: FhirClient::new(root.clone()),
            root,
        }
    }

    /// The value of the address's parameter `name`, decoded, if it carries
    /// one.
    #[must_use]
    pub(crate) fn param(&self, name: &str) -> Option<String> {
        param_in(&self.query, name)
    }

    /// The value of the address's parameter `name`, trimmed, empty when the
    /// address carries none.
    #[must_use]
    pub(crate) fn text(&self, name: &str) -> String {
        self.param(name).unwrap_or_default().trim().to_owned()
    }

    /// The page as an address the console returns to after a post: its path
    /// and its query.
    #[must_use]
    pub(crate) fn back(&self) -> String {
        if self.query.is_empty() {
            self.path.clone()
        } else {
            format!("{}?{}", self.path, self.query)
        }
    }

    /// Whether the reader's token opens `letter` on `resource_type`.
    #[must_use]
    pub(crate) fn can(&self, resource_type: &str, letter: Letter) -> bool {
        self.session.can(resource_type, letter)
    }
}

/// The decoded value of the parameter `name` in the query `query`, the first
/// one when it repeats.
#[must_use]
pub(crate) fn param_in(query: &str, name: &str) -> Option<String> {
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

/// Every value of the parameter `name` in `query`, in the order it carries
/// them.
#[must_use]
pub(crate) fn params_in(query: &str, name: &str) -> Vec<String> {
    url::form_urlencoded::parse(query.as_bytes())
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .collect()
}

/// The request the page tree provides, read once at its root.
#[must_use]
pub(crate) fn request() -> Request {
    use_context::<Request>().unwrap_or_else(Request::read)
}

#[cfg(test)]
mod tests {
    use super::param_in;

    #[test]
    fn a_parameter_is_read_decoded_and_the_first_wins() {
        assert_eq!(
            Some(String::from("http://x/a b")),
            param_in("url=http%3A%2F%2Fx%2Fa+b&url=other", "url")
        );
        assert_eq!(None, param_in("fhir=r4", "url"));
        assert_eq!(Some(String::new()), param_in("flat", "flat"));
    }
}
