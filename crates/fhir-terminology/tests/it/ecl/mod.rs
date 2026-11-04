// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ECL evaluator over the synthetic edition: one case per construct, with
//! the expected set from the ECL specification's definitions
//! (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-expression-constraint-language>,
//! Appendix D, the quick reference, unless a test cites another section).

use ferroterm_testkit::snomed::{SAME_AS_SCTID, item, sctid};
use fhir_terminology::snomed::SnomedProvider;
use sct_ecl::eval::{EvalError, evaluate};

mod constraints;
mod filters;

fn provider() -> (tempfile::TempDir, SnomedProvider) {
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::snomed::write(dir.path()).expect("writes");
    let provider = SnomedProvider::open(dir.path(), "en").expect("opens");
    (dir, provider)
}

/// The code of a fixture concept.
fn c(ordinal: u32) -> String {
    sctid(item(ordinal))
}

/// The published code of the SAME AS association reference set.
fn same_as() -> &'static str {
    SAME_AS_SCTID
}

fn set(provider: &SnomedProvider, text: &str) -> Vec<u32> {
    let tree = sct_ecl::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
    evaluate(provider, &tree)
        .unwrap_or_else(|e| panic!("{text}: {e}"))
        .iter()
        .collect()
}

fn refused(provider: &SnomedProvider, text: &str) -> EvalError {
    let tree = sct_ecl::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
    evaluate(provider, &tree).expect_err("refused")
}
