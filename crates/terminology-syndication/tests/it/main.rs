// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `syndication` integration suite.
//!
//! One binary, one module per topic
//! (<https://doc.rust-lang.org/cargo/reference/cargo-targets.html>).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::indexing_slicing,
    reason = "test assertions"
)]

mod corpus;
mod digest;
mod download;
mod fhir_api;
mod fixtures;
mod parse;
mod reference;
mod select;
mod source;
