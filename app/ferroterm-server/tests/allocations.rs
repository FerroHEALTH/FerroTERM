// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The allocations one request of each hot operation makes through the
//! router, held to a committed budget (#1339).
//!
//! The A/B performance gate judges latency, CPU and memory under load against
//! the base of a change; this test holds a single request to a number, so a
//! creep inside the gate's tolerance still fails here. It is its own test
//! binary because it replaces the global allocator with `dhat`'s counting
//! one. No specification governs this: our own design.
#![allow(
    clippy::expect_used,
    clippy::print_stdout,
    reason = "a test fails loud and prints the counts it measured"
)]

use std::sync::Arc;

use axum::body::Body;
use ferroterm_server::config::{Config, TerminologySection};
use ferroterm_server::reload::Serving;
use ferroterm_server::state::AppState;
use http::{Request, StatusCode};
use tower::ServiceExt;

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

/// The edition the test builds, the size the HTTP bench uses.
const CONCEPTS: u32 = 20_000;
/// A concept deep in the tree, so a read is never the first ordinal.
const DEEP: u32 = 17_777;

/// The budget of one request: the allocations it makes, as measured, and the
/// bytes it allocates, rounded up to the next KiB because a rendered instant
/// or identifier varies by a few bytes.
struct Budget {
    /// The operation, as the report names it.
    name: &'static str,
    /// The request path and query.
    uri: String,
    /// The most allocations the request may make.
    blocks: u64,
    /// The most bytes the request may allocate in total.
    bytes: u64,
}

fn telemetry() -> Arc<ferrohealth_telemetry::telemetry::Telemetry> {
    Arc::new(
        ferrohealth_telemetry::telemetry::Telemetry::new(
            &ferrohealth_telemetry::section::TelemetrySection::default(),
            ferrohealth_telemetry::telemetry::Service::new("ferroterm", "allocations"),
        )
        .expect("the default telemetry builds"),
    )
}

#[test]
fn each_hot_request_stays_within_its_allocation_budget() {
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::scaled::write(dir.path(), CONCEPTS).expect("writes the edition");
    let config = Config {
        terminology: TerminologySection {
            index: vec![dir.path().to_path_buf()],
            ..TerminologySection::default()
        },
        ..Config::default()
    };
    let state = Arc::new(AppState::load(&config).expect("loads"));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let built = ferroterm_server::router(Serving::new(config, state, telemetry()));
    let root = ferroterm_testkit::scaled::code(ferroterm_testkit::scaled::ROOT);
    let deep = ferroterm_testkit::scaled::code(DEEP);
    let system = "http://snomed.info/sct";
    let budgets = [
        Budget {
            name: "lookup",
            uri: format!("/r4b/CodeSystem/$lookup?system={system}&code={deep}"),
            blocks: 268,
            bytes: 54_272,
        },
        Budget {
            name: "validate-code",
            uri: format!("/r4b/CodeSystem/$validate-code?url={system}&code={deep}"),
            blocks: 142,
            bytes: 23_552,
        },
        Budget {
            name: "subsumes",
            uri: format!("/r4b/CodeSystem/$subsumes?system={system}&codeA={root}&codeB={deep}"),
            blocks: 135,
            bytes: 27_648,
        },
        Budget {
            name: "expand",
            uri: format!(
                "/r4b/ValueSet/$expand?url=http%3A%2F%2Fsnomed.info%2Fsct%3Ffhir_vs%3Disa%2F{root}&count=10&offset=1000"
            ),
            blocks: 1298,
            bytes: 235_520,
        },
        Budget {
            name: "translate",
            uri: format!(
                "/r4b/ConceptMap/$translate?url=http%3A%2F%2Fsnomed.info%2Fsct%3Ffhir_cm%3D{}&system={system}&code={deep}",
                ferroterm_testkit::snomed::ICD10_MAP_SCTID
            ),
            blocks: 516,
            bytes: 137_216,
        },
    ];
    let answer = |uri: &str| {
        let router = built.clone();
        let request = Request::get(uri).body(Body::empty()).expect("request");
        runtime.block_on(async {
            let response = router.oneshot(request).await.expect("response");
            let status = response.status();
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body");
            (status, body)
        })
    };
    let _profiler = dhat::Profiler::builder().testing().build();
    let mut over = Vec::new();
    for budget in &budgets {
        // NOTE: the first request fills the caches a deployment fills once, so the
        // budget is the cost of every request after it.
        let (status, body) = answer(&budget.uri);
        assert_eq!(status, StatusCode::OK, "{}: {body:?}", budget.name);
        let before = dhat::HeapStats::get();
        let (status, body) = answer(&budget.uri);
        let after = dhat::HeapStats::get();
        drop(body);
        assert_eq!(status, StatusCode::OK, "{}", budget.name);
        let blocks = after.total_blocks.saturating_sub(before.total_blocks);
        let bytes = after.total_bytes.saturating_sub(before.total_bytes);
        println!("{}: {blocks} allocations, {bytes} bytes", budget.name);
        if blocks > budget.blocks || bytes > budget.bytes {
            over.push(format!(
                "{}: {blocks} allocations of {}, {bytes} bytes of {}",
                budget.name, budget.blocks, budget.bytes
            ));
        }
    }
    assert!(over.is_empty(), "over budget: {over:?}");
}
