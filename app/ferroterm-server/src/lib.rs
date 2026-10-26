// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroTERM FHIR terminology server.
//!
//! The library holds the whole run path so integration tests can drive it:
//! [`config::Config`] names the artifacts, [`state::AppState`] loads them into
//! a registry, [`reload::Serving`] holds that state and swaps it on a reload,
//! [`router`] builds the `axum` application over it, and [`serve`] runs it on a
//! bound listener until the process is asked to stop. `main.rs` only reads the
//! environment and calls in.
//!
//! Every served FHIR version has its own path prefix (`/r4`, `/r4b`, `/r5`), and within it
//! the resource and operation URLs the FHIR REST API defines
//! (<https://hl7.org/fhir/R4B/http.html>, <https://hl7.org/fhir/R4B/operations.html>).
#![doc(test(attr(deny(warnings))))]

pub mod banner;
pub mod cli;
pub mod config;
pub mod elements;
pub mod healthcheck;
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
use ferrohealth_health::lifecycle::Lifecycle;
use ferrohealth_telemetry::request_log::{self, RequestLog};
use tokio::net::TcpListener;

use crate::reload::Serving;

/// The base path of every served FHIR version; the router mounts each and the
/// boot lines name them.
pub const FHIR_BASES: [&str; 4] = ["/r4", "/r4b", "/r5", "/r6"];

/// Builds the HTTP application over `serving`.
///
/// `GET /health`, `GET /health/liveness` and `GET /health/readiness` are the
/// family's health routes over [`Serving::health`] ([`healthcheck`] is the
/// probe the container runs against `/health`), and `GET /metrics` is the
/// Prometheus exposition of [`Serving::telemetry`]; every FHIR route lives
/// under its version prefix. Any other path is an `OperationOutcome`
/// `not-found`, and that includes `/reload`, which only the admin listener
/// serves. Every request gets the server's request id and one line of the
/// request log of `ferrohealth-telemetry`, with the fields
/// [`request_fields`] adds; the log feeds the usage report window, naming a
/// request under a FHIR base that no route matched by its base.
pub fn router(serving: Serving) -> Router {
    // The gate sits inside each version's router, so the path it reads is the
    // one under the version prefix that `nest` leaves.
    let smart = serving.smart();
    let gated = |router: Router<Serving>, base: &'static str| match &smart {
        Some(smart) => smart::guard::fhir(router, smart, base),
        None => router,
    };
    let [r4, r4b, r5, r6] = FHIR_BASES;
    let health = ferrohealth_server::health::routes(serving.health().clone());
    let telemetry = std::sync::Arc::clone(serving.telemetry());
    let log = RequestLog::new(&serving.config().telemetry.logged_query_parameters)
        .with_metrics(&telemetry.meter(metrics::SCOPE))
        .with_recorder(serving.usage_window())
        .with_route_namer(std::sync::Arc::new(usage_report::FhirBaseNamer));
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
        .merge(health)
        .fallback(outcome::not_found)
        .layer(axum::middleware::from_fn_with_state(
            serving,
            request_fields::record,
        ))
        .layer(axum::middleware::from_fn_with_state(
            log,
            request_log::middleware,
        ))
        .layer(axum::middleware::from_fn(
            ferrohealth_server::request_id::assign,
        ))
}

/// Serves [`router`] on an already-bound listener until the process receives
/// `SIGTERM` or `SIGINT`, then finishes the connections in flight.
///
/// A container runtime stops a container with `SIGTERM` to PID 1 and kills it
/// after a grace period, so the server must answer the signal itself: there
/// is no shell and no init in the image to do it.
///
/// # Errors
///
/// Returns the I/O error from accepting connections or serving them.
pub async fn serve(listener: TcpListener, serving: Serving) -> std::io::Result<()> {
    serve_until(listener, serving, shutdown_signal()).await
}

/// Serves [`router`] on an already-bound listener until `shutdown` completes,
/// then finishes the connections in flight and returns.
///
/// # Errors
///
/// Returns the I/O error from accepting connections or serving them.
pub async fn serve_until<F>(
    listener: TcpListener,
    serving: Serving,
    shutdown: F,
) -> std::io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let lifecycle = serving.lifecycle().clone();
    axum::serve(listener, router(serving))
        .with_graceful_shutdown(draining(lifecycle, shutdown))
        .await
}

/// Serves the admin application ([`reload::router`]) on an already-bound
/// listener until `shutdown` completes.
///
/// The admin listener carries `POST /reload` and nothing of the FHIR surface,
/// and every route on it requires the admin scope `[auth]` names, unless the
/// deployment turned authentication off.
///
/// # Errors
///
/// Returns the I/O error from accepting connections or serving them.
pub async fn serve_admin_until<F>(
    listener: TcpListener,
    serving: Serving,
    shutdown: F,
) -> std::io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let lifecycle = serving.lifecycle().clone();
    axum::serve(listener, reload::router(serving))
        .with_graceful_shutdown(draining(lifecycle, shutdown))
        .await
}

/// Completes with `shutdown`, after marking `lifecycle` draining so the
/// health route withdraws readiness while the requests in flight finish.
async fn draining<F>(lifecycle: Lifecycle, shutdown: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    shutdown.await;
    lifecycle.drain();
}

/// Completes when the process receives `SIGTERM` or `SIGINT`.
///
/// A failure to install a handler is logged and the future never completes,
/// so the server keeps serving and the runtime's kill after the grace period
/// remains the backstop.
pub async fn shutdown_signal() {
    let interrupt = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "cannot listen for SIGINT");
            std::future::pending::<()>().await;
        }
    };
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    tokio::select! {
        () = interrupt => tracing::info!("SIGINT received, shutting down"),
        () = terminate => tracing::info!("SIGTERM received, shutting down"),
    }
}
