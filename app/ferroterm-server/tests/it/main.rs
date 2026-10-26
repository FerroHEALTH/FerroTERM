// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one integration-test binary for `ferroterm-server`; one module per topic.
//!
//! The FHIR tests drive the router with `tower::ServiceExt::oneshot` over a
//! synthetic SNOMED edition written by `ferroterm-testkit`; no socket, no
//! SNOMED content.
#![allow(
    clippy::panic_in_result_fn,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test assertions"
)]

mod batch;
mod batch_validate;
mod canonical;
mod closure;
mod code_system;
mod concept_map;
mod config;
mod console_fixtures;
mod descriptive;
mod ecosystem;
mod elements;
mod fixture;
mod health;
mod healthcheck;
mod history;
mod logical_id;
mod metadata;
mod metrics;
mod openehr;
mod operations;
mod persisted;
mod primitives;
mod r4;
mod r5;
mod r6;
mod reload;
mod scope;
mod shutdown;
mod smart;
mod snomed_expression;
mod stack;
mod telemetry;
mod translate;
mod usage_report;
mod value_set;
mod xml;
