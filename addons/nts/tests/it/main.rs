// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `addon-nts` integration suite.
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

mod listing;
mod resource;
mod support;
mod tokens;
