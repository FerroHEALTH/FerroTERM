// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The usage report to `FerroPULSE`: the rendered payloads, the window fed by
//! the router, and the start and daily sends to a mock collector.
//!
//! No FHIR or SNOMED CT specification governs the report: our own design,
//! held to `FerroPULSE`'s report envelope v2. The tests pin the exact JSON
//! shape the contract names.

use std::time::Instant;

use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use ferroterm_server::usage_report::config::{Deployment, UsageReportConfig};
use ferroterm_server::usage_report::reporter::{Prepared, Reporter, StartOutcome, prepare};
use ferroterm_server::usage_report::state::LAST_START_FILE;
use ferroterm_testkit::snomed::{CAT, item, sctid};

use crate::fixture::Server;

/// A report configuration over `state` posting to `endpoint`.
fn config(state: &std::path::Path, endpoint: &str) -> UsageReportConfig {
    UsageReportConfig {
        endpoint: endpoint.to_owned(),
        state_dir: Some(state.to_path_buf()),
        deployment: Deployment::Compose,
        ..UsageReportConfig::default()
    }
}

fn prepared(config: &UsageReportConfig) -> Prepared {
    prepare(config).unwrap_or_else(|off| panic!("the report runs: {off:?}"))
}

/// The vendored `FerroPULSE` fixtures, `scripts/vendor/ferropulse-schemas.sh`.
fn fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ferropulse/schemas")
}

fn read_json(path: &std::path::Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The report envelope v2 validator.
fn validator() -> jsonschema::Validator {
    let schema = read_json(&fixtures().join("report-envelope.v2.json"));
    jsonschema::validator_for(&schema).expect("the schema compiles")
}

/// The examples of one validity, by file name.
fn examples(validity: &str) -> Vec<(String, Value)> {
    let dir = fixtures()
        .join("examples/report-envelope.v2")
        .join(validity);
    let mut out: Vec<(String, Value)> = std::fs::read_dir(&dir)
        .expect("the examples are vendored")
        .map(|entry| {
            let path = entry.expect("an entry").path();
            let name = path
                .file_name()
                .expect("a name")
                .to_string_lossy()
                .into_owned();
            (name, read_json(&path))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Fails unless `report` is valid against the vendored schema.
fn assert_valid(report: &Value) {
    let errors: Vec<String> = validator()
        .iter_errors(report)
        .map(|error| format!("{error} at {}", error.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{errors:?}\n{report:#}");
}

/// The member names of an object, sorted.
fn keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

/// `report` with the members that depend on the host or the build replaced
/// by a placeholder, after checking it against the schema and each member
/// against the contract's form.
fn stable(mut report: Value) -> Value {
    assert_valid(&report);
    let object = report.as_object_mut().expect("an object");
    let id = object["instance_id"].as_str().expect("a string");
    assert!(uuid::Uuid::parse_str(id).is_ok(), "{id}");
    assert_eq!(id.len(), 36, "hyphenated: {id}");
    let sha = object["git_sha"].as_str().expect("a string");
    assert!(
        (7..=40).contains(&sha.len())
            && sha
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "{sha}"
    );
    assert!(
        object["uptime_s"].as_u64().is_some(),
        "an uptime in seconds"
    );
    assert!(
        ["1-2", "3-4", "5-8", "9-16", "17+"]
            .contains(&object["cpu_bucket"].as_str().expect("a string")),
        "a cpu bucket"
    );
    assert!(
        ["<4", "4-8", "8-16", "16-32", "32+"]
            .contains(&object["memory_bucket"].as_str().expect("a string")),
        "a memory bucket"
    );
    for member in [
        "instance_id",
        "git_sha",
        "uptime_s",
        "cpu_bucket",
        "memory_bucket",
    ] {
        object.insert(member.to_owned(), json!("*"));
    }
    report
}

#[test]
fn the_start_report_is_exactly_the_v2_envelope() {
    let server = Server::start_with_every_loader();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = config(dir.path(), "https://report.ferropulse.eu/v1/report");
    let reporter = Reporter::new(
        &config,
        prepared(&config),
        server.serving.clone(),
        Instant::now(),
    )
    .expect("builds");
    let report = serde_json::to_value(reporter.start_report()).expect("serializes");
    assert_eq!(
        stable(report),
        json!({
            "schema": 2,
            "event": "start",
            "first_start": true,
            "product": "ferroterm",
            "instance_id": "*",
            "version": env!("CARGO_PKG_VERSION"),
            "git_sha": "*",
            "licence": "non-commercial",
            "deployment": "compose",
            "uptime_s": "*",
            "cpu_bucket": "*",
            "memory_bucket": "*",
            "status": {
                "index_ok": true,
                "code_systems": ["fhir", "loinc", "rxnorm", "snomed"],
                "persistence": false
            }
        })
    );
}

/// The groups the daily test's requests fill: two lookups, one each of
/// metadata, search, and other, and no group without a request.
fn assert_recorded_groups(groups: &Value) {
    let histogram: Vec<u64> = groups
        .pointer("/lookup/histogram")
        .and_then(Value::as_array)
        .expect("an array")
        .iter()
        .map(|count| count.as_u64().expect("a count"))
        .collect();
    assert_eq!(histogram.len(), 11, "eleven buckets");
    assert_eq!(histogram.iter().sum::<u64>(), 2, "two lookups");
    for (group, total) in [("metadata", 1), ("search", 1), ("other", 1)] {
        let counts = groups
            .pointer(&format!("/{group}/histogram"))
            .and_then(Value::as_array)
            .expect("an array");
        assert_eq!(
            counts.iter().filter_map(Value::as_u64).sum::<u64>(),
            total,
            "{group}"
        );
    }
    assert_eq!(
        keys(groups),
        ["lookup", "metadata", "other", "search"],
        "a group without requests is left out"
    );
    for group in groups.as_object().expect("an object").values() {
        assert_eq!(
            keys(group),
            ["errors_5xx", "histogram", "p50_ms", "p95_ms", "p99_ms"],
            "the members of a group"
        );
    }
}

#[tokio::test]
async fn the_daily_report_carries_the_window_the_router_recorded() {
    let server = Server::start_persisting();
    let code = sctid(item(CAT));
    for uri in [
        format!("/r4b/CodeSystem/$lookup?system=http://snomed.info/sct&code={code}"),
        format!("/r4b/CodeSystem/$lookup?system=http://snomed.info/sct&code={code}"),
        String::from("/r4/metadata"),
        String::from("/r5/ValueSet"),
        String::from("/r4b/Nowhere/at-all"),
        String::from("/health"),
        String::from("/metrics"),
    ] {
        let request = http::Request::get(uri.as_str())
            .body(axum::body::Body::empty())
            .expect("request");
        let status = server.send(request).await.status();
        assert!(!status.is_server_error(), "{uri}: {status}");
    }
    let collector = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/report"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&collector)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = config(dir.path(), &format!("{}/v1/report", collector.uri()));
    let reporter = Reporter::new(
        &config,
        prepared(&config),
        server.serving.clone(),
        Instant::now(),
    )
    .expect("builds");
    let status = reporter.send_daily().await.expect("sends");
    assert_eq!(status, http::StatusCode::NO_CONTENT);
    let requests = collector.received_requests().await.expect("recorded");
    let [request] = requests.as_slice() else {
        panic!("one report, got {}", requests.len());
    };
    let body: Value = serde_json::from_slice(&request.body).expect("json");
    let groups = body["metrics"]["window_24h"]["groups"].clone();
    assert_recorded_groups(&groups);
    assert_eq!(
        stable(body),
        json!({
            "schema": 2,
            "event": "daily",
            "product": "ferroterm",
            "instance_id": "*",
            "version": env!("CARGO_PKG_VERSION"),
            "git_sha": "*",
            "licence": "non-commercial",
            "deployment": "compose",
            "uptime_s": "*",
            "cpu_bucket": "*",
            "memory_bucket": "*",
            "status": {
                "index_ok": true,
                "code_systems": ["fhir", "snomed"],
                "persistence": true
            },
            "metrics": {
                "window_24h": {
                    "requests_bucket": "1-100",
                    "groups": groups,
                    "counters": { "fhir_r4": 1, "fhir_r4b": 3, "fhir_r5": 1, "fhir_r6": 0 }
                }
            }
        })
    );
    let again = reporter.send_daily().await.expect("sends");
    assert_eq!(again, http::StatusCode::NO_CONTENT);
    let requests = collector.received_requests().await.expect("recorded");
    let second: Value =
        serde_json::from_slice(&requests.get(1).expect("a second report").body).expect("json");
    assert_eq!(
        second["metrics"]["window_24h"],
        json!({
            "requests_bucket": "0",
            "groups": {},
            "counters": { "fhir_r4": 0, "fhir_r4b": 0, "fhir_r5": 0, "fhir_r6": 0 }
        }),
        "a sent window is not sent again"
    );
}

#[tokio::test]
async fn a_daily_report_that_does_not_reach_the_collector_keeps_its_window() {
    let server = Server::start();
    let (status, _) = server.get("/r4b/metadata").await;
    assert!(status.is_success());
    let dir = tempfile::tempdir().expect("tempdir");
    let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
    let address = closed.local_addr().expect("an address");
    drop(closed);
    let unreachable = config(dir.path(), &format!("http://{address}/v1/report"));
    let reporter = Reporter::new(
        &unreachable,
        prepared(&unreachable),
        server.serving.clone(),
        Instant::now(),
    )
    .expect("builds");
    assert!(reporter.send_daily().await.is_err());
    let collector = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&collector)
        .await;
    let reachable = config(dir.path(), &format!("{}/v1/report", collector.uri()));
    let reporter = Reporter::new(
        &reachable,
        prepared(&reachable),
        server.serving.clone(),
        Instant::now(),
    )
    .expect("builds");
    assert_eq!(
        reporter.send_daily().await.expect("answered"),
        http::StatusCode::BAD_REQUEST
    );
    let requests = collector.received_requests().await.expect("recorded");
    let body: Value =
        serde_json::from_slice(&requests.first().expect("a report").body).expect("json");
    assert_eq!(
        body["metrics"]["window_24h"]["counters"]["fhir_r4b"], 1,
        "the failed window is carried over"
    );
}

#[tokio::test]
async fn a_start_report_is_sent_first_true_then_false_and_skipped_within_ten_minutes() {
    let server = Server::start();
    let collector = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/report"))
        .and(header("content-type", "application/json"))
        .and(header(
            "user-agent",
            format!("ferroterm/{}", env!("CARGO_PKG_VERSION")).as_str(),
        ))
        .respond_with(ResponseTemplate::new(204))
        .mount(&collector)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let state = dir.path().join("state");
    let config = config(&state, &format!("{}/v1/report", collector.uri()));

    let first = Reporter::new(
        &config,
        prepared(&config),
        server.serving.clone(),
        Instant::now(),
    )
    .expect("builds");
    assert_eq!(
        first.send_start().await.expect("sends"),
        StartOutcome::Sent(http::StatusCode::NO_CONTENT)
    );

    // A second start more than ten minutes later.
    std::fs::write(state.join(LAST_START_FILE), "2000-01-01T00:00:00Z\n").expect("writes");
    let second = Reporter::new(
        &config,
        prepared(&config),
        server.serving.clone(),
        Instant::now(),
    )
    .expect("builds");
    assert_eq!(
        second.send_start().await.expect("sends"),
        StartOutcome::Sent(http::StatusCode::NO_CONTENT)
    );

    // A third start at once: inside the ten minutes, nothing is sent.
    let third = Reporter::new(
        &config,
        prepared(&config),
        server.serving.clone(),
        Instant::now(),
    )
    .expect("builds");
    assert_eq!(
        third.send_start().await.expect("decides"),
        StartOutcome::RateLimited
    );

    let requests = collector.received_requests().await.expect("recorded");
    assert_eq!(requests.len(), 2, "the third start sent nothing");
    let bodies: Vec<Value> = requests
        .iter()
        .map(|request| serde_json::from_slice(&request.body).expect("json"))
        .collect();
    assert_eq!(bodies[0]["first_start"], true);
    assert_eq!(bodies[1]["first_start"], false);
    assert_eq!(
        bodies[0]["instance_id"], bodies[1]["instance_id"],
        "one installation"
    );
    assert!(bodies.iter().all(|body| body.get("metrics").is_none()));
}

#[test]
fn a_plain_http_collector_off_loopback_is_refused() {
    let server = Server::start();
    let dir = tempfile::tempdir().expect("tempdir");
    let config = config(dir.path(), "http://report.example/v1/report");
    assert!(
        Reporter::new(
            &config,
            prepared(&config),
            server.serving.clone(),
            Instant::now()
        )
        .is_err()
    );
}

#[test]
fn the_validator_admits_every_valid_example_and_refuses_every_invalid_one() {
    let validator = validator();
    let valid = examples("valid");
    let invalid = examples("invalid");
    assert!(
        !valid.is_empty() && !invalid.is_empty(),
        "the examples are vendored"
    );
    for (name, example) in &valid {
        assert!(validator.is_valid(example), "valid/{name} is refused");
    }
    for (name, example) in &invalid {
        assert!(!validator.is_valid(example), "invalid/{name} is admitted");
    }
}

#[tokio::test]
async fn every_rendered_report_has_the_shape_of_the_contract_examples() {
    let example = |name: &str| {
        examples("valid")
            .into_iter()
            .find(|(file, _)| file == name)
            .map_or_else(|| panic!("{name} is vendored"), |(_, value)| value)
    };
    let server = Server::start_with_every_loader();
    let code = sctid(item(CAT));
    let (status, _) = server
        .get(&format!(
            "/r4b/CodeSystem/$lookup?system=http://snomed.info/sct&code={code}"
        ))
        .await;
    assert!(status.is_success());
    let collector = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&collector)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = config(dir.path(), &format!("{}/v1/report", collector.uri()));
    let first = Reporter::new(
        &config,
        prepared(&config),
        server.serving.clone(),
        Instant::now(),
    )
    .expect("builds");
    first.send_start().await.expect("sends");
    first.send_daily().await.expect("sends");
    first.send_daily().await.expect("sends");
    std::fs::write(dir.path().join(LAST_START_FILE), "2000-01-01T00:00:00Z\n").expect("writes");
    let second = Reporter::new(
        &config,
        prepared(&config),
        server.serving.clone(),
        Instant::now(),
    )
    .expect("builds");
    second.send_start().await.expect("sends");
    let sent: Vec<Value> = collector
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .map(|request| serde_json::from_slice(&request.body).expect("json"))
        .collect();
    let [start, daily, idle, restart] = sent.as_slice() else {
        panic!("four reports, got {}", sent.len());
    };
    let group_members =
        keys(&example("ferroterm-daily.json")["metrics"]["window_24h"]["groups"]["lookup"]);
    for (ours, name) in [
        (start, "ferroterm-start.json"),
        (daily, "ferroterm-daily.json"),
        (idle, "ferroterm-daily-idle.json"),
        (restart, "ferroterm-restart.json"),
    ] {
        assert_valid(ours);
        let theirs = example(name);
        assert_eq!(keys(ours), keys(&theirs), "{name}");
        assert_eq!(keys(&ours["status"]), keys(&theirs["status"]), "{name}");
        assert_eq!(ours["event"], theirs["event"], "{name}");
        assert_eq!(ours.get("first_start"), theirs.get("first_start"), "{name}");
        let window = &theirs["metrics"]["window_24h"];
        if window.is_object() {
            let mine = &ours["metrics"]["window_24h"];
            assert_eq!(keys(mine), keys(window), "{name}");
            assert_eq!(keys(&mine["counters"]), keys(&window["counters"]), "{name}");
            for group in mine["groups"].as_object().expect("an object").values() {
                assert_eq!(keys(group), group_members, "{name}");
            }
        }
    }
    assert_eq!(
        idle["metrics"],
        example("ferroterm-daily-idle.json")["metrics"],
        "an idle window renders as the contract's idle example"
    );
}
