// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology engine.
//!
//! Implements the FHIR terminology operations (`$lookup`, `$validate-code`,
//! `$expand`, `$subsumes`, `$translate`) over the code system provider seam,
//! dispatched per FHIR version against the generated `fhir-types` contracts.
//! SNOMED CT is the first provider; every other code system reaches the
//! operations through the same seam. No specification governs the seam
//! itself (our own design); the FHIR `CodeSystem` metadata is the capability
//! declaration a provider returns
//! (<https://hl7.org/fhir/R4B/codesystem.html>).
#![doc(test(attr(deny(warnings))))]

pub mod artifact;
pub mod capabilities;
pub mod classification;
pub mod clock;
pub mod compose;
pub mod conceptmap;
pub mod fhir_codesystem;
pub mod fhir_core;
pub mod filter;
pub mod icd11;
pub mod language;
pub mod loinc;
pub mod normal_form;
pub mod operations;
pub mod position;
pub mod provider;
pub mod registries;
pub mod registry;
pub mod rxnorm;
pub mod snomed;
pub mod supplement;
pub mod text_match;
pub mod valueset;
pub mod versioned;
