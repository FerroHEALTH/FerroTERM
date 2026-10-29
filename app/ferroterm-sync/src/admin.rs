// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The service's own admin listener.
//!
//! `POST /run` and `POST /activate` are the manual triggers, `GET /runs` and
//! `GET /runs/{id}` are the run records, and `GET /metrics` is the Prometheus
//! exposition ([`routes`]). [`app`] wraps them in the stack of
//! `ferrohealth-server`, which adds `GET /health`, `GET /health/liveness` and
//! `GET /health/readiness`, the request log, the request timeout, the body
//! limit and the panic catch. [`crate::auth`] puts every route but the health
//! routes behind a credential carrying the product scope.
//!
//! A `POST /run` answers when the run has finished, which is how an operator
//! reads the outcome of the run they asked for. A run that builds an edition
//! keeps the request open for as long as the build takes.
//!
//! No FHIR specification governs these routes: our own design.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use ferrohealth_server::section::ServerSection;
use ferrohealth_server::stack::{self, StackPolicy};
use ferrohealth_telemetry::request_log::RequestLog;

use crate::problem::{self, Kind};
use crate::record::RecordError;
use crate::run::Service;

/// The admin application over `service`: [`routes`] under the stack, with no
/// credential required.
#[must_use = "the router is the application the listener serves"]
pub fn router(service: Arc<Service>) -> Router {
    let held = Arc::clone(&service);
    app(&held, routes(service))
}

/// `routes`, which [`crate::auth`] may have guarded, under the stack of
/// `ferrohealth-server` with the health routes of `service`.
#[must_use = "the router is the application the listener serves"]
pub fn app(service: &Service, routes: Router) -> Router {
    let section = ServerSection {
        listen: service.config().listen,
        ..ServerSection::default()
    };
    let policy = StackPolicy {
        panic_body: ferrohealth_server::problem::panic_body(),
        refusal_body: ferrohealth_server::problem::refusal_body(),
        request_log: RequestLog::new(&service.config().telemetry.logged_query_parameters)
            .with_metrics(&service.telemetry().meter(crate::metrics::SCOPE)),
    };
    stack::app(routes, service.health().clone(), &section, policy)
}

/// The admin routes over `service`, without the health routes and the stack.
#[must_use = "the router is the application the listener serves"]
pub fn routes(service: Arc<Service>) -> Router {
    Router::new()
        .route("/run", post(run))
        .route("/activate", post(activate))
        .route("/runs", get(runs))
        .route("/runs/{id}", get(one_run))
        .route(
            "/metrics",
            ferrohealth_telemetry::telemetry::prometheus_route(Arc::clone(service.telemetry())),
        )
        .with_state(service)
        .fallback(unknown)
}

/// `POST /run`: one run now, answered when it has finished.
async fn run(State(service): State<Arc<Service>>) -> Response {
    let record = service.run(crate::record::Trigger::Manual).await;
    (StatusCode::OK, axum::Json(record.summary())).into_response()
}

/// `POST /activate`: put what is staged in front of the server.
async fn activate(State(service): State<Arc<Service>>) -> Response {
    let record = service.activate().await;
    (StatusCode::OK, axum::Json(record.summary())).into_response()
}

/// `GET /runs`: the runs this service performed, newest first.
async fn runs(State(service): State<Arc<Service>>) -> Response {
    match service.records().summaries() {
        Ok(summaries) => (StatusCode::OK, axum::Json(summaries)).into_response(),
        Err(error) => failed(&error),
    }
}

/// `GET /runs/{id}`: one whole run record.
async fn one_run(State(service): State<Arc<Service>>, Path(id): Path<String>) -> Response {
    match service.records().read(&id) {
        Ok(record) => (StatusCode::OK, axum::Json(record)).into_response(),
        Err(RecordError::BadId { id }) => {
            tracing::debug!(id, "a run id that is no run id");
            not_a_run()
        }
        Err(RecordError::Io { path, source }) if source.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(path = %path.display(), "no such run record");
            not_a_run()
        }
        Err(error) => failed(&error),
    }
}

/// The answer to a run id the service holds no record of.
fn not_a_run() -> Response {
    problem::respond(
        Kind::RunNotFound.problem("no run of this service has that id; GET /runs lists them"),
        None,
    )
}

/// Anything else on the admin listener.
async fn unknown() -> Response {
    problem::blank(
        StatusCode::NOT_FOUND,
        "the admin listener answers POST /run, POST /activate, GET /runs, GET /runs/{id}, GET /metrics and the health routes",
    )
}

/// The answer to a request the service could not serve; the log names the
/// cause, and the problem quotes none of it.
fn failed(error: &dyn core::error::Error) -> Response {
    let reason = crate::reason(error);
    tracing::error!(error = reason, "an admin request could not be served");
    problem::respond(
        Kind::RecordsUnreadable
            .problem("the run records could not be read; the service log names the cause"),
        None,
    )
}
