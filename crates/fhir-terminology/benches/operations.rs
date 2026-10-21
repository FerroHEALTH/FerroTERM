// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The four served read operations over a synthetic edition, so the latency
//! bar is measured on any machine and in CI, not only where a licensed
//! edition sits.
//!
//! The bar is one millisecond per point read and ten milliseconds for a page
//! of an expansion (`docs/architecture.md`); `scripts/checks/bench-bars.sh`
//! reads the medians these benches record and fails when one crosses its bar.
//! The edition is generated content in an invented namespace and holds no
//! SNOMED CT content.
#![allow(
    clippy::expect_used,
    clippy::print_stderr,
    reason = "a benchmark harness fails loud and reports to stderr, like a test binary"
)]

use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use fhir_terminology::compose::{Expander, Options};
use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::operations::{
    Invocation, Sources, lookup, subsumes, translate, validate_code,
};
use fhir_terminology::registry::Registry;
use fhir_terminology::snomed::SnomedProvider;
use fhir_terminology::valueset::store::ValueSetStore;

const SCT: &str = "http://snomed.info/sct";
/// The edition the benches build. Large enough that a point read measures the
/// index rather than a cache line, small enough to build in a few seconds.
const CONCEPTS: u32 = 20_000;
/// A concept deep in the tree, so a read is never the first ordinal.
const DEEP: u32 = 17_777;

fn edition() -> (tempfile::TempDir, SnomedProvider) {
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::scaled::write(dir.path(), CONCEPTS).expect("writes the edition");
    let provider = SnomedProvider::open(dir.path(), "en").expect("the edition opens");
    (dir, provider)
}

fn operations(c: &mut Criterion) {
    let (_dir, provider) = edition();
    let root = ferroterm_testkit::scaled::code(ferroterm_testkit::scaled::ROOT);
    let deep = ferroterm_testkit::scaled::code(DEEP);
    let mut registry = Registry::new();
    registry.register(Arc::new(provider)).expect("registers");

    let mut group = c.benchmark_group("operations");
    let lookup_request = lookup::LookupInput {
        system: Some(SCT.to_owned()),
        code: Some(deep.clone()),
        ..lookup::LookupInput::default()
    };
    group.bench_function("lookup", |b| {
        b.iter(|| lookup::lookup(&registry, &Invocation::Type, &lookup_request).expect("looks up"));
    });
    let validate_request = validate_code::ValidateCodeInput {
        url: Some(SCT.to_owned()),
        code: Some(deep.clone()),
        display: Some(format!("Synthetic concept {DEEP}")),
        ..validate_code::ValidateCodeInput::default()
    };
    group.bench_function("validate_code", |b| {
        b.iter(|| {
            validate_code::validate_code(&registry, &Invocation::Type, &validate_request)
                .expect("validates")
        });
    });
    // The edition's ICD-10 extended map states one mapping per concept, so a
    // translation that read the map whole would cost the release. It reads the
    // mappings of the code it was asked about, so it costs the answer (#369).
    let value_sets = ValueSetStore::new();
    let concept_maps = ConceptMapStore::new();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let translate_request = translate::TranslateInput {
        url: Some(format!(
            "{SCT}?fhir_cm={}",
            ferroterm_testkit::snomed::ICD10_MAP_SCTID
        )),
        system: Some(SCT.to_owned()),
        code: Some(deep.clone()),
        ..translate::TranslateInput::default()
    };
    group.bench_function("translate", |b| {
        b.iter(|| translate::translate(&sources, &translate_request).expect("translates"));
    });
    let subsumes_request = subsumes::SubsumesInput {
        system: Some(SCT.to_owned()),
        code_a: Some(root.clone()),
        code_b: Some(deep),
        ..subsumes::SubsumesInput::default()
    };
    group.bench_function("subsumes", |b| {
        b.iter(|| {
            subsumes::subsumes(&registry, &Invocation::Type, &subsumes_request).expect("subsumes")
        });
    });
    group.finish();

    // A page cut from the middle of the whole edition: the page comes off the
    // selection bitmaps before any concept is read, so the cost is the page,
    // not the set (.claude/rules/fhir-terminology.md, [F-EXP-1]).
    let mut group = c.benchmark_group("expand");
    let expander = Expander::new(&registry);
    let descendants = registry
        .implicit_value_set(&format!("{SCT}?fhir_vs=isa/{root}"))
        .expect("snomed claims the URI")
        .expect("well formed");
    let page = Options {
        count: Some(10),
        offset: 1000,
        ..Options::default()
    };
    group.bench_function("isa_root_page_10", |b| {
        b.iter(|| expander.expand(&descendants, &page).expect("expands"));
    });
    // The first page of the whole edition: ten items off the front of a set of
    // every concept. It costs the page, so its bar is the point-read bar, and a
    // walk that flattens the set before it cuts breaches it (#309).
    let first = Options {
        count: Some(10),
        offset: 0,
        ..Options::default()
    };
    group.bench_function("isa_root_first_10", |b| {
        b.iter(|| expander.expand(&descendants, &first).expect("expands"));
    });
    group.finish();
}

criterion_group!(benches, operations);
criterion_main!(benches);
