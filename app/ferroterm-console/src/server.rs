// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's HTTP service: the health route, the sign-in routes, the
//! Leptos pages, the server functions and the site bundle, under one set of
//! browser security headers.
//!
//! Every response carries a Content-Security-Policy whose script source is a
//! nonce minted for that response alone, `nosniff`, `DENY` framing and no
//! referrer, and every document is `no-store`. Only the site bundle is
//! compressed. A request that is not a safe method is taken only from the
//! console's own pages, and a session's access token is refreshed before
//! the request it arrives with is served. No specification governs the
//! console's HTTP surface: our own design.

use std::sync::Arc;

use axum::Extension;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS,
    X_FRAME_OPTIONS,
};
use http::{HeaderValue, StatusCode};
use leptos::config::LeptosOptions;
use leptos::nonce::Nonce;
use leptos_axum::LeptosRoutes as _;
use serde::Serialize;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{And, Predicate as _, SizeAbove};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::config::settings::{OidcSettings, Settings};
use crate::session::Sessions;

/// The path of the console's own liveness route.
pub const HEALTH: &str = "/health";

/// The name cargo-leptos gives the site bundle's files under `pkg/`.
pub const OUTPUT_NAME: &str = "ferroterm-console";

/// The path the components' stylesheet is served at.
pub const COMPONENTS_CSS: &str = "/ferrohealth-ui.css";

/// The path the brand theme is served at.
pub const THEME_CSS: &str = "/theme.css";

/// The FerroTERM brand: FerroUI's teal theme over the roles of the
/// components' stylesheet, loaded after it.
pub const THEME: &str = ferrohealth_ui::style::Brand::Term.theme();

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
        Ok(Self {
            sessions: Sessions::new(settings.session),
            settings: Arc::new(settings),
            upstream,
            provider,
        })
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

/// The Leptos options the console renders with: its site bundle under the
/// configured site root and its listen address.
#[must_use]
pub fn leptos_options(settings: &Settings) -> LeptosOptions {
    LeptosOptions::builder()
        .output_name(OUTPUT_NAME)
        .site_root(settings.site_root.to_string_lossy().into_owned())
        .site_pkg_dir("pkg")
        .site_addr(settings.listen)
        .build()
}

/// Builds the console's whole HTTP service over `state`.
pub fn router(state: ConsoleState) -> axum::Router {
    let options = leptos_options(state.settings());
    let form_action = form_action(state.settings());
    let routes = leptos_axum::generate_route_list(crate::app::App);
    let context = {
        let state = state.clone();
        move || {
            leptos::context::provide_context(state.clone());
            provide_request_nonce();
        }
    };
    let pages = axum::Router::new()
        .route(HEALTH, get(health))
        .route(COMPONENTS_CSS, get(components_css))
        .route(THEME_CSS, get(theme_css))
        .route(crate::oidc::LOGIN, get(crate::oidc::login))
        .route(crate::oidc::CALLBACK, get(crate::oidc::callback))
        .route(
            crate::oidc::logout::LOGOUT,
            axum::routing::post(crate::oidc::logout::logout),
        )
        .leptos_routes_with_context(&options, routes, context.clone(), {
            let options = options.clone();
            move || crate::app::shell(options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler_with_context(
            context,
            crate::app::shell,
        ))
        .layer(axum::middleware::from_fn(
            crate::oidc::refresh::refresh_session,
        ))
        .layer(axum::middleware::from_fn(same_origin_only))
        .layer(Extension(state));
    with_security_headers(pages.layer(bundle_compression()), form_action).with_state(options)
}

/// The fetch metadata header that names where a request comes from.
const SEC_FETCH_SITE: &str = "sec-fetch-site";

/// Refuses every request that is not a safe method, a server function call
/// and a sign-out among them, unless it comes from the console's own pages
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
/// closed. The `Origin` and `Referer` fallbacks serve a server function the
/// page calls with `fetch`.
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

/// Serves the components' stylesheet, from the pinned `ferrohealth-ui`.
async fn components_css() -> Response {
    stylesheet(ferrohealth_ui::style::STYLESHEET)
}

/// Serves the FerroTERM brand theme.
async fn theme_css() -> Response {
    stylesheet(THEME)
}

/// A stylesheet answer of `text`, revalidated so a new release is fetched.
fn stylesheet(text: &'static str) -> Response {
    (
        StatusCode::OK,
        [(http::header::CONTENT_TYPE, "text/css; charset=utf-8")],
        text,
    )
        .into_response()
}

/// The body of the liveness route.
#[derive(Debug, Serialize)]
struct Health {
    /// Always `up`: the route answers while the process serves.
    status: &'static str,
}

/// Serves `GET /health`: `200` while the process serves.
async fn health() -> Response {
    (StatusCode::OK, axum::Json(Health { status: "up" })).into_response()
}

/// The media types of the site bundle: the WebAssembly, its JavaScript glue
/// and the stylesheet.
const BUNDLE_TYPES: [&str; 4] = [
    "application/wasm",
    "text/javascript",
    "application/javascript",
    "text/css",
];

/// A test of a response, the shape the compression layer's predicate takes.
type ResponsePredicate = fn(StatusCode, http::Version, &http::HeaderMap, &http::Extensions) -> bool;

/// Whether a response carries a file of the site bundle, by its media type.
fn is_bundle(
    _status: StatusCode,
    _version: http::Version,
    headers: &http::HeaderMap,
    _extensions: &http::Extensions,
) -> bool {
    headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|media| BUNDLE_TYPES.iter().any(|bundle| media.starts_with(bundle)))
}

/// Compresses the site bundle with brotli or gzip, as the request's
/// `Accept-Encoding` chooses, and marks it `Vary: Accept-Encoding`.
///
/// Only the bundle is compressed. A document or a server function's answer
/// can carry what the operator entered beside data an attacker would want,
/// so it is never compressed, which keeps a compression side channel (BREACH)
/// shut. The Leptos book (`deployment/binary_size`) has a site serve its
/// WebAssembly compressed.
fn bundle_compression() -> CompressionLayer<And<SizeAbove, ResponsePredicate>> {
    let bundle: ResponsePredicate = is_bundle;
    CompressionLayer::new()
        .br(true)
        .gzip(true)
        .compress_when(SizeAbove::new(256).and(bundle))
}

/// Wraps `router` in the browser security headers every response carries,
/// with `form_action` as the policy's `form-action` sources.
fn with_security_headers(
    router: axum::Router<LeptosOptions>,
    form_action: String,
) -> axum::Router<LeptosOptions> {
    let form_action: Arc<str> = Arc::from(form_action);
    router
        .layer(axum::middleware::from_fn(
            move |request: Request, next: Next| {
                content_security_policy(Arc::clone(&form_action), request, next)
            },
        ))
        .layer(axum::middleware::from_fn(cache_control))
        .layer(SetResponseHeaderLayer::overriding(
            X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
}

/// Mints this response's script nonce, hands it to the renderer through the
/// request, and answers with the policy that names it and `form_action`.
async fn content_security_policy(
    form_action: Arc<str>,
    mut request: Request,
    next: Next,
) -> Response {
    let nonce = Nonce::new();
    let policy = HeaderValue::from_str(&policy(&nonce, &form_action));
    request.extensions_mut().insert(nonce);
    let mut response = next.run(request).await;
    match policy {
        Ok(policy) => {
            response
                .headers_mut()
                .insert(CONTENT_SECURITY_POLICY, policy);
            response
        }
        Err(_invalid) => {
            tracing::error!("the Content-Security-Policy could not be written as a header");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Re-provides the request's nonce into the render, so the bootstrap script
/// carries the nonce the response header authorizes.
///
/// `leptos_axum` provides a nonce of its own before this runs, and the
/// request parts it provides carry the one [`content_security_policy`]
/// minted, which replaces it.
fn provide_request_nonce() {
    let Some(parts) = leptos::context::use_context::<http::request::Parts>() else {
        return;
    };
    if let Some(nonce) = parts.extensions.get::<Nonce>() {
        leptos::context::provide_context(nonce.clone());
    }
}

/// The Content-Security-Policy of one response, naming its script nonce and
/// the `form-action` sources [`form_action`] returns.
///
/// The WebAssembly bundle needs `'wasm-unsafe-eval'` to be compiled, which
/// admits no JavaScript `eval` (Content Security Policy Level 3). Scripts run
/// by nonce alone, and nothing admits `'unsafe-inline'`.
#[must_use]
pub fn policy(nonce: &Nonce, form_action: &str) -> String {
    format!(
        "default-src 'self'; \
         script-src 'self' 'wasm-unsafe-eval' 'nonce-{nonce}'; \
         style-src 'self'; \
         img-src 'self' data:; \
         connect-src 'self'; \
         object-src 'none'; \
         base-uri 'self'; \
         form-action {form_action}; \
         frame-ancestors 'none'"
    )
}

/// Marks every document `no-store`, and the site bundle `no-cache`, so a
/// rebuilt bundle is fetched again (RFC 9111 §5.2.2).
async fn cache_control(request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let bundle = path.starts_with("/pkg/") || path == COMPONENTS_CSS || path == THEME_CSS;
    let mut response = next.run(request).await;
    let value = if bundle && response.status().is_success() {
        "no-cache"
    } else {
        "no-store"
    };
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static(value));
    response
}

/// Serves `router` on `listener` until `shutdown` resolves.
///
/// # Errors
/// Returns the I/O error of a listener that fails.
pub async fn serve(
    listener: tokio::net::TcpListener,
    router: axum::Router,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
}

#[cfg(test)]
mod tests {
    use super::policy;
    use leptos::nonce::Nonce;

    #[test]
    fn the_policy_allows_scripts_only_by_nonce_and_never_eval() {
        let nonce = Nonce::new();
        let policy = policy(&nonce, "'self'");
        let script = policy
            .split(';')
            .map(str::trim)
            .find(|directive| directive.starts_with("script-src"))
            .expect("a script-src directive");
        assert_eq!(
            format!("script-src 'self' 'wasm-unsafe-eval' 'nonce-{nonce}'"),
            script
        );
        assert!(!policy.contains("'unsafe-eval'"), "{policy}");
        assert!(!policy.contains("'unsafe-inline'"), "{policy}");
    }
}
