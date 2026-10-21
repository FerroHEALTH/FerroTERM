// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The materialized hierarchy.
//!
//! Integer-keyed compressed sparse row adjacency for a code system's is-a
//! hierarchy and each typed relationship, plus roaring transitive-closure
//! bitmaps. Subsumption is a bitmap membership test, and the ECL evaluator
//! compiles constraints to set algebra over these bitmaps. The graph is built
//! offline by a loader and served read-only; no request traverses edges live.
//! No FHIR or SNOMED specification governs the layout: our own design
//! (`docs/architecture.md` decisions 1 and 3).
#![doc(test(attr(deny(warnings))))]

pub mod attributes;
pub mod closure;
pub mod csr;
pub mod footprint;
pub mod identifiers;
pub mod members;
pub mod ordinal;
pub mod persist;
pub mod read;
pub mod refsets;
pub mod relations;
pub mod subsumption;
