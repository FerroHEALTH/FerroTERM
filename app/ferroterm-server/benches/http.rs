// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The same four operations as `fhir-terminology`'s `operations` bench, this
//! time through the router: routing, content negotiation, the codec, and the
//! response body included.
//!
//! The engine's bar is one millisecond per point read
//! (`docs/architecture.md`); this bench says what the server adds on top of
//! it. `scripts/checks/bench-bars.sh` reads the medians and fails when one
//! crosses its bar. The edition is generated content in an invented namespace
//! and holds no SNOMED CT content.
#![allow(
    clippy::expect_used,
    clippy::print_stderr,
    reason = "a benchmark harness fails loud and reports to stderr, like a test binary"
)]

use std::sync::Arc;

use axum::body::Body;
use criterion::{Criterion, criterion_group, criterion_main};
use ferroterm_server::config::{Config, TerminologySection};
use ferroterm_server::reload::Serving;
use ferroterm_server::state::AppState;
use http::Request;
use tower::ServiceExt;

/// The edition the bench builds, the size the engine bench uses.
const CONCEPTS: u32 = 20_000;
/// A concept deep in the tree, so a read is never the first ordinal.
const DEEP: u32 = 17_777;

fn requests(c: &mut Criterion) {
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
    let root = ferroterm_testkit::scaled::code(ferroterm_testkit::scaled::ROOT);
    let deep = ferroterm_testkit::scaled::code(DEEP);
    let system = "http://snomed.info/sct";

    // The router is built once, as the binary builds it, and cloned per
    // request because `oneshot` consumes the service.
    let built = ferroterm_server::router(Serving::new(
        config.clone(),
        Arc::clone(&state),
        telemetry(),
    ));
    let answer = |uri: &str| {
        let router = built.clone();
        let request = Request::get(uri).body(Body::empty()).expect("request");
        runtime.block_on(async {
            let response = router.oneshot(request).await.expect("response");
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body")
        })
    };

    let lookup = format!("/r4b/CodeSystem/$lookup?system={system}&code={deep}");
    let validate = format!("/r4b/CodeSystem/$validate-code?url={system}&code={deep}");
    let subsumes = format!("/r4b/CodeSystem/$subsumes?system={system}&codeA={root}&codeB={deep}");
    let expand = format!(
        "/r4b/ValueSet/$expand?url=http%3A%2F%2Fsnomed.info%2Fsct%3Ffhir_vs%3Disa%2F{root}&count=10&offset=1000"
    );
    // The implicit concept map of the edition's ICD-10 extended map reference
    // set, which states one mapping per concept (#369).
    let translate = format!(
        "/r4b/ConceptMap/$translate?url=http%3A%2F%2Fsnomed.info%2Fsct%3Ffhir_cm%3D{}&system={system}&code={deep}",
        ferroterm_testkit::snomed::ICD10_MAP_SCTID
    );

    let mut group = c.benchmark_group("http");
    group.bench_function("lookup", |b| b.iter(|| answer(&lookup)));
    group.bench_function("validate_code", |b| b.iter(|| answer(&validate)));
    group.bench_function("subsumes", |b| b.iter(|| answer(&subsumes)));
    group.bench_function("expand_page_10", |b| b.iter(|| answer(&expand)));
    group.bench_function("translate", |b| b.iter(|| answer(&translate)));
    group.finish();
}

/// What a request costs before the operation starts: the `axum` floor, and
/// what this server's own layers add to it (#628).
///
/// `/health` answers a constant, so the difference between the two floors is
/// the request log, the metrics sample, and the request identifier, and
/// nothing of the terminology engine. The SMART gate sits inside each
/// version's router, so a third floor asks for a path under `/r4b` that no
/// route matches and pays the gate before the fallback answers.
fn fixed(c: &mut Criterion) {
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
    let bare = axum::Router::new().route("/health", axum::routing::get(async || "ok"));
    let served = ferroterm_server::router(Serving::new(
        config.clone(),
        Arc::clone(&state),
        telemetry(),
    ));
    let call = |router: axum::Router, uri: &str| {
        let request = Request::get(uri).body(Body::empty()).expect("request");
        runtime.block_on(async {
            let response = router.oneshot(request).await.expect("response");
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body")
        })
    };

    let mut group = c.benchmark_group("fixed");
    group.bench_function("axum_floor", |b| {
        b.iter(|| call(bare.clone(), "/health"));
    });
    group.bench_function("served_floor", |b| {
        b.iter(|| call(served.clone(), "/health"));
    });
    group.bench_function("gated_floor", |b| {
        b.iter(|| call(served.clone(), "/r4b/no-such-route"));
    });
    group.finish();
}

/// The same router over the local `RxNorm` and SNOMED artifacts, when they are
/// built, so the served figure a record reports has a bench behind it: a
/// record measures a socket too, this measures everything above it (#304).
fn local(c: &mut Criterion) {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts");
    let built: Vec<std::path::PathBuf> = ["rxnorm", "nl", "icd10nl"]
        .iter()
        .map(|name| root.join(name))
        .filter(|dir| dir.join(fhir_terminology::artifact::MANIFEST_FILE).exists())
        .collect();
    if built.is_empty() {
        eprintln!("no local artifacts: skipping the served read benchmarks");
        return;
    }
    let config = Config {
        terminology: TerminologySection {
            index: built,
            ..TerminologySection::default()
        },
        ..Config::default()
    };
    // An artifact of an older layout is as absent as a missing one, as far as
    // this bench is concerned: it skips rather than failing the bars run.
    let state = match AppState::load(&config) {
        Ok(state) => Arc::new(state),
        Err(error) => {
            eprintln!("the local artifacts do not open ({error}): skipping the served benchmarks");
            return;
        }
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let router = ferroterm_server::router(Serving::new(
        config.clone(),
        Arc::clone(&state),
        telemetry(),
    ));
    let answer = |uri: &str| {
        let router = router.clone();
        let request = Request::get(uri).body(Body::empty()).expect("request");
        runtime.block_on(async {
            let response = router.oneshot(request).await.expect("response");
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body")
        })
    };
    let mut group = c.benchmark_group("served");
    // The smallest answer any served system gives, so the three `$lookup`
    // benches of this group span 862 bytes to 63 KB and the fixed term of a
    // served read separates from its per-byte term in one process (#628).
    let icd10nl = "http%3A%2F%2Fhl7.org%2Ffhir%2Fsid%2Ficd-10-nl";
    group.bench_function("icd10nl_lookup", |b| {
        b.iter(|| {
            answer(&format!(
                "/r4b/CodeSystem/$lookup?system={icd10nl}&code=M95.3"
            ))
        });
    });
    let rxnorm = "http%3A%2F%2Fwww.nlm.nih.gov%2Fresearch%2Fumls%2Frxnorm";
    group.bench_function("rxnorm_lookup", |b| {
        b.iter(|| {
            answer(&format!(
                "/r4b/CodeSystem/$lookup?system={rxnorm}&code=313782"
            ))
        });
    });
    let sct = "http%3A%2F%2Fsnomed.info%2Fsct";
    group.bench_function("snomed_lookup", |b| {
        b.iter(|| {
            answer(&format!(
                "/r4b/CodeSystem/$lookup?system={sct}&code=404684003"
            ))
        });
    });
    // What writing the same body costs once the object exists, so the served
    // figure splits into building the object and serializing it (#304).
    let body = answer(&format!(
        "/r4b/CodeSystem/$lookup?system={rxnorm}&code=313782"
    ));
    let object: fhir_types::codec::Object = serde_json::from_slice(&body).expect("the body parses");
    group.bench_function("rxnorm_serialize", |b| {
        b.iter(|| serde_json::to_vec(&object).expect("serializes"));
    });
    // The same body from the typed resource, the path a response now takes, and
    // the document build it no longer pays for, so the served figure splits into
    // answering, building, and writing (#312).
    let typed: fhir_types::r4b::parameters::Parameters =
        serde_json::from_slice(&body).expect("the body decodes");
    group.bench_function("rxnorm_to_json", |b| {
        b.iter(|| fhir_types::codec::Json::to_json(&typed).expect("builds the document"));
    });
    group.bench_function("rxnorm_serialize_typed", |b| {
        b.iter(|| serde_json::to_vec(&typed).expect("serializes"));
    });
    // The two ways the same answer becomes a `Parameters`, timed in one process so
    // the ratio holds whatever else the machine is doing: the answer path moves its
    // values into the wire tree, and a caller that keeps its own copies first (#378).
    let answer =
        fhir_types::r4b::operations::code_system_lookup::CodeSystemLookupResponse::from_parameters(
            &typed,
        )
        .expect("the answer reads back");
    group.bench_function("rxnorm_answer_moved", |b| {
        b.iter_batched(
            || answer.clone(),
            fhir_types::r4b::operations::code_system_lookup::CodeSystemLookupResponse::into_parameters,
            criterion::BatchSize::SmallInput,
        );
    });
    group.bench_function("rxnorm_answer_copied", |b| {
        b.iter(|| answer.to_parameters());
    });
    group.finish();
}

criterion_group!(benches, requests, fixed, local);
criterion_main!(benches);

/// A telemetry of its own for the measured router.
fn telemetry() -> Arc<ferrohealth_telemetry::telemetry::Telemetry> {
    Arc::new(
        ferrohealth_telemetry::telemetry::Telemetry::new(
            &ferrohealth_telemetry::section::TelemetrySection::default(),
            ferrohealth_telemetry::telemetry::Service::new("ferroterm", "bench"),
        )
        .expect("the default telemetry builds"),
    )
}
