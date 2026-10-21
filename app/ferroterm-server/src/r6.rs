// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR R6 ballot (6.0.0-ballot5) under `/r6`: the generated
//! `fhir_types::r6` contracts over the engine
//! (<https://hl7.org/fhir/6.0.0-ballot5/terminology-service.html>).
//!
//! The ballot shares R5's result shapes and renames or adds a few inputs
//! (`sourceSystem` and `sourceVersion` alone on `$translate`, a repeating
//! `displayLanguage` on `ValueSet/$validate-code`, `manifest`,
//! `filterProperty`, `handle-unclosed-expansion`), so its `map` is the R5
//! family's with the R6 arms. Every behaviour grounded only in the ballot is
//! marked as such in the capability statement and re-verified when R6
//! publishes.

crate::version::map_r5::family_map!(r6, r6);
crate::version::surface!(r6, "6.0.0-ballot5", "R6 ballot", to_r6);
