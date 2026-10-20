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
pub mod request_log;
pub mod scope;
pub mod smart;
pub mod state;
pub mod telemetry;
pub mod usage_report;
pub mod version;
pub mod wire;

use std::future::Future;
use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use http::StatusCode;
use tokio::net::TcpListener;

use crate::reload::Serving;
use crate::state::AppState;

/// The base path of every served FHIR version; the router mounts each and the
/// boot lines name them.
pub const FHIR_BASES: [&str; 4] = ["/r4", "/r4b", "/r5", "/r6"];

/// Builds the HTTP application over `serving`.
///
/// `GET /health` answers `200 OK` while the process is up ([`healthcheck`] is
/// the probe the container runs against it); every FHIR route lives under its
/// version prefix. Any other path is an `OperationOutcome` `not-found`, and
/// that includes `/reload`, which only the admin listener serves.
pub fn router(serving: Serving) -> Router {
    // The gate sits inside each version's router, so the path it reads is the
    // one under the version prefix that `nest` leaves.
    let smart = serving.smart();
    let gated = |router: Router<Serving>, base: &'static str| match &smart {
        Some(smart) => smart::guard::fhir(router, smart, base),
        None => router,
    };
    let [r4, r4b, r5, r6] = FHIR_BASES;
    Router::new()
        .route("/health", get(health))
        .nest(r4, gated(r4::router(), r4))
        .nest(r4b, gated(r4b::router(), r4b))
        .nest(r5, gated(r5::router(), r5))
        .nest(r6, gated(r6::router(), r6))
        .route("/metrics", get(metrics_scrape))
        .fallback(outcome::not_found)
        .layer(axum::middleware::from_fn_with_state(
            serving.clone(),
            request_log::log,
        ))
        .with_state(serving)
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
    axum::serve(listener, router(serving))
        .with_graceful_shutdown(shutdown)
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
    axum::serve(listener, reload::router(serving))
        .with_graceful_shutdown(shutdown)
        .await
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

async fn health() -> StatusCode {
    StatusCode::OK
}

/// `GET /metrics`: the Prometheus exposition of this server.
///
/// The endpoint is off the FHIR base path, so a scrape is never a terminology
/// request and no FHIR content negotiation applies. It answers the text
/// exposition format Prometheus reads
/// (<https://prometheus.io/docs/instrumenting/exposition_formats/>).
async fn metrics_scrape(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    match state.metrics().exposition() {
        Ok(text) => (
            StatusCode::OK,
            [(
                http::header::CONTENT_TYPE,
                "application/openmetrics-text; version=1.0.0; charset=utf-8",
            )],
            text,
        )
            .into_response(),
        Err(error) => {
            tracing::error!(%error, "the metrics registry cannot be written");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
