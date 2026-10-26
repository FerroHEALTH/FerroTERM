// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroTERM FHIR terminology server.
//!
//! The library holds the whole run path so integration tests can drive it:
//! [`config::Config`] names the artifacts, [`state::AppState`] loads them into
//! a registry, [`reload::Serving`] holds that state and swaps it on a reload,
//! [`router`] builds the `axum` application over it inside the stack of
//! `ferrohealth-server`, and [`serve`] runs it and the admin listener on bound
//! listeners until the process is asked to stop, then drains within
//! `server.shutdown_timeout_ms`. `main.rs` only reads the configuration and
//! calls in.
//!
//! Every served FHIR version has its own path prefix (`/r4`, `/r4b`, `/r5`), and within it
//! the resource and operation URLs the FHIR REST API defines
//! (<https://hl7.org/fhir/R4B/http.html>, <https://hl7.org/fhir/R4B/operations.html>).
#![doc(test(attr(deny(warnings))))]

pub mod banner;
pub mod cli;
pub mod config;
pub mod elements;
pub mod metrics;
pub mod outcome;
pub mod persistence;
pub mod r4;
pub mod r4b;
pub mod r5;
pub mod r6;
pub mod reload;
// The build script is the only caller: it `include!`s this source rather than
// linking the library it builds, so the module is compiled here for its tests.
#[cfg(test)]
mod release_date;
pub mod request_fields;
pub mod scope;
pub mod smart;
pub mod state;
pub mod usage_report;
pub mod version;
pub mod wire;

use std::future::Future;

use axum::Router;
use ferrohealth_server::healthcheck::Outcome;
use ferrohealth_server::section::ServerSection;
use ferrohealth_server::serve::{Drained, Listener, ServeError};
use ferrohealth_server::stack::{self, StackPolicy};
use ferrohealth_telemetry::request_log::RequestLog;
use tokio::net::TcpListener;

use crate::reload::Serving;

/// The base path of every served FHIR version; the router mounts each and the
/// boot lines name them.
pub const FHIR_BASES: [&str; 4] = ["/r4", "/r4b", "/r5", "/r6"];

/// How long `ferroterm healthcheck` waits for the readiness answer: shorter
/// than the image's `HEALTHCHECK --timeout=5s`, so the probe reports the
/// reason itself instead of being killed.
pub const HEALTHCHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Builds the HTTP application over `serving`: [`routes`] inside the stack of
/// `ferrohealth-server` as `[server]` configures it.
///
/// The stack adds `GET /health`, `GET /health/liveness` and
/// `GET /health/readiness` over [`Serving::health`], the server's request id,
/// one line of the request log of `ferrohealth-telemetry` per request, the
/// request timeout, the body limit and the panic catch; a panic and each
/// refusal of the stack answer an `OperationOutcome` ([`stack_policy`]).
pub fn router(serving: Serving) -> Router {
    let policy = stack_policy(&serving);
    let health = serving.health().clone();
    let section = serving.config().server.clone();
    stack::app(routes(serving), health, &section, policy)
}

/// The FHIR routes over `serving`, without the stack.
///
/// `GET /metrics` is the Prometheus exposition of [`Serving::telemetry`];
/// every FHIR route lives under its version prefix. Any other path is an
/// `OperationOutcome` `not-found`, and that includes `/reload`, which only
/// the admin listener serves. Every request gets the fields
/// [`request_fields`] adds to its request line.
pub fn routes(serving: Serving) -> Router {
    // The gate sits inside each version's router, so the path it reads is the
    // one under the version prefix that `nest` leaves.
    let smart = serving.smart();
    let gated = |router: Router<Serving>, base: &'static str| match &smart {
        Some(smart) => smart::guard::fhir(router, smart, base),
        None => router,
    };
    let [r4, r4b, r5, r6] = FHIR_BASES;
    let telemetry = std::sync::Arc::clone(serving.telemetry());
    Router::new()
        .nest(r4, gated(r4::router(), r4))
        .nest(r4b, gated(r4b::router(), r4b))
        .nest(r5, gated(r5::router(), r5))
        .nest(r6, gated(r6::router(), r6))
        .route(
            "/metrics",
            ferrohealth_telemetry::telemetry::prometheus_route(telemetry),
        )
        .with_state(serving.clone())
        .fallback(outcome::not_found)
        .layer(axum::middleware::from_fn_with_state(
            serving,
            request_fields::record,
        ))
}

/// What the FHIR listener decides inside the stack.
///
/// A panic and each refusal answer an `OperationOutcome`, and the request
/// log feeds the metrics and the usage report window, naming a request under
/// a FHIR base that no route matched by its base.
#[must_use]
pub fn stack_policy(serving: &Serving) -> StackPolicy {
    let telemetry = serving.telemetry();
    StackPolicy {
        panic_body: outcome::panic_body(),
        refusal_body: outcome::refusal_body(),
        request_log: RequestLog::new(&serving.config().telemetry.logged_query_parameters)
            .with_metrics(&telemetry.meter(metrics::SCOPE))
            .with_recorder(serving.usage_window())
            .with_route_namer(std::sync::Arc::new(usage_report::FhirBaseNamer)),
    }
}

/// Builds the admin application over `serving`: [`reload::router`] inside the
/// stack, with the same health routes and the request log feeding the
/// metrics.
///
/// The admin listener carries `POST /reload` and nothing of the FHIR surface,
/// and every route on it but the health routes requires the admin scope
/// `[auth]` names, unless the deployment turned authentication off.
pub fn admin_router(serving: Serving) -> Router {
    let telemetry = serving.telemetry();
    let policy = StackPolicy {
        request_log: RequestLog::new(&serving.config().telemetry.logged_query_parameters)
            .with_metrics(&telemetry.meter(metrics::SCOPE)),
        ..StackPolicy::default()
    };
    let health = serving.health().clone();
    let section = serving.config().server.clone();
    stack::app(reload::router(serving), health, &section, policy)
}

/// Serves [`router`] on `listener` and, when one is given, [`admin_router`]
/// on `admin` until the process receives `SIGTERM` or `SIGINT`, then drains
/// both.
///
/// A container runtime stops a container with `SIGTERM` to PID 1 and kills it
/// after a grace period, so the server answers the signal itself: there is no
/// shell and no init in the image to do it.
///
/// # Errors
///
/// Returns [`ServeError`] when the TLS files `[server.tls]` names cannot be
/// read or a listener fails.
pub async fn serve(
    listener: TcpListener,
    admin: Option<TcpListener>,
    serving: Serving,
) -> Result<Drained, ServeError> {
    serve_until(
        listener,
        admin,
        serving,
        ferrohealth_server::serve::shutdown_signal(),
    )
    .await
}

/// Serves as [`serve`] does until `shutdown` completes.
///
/// Readiness turns `503` at once, the listeners keep accepting for
/// `server.drain_delay_ms`, then close, and the requests in flight get
/// `server.shutdown_timeout_ms` to finish; a connection still open then is
/// dropped and the result is [`Drained::Cut`]. Both listeners speak the TLS
/// `[server.tls]` configures, when it is set.
///
/// # Errors
///
/// Returns [`ServeError`] when the TLS files cannot be read or a listener
/// fails.
pub async fn serve_until<F>(
    listener: TcpListener,
    admin: Option<TcpListener>,
    serving: Serving,
    shutdown: F,
) -> Result<Drained, ServeError>
where
    F: Future<Output = ()> + Send + 'static,
{
    let section = serving.config().server.clone();
    let lifecycle = serving.lifecycle().clone();
    let with_tls = |one: Listener| match &section.tls {
        Some(tls) => one.with_tls(tls.clone()),
        None => one,
    };
    let mut listeners = vec![with_tls(Listener::new(listener, router(serving.clone())))];
    if let Some(admin) = admin {
        listeners.push(with_tls(Listener::new(admin, admin_router(serving))));
    }
    ferrohealth_server::serve::run_all(listeners, &section, lifecycle, shutdown).await
}

/// Asks the server `section` configures for its readiness, over the TLS it
/// configures, on the loopback when it listens on every interface: the
/// container's `HEALTHCHECK`.
pub async fn healthcheck(section: &ServerSection) -> Outcome {
    use ferrohealth_server::healthcheck::{check, check_tls, target};

    let address = target(section.listen);
    let base = section.base_path.as_str();
    match &section.tls {
        None => check(address, base, HEALTHCHECK_TIMEOUT).await,
        Some(tls) => match ferrohealth_server::tls::HealthcheckTls::read(tls) {
            Ok(pinned) => check_tls(address, base, HEALTHCHECK_TIMEOUT, &pinned).await,
            Err(error) => Outcome::Failed(format!("the TLS files do not read: {error}")),
        },
    }
}
