// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Arbitrary text as an expression constraint.
//!
//! The parser answers a typed error or an AST. A panic, an arithmetic
//! overflow, or a hang is a defect: malformed ECL is an `OperationOutcome`,
//! never a 500 (`.claude/rules/spec-adherence.md` [S-ECL-4]).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _parsed = sct_ecl::parse(text);
    }
});
