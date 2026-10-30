// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! What a page reads of its request: the address, the reader's
//! preferences, who is signed in and the console's own settings, and what
//! the page says of its answer: its title, its status, or a redirect.
//!
//! A handler builds a [`Request`] from the visit the server read
//! ([`crate::visit::Visit`]) and the address, and the screen draws the page
//! from it. No specification governs this: our own design.

use std::sync::{Arc, Mutex};

use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::view::session::Left;
use http::StatusCode;

use crate::auth::Session;
use crate::auth::scopes::Letter;
use crate::fhir::FhirClient;
use crate::fhir::version::FhirVersion;
use crate::posted::Posted;
use crate::prefs::Prefs;
use crate::routes::VERSION_PARAM;
use crate::server::ConsoleState;
use crate::visit::{Listeners, Visit};

/// What a screen says of the answer it is drawn into: the title the browser
/// tab shows, the status, and an address to send the reader to in place of
/// the page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Said {
    /// The page's own words in the document title.
    pub(crate) title: Option<String>,
    /// The answer's status, `200 OK` when the screen names none.
    pub(crate) status: Option<StatusCode>,
    /// Where the reader goes in place of the page, as a `303 See Other`.
    pub(crate) redirect: Option<String>,
}

/// Where a screen says what it says of its answer, shared by every clone of
/// the request it was drawn for.
#[derive(Clone, Debug, Default)]
pub(crate) struct Answering(Arc<Mutex<Said>>);

impl Answering {
    /// Changes what is said with `change`.
    fn say(&self, change: impl FnOnce(&mut Said)) {
        // NOTE: no specification governs this: our own design; a lock poisoned by a
        // panicking screen keeps what it held, and the panic answers the request.
        match self.0.lock() {
            Ok(mut said) => change(&mut said),
            Err(poisoned) => change(&mut poisoned.into_inner()),
        }
    }

    /// The page's title in the browser tab is `title`.
    pub(crate) fn title(&self, title: &str) {
        self.say(|said| said.title = Some(title.to_owned()));
    }

    /// The answer's status is `status`.
    pub(crate) fn status(&self, status: StatusCode) {
        self.say(|said| said.status = Some(status));
    }

    /// The reader goes to `to` in place of the page.
    pub(crate) fn redirect(&self, to: &str) {
        self.say(|said| said.redirect = Some(to.to_owned()));
    }

    /// What has been said.
    #[must_use]
    pub(crate) fn said(&self) -> Said {
        match self.0.lock() {
            Ok(said) => said.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

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
    /// What the reader's session had left as the server read the request.
    pub(crate) left: Option<Left>,
    /// The console's state; none outside a server.
    pub(crate) state: Option<ConsoleState>,
    /// What the screen says of its answer.
    pub(crate) answering: Answering,
}

impl Request {
    /// The request for the address `path` and `query`, as `visit` read it.
    #[must_use]
    pub(crate) fn of(visit: &Visit, path: String, query: String) -> Self {
        let prefs = Prefs::read(&visit.cookies, &visit.prefix);
        let named =
            param_in(&query, VERSION_PARAM).and_then(|text| text.parse::<FhirVersion>().ok());
        Self {
            locale: visit.locale.clone(),
            browser: visit.browser.clone(),
            version: named.or(prefs.session_fhir).unwrap_or(prefs.fhir),
            version_named: named.is_some(),
            prefs,
            path,
            query,
            session: Session::new(visit.operator.clone()),
            sign_in: visit.issuer.is_some(),
            issuer: visit.issuer.clone(),
            listeners: visit.listeners,
            client: match &visit.upstream {
                Some(upstream) => {
                    FhirClient::new(visit.root.clone()).with_upstream(upstream.clone())
                }
                None => FhirClient::new(visit.root.clone()),
            }
            .with_locale(visit.locale.clone()),
            root: visit.root.clone(),
            posted: visit.posted.clone(),
            left: visit.left,
            state: visit.state.clone(),
            answering: Answering::default(),
        }
    }

    /// The request the server reads of `parts`, served by `state`.
    #[must_use]
    pub(crate) fn read(state: &ConsoleState, parts: &http::request::Parts) -> Self {
        Self::of(
            &Visit::read(state, parts),
            parts.uri.path().to_owned(),
            parts.uri.query().unwrap_or_default().to_owned(),
        )
    }

    /// The console's message `msg` in the page's locale.
    #[must_use]
    pub(crate) fn tr(&self, msg: crate::locale::Msg) -> String {
        crate::locale::tr(&self.locale, msg)
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

#[cfg(test)]
mod tests {
    use secrecy::SecretString;

    use super::{Request, param_in, params_in};
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
    fn read(state: &ConsoleState, uri: &str, cookies: &[&str]) -> Request {
        let mut builder = http::Request::get(uri);
        for cookie in cookies {
            builder = builder.header(http::header::COOKIE, *cookie);
        }
        let (parts, ()) = builder.body(()).expect("a request").into_parts();
        Request::read(state, &parts)
    }

    #[test]
    fn the_page_is_read_from_the_address_and_returns_to_itself() {
        let state = state(false);
        let page = read(&state, "/systems?q=%20colours%20&q=other", &[]);
        assert_eq!("/systems", page.path);
        assert_eq!("q=%20colours%20&q=other", page.query);
        assert_eq!("/systems?q=%20colours%20&q=other", page.back());
        assert_eq!(Some(String::from(" colours ")), page.param("q"));
        assert_eq!("colours", page.text("q"), "the first value, trimmed");
        assert_eq!("", page.text("absent"));
        assert_eq!("/about", read(&state, "/about", &[]).back());
    }

    #[test]
    fn the_address_names_the_fhir_version_before_any_preference() {
        let state = state(false);
        let page = read(
            &state,
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
            &state,
            "/systems",
            &[
                "__Host-ferroterm_console_fhir=r6",
                "__Host-ferroterm_console_session-fhir=r4b",
            ],
        );
        assert_eq!(FhirVersion::R4B, page.version);
        assert!(!page.version_named);
        let default_only = read(&state, "/systems", &["__Host-ferroterm_console_fhir=r6"]);
        assert_eq!(FhirVersion::R6, default_only.version);
    }

    #[test]
    fn a_preference_cookie_without_the_host_prefix_is_not_read_from_a_secure_console() {
        let state = state(false);
        let page = read(&state, "/systems", &["ferroterm_console_fhir=r6"]);
        assert_eq!(
            read(&state, "/systems", &[]).version,
            page.version,
            "a cookie a sibling host could set is ignored"
        );
    }

    #[test]
    fn the_root_and_the_client_are_the_address_readers_reach_the_server_at() {
        let state = state(false);
        let page = read(&state, "/", &[]);
        assert_eq!("https://tx.example.org", page.root);
        assert_eq!(
            "https://tx.example.org/r4/metadata",
            page.client.metadata_url(FhirVersion::R4)
        );
    }

    #[test]
    fn the_page_offers_a_sign_in_only_when_a_provider_is_configured() {
        assert!(read(&state(true), "/", &[]).sign_in);
        assert!(!read(&state(false), "/", &[]).sign_in);
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
        let page = read(&state, "/", &[&cookie]);
        assert_eq!(Some(String::from("Synthetic Operator")), page.session.who());
        assert!(page.can("CodeSystem", Letter::Update));
        assert!(!page.can("ValueSet", Letter::Update));
        let visitor = read(&state, "/", &[]);
        assert!(!visitor.session.signed_in());
        assert!(!visitor.can("CodeSystem", Letter::Update));
    }

    #[test]
    fn a_page_drawn_with_no_server_reads_the_defaults() {
        let page = Request::of(
            &crate::visit::Visit::default(),
            String::from("/"),
            String::new(),
        );
        assert_eq!("/", page.path);
        assert_eq!("", page.query);
        assert_eq!("", page.root);
        assert!(!page.sign_in);
        assert!(!page.session.signed_in());
    }

    #[test]
    fn every_clone_of_a_request_says_into_one_answer() {
        let state = state(false);
        let page = read(&state, "/expand?fhir=r4", &[]);
        assert_eq!("/expand", page.path);
        let drawn = page.clone();
        drawn.answering.title("Expand");
        drawn.answering.status(http::StatusCode::NOT_FOUND);
        let said = page.answering.said();
        assert_eq!(Some(String::from("Expand")), said.title);
        assert_eq!(Some(http::StatusCode::NOT_FOUND), said.status);
        assert_eq!(None, said.redirect);
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
