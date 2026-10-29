// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! What a page reads of its request: the address, the reader's
//! preferences, who is signed in and the console's own settings.
//!
//! A page is drawn from a [`Request`] built from the visit the server read
//! ([`crate::visit::Visit`]) and the address the router names, so the server
//! and the browser draw it from the same value. No specification governs
//! this: our own design.

use ferrohealth_ui::i18n::Locale;
use leptos::prelude::*;

use crate::auth::Session;
use crate::auth::scopes::Letter;
use crate::fhir::FhirClient;
use crate::fhir::version::FhirVersion;
use crate::posted::Posted;
use crate::prefs::Prefs;
use crate::routes::VERSION_PARAM;
use crate::visit::{Listeners, Visit};

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
    /// The issuer of that provider.
    pub(crate) issuer: Option<String>,
    /// The admin listeners the console reaches.
    pub(crate) listeners: Listeners,
    /// The client of the terminology server.
    pub(crate) client: FhirClient,
    /// The address readers reach the terminology server at.
    pub(crate) root: String,
    /// The locale the page is drawn in.
    pub(crate) locale: Locale,
    /// The locale the browser asks for.
    pub(crate) browser: Locale,
    /// The form the request posted, for a page drawn as a post's answer.
    pub(crate) posted: Option<Posted>,
}

impl Request {
    /// The request for the address `path` and `query`, as `visit` read it.
    #[must_use]
    pub(crate) fn of(visit: &Visit, path: String, query: String) -> Self {
        let prefs = Prefs::read(&visit.cookies, &visit.prefix);
        let named =
            param_in(&query, VERSION_PARAM).and_then(|text| text.parse::<FhirVersion>().ok());
        Self {
            locale: visit.locale(),
            browser: visit.browser(),
            version: named.or(prefs.session_fhir).unwrap_or(prefs.fhir),
            version_named: named.is_some(),
            prefs,
            path,
            query,
            session: Session::new(visit.operator.clone()),
            sign_in: visit.issuer.is_some(),
            issuer: visit.issuer.clone(),
            listeners: visit.listeners,
            client: FhirClient::new(visit.root.clone()),
            root: visit.root.clone(),
            posted: visit.posted.clone(),
        }
    }

    /// The same request for the address `path` and `query`, as a part of the
    /// shell drawn once reads the page in view.
    #[must_use]
    pub(crate) fn at(&self, path: String, query: String) -> Self {
        let named =
            param_in(&query, VERSION_PARAM).and_then(|text| text.parse::<FhirVersion>().ok());
        Self {
            version: named.or(self.prefs.session_fhir).unwrap_or(self.prefs.fhir),
            version_named: named.is_some(),
            path,
            query,
            ..self.clone()
        }
    }

    /// The request the server is drawing, read from its context: the
    /// console's state and the request's parts.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub(crate) fn read() -> Self {
        let (path, query) = use_context::<http::request::Parts>().map_or_else(
            || (String::from("/"), String::new()),
            |parts| {
                (
                    parts.uri.path().to_owned(),
                    parts.uri.query().unwrap_or_default().to_owned(),
                )
            },
        );
        Self::of(&Visit::requested(), path, query)
    }

    /// The `index`th segment of the page's path, counted from the first after
    /// the root, when it has one.
    #[must_use]
    pub(crate) fn segment(&self, index: usize) -> Option<String> {
        self.path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .nth(index)
            .map(|segment| {
                percent_encoding::percent_decode_str(segment)
                    .decode_utf8_lossy()
                    .into_owned()
            })
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

    /// Whether the console reaches a sync service and the reader's token
    /// carries its product scope, so the synchronisation screens are drawn.
    #[must_use]
    pub(crate) fn reads_sync(&self) -> bool {
        self.listeners.sync && self.session.holds(crate::auth::scopes::SYNC)
    }

    /// Whether the console reaches the server's admin listener and the
    /// reader's token carries its scope, so Reload the served set is offered.
    #[must_use]
    pub(crate) fn reloads(&self) -> bool {
        self.listeners.admin && self.session.holds(crate::auth::scopes::ADMIN)
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

/// The request the page being drawn was provided with, or the server's own
/// request outside a page.
#[must_use]
pub(crate) fn request() -> Request {
    if let Some(request) = use_context::<Request>() {
        return request;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Request::read()
    }
    #[cfg(target_arch = "wasm32")]
    {
        Request::of(
            &crate::visit::visit_untracked(),
            String::from("/"),
            String::new(),
        )
    }
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
