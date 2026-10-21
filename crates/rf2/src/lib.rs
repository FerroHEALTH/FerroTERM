// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: Apache-2.0

//! SNOMED CT RF2 loading and the typed component model.
//!
//! Streams the RF2 release files (concepts, descriptions, relationships,
//! concrete-value relationships, alternate identifiers, and every reference
//! set) into typed rows keyed by distinct identifier newtypes, following the
//! release file specification
//! (<https://docs.snomed.org/snomed-ct-specifications/release-file-specification>).
//! Everything downstream (the graph, the store, the text index) is built from
//! this crate's output by `ferroterm-build`, offline, once per edition.
#![doc(test(attr(deny(warnings))))]

pub mod component;
pub mod constants;
pub mod edition;
pub mod file;
pub mod id;
pub mod reader;
pub mod refset;
pub mod time;
