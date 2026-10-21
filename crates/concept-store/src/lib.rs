// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The disk-backed concept store.
//!
//! A `redb` database holding one code system version's concepts, displays,
//! designations, and property values, keyed by dense ordinal, that the
//! terminology operations read point-wise: `$lookup` and `$validate-code`
//! resolve a code here without touching the graph. Built offline by
//! `ferroterm-build`, opened read-only by the server. No spec governs the
//! layout: our own design (`docs/architecture.md` decision 3).
#![doc(test(attr(deny(warnings))))]

/// The manifest file name inside an artifact directory: the one place the
/// build tool writes it and every reader reads it from.
pub const MANIFEST_FILE: &str = "manifest.json";

pub mod builder;
pub mod column;
pub mod keys;
pub mod record;
pub mod store;
pub mod tables;
