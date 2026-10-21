// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Builds the licensed release under `data/`, when present.
//!
//! Ignored by default (`.claude/rules/vendored-inputs.md`). Run with
//! `cargo nextest run --release -p ferroterm-build --run-ignored all`; it
//! prints the build time and the artifact size.

use std::path::PathBuf;
use std::time::Instant;

use ferroterm_build::pipeline;

fn local_release() -> Option<PathBuf> {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/snomed");
    std::fs::read_dir(data)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.is_dir())
}

#[test]
#[ignore = "needs a licensed RF2 release under data/snomed/"]
fn the_local_edition_builds_end_to_end() {
    let Some(root) = local_release() else {
        panic!("no release under data/snomed/");
    };
    let out = tempfile::tempdir().expect("tempdir");
    let started = Instant::now();
    let report = pipeline::build(&root, &[], out.path()).expect("builds");
    let built = started.elapsed();
    let size = std::fs::metadata(&report.store).expect("metadata").len();
    println!(
        "{} : concepts {}, designations {}, is-a edges {}, words {}, store {size} bytes, build {built:?}",
        report.version_uri, report.concepts, report.designations, report.is_a_edges, report.words
    );
    assert!(report.version_uri.starts_with("http://snomed.info/sct/"));
    assert!(report.concepts > 0);
}
