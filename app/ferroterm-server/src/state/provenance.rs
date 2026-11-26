// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `Provenance` resources a deployment loads beside its terminology.
//!
//! A `Provenance` records who produced or validated a resource and when
//! (<https://hl7.org/fhir/R4B/provenance.html>). The draft `EEHRxF` implementing
//! act Art 6 admits a national code only with a validated mapping whose
//! versions and provenance are recorded, so a `ConceptMap`'s provenance is
//! kept from the same package directories and read by the canonical its
//! `target` names.

use fhir_types::codec::{Object, Value};

/// The `target.reference` values of a `Provenance` resource.
pub fn targets(object: &Object) -> impl Iterator<Item = &str> {
    object
        .get("target")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|target| target.get("reference").and_then(Value::as_str))
}
