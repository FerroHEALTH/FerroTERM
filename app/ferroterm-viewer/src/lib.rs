// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The FerroTERM Console: a FHIR terminology browser and editor, rendered on
//! the server and hydrated in the browser, with a backend-for-frontend in
//! front of the terminology server.
//!
//! The console is one more client of the server's public FHIR API: every
//! request it makes is an ordinary FHIR request (`fhir`), sent server to
//! server by [`upstream`] with the signed-in reader's own access token, and
//! it links no crate from this workspace, so anything the console can do, a
//! client can do. It holds the reader's sign-in on its server ([`session`],
//! [`oidc`]), so the browser carries an opaque cookie and never a token.
//! [`app`] is the page tree both halves share, drawn with the components of
//! `ferrohealth-ui`; [`server`] serves it with the health and sign-in routes
//! under the browser security headers, [`config`] reads the TOML
//! configuration, [`cli`] and [`command`] run the binary, and
//! [`healthcheck`] is the job a container runtime runs beside it. No
//! specification governs the console: our own design.
//!
//! The server half compiles for the host and the browser half for
//! `wasm32-unknown-unknown`; the target, never a Cargo feature, chooses
//! between them.
#![doc(test(attr(deny(warnings))))]

pub mod app;
pub mod auth;
mod authoring;
mod comparison;
mod components;
mod evidence;
// The browser half of a server function, which the server macro writes, is a
// network call that awaits nothing, and the lint names the macro alone.
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod fhir;
mod find;
mod listing;
mod locale;
mod offers;
mod pages;
mod paging;
mod routes;
mod runs;
// The browser half of a server function, which the server macro writes, is a
// network call that awaits nothing, and the lint names the macro alone.
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod settings;
mod styles;
mod tree;
mod url;

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod command;
#[cfg(not(target_arch = "wasm32"))]
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod healthcheck;
#[cfg(not(target_arch = "wasm32"))]
pub mod oidc;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;
#[cfg(not(target_arch = "wasm32"))]
pub mod upstream;

/// Attaches the browser half to the page the server rendered.
///
/// It marks `<html>` with `data-hydrated` once it has, so a journey waits for
/// the controls' handlers before it types (no specification governs this:
/// our own design).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
    if let Some(root) = leptos::prelude::document().document_element()
        && let Err(error) = root.set_attribute("data-hydrated", "")
    {
        leptos::logging::warn!("the page could not be marked hydrated: {error:?}");
    }
}
