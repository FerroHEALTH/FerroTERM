// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Arbitrary text as a FHIR XML body.
//!
//! The XML wire is the second body form a client can send, and it has its own
//! reader (`fhir_types::xml::from_xml`) over the generated schemas.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _object = fhir_types::xml::from_xml(&fhir_types::r4b::schema::SCHEMAS, text);
    }
});
