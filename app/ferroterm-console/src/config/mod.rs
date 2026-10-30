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
//! timeout_ms = 30000
//!
//! [oidc]
//! issuer = "https://idp.example.org/realms/ferroterm"
//! authorization_endpoint = "https://idp.example.org/realms/ferroterm/protocol/openid-connect/auth"
//! token_endpoint = "https://idp.example.org/realms/ferroterm/protocol/openid-connect/token"
//! jwks_uri = "https://idp.example.org/realms/ferroterm/protocol/openid-connect/certs"
//! client_id = "ferroterm-console"
//! client_secret_file = "/run/secrets/viewer-client-secret"
//! redirect_uri = "https://console.example.org/auth/callback"
//! end_session_endpoint = "https://idp.example.org/realms/ferroterm/protocol/openid-connect/logout"
//! revocation_endpoint = "https://idp.example.org/realms/ferroterm/protocol/openid-connect/revoke"
//! post_logout_redirect_uri = "https://console.example.org/"
//! offline_access = true
//! ```

pub mod error;
pub mod settings;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ferrohealth_config::loader::{Loaded, Loader};
use ferrohealth_secret::secret::Secret;
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
    /// `[session]`: the server-side sign-in sessions.
    pub session: Session,
    /// `[oidc]`: the OpenID Provider operators sign in with; without it the
    /// console offers no sign-in.
    pub oidc: Option<Oidc>,
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
    /// How long one call to the server may take, in milliseconds.
    pub timeout_ms: u64,
}

impl Default for Ferroterm {
    fn default() -> Self {
        Self {
            base_url: String::from("http://127.0.0.1:8080/"),
            public_url: None,
            sync_url: None,
            admin_url: None,
            timeout_ms: 30_000,
        }
    }
}

/// `[session]`: the pending sign-ins and the signed-in sessions, each a pool
/// of its own.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Session {
    /// Whether the cookies carry `Secure`, so a browser sends them over HTTPS
    /// only; off only for a console on loopback.
    pub secure_cookie: bool,
    /// How long a sign-in begun at `GET /login` waits for the provider's
    /// redirect back, in seconds.
    pub sign_in_timeout_s: u64,
    /// How many pending sign-ins the console holds at once; a new one drops
    /// the oldest.
    pub max_sign_ins: usize,
    /// How long a signed-in session lives without a request, in seconds.
    pub idle_timeout_s: u64,
    /// How long a signed-in session lives at most, in seconds.
    pub absolute_timeout_s: u64,
    /// How many signed-in sessions the console holds at once; a sign-in past
    /// it is refused.
    pub max_sessions: usize,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            secure_cookie: true,
            sign_in_timeout_s: 300,
            max_sign_ins: 1000,
            idle_timeout_s: 1800,
            absolute_timeout_s: 43_200,
            max_sessions: 10_000,
        }
    }
}

/// `[oidc]`: an OAuth 2.0 authorization code client with PKCE at an OpenID
/// Provider.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Oidc {
    /// The provider's issuer identifier, which an ID Token's `iss` must equal
    /// exactly (OpenID Connect Core 1.0 §3.1.3.7).
    pub issuer: String,
    /// The provider's authorization endpoint (RFC 6749 §3.1).
    pub authorization_endpoint: String,
    /// The provider's token endpoint, where the authorization code is
    /// exchanged and the access token refreshed (RFC 6749 §3.2).
    pub token_endpoint: String,
    /// The provider's JWK Set, whose keys sign its ID Tokens (OpenID Connect
    /// Core 1.0 §10.1).
    pub jwks_uri: String,
    /// The console's client identifier at the provider (RFC 6749 §2.2).
    pub client_id: String,
    /// The console's client secret, inline, for a confidential client.
    pub client_secret: Option<Secret>,
    /// A file holding the console's client secret.
    pub client_secret_file: Option<PathBuf>,
    /// The redirection endpoint the provider sends the operator back to
    /// (RFC 6749 §3.1.2), the console's `/auth/callback`.
    pub redirect_uri: String,
    /// The scopes requested, `openid` among them.
    pub scopes: Vec<String>,
    /// Whether the sign-in asks for `offline_access` (OpenID Connect Core
    /// 1.0 §11), so the provider issues a refresh token the console renews
    /// the operator's access token with.
    pub offline_access: bool,
    /// The provider's end-session endpoint, where a sign-out sends the
    /// operator (OpenID Connect RP-Initiated Logout 1.0 §2); without it a
    /// sign-out ends the console's session alone.
    pub end_session_endpoint: Option<String>,
    /// Where the provider sends the operator after it signed them out, as
    /// registered with the provider (OpenID Connect RP-Initiated Logout 1.0
    /// §3).
    pub post_logout_redirect_uri: Option<String>,
    /// The provider's token revocation endpoint (RFC 7009 §2), which a
    /// sign-out tells the session's token is finished with.
    pub revocation_endpoint: Option<String>,
}

impl Default for Oidc {
    fn default() -> Self {
        Self {
            issuer: String::new(),
            authorization_endpoint: String::new(),
            token_endpoint: String::new(),
            jwks_uri: String::new(),
            client_id: String::new(),
            client_secret: None,
            client_secret_file: None,
            redirect_uri: String::new(),
            scopes: vec![String::from("openid")],
            offline_access: false,
            end_session_endpoint: None,
            post_logout_redirect_uri: None,
            revocation_endpoint: None,
        }
    }
}
