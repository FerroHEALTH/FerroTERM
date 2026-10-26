// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The JSON log format of `ferrohealth-telemetry` carries the startup and
//! request fields a pipeline reads, the system and code among them.

use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use ferrohealth_telemetry::console;
use ferrohealth_telemetry::section::Rendering;
use http::Request;
use serde_json::Value;
use tower::ServiceExt;
use tracing::Subscriber;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;

use crate::fixture::Server;

/// A writer into a shared buffer, for reading the lines back.
#[derive(Clone, Default)]
pub(crate) struct Capture(Arc<Mutex<Vec<u8>>>);

pub(crate) struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Capture {
    type Writer = CaptureWriter;

    fn make_writer(&'a self) -> CaptureWriter {
        CaptureWriter(Arc::clone(&self.0))
    }
}

impl Capture {
    pub(crate) fn lines(&self) -> Vec<Value> {
        let bytes = self.0.lock().expect("lock").clone();
        String::from_utf8(bytes)
            .expect("utf-8")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("not JSON: {line}")))
            .collect()
    }
}

/// The console of `ferrohealth-telemetry` in `rendering` with `filter`,
/// writing through `writer`, the subscriber the binary installs.
pub(crate) fn subscriber<W>(
    rendering: Rendering,
    filter: &str,
    ansi: bool,
    writer: W,
) -> impl Subscriber + Send + Sync
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    tracing_subscriber::registry()
        .with(EnvFilter::new(filter))
        .with(console::layer(rendering, ansi, writer))
}

/// Keeps the process-global maximum level permissive for the whole test binary.
///
/// `tracing::subscriber::set_default` installs a subscriber for one thread, but
/// the maximum level a macro checks before it consults that subscriber is
/// process-global. `cargo test` runs a binary's cases as threads in one
/// process, so the sibling cases driving the router recompute that maximum
/// while a telemetry case holds its capture, and the event the case is about to
/// assert on is filtered before it reaches the capture. Registering one global
/// subscriber that admits everything pins the maximum open; the thread-local
/// capture still decides what is recorded (#293).
pub(crate) fn admit_every_level() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let permissive = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(std::io::sink)
            .finish();
        // Another case may have set one first; either way the level is open.
        let already_set: Result<(), _> = tracing::subscriber::set_global_default(permissive);
        drop(already_set);
    });
}

// The two formats are one case, so only one capture guard exists at a time.
#[tokio::test]
async fn the_json_and_pretty_formats_carry_what_each_promises() {
    admit_every_level();
    let capture = Capture::default();
    let json = subscriber(Rendering::Json, "info,hyper=warn", false, capture.clone());
    let guard = tracing::subscriber::set_default(json);

    tracing::info!(
        listen = "127.0.0.1:8080",
        code_systems = 1_u64,
        "ferroterm starting"
    );
    let server = Server::start();
    let request =
        Request::get("/r4b/CodeSystem/$lookup?system=http://snomed.info/sct&code=1&display=x")
            .body(Body::empty())
            .expect("request");
    let response = server.router().oneshot(request).await.expect("response");
    assert!(response.status().is_client_error());

    let lines = capture.lines();
    let start = lines
        .iter()
        .find(|l| l["message"] == "ferroterm starting")
        .expect("startup line");
    assert_eq!(start["level"], "INFO");
    assert_eq!(start["listen"], "127.0.0.1:8080");
    assert_eq!(start["code_systems"], 1);
    assert!(start["timestamp"].as_str().is_some());
    let request_line = lines
        .iter()
        .find(|l| l["message"] == "request")
        .expect("request line");
    assert_eq!(request_line["level"], "WARN", "a client error logs at warn");
    assert_eq!(request_line["method"], "GET");
    assert_eq!(request_line["route"], "/r4b/CodeSystem/$lookup");
    assert_eq!(request_line["status"], 400);
    assert!(request_line["latency_ms"].as_f64().is_some());
    assert_eq!(request_line["system"], "http://snomed.info/sct");
    assert_eq!(request_line["code"], "1");
    assert_eq!(request_line["client_named"], false);
    assert!(
        request_line["request_id"]
            .as_str()
            .is_some_and(|id| id.len() == 36),
        "the server's own id: {request_line}"
    );
    assert!(
        request_line.get("display").is_none(),
        "free-text parameters are not logged"
    );
    drop(guard);

    // The pretty format keeps one event on one line, so a line-oriented
    // collector reads it as one record.
    let capture = Capture::default();
    let pretty = subscriber(Rendering::Pretty, "info", false, capture.clone());
    let guard = tracing::subscriber::set_default(pretty);
    tracing::info!(value = "two\nlines", "one event");
    let bytes = capture.0.lock().expect("lock").clone();
    let text = String::from_utf8(bytes).expect("utf-8");
    assert_eq!(text.lines().count(), 1, "the interior line feed is escaped");
    assert!(text.contains("two\\nlines"));
    assert!(!text.contains("\u{1b}["), "no colour when ansi is off");
    drop(guard);
}
