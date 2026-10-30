// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's HTTP service: the health route, the sign-in routes, the
//! form routes, the pages and FerroUI's assets, under one set of browser
//! security headers.
//!
//! Every page is a handler that reads the terminology server and answers the
//! whole document ([`page`]). Every response carries the browser security
//! headers of `ferrohealth-server`'s `console` (`nosniff`, `DENY` framing, no
//! referrer, a same-origin opener, every document `no-store`) with the
//! console's own Content-Security-Policy in place of the family's
//! ([`policy`]): scripts and styles from this origin alone, no nonce and no
//! WebAssembly, as FerroUI's server-drawn pages need. A request that is not a
//! safe method is taken only from the console's own pages, and a session's
//! access token is refreshed before the request it arrives with is served.
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
use http::header::{CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE};
use http::{HeaderValue, Method, StatusCode};

use crate::config::settings::{OidcSettings, Settings};
use crate::screen::Page;
use crate::session::Sessions;
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

/// An HTTP client that could not be built.
#[derive(Debug, thiserror::Error)]
#[error("the HTTP client of the console could not be built")]
pub struct ClientError {
    /// What the builder reported.
    #[source]
    source: reqwest::Error,
}

/// What every request handler shares.
#[derive(Debug, Clone)]
pub struct ConsoleState {
    settings: Arc<Settings>,
    sessions: Sessions,
    upstream: reqwest::Client,
    provider: reqwest::Client,
    health: Health,
}

impl ConsoleState {
    /// The state for `settings`, with an empty session store.
    ///
    /// # Errors
    /// Returns the [`ClientError`] of the terminology server's client, or of
    /// the OpenID Provider's, that cannot be built.
    pub fn new(settings: Settings) -> Result<Self, ClientError> {
        // NOTE: no specification governs this: our own design; a redirect from
        // the FHIR server is handed to the page as it came, never followed.
        let upstream = reqwest::Client::builder()
            .timeout(settings.ferroterm.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|source| ClientError { source })?;
        // NOTE: RFC 6749 §3.2: the token endpoint answers the request itself, so
        // the provider's client follows no redirect.
        let provider = reqwest::Client::builder()
            .timeout(settings.ferroterm.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|source| ClientError { source })?;
        // The settings are resolved and both clients built, so the console
        // serves from here.
        let lifecycle = Lifecycle::default();
        lifecycle.booted();
        Ok(Self {
            sessions: Sessions::new(settings.session),
            settings: Arc::new(settings),
            upstream,
            provider,
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

    /// The session store.
    #[must_use]
    pub fn sessions(&self) -> &Sessions {
        &self.sessions
    }

    /// The HTTP client of the terminology server and the sync service.
    #[must_use]
    pub fn upstream(&self) -> &reqwest::Client {
        &self.upstream
    }

    /// The HTTP client of the OpenID Provider, which the sign-in exchanges
    /// its code with and a session refreshes its access token with.
    #[must_use]
    pub fn provider(&self) -> &reqwest::Client {
        &self.provider
    }
}

/// Builds the console's whole HTTP service over `state`.
pub fn router(state: ConsoleState) -> axum::Router {
    let form_action = form_action(state.settings());
    let policy = policy(&form_action);
    let health = ferrohealth_server::health::routes(state.health().clone());
    let mut pages = axum::Router::new()
        .merge(health)
        .nest(
            ferrohealth_ui::serve::PREFIX,
            ferrohealth_ui::serve::axum_router::router(),
        )
        .route(crate::routes::LOGIN_ROUTE, get(crate::oidc::login))
        .route(crate::oidc::CALLBACK, get(crate::oidc::callback))
        .route(
            crate::routes::SIGN_OUT_ROUTE,
            post(crate::oidc::logout::logout),
        )
        .route(
            crate::routes::SESSION_ROUTE,
            get(crate::session::watch::status).post(crate::session::watch::status),
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
    let headers = ConsoleHeaders::new(form_action);
    pages
        .fallback(page)
        .layer(axum::middleware::from_fn(
            crate::oidc::refresh::refresh_session,
        ))
        .layer(axum::middleware::from_fn(same_origin_only))
        .layer(Extension(state))
        .layer(axum::middleware::from_fn(move |request, next| {
            console::headers(headers.clone(), request, next)
        }))
        .layer(axum::middleware::from_fn(move |request, next| {
            strict_policy(policy.clone(), request, next)
        }))
}

/// The Content-Security-Policy of every response, whose forms post to
/// `form_action`.
///
/// Scripts and styles come from the console alone, with no inline script
/// or style, no nonce and no `eval` (Content Security Policy Level 3), the
/// policy FerroUI's server-drawn pages need (`docs/migration-0.6.md`,
/// step 2).
#[must_use]
pub fn policy(form_action: &str) -> HeaderValue {
    let text = format!(
        "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
         connect-src 'self'; object-src 'none'; base-uri 'self'; form-action {form_action}; \
         frame-ancestors 'none'"
    );
    // NOTE: no specification governs this: our own design; a form action that cannot
    // be a header falls back to the strictest policy, which still blocks scripts.
    HeaderValue::from_str(&text)
        .unwrap_or_else(|_invalid| HeaderValue::from_static("default-src 'none'"))
}

/// Sets the console's own policy `policy` on the answer, in the place of
/// the family's, which admits the WebAssembly and the nonce a hydrated
/// console needs.
async fn strict_policy(policy: HeaderValue, request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(CONTENT_SECURITY_POLICY, policy);
    response
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
    let origin = state.settings().oidc.as_ref().map(OidcSettings::origin);
    if same_origin(request.headers(), origin.as_deref()) {
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
/// origin of the provider's end-session endpoint when one is configured.
///
/// A browser holds every redirect a form submission follows to
/// `form-action` (Content Security Policy Level 3, `form-action`), and the
/// sign-out form is answered with a redirect to that endpoint (OpenID
/// Connect RP-Initiated Logout 1.0 §2), so without its origin the browser
/// blocks the sign-out on its way to the provider.
#[must_use]
pub fn form_action(settings: &Settings) -> String {
    let end_session = settings
        .oidc
        .as_ref()
        .and_then(|oidc| oidc.end_session_endpoint.as_ref())
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
