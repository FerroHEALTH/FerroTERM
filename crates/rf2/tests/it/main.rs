// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: Apache-2.0

//! The one integration-test binary for `rf2`; one module per topic.
//!
//! Every fixture is synthetic: identifiers are minted with valid check digits
//! in an invented namespace, and terms are invented. Nothing here comes from
//! a licensed release (`.claude/rules/vendored-inputs.md`).
#![allow(
    clippy::panic_in_result_fn,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::too_many_lines,
    reason = "test assertions and one long synthetic fixture"
)]

mod characteristic_type;
mod fixture;
mod local_edition;
mod release;
