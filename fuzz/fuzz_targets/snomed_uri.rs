// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Arbitrary text as a SNOMED CT implicit value set or concept map URI.
//!
//! `?fhir_vs`, `?fhir_vs=isa/[sctid]`, `?fhir_vs=ecl/[ecl]`, `?fhir_vs=refset`,
//! `?fhir_vs=refset/[sctid]`, and `?fhir_cm=[sctid]`, on the bare system URI or
//! on an edition or version URI: an unknown form is an `OperationOutcome`,
//! never a 500 (`.claude/rules/snomed-terminology.md` [S-IMP-1]).
#![no_main]

use std::sync::{Arc, LazyLock};

use fhir_terminology::provider::MapSelection;
use fhir_terminology::registry::Registry;
use fhir_terminology::snomed::SnomedProvider;
use libfuzzer_sys::fuzz_target;

/// The synthetic edition, opened once for the whole run: the target
/// exercises the URI grammar, not the store behind it.
static EDITION: LazyLock<(tempfile::TempDir, Registry)> = LazyLock::new(|| {
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::snomed::write(dir.path()).expect("writes the edition");
    let provider = SnomedProvider::open(dir.path(), "en").expect("opens");
    let mut registry = Registry::new();
    registry.register(Arc::new(provider)).expect("registers");
    (dir, registry)
});

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let (_dir, registry) = &*EDITION;
    let _value_set = registry.implicit_value_set(text);
    let _concept_map = registry.implicit_concept_map(text, MapSelection::Whole);
});
