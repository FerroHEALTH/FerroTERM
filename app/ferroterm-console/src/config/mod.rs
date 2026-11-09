// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's configuration, read by `ferrohealth-config`.
//!
//! The tree's defaults, the TOML file `--config` or
//! `FERROTERM_CONSOLE_CONFIG` names, every `FERROTERM_CONSOLE__SECTION__KEY`
//! override and every `--set key=value`, with every secret reachable through
//! a `<key>_file` sibling read at start.
//!
//! [`Config`] is the tree as written, every default in its struct's
//! `Default` impl; [`Config::load`] reads it and
//! [`Config::resolve`](crate::config::Config::resolve) turns it into the
//! checked [`settings::Settings`] the server runs on. An override names its
//! key by path, `FERROTERM_CONSOLE__FERROTERM__BASE_URL` for `[ferroterm]
//! base_url`. `RUST_LOG` is the alias of `telemetry.filter`. No
//! specification governs the configuration: our own design.
//!
//! ```toml
//! [server]
//! listen = "0.0.0.0:3000"
//!
//! [ferroterm]
//! base_url = "http://ferroterm:8080/"
//! public_url = "https://terminology.example.org/"
//! sync_url = "http://ferroterm-sync:8090/"
//! admin_url = "http://ferroterm:8081/"
//! timeout = "30s"
//!
//! [oidc]
//! issuer = "https://idp.example.org/realms/ferroterm"
//! client_id = "ferroterm-console"
//! client_secret_file = "/run/secrets/viewer-client-secret"
//! redirect_uri = "https://console.example.org/auth/callback"
//! post_logout_redirect_uri = "https://console.example.org/"
//! offline_access = true
//!
//! [session]
//! store = "memory"
//! idle_timeout = "30min"
//! absolute_timeout = "12h"
//! ```
//!
//! The `[oidc]` and `[session]` sections are `ferrohealth-session`'s
//! (`ferrohealth_session::config`): an endpoint `[oidc]` leaves out is read
//! from the provider's metadata when the console starts.

pub mod error;
pub mod settings;

use std::collections::BTreeMap;
use std::path::Path;

use ferrohealth_config::loader::{Loaded, Loader};
use ferrohealth_config::removed::Removed;
use ferrohealth_config::units::Duration;
use ferrohealth_session::config::{OidcSection, SessionSection};
use ferrohealth_telemetry::section::TelemetrySection;
use serde::Deserialize;

use crate::config::error::Error;

/// The environment prefix: `FERROTERM_CONSOLE__SERVER__LISTEN` sets
/// `[server] listen`, and `FERROTERM_CONSOLE_CONFIG` names the file.
pub const PREFIX: &str = "FERROTERM_CONSOLE";

/// The variable that names the `tracing` filter, the alias of
/// `telemetry.filter`.
pub const FILTER_ENV: &str = "RUST_LOG";

/// The loader of the console's tree.
///
/// # Errors
/// Returns the loader's error when the prefix is refused, which it never is
/// for [`PREFIX`].
pub fn loader() -> Result<Loader, ferrohealth_config::error::ConfigError> {
    Ok(Loader::new(PREFIX)?.alias(FILTER_ENV, "telemetry.filter"))
}

/// The configuration tree as written.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// `[server]`: where the console listens and the locale it falls back to.
    pub server: Server,
    /// `[ferroterm]`: the terminology server the console is a client of.
    pub ferroterm: Ferroterm,
    /// `[session]`: the sign-in sessions.
    pub session: SessionSection,
    /// `[oidc]`: the OpenID Provider operators sign in with; without it the
    /// console offers no sign-in.
    pub oidc: Option<OidcSection>,
    /// `[telemetry]`: the console log; `RUST_LOG` sets its filter too.
    pub telemetry: TelemetrySection,
}

impl Config {
    /// Reads the configuration of the running process: the file `path`
    /// names, or the one `FERROTERM_CONSOLE_CONFIG` names, the environment
    /// and the `--set` pairs `sets`.
    ///
    /// # Errors
    /// Returns [`Error::Load`] when the file cannot be read, a key or
    /// variable is unknown, or a value does not read as its key.
    pub fn load(path: Option<&Path>, sets: &[(String, String)]) -> Result<Loaded<Self>, Error> {
        Ok(loader()?.load_with(path, sets)?)
    }

    /// Reads `text` as TOML and applies `environment` over it.
    ///
    /// Reading the sources and reading the process environment are separate
    /// so a test can state both: setting an environment variable is `unsafe`
    /// in edition 2024 and this workspace forbids `unsafe`.
    ///
    /// # Errors
    /// Returns [`Error::Load`] for TOML that does not read as this tree and
    /// for an override that names no key.
    pub fn from_sources(
        text: Option<&str>,
        environment: &BTreeMap<String, String>,
    ) -> Result<Self, Error> {
        let loaded: Loaded<Self> = loader()?.load_sources(text, None, environment)?;
        Ok(loaded.into_parts().0)
    }
}

/// `[server]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    /// The socket address the console listens on.
    pub listen: String,
    /// The BCP 47 tag of the locale a page renders in when the request's
    /// `Accept-Language` names no language the console's catalog holds.
    pub locale: String,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            listen: String::from("127.0.0.1:3000"),
            locale: String::from("en"),
        }
    }
}

/// `[ferroterm]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ferroterm {
    /// The root of the terminology server's HTTP surface, under which it
    /// serves its FHIR roots (`/r4`, `/r4b`, `/r5`, `/r6`) and `/health`.
    pub base_url: String,
    /// The root the server's readers reach it at, which the console shows
    /// beside every request it makes; the base URL when unset.
    pub public_url: Option<String>,
    /// The root of the sync service's admin listener, whose runs the
    /// Synchronisation screens show and whose Run now and Activate they post;
    /// none when unset. The console calls it with the signed-in reader's
    /// access token, so the reader's token must carry the listener's product
    /// scope (`__ferroterm.sync` by default).
    pub sync_url: Option<String>,
    /// The root of the terminology server's admin listener, whose
    /// `POST /reload` the Server screen offers as Reload the served set; none
    /// when unset. The reader's token must carry `__ferroterm.admin`.
    pub admin_url: Option<String>,
    /// How long one call to the server may take, a duration such as `30s`.
    pub timeout: Duration,
    /// `timeout_ms`, which `timeout` replaced: refused, naming it.
    pub timeout_ms: Removed,
}

impl Default for Ferroterm {
    fn default() -> Self {
        Self {
            base_url: String::from("http://127.0.0.1:8080/"),
            public_url: None,
            sync_url: None,
            admin_url: None,
            timeout: Duration::from_secs(30),
            timeout_ms: Removed::default(),
        }
    }
}
