//! The `ferroterm-sync` integration suite.
//!
//! One binary, one module per topic
//! (<https://doc.rust-lang.org/cargo/reference/cargo-targets.html>). Every
//! case runs against the harness in [`support`]: a mock feed, a mock server
//! admin listener, a mock webhook, a fake offline build, and a clock the test
//! drives, so nothing sleeps and nothing reaches the network.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::indexing_slicing,
    reason = "test assertions"
)]

mod admin;
mod api;
mod credentials;
mod failure;
mod lanes;
mod manual;
mod retention;
mod revalidation;
mod schedule;
mod support;
