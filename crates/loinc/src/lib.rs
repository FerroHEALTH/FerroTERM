// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! LOINC release loading and the typed row model.
//!
//! Reads a LOINC release (the unpacked `Loinc_<version>` directory) into typed
//! rows: the term table, the parts and the multiaxial hierarchy, the answer
//! lists, and the linguistic variants, each file located by name and read by
//! column name (<https://loinc.org/kb/users-guide/loinc-database-structure/>).
//! `ferroterm-build` turns this crate's output into the served artifacts.
#![doc(test(attr(deny(warnings))))]

pub mod answer;
pub mod id;
pub mod part;
pub mod release;
pub mod term;
pub mod variant;
