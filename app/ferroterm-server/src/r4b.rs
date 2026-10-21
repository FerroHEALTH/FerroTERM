// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FHIR R4B (4.3.0) under `/r4b`: the generated `fhir_types::r4b` contracts
//! over the engine (<https://hl7.org/fhir/R4B/terminology-service.html>).

crate::version::surface!(r4b, "4.3.0", "R4B", to_r4b, closure);

crate::version::map_r4::family_map!(r4b);
