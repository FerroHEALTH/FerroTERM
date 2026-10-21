// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Arbitrary text as the parts of an RF2 release.
//!
//! A release is client-supplied content: a deployment brings its own. A
//! malformed identifier, effective time, or file name is a typed error naming
//! the row, never a panic (RF2 release file specification).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _name = rf2::file::FileName::parse(text);
    let _time = rf2::time::EffectiveTime::parse(text);
    let _concept = rf2::id::ConceptId::parse(text);
    let _description = rf2::id::DescriptionId::parse(text);
    let _refset = rf2::id::RefsetId::parse(text);
});
