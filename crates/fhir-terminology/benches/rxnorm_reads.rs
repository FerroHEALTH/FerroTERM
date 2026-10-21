// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Read-path latency over the local `RxNorm` artifact (`artifacts/rxnorm`), when
//! present; the bar is one millisecond per read (`bench/bars.json`). Skips
//! silently without the artifact.
//!
//! `$lookup` costs more here than on any other system (#304), so the group
//! measures each step the operation takes rather than the operation alone.
#![allow(
    clippy::expect_used,
    clippy::print_stderr,
    reason = "a benchmark harness fails loud and reports to stderr, like a test binary"
)]

use std::path::PathBuf;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use fhir_terminology::operations::{Invocation, lookup, validate_code};
use fhir_terminology::provider::CodeSystemProvider;
use fhir_terminology::registry::Registry;
use fhir_terminology::rxnorm::RxNormProvider;

const SYSTEM: &str = "http://www.nlm.nih.gov/research/umls/rxnorm";
/// `313782`, the RXCUI the committed records measure.
const CODE: &str = "313782";

fn artifact() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/rxnorm");
    dir.join(fhir_terminology::artifact::MANIFEST_FILE)
        .exists()
        .then_some(dir)
}

fn reads(c: &mut Criterion) {
    let Some(dir) = artifact() else {
        eprintln!("no artifacts/rxnorm: skipping the RxNorm read benchmarks");
        return;
    };
    let provider = RxNormProvider::open(&dir).expect("the RxNorm artifact opens");
    let concept = provider
        .locate(CODE)
        .expect("reads")
        .expect("the code is in the release")
        .concept;

    let mut group = c.benchmark_group("rxnorm");
    group.bench_function("locate", |b| {
        b.iter(|| provider.locate(CODE).expect("reads"));
    });
    group.bench_function("display", |b| {
        b.iter(|| provider.display(concept, None).expect("reads"));
    });
    group.bench_function("designations", |b| {
        b.iter(|| provider.designations(concept, None).expect("reads"));
    });
    group.bench_function("properties", |b| {
        b.iter(|| provider.properties(concept).expect("reads"));
    });
    group.bench_function("status", |b| {
        b.iter(|| provider.status(concept).expect("reads"));
    });
    group.bench_function("search_metformin", |b| {
        b.iter(|| provider.search("metformin", None).expect("reads"));
    });
    group.finish();

    let mut registry = Registry::new();
    registry.register(Arc::new(provider)).expect("registers");
    let mut group = c.benchmark_group("rxnorm_operations");
    let lookup_request = lookup::LookupInput {
        system: Some(SYSTEM.to_owned()),
        code: Some(CODE.to_owned()),
        ..lookup::LookupInput::default()
    };
    group.bench_function("lookup", |b| {
        b.iter(|| lookup::lookup(&registry, &Invocation::Type, &lookup_request).expect("looks up"));
    });
    let validate_request = validate_code::ValidateCodeInput {
        url: Some(SYSTEM.to_owned()),
        code: Some(CODE.to_owned()),
        ..validate_code::ValidateCodeInput::default()
    };
    group.bench_function("validate_code", |b| {
        b.iter(|| {
            validate_code::validate_code(&registry, &Invocation::Type, &validate_request)
                .expect("validates")
        });
    });
    group.finish();
}

criterion_group!(benches, reads);
criterion_main!(benches);
