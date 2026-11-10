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
//! key by path, `FERROTERM_CONSOLE__FERROTERM__SERVER__URL` for
//! `[ferroterm.server] url`. `RUST_LOG` is the alias of `telemetry.filter`. No
//! specification governs the configuration: our own design.
//!
//! ```toml
//! [server]
//! listen = "0.0.0.0:3000"
//!
//! [ferroterm]
//! public_url = "https://terminology.example.org/"
//!
//! [ferroterm.server]
//! url = "http://ferroterm:8080/"
//! timeout = "30s"
//!
//! [ferroterm.sync]
//! url = "http://ferroterm-sync:8090/"
//!
//! [ferroterm.admin]
//! url = "http://ferroterm:8081/"
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
use ferrohealth_config::removed::{Removed, Retired};
use ferrohealth_config::units::Duration;
use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_session::config::{OidcSection, SessionSection};
use ferrohealth_smart::config::endpoint::EndpointSection;
use ferrohealth_telemetry::section::TelemetrySection;
use serde::Deserialize;

use crate::config::error::Error;

/// The environment prefix: `FERROTERM_CONSOLE__SERVER__LISTEN` sets
/// `[server] listen`, and `FERROTERM_CONSOLE_CONFIG` names the file.
pub const PREFIX: &str = "FERROTERM_CONSOLE";

/// The variable that names the `tracing` filter, the alias of
/// `telemetry.filter`.
pub const FILTER_ENV: &str = "RUST_LOG";

/// The loader of the console's tree, with the product's
/// `[ferroterm.server]` defaults layered under the file key by key, so a
/// table written in part keeps the console's timeout.
///
/// # Errors
/// Returns the loader's error when the prefix is refused, which it never is
/// for [`PREFIX`], or when the defaults do not serialize to a table.
pub fn loader() -> Result<Loader, ferrohealth_config::error::ConfigError> {
    Loader::new(PREFIX)?
        .alias(FILTER_ENV, "telemetry.filter")
        .defaults("ferroterm.server", &*Ferroterm::default().server)
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

/// `[ferroterm]`: the terminology server, its sync service and its admin
/// listener, each an outbound endpoint table the console calls with the
/// signed-in reader's credential.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ferroterm {
    /// The root the server's readers reach it at, which the console shows
    /// beside every request it makes; `[ferroterm.server] url` when unset.
    pub public_url: Option<String>,
    /// `[ferroterm.server]`: the root of the terminology server's HTTP
    /// surface, under which it serves its FHIR roots (`/r4`, `/r4b`, `/r5`,
    /// `/r6`) and `/health`, with the timeout, retry and TLS of each call.
    pub server: Box<EndpointSection>,
    /// `[ferroterm.sync]`: the root of the sync service's admin listener,
    /// whose runs the Synchronisation screens show and whose Run now and
    /// Activate they post; none when unset. The reader's token must carry the
    /// listener's product scope (`__ferroterm.sync` by default).
    pub sync: Option<Box<EndpointSection>>,
    /// `[ferroterm.admin]`: the root of the terminology server's admin
    /// listener, whose `POST /reload` the Server screen offers as Reload the
    /// served set; none when unset. The reader's token must carry
    /// `__ferroterm.admin`.
    pub admin: Option<Box<EndpointSection>>,
    /// `base_url`, which `[ferroterm.server] url` replaced in 0.4.5.
    pub base_url: Removed,
    /// `timeout`, which `[ferroterm.server] timeout` replaced in 0.4.5.
    pub timeout: Removed,
    /// `sync_url`, which `[ferroterm.sync] url` replaced in 0.4.5.
    pub sync_url: Removed,
    /// `admin_url`, which `[ferroterm.admin] url` replaced in 0.4.5.
    pub admin_url: Removed,
}

impl Default for Ferroterm {
    fn default() -> Self {
        Self {
            public_url: None,
            server: Box::new(EndpointSection {
                url: SecretUrl::new("http://127.0.0.1:8080/"),
                timeout: Duration::from_secs(30),
                ..EndpointSection::default()
            }),
            sync: None,
            admin: None,
            base_url: Removed::default(),
            timeout: Removed::default(),
            sync_url: Removed::default(),
            admin_url: Removed::default(),
        }
    }
}

/// The release these refusals belong to: each lives for that one release.
pub const PREPARING: &str = "0.4.5";

/// The `[ferroterm]` keys 0.4.5 retired, each with the key that took its
/// place.
// TODO(#1202): the refusals ship in 0.4.5 alone; delete them in 0.4.6.
pub const RETIRED: [Retired; 4] = [
    Retired::replaced("ferroterm.base_url", "ferroterm.server.url", "0.4.5"),
    Retired::replaced("ferroterm.timeout", "ferroterm.server.timeout", "0.4.5"),
    Retired::replaced("ferroterm.sync_url", "ferroterm.sync.url", "0.4.5"),
    Retired::replaced("ferroterm.admin_url", "ferroterm.admin.url", "0.4.5"),
];
