// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The one integration-test binary for the browser journeys; one module per
//! topic, over the shared `harness`.
//!
//! `scripts/ui-e2e.sh` stands up the server and the browser and sets the two
//! variables the harness reads. Without them there is nothing to drive and
//! every journey skips, saying so; the `ui-e2e` CI job always runs that
//! script, so a skip is never a pass.
//!
//! `docs_shots` is a capture pass rather than a journey: it writes the book's
//! screenshots and runs only when the script is asked for it, so an ordinary
//! run rewrites no tracked file.
//!
//! The console draws every page on its server, so a page arrives whole and
//! FerroUI's script modules answer once its loader has run over it; the
//! harness waits for the loader's mark before it acts on a page, and the
//! `no_script` journeys refuse the browser the modules and hold every page
//! to its content, its links and its forms. Nothing here reads the page
//! source: inert markup satisfies a source assertion even when the control
//! it describes is unreachable, which is the failure a journey exists to
//! catch.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    reason = "test assertions, and the notice a skipped journey prints"
)]

mod accessibility;
mod composer;
mod concept_map_editor;
mod concurrent;
mod console;
mod docs_shots;
mod editor;
mod harness;
mod history;
mod navigation;
mod no_script;
mod reflow;
mod sign_in;
mod signed_in;
mod sync;
