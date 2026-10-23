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
    use leptos::prelude::{Owner, provide_context};
    use secrecy::SecretString;

    use super::{Request, param_in, params_in, request};
    use crate::auth::scopes::Letter;
    use crate::config::{Config, Oidc};
    use crate::fhir::version::FhirVersion;
    use crate::server::ConsoleState;
    use crate::session::SignedIn;

    /// The console's state, reached at `https://tx.example.org/`, with a
    /// sign-in configured when `sign_in`.
    fn state(sign_in: bool) -> ConsoleState {
        let mut config = Config::default();
        config.ferroterm.public_url = Some(String::from("https://tx.example.org/"));
        if sign_in {
            config.oidc = Some(Oidc {
                issuer: String::from("https://idp.example"),
                authorization_endpoint: String::from("https://idp.example/authorize"),
                token_endpoint: String::from("https://idp.example/token"),
                jwks_uri: String::from("https://idp.example/jwks"),
                client_id: String::from("ferroterm-console"),
                redirect_uri: String::from("http://127.0.0.1:3000/auth/callback"),
                ..Oidc::default()
            });
        }
        ConsoleState::new(config.resolve().expect("the settings resolve")).expect("the clients")
    }

    /// The request the server reads for `uri` carrying `cookies`, under
    /// `state`.
    fn read(state: Option<&ConsoleState>, uri: &str, cookies: &[&str]) -> Request {
        Owner::new().with(|| {
            if let Some(state) = state {
                provide_context(state.clone());
            }
            let mut builder = http::Request::get(uri);
            for cookie in cookies {
                builder = builder.header(http::header::COOKIE, *cookie);
            }
            let (parts, ()) = builder.body(()).expect("a request").into_parts();
            provide_context(parts);
            Request::read()
        })
    }

    #[test]
    fn the_page_is_read_from_the_address_and_returns_to_itself() {
        let state = state(false);
        let page = read(Some(&state), "/systems?q=%20colours%20&q=other", &[]);
        assert_eq!("/systems", page.path);
        assert_eq!("q=%20colours%20&q=other", page.query);
        assert_eq!("/systems?q=%20colours%20&q=other", page.back());
        assert_eq!(Some(String::from(" colours ")), page.param("q"));
        assert_eq!("colours", page.text("q"), "the first value, trimmed");
        assert_eq!("", page.text("absent"));
        assert_eq!("/about", read(Some(&state), "/about", &[]).back());
    }

    #[test]
    fn the_address_names_the_fhir_version_before_any_preference() {
        let state = state(false);
        let page = read(
            Some(&state),
            "/systems?fhir=r5",
            &["__Host-ferroterm_console_session-fhir=r4b; __Host-ferroterm_console_fhir=r6"],
        );
        assert_eq!(FhirVersion::R5, page.version);
        assert!(page.version_named);
    }

    #[test]
    fn the_session_s_version_wins_over_the_reader_s_default() {
        let state = state(false);
        let page = read(
            Some(&state),
            "/systems",
            &[
                "__Host-ferroterm_console_fhir=r6",
                "__Host-ferroterm_console_session-fhir=r4b",
            ],
        );
        assert_eq!(FhirVersion::R4B, page.version);
        assert!(!page.version_named);
        let default_only = read(
            Some(&state),
            "/systems",
            &["__Host-ferroterm_console_fhir=r6"],
        );
        assert_eq!(FhirVersion::R6, default_only.version);
    }

    #[test]
    fn a_preference_cookie_without_the_host_prefix_is_not_read_from_a_secure_console() {
        let state = state(false);
        let page = read(Some(&state), "/systems", &["ferroterm_console_fhir=r6"]);
        assert_eq!(
            read(Some(&state), "/systems", &[]).version,
            page.version,
            "a cookie a sibling host could set is ignored"
        );
    }

    #[test]
    fn the_root_and_the_client_are_the_address_readers_reach_the_server_at() {
        let state = state(false);
        let page = read(Some(&state), "/", &[]);
        assert_eq!("https://tx.example.org", page.root);
        assert_eq!(
            "https://tx.example.org/r4/metadata",
            page.client.metadata_url(FhirVersion::R4)
        );
    }

    #[test]
    fn the_page_offers_a_sign_in_only_when_a_provider_is_configured() {
        assert!(read(Some(&state(true)), "/", &[]).sign_in);
        assert!(!read(Some(&state(false)), "/", &[]).sign_in);
    }

    #[test]
    fn a_signed_in_reader_is_named_and_may_do_what_the_token_grants() {
        let state = state(true);
        let id = state
            .sessions()
            .establish(SignedIn {
                access_token: SecretString::from("synthetic-access-token"),
                expires_in: None,
                refresh_token: None,
                id_token: None,
                operator: Some(String::from("Synthetic Operator")),
                scopes: vec![String::from("user/CodeSystem.cu")],
            })
            .expect("room");
        let cookie = format!("__Host-ferroterm_console_session={}", id.as_str());
        let page = read(Some(&state), "/", &[&cookie]);
        assert_eq!(Some(String::from("Synthetic Operator")), page.session.who());
        assert!(page.can("CodeSystem", Letter::Update));
        assert!(!page.can("ValueSet", Letter::Update));
        let visitor = read(Some(&state), "/", &[]);
        assert!(!visitor.session.signed_in());
        assert!(!visitor.can("CodeSystem", Letter::Update));
    }

    #[test]
    fn a_page_drawn_with_no_server_context_reads_the_defaults() {
        let page = Owner::new().with(Request::read);
        assert_eq!("/", page.path);
        assert_eq!("", page.query);
        assert_eq!("", page.root);
        assert!(!page.sign_in);
        assert!(!page.session.signed_in());
    }

    #[test]
    fn the_page_tree_reads_the_request_its_root_provided() {
        let state = state(false);
        let provided = read(Some(&state), "/expand?fhir=r4", &[]);
        let path = Owner::new().with(|| {
            provide_context(provided.clone());
            request().path
        });
        assert_eq!("/expand", path);
    }

    #[test]
    fn every_value_of_a_repeated_parameter_is_read_in_order() {
        assert_eq!(
            vec![String::from("a b"), String::from("c")],
            params_in("system=a+b&other=x&system=c", "system")
        );
        assert_eq!(Vec::<String>::new(), params_in("other=x", "system"));
    }

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
