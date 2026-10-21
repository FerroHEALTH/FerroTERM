// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one integration-test binary for `fhir-terminology`; one module per topic.
//!
//! Every test drives the seam through a synthetic in-memory provider
//! (`fixture`); nothing here knows a real code system.
#![allow(
    clippy::panic_in_result_fn,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test assertions"
)]

mod capabilities;
mod classification;
mod compose;
mod concept_map;
mod concept_status;
mod ecl;
mod fhir_codesystem;
mod fhir_core;
mod filter;
mod fixture;
mod icd11;
mod implicit_forms;
mod labcodeset;
mod layering;
mod loinc;
mod operations;
mod registries;
mod registry;
mod rxnorm;
mod scaled;
mod snomed;
mod snomed_concept_map;
mod snomed_editions;
mod snomed_expression;
mod supplement;
mod ucum;
mod value_set;
