// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The concept browser's tests.

use super::State;
use crate::fhir::version::FhirVersion;

#[test]
fn a_toggle_moves_only_its_level_and_keeps_the_concept() {
    let state = State {
        system: String::from("http://x/cs"),
        code: String::from("a"),
        ..State::default()
    };
    let opened = state.toggling("b", true);
    assert!(opened.open.contains("b"));
    assert_eq!("a", opened.code);
    assert!(!opened.toggling("b", false).open.contains("b"));
    let href = opened.href(FhirVersion::R4);
    assert!(href.contains("system=http%3A%2F%2Fx%2Fcs"), "{href}");
    assert!(href.contains("code=a"), "{href}");
    assert!(href.contains("open=b"), "{href}");
}
