// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The FerroTERM Console: a FHIR terminology browser and editor whose every
//! page its server draws, with a backend-for-frontend in front of the
//! terminology server.
//!
//! The console is one more client of the server's public FHIR API: every
//! request it makes is an ordinary FHIR request (`fhir`), sent server to
//! server with the signed-in reader's own access token (`upstream`), and it
//! links no crate from this workspace, so anything the console can do, a
//! client can do. It holds the reader's sign-in on its server (`session`,
//! `oidc`), so the browser carries an opaque cookie and never a token. Each
//! page is an axum handler (`server`) that reads the request (`visit`,
//! `request`), runs the screen its path names (`screen`, `pages`) against
//! the terminology server, and answers the whole document (`app`) inside the
//! shell (`shell`), drawn with the components of `ferrohealth-ui`; what a
//! page does in the browser is FerroUI's script modules over what the server
//! drew, and every page works with them refused. `config` reads the TOML
//! configuration, `cli` and `command` run the binary, `healthcheck` among
//! its jobs. No specification governs the console: our own design.
#![doc(test(attr(deny(warnings))))]

pub mod app;
pub mod auth;
mod authoring;
pub mod cli;
pub mod command;
mod comparison;
pub mod config;
mod evidence;
pub mod fhir;
mod find;
mod locale;
mod markup;
pub mod oidc;
mod pages;
mod post;
mod posted;
mod prefs;
mod request;
mod routes;
mod screen;
pub mod server;
pub mod session;
mod shell;
mod tree;
pub mod upstream;
mod url;
mod visit;
