// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The generic provider for any code system published as a FHIR `CodeSystem`
//! resource: HL7 Terminology, custom systems, and supplements.
//!
//! The FHIR `CodeSystem` contract is the authority
//! (<https://hl7.org/fhir/R4B/codesystem.html>): the resource's own `property`
//! and `filter` declarations, `caseSensitive`, `content`, `versionNeeded`,
//! `compositional`, `hierarchyMeaning` (subsumption only for `is-a`), the
//! standard concept properties (`inactive`, `status`, `deprecated`,
//! `notSelectable`, `parent`, `child`, <https://hl7.org/fhir/R4B/codesystem-concept-properties.html>),
//! and the generic filter operators. A resource of any served FHIR version
//! converts into one [`model::CodeSystemModel`], and one [`provider::FhirCodeSystem`]
//! serves it through the seam.

pub mod convert;
pub mod load;
pub mod model;
pub mod provider;
pub mod render;
