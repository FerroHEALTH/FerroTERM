// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The FerroTERM Console: a FHIR terminology browser and editor, a Leptos
//! app the server draws and the browser hydrates whole, with a
//! backend-for-frontend in front of the terminology server.
//!
//! The console is one more client of the server's public FHIR API: every
//! request it makes is an ordinary FHIR request (`fhir`), sent server to
//! server with the signed-in reader's own access token, and it links no
//! crate from this workspace, so anything the console can do, a client can
//! do. It holds the reader's sign-in on its server (`session`, `oidc`), so
//! the browser carries an opaque cookie and never a token. [`app`] is the
//! page tree, drawn with the components of `ferrohealth-ui`, its pages
//! grouped into lazy routes by `groups`, each read on the server and drawn
//! from what that read recorded (`fhir::tape`), with what the server read of
//! the request in `visit`. `server` serves it with the health, sign-in and
//! form routes under the browser security headers, `config` reads the TOML
//! configuration, `cli` and `command` run the binary, `healthcheck` among
//! its jobs. No specification governs the console: our own design.
//!
//! The server half compiles for the host and the browser half for
//! `wasm32-unknown-unknown`; the target, never a Cargo feature, chooses
//! between them. Every page, the FHIR client and the catalog compile for
//! both.
#![doc(test(attr(deny(warnings))))]

pub mod app;
pub mod auth;
mod authoring;
mod comparison;
mod evidence;
pub mod fhir;
mod find;
mod groups;
mod locale;
mod pages;
mod posted;
mod prefs;
mod request;
mod routes;
mod shell;
mod shortcuts;
mod tree;
mod url;
mod visit;

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod command;
#[cfg(not(target_arch = "wasm32"))]
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod oidc;
#[cfg(not(target_arch = "wasm32"))]
mod post;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;
#[cfg(not(target_arch = "wasm32"))]
pub mod upstream;

/// Hydrates the page the server drew, whole, through FerroUI's entry.
///
/// Its lazy routes load their code from their own chunks, and the root
/// element takes `data-ferroui-hydrated` once the page and its route have
/// hydrated.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    ferrohealth_ui::widget::hydration::explain();
    ferrohealth_ui::widget::hydration::hydrate(app::App);
}
