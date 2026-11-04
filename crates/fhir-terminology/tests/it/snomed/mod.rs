// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SNOMED CT provider through the seam, over the synthetic artifact.

use fhir_terminology::snomed::SnomedProvider;

use ferroterm_testkit::snomed;

mod concepts;
mod grammar;
mod implicit;

fn provider() -> (tempfile::TempDir, SnomedProvider) {
    let dir = tempfile::tempdir().expect("tempdir");
    snomed::write(dir.path()).expect("writes the fixture");
    let provider = SnomedProvider::open(dir.path(), "en").expect("opens");
    (dir, provider)
}
