// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's configuration: a TOML file, `FERROTERM_VIEWER__`
//! environment overrides over it, and every secret reachable through a
//! `<key>_file` sibling read at start.
//!
//! [`Config`] is the tree as written, every default in its struct's
//! `Default` impl; [`Config::load`](crate::config::Config::load) reads it and
//! [`Config::resolve`](crate::config::Config::resolve) turns it into the
//! checked [`settings::Settings`] the server runs on. An override names its
//! key by path, `FERROTERM_VIEWER__FERROTERM__BASE_URL` for `[ferroterm]
//! base_url`. No specification governs the configuration: our own design.
//!
//! ```toml
//! [server]
//! listen = "0.0.0.0:3000"
//! site_root = "/site"
//!
//! [ferroterm]
//! base_url = "http://ferroterm:8080/"
//! public_url = "https://terminology.example.org/"
//! sync_url = "http://ferroterm-sync:8090/"
//! timeout_ms = 30000
//!
//! [oidc]
//! issuer = "https://idp.example.org/realms/ferroterm"
//! authorization_endpoint = "https://idp.example.org/realms/ferroterm/protocol/openid-connect/auth"
//! token_endpoint = "https://idp.example.org/realms/ferroterm/protocol/openid-connect/token"
//! jwks_uri = "https://idp.example.org/realms/ferroterm/protocol/openid-connect/certs"
//! client_id = "ferroterm-viewer"
//! client_secret_file = "/run/secrets/viewer-client-secret"
//! redirect_uri = "https://console.example.org/auth/callback"
//! end_session_endpoint = "https://idp.example.org/realms/ferroterm/protocol/openid-connect/logout"
//! revocation_endpoint = "https://idp.example.org/realms/ferroterm/protocol/openid-connect/revoke"
//! post_logout_redirect_uri = "https://console.example.org/"
//! offline_access = true
//! ```

pub mod error;
pub mod load;
pub mod secret;
pub mod settings;

use std::path::PathBuf;

use secrecy::SecretString;
use serde::Deserialize;

/// The environment variable that names the configuration file when
/// `--config` does not.
pub const CONFIG_PATH_ENV: &str = "FERROTERM_VIEWER_CONFIG";

/// The prefix of every environment override.
pub const ENV_PREFIX: &str = "FERROTERM_VIEWER__";

/// The configuration tree as written.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// `[server]`: where the console listens and where its site bundle is.
    pub server: Server,
    /// `[ferroterm]`: the terminology server the console is a client of.
    pub ferroterm: Ferroterm,
    /// `[session]`: the server-side sign-in sessions.
    pub session: Session,
    /// `[oidc]`: the OpenID Provider operators sign in with; without it the
    /// console offers no sign-in.
    pub oidc: Option<Oidc>,
}

/// `[server]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    /// The socket address the console listens on.
    pub listen: String,
    /// The directory holding the site bundle cargo-leptos builds: the
    /// WebAssembly, its JavaScript glue and the stylesheet under `pkg/`.
    pub site_root: PathBuf,
    /// The BCP 47 tag of the locale a page renders in when the request's
    /// `Accept-Language` names no language the console's catalog holds.
    pub locale: String,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            listen: String::from("127.0.0.1:3000"),
            site_root: PathBuf::from("target/site"),
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
    /// The root of the sync service's admin listener, whose newest run's
    /// findings the console shows; none when unset.
    pub sync_url: Option<String>,
    /// How long one call to the server may take, in milliseconds.
    pub timeout_ms: u64,
}

impl Default for Ferroterm {
    fn default() -> Self {
        Self {
            base_url: String::from("http://127.0.0.1:8080/"),
            public_url: None,
            sync_url: None,
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
    pub client_secret: Option<SecretString>,
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
