// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's HTTP service: the health route, the sign-in routes, the
//! form routes, the pages and FerroUI's assets, under one set of browser
//! security headers.
//!
//! Every page is a handler that reads the terminology server and answers the
//! whole document (`page`). Every response carries the browser security
//! headers of `ferrohealth-server`'s `console` (`nosniff`, `DENY` framing, no
//! referrer, a same-origin opener, every document `no-store`) under the
//! policy of a server-rendered console (`ConsoleHeaders::server_rendered`):
//! scripts and styles from this origin alone, no nonce and no WebAssembly,
//! as FerroUI's server-drawn pages need. A request that is not a safe
//! method is taken only from the console's own pages. The sign-in routes and
//! the middleware that keeps a session fresh and hands every page what its
//! request holds are `ferrohealth-session`'s (`ferrohealth_session::routes`).
//! No specification governs the console's HTTP surface: our own design.

use std::sync::Arc;

use axum::Extension;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use ferrohealth_health::lifecycle::Lifecycle;
use ferrohealth_health::registry::Registry;
use ferrohealth_server::console::{self, ConsoleHeaders};
use ferrohealth_server::health::Health;
use ferrohealth_server::section::ServerSection;
use ferrohealth_server::serve::Drained;
use http::header::{CACHE_CONTROL, CONTENT_TYPE};
use http::{HeaderValue, Method, StatusCode};

use ferrohealth_session::cookies::CookieNames;
use ferrohealth_session::routes::{SignIn, keep_session};
use ferrohealth_session::sessions::Sessions;

use crate::auth::pages::ConsolePages;
use crate::config::settings::Settings;
use crate::screen::Page;
use crate::visit::Visit;

/// The path of the console's health summary route.
pub const HEALTH: &str = ferrohealth_server::health::SUMMARY;

/// The pages that take a form post to their own address: a restore, a run,
/// a reload or an editor's draft; a post to any other page is refused.
const POSTED: [&str; 7] = [
    crate::routes::HISTORY_ROUTE,
    crate::routes::SYNC_ROUTE,
    crate::routes::SERVER_ROUTE,
    "/valuesets/{id}",
    crate::routes::CODE_SYSTEM_EDITOR_ROUTE,
    crate::routes::COMPOSE_ROUTE,
    crate::routes::CONCEPT_MAP_EDITOR_ROUTE,
];

/// The brand's files, served from the binary: the favicon and the mark the
/// lockup draws on a light and on a dark ground.
const BRAND_FILES: [(&str, &[u8], &str); 4] = [
    (
        crate::app::FAVICON,
        include_bytes!("../public/favicon.svg"),
        "image/svg+xml",
    ),
    (
        "/favicon.ico",
        include_bytes!("../public/favicon.ico"),
        "image/x-icon",
    ),
    (
        crate::shell::MARK_LIGHT,
        include_bytes!("../public/mark-light.svg"),
        "image/svg+xml",
    ),
    (
        crate::shell::MARK_DARK,
        include_bytes!("../public/mark-dark.svg"),
        "image/svg+xml",
    ),
];

/// The base name of the console's cookies.
///
/// Its session cookie is `ferroterm_console_session`, its pending sign-in
/// cookie `ferroterm_console_sign_in` and a preference cookie
/// `ferroterm_console_<key>`, each under the `__Host-` prefix when `Secure`.
pub const COOKIES: &str = "ferroterm_console";

/// Why the console could not start.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// The HTTP client of the terminology server could not be built.
    #[error("the HTTP client of the console could not be built")]
    Client(#[source] reqwest::Error),
    /// The OpenID Provider's relying party could not be built, its metadata
    /// unread among the reasons.
    #[error("the sign-in at the OpenID Provider could not be set up")]
    Provider(#[source] Box<ferrohealth_session::config::ConfigError>),
}

/// What every request handler shares.
#[derive(Debug, Clone)]
pub struct ConsoleState {
    settings: Arc<Settings>,
    sign_in: SignIn<ConsolePages>,
    upstream: reqwest::Client,
    health: Health,
}

/// The client of the terminology server and the sync service for
/// `settings`, which `config check` builds with nothing else.
///
/// # Errors
/// Returns [`StartError::Client`] when the client cannot be built.
pub fn upstream_client(settings: &Settings) -> Result<reqwest::Client, StartError> {
    // NOTE: no specification governs this: our own design; a redirect from
    // the FHIR server is handed to the page as it came, never followed.
    reqwest::Client::builder()
        .timeout(settings.ferroterm.timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(StartError::Client)
}

impl ConsoleState {
    /// The state for `settings`, with an empty session store and, when
    /// `[oidc]` is set, the relying party at its provider, whose metadata is
    /// read here for every endpoint `[oidc]` leaves out.
    ///
    /// # Errors
    /// Returns [`StartError::Client`] when the terminology server's client
    /// cannot be built and [`StartError::Provider`] when the relying party
    /// cannot.
    pub async fn new(settings: Settings) -> Result<Self, StartError> {
        let upstream = upstream_client(&settings)?;
        let party = match &settings.sign_in.provider {
            Some(provider) => Some(
                provider
                    .relying_party()
                    .await
                    .map_err(|error| StartError::Provider(Box::new(error)))?,
            ),
            None => None,
        };
        let sessions = Sessions::new(
            settings.sign_in.store.clone(),
            CookieNames::for_console(COOKIES),
        );
        // The settings are resolved, the client and the relying party built,
        // so the console serves from here.
        let lifecycle = Lifecycle::default();
        lifecycle.booted();
        Ok(Self {
            settings: Arc::new(settings),
            sign_in: SignIn::new(sessions, party, ConsolePages),
            upstream,
            health: Health::new(Registry::default(), lifecycle),
        })
    }

    /// The indicators and the phase the health routes read; the phase is
    /// serving from construction and draining once a stop is requested.
    #[must_use]
    pub fn health(&self) -> &Health {
        &self.health
    }

    /// The phase of the console.
    #[must_use]
    pub fn lifecycle(&self) -> &Lifecycle {
        self.health.lifecycle()
    }

    /// The resolved configuration.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The sign-in: the session store and the relying party.
    #[must_use]
    pub fn sign_in(&self) -> &SignIn<ConsolePages> {
        &self.sign_in
    }

    /// The session store.
    #[must_use]
    pub fn sessions(&self) -> &Sessions {
        self.sign_in.sessions()
    }

    /// The name the cookie `base` takes on this console: under the
    /// `__Host-` prefix when the cookies are `Secure`.
    #[must_use]
    pub fn cookie_name(&self, base: &str) -> String {
        if self.settings.sign_in.store.secure_cookie() {
            format!("{}{base}", ferrohealth_session::cookies::HOST_PREFIX)
        } else {
            base.to_owned()
        }
    }

    /// The HTTP client of the terminology server and the sync service.
    #[must_use]
    pub fn upstream(&self) -> &reqwest::Client {
        &self.upstream
    }
}

/// Builds the console's whole HTTP service over `state`.
pub fn router(state: ConsoleState) -> axum::Router {
    let form_action = form_action(&state);
    let sign_in = state.sign_in().clone();
    let health = ferrohealth_server::health::routes(state.health().clone());
    let mut pages = axum::Router::new()
        .merge(health)
        .nest(
            ferrohealth_ui::serve::PREFIX,
            ferrohealth_ui::serve::axum_router::router(),
        )
        .route(
            crate::routes::PREFERENCE_ROUTE,
            post(crate::post::preference),
        )
        .route(crate::routes::RESET_ROUTE, post(crate::post::reset))
        .route(crate::routes::SIDEBAR_ROUTE, post(crate::post::sidebar))
        .route(
            crate::routes::FHIR_VERSION_ROUTE,
            get(crate::post::fhir_version),
        )
        .route(
            crate::pages::find::ANSWER_ROUTE,
            get(crate::pages::find::answer_route),
        );
    for (path, bytes, media) in BRAND_FILES {
        pages = pages.route(path, get(move || async move { brand_file(bytes, media) }));
    }
    for path in crate::screen::ROUTES {
        pages = if POSTED.contains(path) {
            pages.route(path, get(page).post(page))
        } else {
            pages.route(path, get(page))
        };
    }
    let headers = ConsoleHeaders::server_rendered(form_action);
    pages
        .fallback(page)
        .layer(axum::middleware::from_fn_with_state(
            sign_in.clone(),
            keep_session::<ConsolePages>,
        ))
        .merge(ferrohealth_session::routes::router(sign_in))
        .layer(axum::middleware::from_fn(same_origin_only))
        .layer(Extension(state))
        .layer(axum::middleware::from_fn(move |request, next| {
            console::headers(headers.clone(), request, next)
        }))
}

/// One of the brand's files, `bytes` of the media type `media`, revalidated
/// so a new release is fetched (RFC 9111 §5.2.2.4).
fn brand_file(bytes: &'static [u8], media: &'static str) -> Response {
    (
        [
            (CONTENT_TYPE, HeaderValue::from_static(media)),
            (CACHE_CONTROL, HeaderValue::from_static("no-cache")),
        ],
        bytes,
    )
        .into_response()
}

/// Draws the page the request names: a `GET` reads it, and a post to a page
/// that takes one draws it with the posted fields
/// ([`crate::posted::Posted`]), so the page sends the write and answers with
/// what it wrote or with the refusal over the reader's draft. An address that
/// names no page answers the not-found page.
async fn page(Extension(state): Extension<ConsoleState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let mut visit = Visit::read(&state, &parts);
    if parts.method == Method::POST {
        let Ok(bytes) = axum::body::to_bytes(body, crate::posted::LIMIT).await else {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                [(CACHE_CONTROL, "no-store")],
                crate::locale::tr(&visit.locale, crate::locale::Msg::ServerFormTooLarge),
            )
                .into_response();
        };
        visit.posted = Some(crate::posted::Posted::parse(&bytes));
    }
    let request = crate::request::Request::of(
        &visit,
        parts.uri.path().to_owned(),
        parts.uri.query().unwrap_or_default().to_owned(),
    );
    let page = Page::of(&request.path);
    let drawing = async {
        match page {
            Some(page) => crate::screen::draw(page, request.clone()).await,
            None => crate::pages::errors::not_found_page(&request),
        }
    };
    if page.is_some_and(|page| !page.framed()) {
        let content = drawing.await;
        return crate::app::answer(&request, content);
    }
    let (content, reached) = tokio::join!(drawing, crate::shell::reached(&request));
    if request.answering.said().redirect.is_some() {
        return crate::app::answer(&request, content);
    }
    let body = crate::shell::console_shell(&request, reached, content);
    crate::app::answer(&request, body)
}

/// The fetch metadata header that names where a request comes from.
const SEC_FETCH_SITE: &str = "sec-fetch-site";

/// Refuses every request that is not a safe method, a form post and a
/// sign-out among them, unless it comes from the console's own pages
/// ([`same_origin`]), before anything reads the session or asks the server.
///
/// The session cookie is `SameSite=Lax` as well, so a cross-site `POST`
/// carries no session in the first place; this check holds without it.
async fn same_origin_only(
    Extension(state): Extension<ConsoleState>,
    request: Request,
    next: Next,
) -> Response {
    if request.method().is_safe() {
        return next.run(request).await;
    }
    let origin = state.settings().sign_in.origin.as_deref();
    if same_origin(request.headers(), origin) {
        return next.run(request).await;
    }
    tracing::warn!(
        method = %request.method(),
        "a request that did not come from the console's own pages was refused"
    );
    (
        StatusCode::FORBIDDEN,
        [(CACHE_CONTROL, "no-store")],
        "the console takes this request only from its own pages",
    )
        .into_response()
}

/// Whether `headers` show a request sent from a page of the console at
/// `origin`, its own `scheme://host:port`.
///
/// `Sec-Fetch-Site` decides when the browser sends it, and only
/// `same-origin` passes. Without it `Origin` must be `origin`, and without
/// that the origin of `Referer` must be. A request that shows none of them,
/// or a console with no known origin, is refused, so an unsafe request is
/// never taken on trust (the W3C Fetch Metadata Request Headers; RFC 6454
/// §7).
///
/// The console answers with `Referrer-Policy: no-referrer`, under which a
/// browser sends a plain form post with `Origin: null` and no `Referer` (the
/// WHATWG Fetch standard, the request `Origin` header). The console's own
/// forms, the sign-out among them, therefore pass by `Sec-Fetch-Site`
/// alone, and a browser without fetch metadata is refused, which fails
/// closed. The `Origin` and `Referer` fallbacks serve a request a module
/// sends with `fetch`, such as the session watch's.
#[must_use]
pub fn same_origin(headers: &http::HeaderMap, origin: Option<&str>) -> bool {
    if let Some(site) = headers.get(SEC_FETCH_SITE) {
        return site.as_bytes() == b"same-origin";
    }
    let Some(origin) = origin else {
        return false;
    };
    if let Some(sent) = headers.get(http::header::ORIGIN) {
        return sent.as_bytes() == origin.as_bytes();
    }
    // NOTE: RFC 9110 §10.1.3: a `Referer` that is no URL names no origin, which
    // refuses the request as a missing one does.
    headers
        .get(http::header::REFERER)
        .and_then(|referer| referer.to_str().ok())
        .and_then(|referer| url::Url::parse(referer).ok())
        .is_some_and(|referer| referer.origin().ascii_serialization() == origin)
}

/// The sources of the policy's `form-action`: the console itself, and the
/// origin of the provider's end-session endpoint when it has one.
///
/// A browser holds every redirect a form submission follows to
/// `form-action` (Content Security Policy Level 3, `form-action`), and the
/// sign-out form is answered with a redirect to that endpoint (OpenID
/// Connect RP-Initiated Logout 1.0 §2), so without its origin the browser
/// blocks the sign-out on its way to the provider.
#[must_use]
pub fn form_action(state: &ConsoleState) -> String {
    let end_session = state
        .sign_in()
        .party()
        .and_then(|party| party.provider().end_session_endpoint())
        .map(url::Url::origin)
        .filter(url::Origin::is_tuple);
    match end_session {
        Some(origin) => format!("'self' {}", origin.ascii_serialization()),
        None => String::from("'self'"),
    }
}

/// Serves `router` on `listener` until `shutdown` resolves, then drains as
/// `ferrohealth-server` does: `lifecycle` turns draining, and the requests in
/// flight get the family's default shutdown bound.
///
/// # Errors
/// Returns the I/O error of a listener that fails.
pub async fn serve(
    listener: tokio::net::TcpListener,
    router: axum::Router,
    lifecycle: Lifecycle,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<Drained> {
    let section = ServerSection::default();
    ferrohealth_server::serve::serve(
        listener,
        router,
        lifecycle,
        shutdown,
        section.drain_delay(),
        section.shutdown_timeout(),
    )
    .await
}
