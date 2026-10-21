// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The service's own admin listener.
//!
//! Six routes and nothing else: `POST /run` and `POST /activate` are the
//! manual triggers, `GET /runs` and `GET /runs/{id}` are the run records,
//! `GET /metrics` is the Prometheus exposition, and `GET /health` says the
//! process is up. The listener authenticates nobody, like the server's own
//! admin listener, so it belongs on an internal network.
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

use crate::record::RecordError;
use crate::run::Service;

/// The admin application over `service`.
#[must_use = "the router is the application the listener serves"]
pub fn router(service: Arc<Service>) -> Router {
    Router::new()
        .route("/run", post(run))
        .route("/activate", post(activate))
        .route("/runs", get(runs))
        .route("/runs/{id}", get(one_run))
        .route("/metrics", get(metrics))
        .route("/health", get(health))
        .fallback(unknown)
        .with_state(service)
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
        Err(RecordError::BadId { id }) => (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({ "error": format!("`{id}` is not a run") })),
        )
            .into_response(),
        Err(RecordError::Io { path, source }) if source.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(path = %path.display(), "no such run record");
            (
                StatusCode::NOT_FOUND,
                axum::Json(serde_json::json!({ "error": "no such run" })),
            )
                .into_response()
        }
        Err(error) => failed(&error),
    }
}

/// `GET /metrics`: the Prometheus exposition of this service.
async fn metrics(State(service): State<Arc<Service>>) -> Response {
    match service.metrics().exposition() {
        Ok(text) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
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

/// `GET /health`: the process is up.
async fn health() -> StatusCode {
    StatusCode::OK
}

/// Anything else on the admin listener.
async fn unknown() -> Response {
    (
        StatusCode::NOT_FOUND,
        "ferroterm-sync admin: POST /run, POST /activate, GET /runs, GET /runs/{id}, GET /metrics, GET /health\n",
    )
        .into_response()
}

/// The answer to a request the service could not serve.
fn failed(error: &dyn core::error::Error) -> Response {
    let reason = crate::reason(error);
    tracing::error!(error = reason, "an admin request could not be served");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        axum::Json(serde_json::json!({ "error": reason })),
    )
        .into_response()
}
