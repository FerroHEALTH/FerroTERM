// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The FerroTERM Console: a FHIR terminology browser and editor, drawn on the
//! server in Leptos islands mode, with a backend-for-frontend in front of the
//! terminology server.
//!
//! The console is one more client of the server's public FHIR API: every
//! request it makes is an ordinary FHIR request (`fhir`), sent server to
//! server by [`upstream`] with the signed-in reader's own access token, and
//! it links no crate from this workspace, so anything the console can do, a
//! client can do. It holds the reader's sign-in on its server ([`session`],
//! [`oidc`]), so the browser carries an opaque cookie and never a token.
//! [`app`] is the page tree, drawn with the components of `ferrohealth-ui`;
//! [`server`] serves it with the health, sign-in and form routes under the
//! browser security headers, [`config`] reads the TOML configuration, [`cli`]
//! and [`command`] run the binary, and [`healthcheck`] is the job a container
//! runtime runs beside it. No specification governs the console: our own
//! design.
//!
//! The pages are drawn on the server alone, so every module above compiles
//! for the host. The browser half is the WebAssembly the island script
//! loads: it hydrates the library's islands and the console's own
//! shortcuts ([`islands`]) and nothing else, so it carries no page code, no
//! catalog and no FHIR client.
#![doc(test(attr(deny(warnings))))]

#[cfg(not(target_arch = "wasm32"))]
pub mod app;
#[cfg(not(target_arch = "wasm32"))]
mod auth;
#[cfg(not(target_arch = "wasm32"))]
mod authoring;
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod command;
#[cfg(not(target_arch = "wasm32"))]
mod comparison;
#[cfg(not(target_arch = "wasm32"))]
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
mod evidence;
#[cfg(not(target_arch = "wasm32"))]
pub mod fhir;
#[cfg(not(target_arch = "wasm32"))]
mod find;
#[cfg(not(target_arch = "wasm32"))]
pub mod healthcheck;
pub mod islands;
#[cfg(not(target_arch = "wasm32"))]
mod locale;
#[cfg(not(target_arch = "wasm32"))]
pub mod oidc;
#[cfg(not(target_arch = "wasm32"))]
mod pages;
#[cfg(not(target_arch = "wasm32"))]
mod post;
#[cfg(not(target_arch = "wasm32"))]
mod posted;
#[cfg(not(target_arch = "wasm32"))]
mod prefs;
#[cfg(not(target_arch = "wasm32"))]
mod request;
#[cfg(not(target_arch = "wasm32"))]
mod routes;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;
#[cfg(not(target_arch = "wasm32"))]
mod shell;
#[cfg(not(target_arch = "wasm32"))]
mod tree;
#[cfg(not(target_arch = "wasm32"))]
pub mod upstream;
#[cfg(not(target_arch = "wasm32"))]
mod url;

/// Hydrates the islands of the page the server drew.
///
/// Each island takes the words it draws as props, so the browser builds no
/// locale and fetches nothing.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_islands();
}
