// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FHIR R4 (4.0.1) under `/r4`: the generated `fhir_types::r4` contracts
//! over the engine (<https://hl7.org/fhir/R4/terminology-service.html>).

crate::version::surface!(r4, "4.0.1", "R4", to_r4, closure);

crate::version::map_r4::family_map!(r4);
