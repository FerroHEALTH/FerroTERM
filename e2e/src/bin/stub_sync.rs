// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The synchronisation service's admin listener, as the console's
//! Synchronisation journeys reach it.
//!
//! It answers the four routes the console calls, in the shapes the sync
//! service writes (`app/ferroterm-sync/src/record.rs` holds the real ones):
//! `GET /runs` with the run summaries newest first, `GET /runs/{id}` with one
//! whole record, and `POST /run` and `POST /activate` with the summary of a
//! run they add to the list. Every route but `GET /health` refuses a request
//! with no bearer credential, as the real listener refuses one without its
//! product scope, so a journey proves the console sends the reader's token.
//! Nothing here is a product binary, and no figure in it is real: the runs
//! are invented for the journeys and name the synthetic fixture.
//!
//!   stub-sync --listen 127.0.0.1:18090

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use http_body_util::Full;
use hyper::Method;
use hyper::Request;
use hyper::Response;
use hyper::StatusCode;
use hyper::body::Bytes;
use hyper::body::Incoming;
use hyper::header::AUTHORIZATION;
use hyper::header::CONTENT_TYPE;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use serde_json::Value;
use serde_json::json;
use tokio::net::TcpListener;

/// Anything that stops the listener starting.
type Failure = Box<dyn std::error::Error + Send + Sync>;

/// The value set of the synthetic fixture a finding names by its canonical.
const FIXTURE_VALUE_SET: &str = "https://ferroterm.eu/fhir/ValueSet/e2e-taxonomy-all";

/// The code system of the synthetic fixture.
const FIXTURE_SYSTEM: &str = "https://ferroterm.eu/fhir/CodeSystem/e2e-taxonomy";

/// The run that took a release and found one local code it changed.
const TAKEN_RUN: &str = "20261021T030000Z";

/// The run that found nothing new.
const QUIET_RUN: &str = "20261020T030000Z";

/// One whole run record, in the shape the sync service writes.
fn record(id: &str, trigger: &str, started: &str, finished: &str, taken: bool) -> Value {
    let (entries, findings, activated) = if taken {
        (
            json!([{
                "entry_id": "e2e-1", "title": "Synthetic taxonomy", "origin": "feed",
                "canonical": FIXTURE_SYSTEM, "version": "2", "category": "code-system",
                "lane": "fhir", "bytes": 3_100_000, "download_ms": 120, "build_ms": null,
                "staged": null, "fixups": [], "outcome": "ok", "error": null
            }]),
            json!([{
                "resource": format!("ValueSet {FIXTURE_VALUE_SET}"),
                "system": FIXTURE_SYSTEM, "code": "ca-leaf", "kind": "inactive", "release": "2"
            }]),
            json!([{"lane": "fhir", "target": "/data/codesystems/x", "canonical": FIXTURE_SYSTEM, "version": "2"}]),
        )
    } else {
        (json!([]), json!([]), json!([]))
    };
    let count = findings.as_array().map_or(0, Vec::len);
    json!({
        "id": id, "trigger": trigger, "started": started, "finished": finished,
        "duration_ms": if taken { 252_000 } else { 9_400 }, "outcome": "ok",
        "sources": [{
            "source": "nts", "feed_url": "https://feed.example/atom", "entries_seen": 412,
            "fhir_api_url": null, "api_entries_seen": 0, "not_visible": [],
            "taken": entries, "skipped": [], "not_subscribed": [{"origin": "feed", "entries": 388}],
            "errors": []
        }],
        "activation": {
            "mode": "auto", "activated": activated, "staged": 0,
            "reloads": if taken { json!([{"status": 200, "body": "{}"}]) } else { json!([]) },
            "rolled_back": false, "error": null
        },
        "retention": [],
        "revalidation": {
            "resources": 1, "checked_resources": [], "checked": 3, "findings": findings,
            "statement": format!("1 locally written resource was revalidated and {count} local codes changed"),
            "skipped": null
        },
        "bytes_used": 0, "bytes_staged": if taken { 3_100_000 } else { 0 },
        "entries_taken": usize::from(taken), "errors": []
    })
}

/// The short shape of `record`, as `/runs` and the triggers carry it.
fn summary(record: &Value) -> Value {
    let field = |name: &str| record.get(name).cloned().unwrap_or(Value::Null);
    let findings = record
        .pointer("/revalidation/findings")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let activated = record
        .pointer("/activation/activated")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    json!({
        "id": field("id"), "trigger": field("trigger"), "outcome": field("outcome"),
        "started": field("started"), "finished": field("finished"),
        "duration_ms": field("duration_ms"), "entries_taken": field("entries_taken"),
        "entries_skipped": 0, "entries_not_subscribed": 388, "activated": activated,
        "bytes_staged": field("bytes_staged"), "bytes_used": 0,
        "revalidation_findings": findings,
        "revalidation": record.pointer("/revalidation/statement").cloned().unwrap_or(Value::Null),
        "errors": []
    })
}

/// The records this listener holds, newest first.
#[derive(Debug)]
struct Records(Mutex<Vec<Value>>);

impl Records {
    /// The two runs every journey starts from.
    fn seeded() -> Self {
        Self(Mutex::new(vec![
            record(
                TAKEN_RUN,
                "schedule",
                "2026-10-21T03:00:00Z",
                "2026-10-21T03:04:12Z",
                true,
            ),
            record(
                QUIET_RUN,
                "schedule",
                "2026-10-20T03:00:00Z",
                "2026-10-20T03:00:09Z",
                false,
            ),
        ]))
    }

    /// Adds a run started by `trigger` now, and answers its summary.
    fn add(&self, trigger: &str) -> Value {
        let now = jiff::Timestamp::now();
        let id = now.strftime("%Y%m%dT%H%M%S%.fZ").to_string();
        let added = record(&id, trigger, &now.to_string(), &now.to_string(), false);
        let answer = summary(&added);
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(0, added);
        answer
    }

    /// Every summary, newest first.
    fn summaries(&self) -> Value {
        Value::Array(
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .map(summary)
                .collect(),
        )
    }

    /// The whole record of the run `id`.
    fn one(&self, id: &str) -> Option<Value> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|held| held.get("id").and_then(Value::as_str) == Some(id))
            .cloned()
    }
}

/// An answer of `status` carrying `body` as JSON.
fn answer(status: StatusCode, body: &Value) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(body.to_string())));
    *response.status_mut() = status;
    if let Ok(value) = "application/json".parse() {
        response.headers_mut().insert(CONTENT_TYPE, value);
    }
    response
}

/// Answers one request.
fn handle(records: &Records, request: &Request<Incoming>) -> Response<Full<Bytes>> {
    let path = request.uri().path();
    if request.method() == Method::GET && path == "/health" {
        return answer(StatusCode::OK, &json!({"status": "pass"}));
    }
    let bearer = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| !token.is_empty());
    if !bearer {
        return answer(
            StatusCode::UNAUTHORIZED,
            &json!({"error": "a bearer credential carrying the product scope is required"}),
        );
    }
    match (request.method(), path) {
        (&Method::GET, "/runs") => answer(StatusCode::OK, &records.summaries()),
        (&Method::POST, "/run") => answer(StatusCode::OK, &records.add("manual")),
        (&Method::POST, "/activate") => answer(StatusCode::OK, &records.add("activate")),
        (&Method::GET, other) => {
            match other.strip_prefix("/runs/").and_then(|id| records.one(id)) {
                Some(found) => answer(StatusCode::OK, &found),
                None => answer(StatusCode::NOT_FOUND, &json!({"error": "no such run"})),
            }
        }
        _ => answer(StatusCode::NOT_FOUND, &json!({"error": "no such route"})),
    }
}

/// The address to listen on, from `--listen ADDRESS`.
fn listen_address(mut args: impl Iterator<Item = String>) -> Result<SocketAddr, Failure> {
    match (args.next().as_deref(), args.next()) {
        (Some("--listen"), Some(address)) => Ok(address.parse()?),
        _ => Err(Failure::from("usage: stub-sync --listen ADDRESS")),
    }
}

#[tokio::main]
async fn main() -> Result<(), Failure> {
    let address = listen_address(std::env::args().skip(1))?;
    let records = Arc::new(Records::seeded());
    let listener = TcpListener::bind(address).await?;
    println!("listening on {address}");
    loop {
        let (stream, _peer) = listener.accept().await?;
        let records = Arc::clone(&records);
        tokio::spawn(async move {
            let service = service_fn(move |request: Request<Incoming>| {
                let records = Arc::clone(&records);
                async move { Ok::<_, std::convert::Infallible>(handle(&records, &request)) }
            });
            if let Err(error) = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                eprintln!("the connection failed: {error}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_summary_counts_what_its_record_carries() {
        let taken = summary(&record(TAKEN_RUN, "schedule", "a", "b", true));
        assert_eq!(taken["revalidation_findings"], 1);
        assert_eq!(taken["activated"], 1);
        let quiet = summary(&record(QUIET_RUN, "schedule", "a", "b", false));
        assert_eq!(quiet["revalidation_findings"], 0);
    }

    #[test]
    fn a_run_now_is_listed_first() {
        let records = Records::seeded();
        let added = records.add("manual");
        let listed = records.summaries();
        assert_eq!(listed[0]["id"], added["id"]);
        assert_eq!(listed[0]["trigger"], "manual");
    }
}
