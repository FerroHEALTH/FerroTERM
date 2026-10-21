// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FHIR R5 (5.0.0) under `/r5`: the generated `fhir_types::r5` contracts
//! over the engine (<https://hl7.org/fhir/R5/terminology-service.html>).
//!
//! R5 declares shapes the R4 family does not (`$validate-code` `issues` and
//! the validated `code`, `system`, and `version`; `$lookup` `definition`;
//! `$expand` `property` and `useSupplement`; the renamed `$translate`
//! parameters and `relationship`), so its `map` is the R5 family's; the rest
//! of the surface is the shared per-version instantiation.

crate::version::map_r5::family_map!(r5, r5);
crate::version::surface!(r5, "5.0.0", "R5", to_r5, closure);
