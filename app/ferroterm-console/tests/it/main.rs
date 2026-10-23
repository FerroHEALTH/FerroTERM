// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one integration-test binary for `ferroterm-console`; one module per
//! topic.
//!
//! The sign-in tests drive the console's router with
//! `tower::ServiceExt::oneshot` against a `wiremock` OpenID Provider whose
//! signing key is generated per run, so no private key is committed.
#![allow(
    clippy::panic_in_result_fn,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test assertions"
)]

mod exchange;
mod logout;
mod provider;
mod refresh;
mod sign_in;
