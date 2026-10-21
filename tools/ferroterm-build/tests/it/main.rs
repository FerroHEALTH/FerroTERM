// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one integration-test binary for `ferroterm-build`; one module per topic.
#![allow(
    clippy::panic_in_result_fn,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test assertions"
)]

mod archive;
mod classification;
mod dhd;
mod fixture;
mod gstandaard;
mod icd11;
mod labcodeset;
mod layered;
mod local_edition;
mod loinc;
mod output;
mod pipeline;
mod rxnorm;
