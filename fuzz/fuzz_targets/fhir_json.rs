// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Arbitrary bytes as a FHIR JSON body.
//!
//! Every resource a client can POST is decoded here: a malformed body is a
//! `DecodeError` the server renders as an `OperationOutcome`, never a panic
//! (<https://hl7.org/fhir/R4B/operationoutcome.html>).
#![no_main]

use fhir_types::codec::{Json, Path};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(value) = serde_json::from_slice::<fhir_types::codec::Value>(data) else {
        return;
    };
    let Some(object) = value.as_object() else {
        return;
    };
    let _parameters =
        fhir_types::r4b::parameters::Parameters::from_json(object, &mut Path::root("Parameters"));
    let _value_set =
        fhir_types::r4b::value_set::ValueSet::from_json(object, &mut Path::root("ValueSet"));
    let _code_system =
        fhir_types::r4b::code_system::CodeSystem::from_json(object, &mut Path::root("CodeSystem"));
    let _concept_map =
        fhir_types::r4b::concept_map::ConceptMap::from_json(object, &mut Path::root("ConceptMap"));
});
