// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Obtaining and keeping an OAuth 2 access token, without a person present.
//!
//! The service challenges the listing itself, so every request a run makes
//! carries a bearer token. The token endpoint is read from the SMART
//! configuration document rather than hard-coded, because the deployment's own
//! identity provider owns it
//! (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
//!
//! Three exchanges are used, all from the OAuth 2 framework (RFC 6749,
//! <https://www.rfc-editor.org/rfc/rfc6749>): the client-credentials grant
//! (§4.4) when a client secret is configured, the resource-owner password
//! credentials grant (§4.3), which is what the service documents for a
//! personal account, and the refresh-token exchange (§6). A refresh the server
//! refuses ends in a fresh login from the stored credentials, so a run on any
//! day succeeds without interaction.

use core::fmt;
use std::sync::Arc;

use crate::credentials::Credentials;

/// How many seconds before an access token expires the add-on renews it.
///
/// A token that expires between the decision to use it and the server reading
/// it would fail a run for no reason, so the renewal happens early.
pub const EXPIRY_SKEW_SECONDS: i64 = 60;

/// The current instant, which the token lifetimes are measured against.
///
/// The trait exists so a test can drive a run across a token lifetime without
/// waiting for one.
pub trait Clock: fmt::Debug + Send + Sync {
    /// The instant the caller should treat as now.
    fn now(&self) -> jiff::Timestamp;
}

/// The clock every deployment runs on.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> jiff::Timestamp {
        jiff::Timestamp::now()
    }
}

/// The exchange a token request performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grant {
    /// The client-credentials grant (RFC 6749 §4.4).
    ClientCredentials,
    /// The resource-owner password credentials grant (RFC 6749 §4.3).
    Password,
    /// The refresh-token exchange (RFC 6749 §6).
    RefreshToken,
}

impl Grant {
    /// The value the `grant_type` parameter carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClientCredentials => "client_credentials",
            Self::Password => "password",
            Self::RefreshToken => "refresh_token",
        }
    }
}

impl fmt::Display for Grant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One token exchange, with the parameters that exchange needs.
///
/// The parameters travel with the variant so a request can never be sent with
/// an empty credential where the grant requires one.
#[derive(Debug, Clone, Copy)]
enum Exchange<'a> {
    ClientCredentials,
    Password {
        username: &'a str,
        password: &'a str,
    },
    Refresh {
        token: &'a str,
    },
}

impl Exchange<'_> {
    const fn grant(self) -> Grant {
        match self {
            Self::ClientCredentials => Grant::ClientCredentials,
            Self::Password { .. } => Grant::Password,
            Self::Refresh { .. } => Grant::RefreshToken,
        }
    }
}

/// A token that could not be obtained.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// The discovery document could not be fetched.
    #[error("the discovery request to {url} failed")]
    Discovery {
        /// The address the request was sent to.
        url: String,
        /// Why the request failed.
        #[source]
        source: reqwest::Error,
    },
    /// The discovery document answered with a status that is not a success.
    #[error("{url} answered {status}")]
    DiscoveryStatus {
        /// The address the request was sent to.
        url: String,
        /// The status the server answered with.
        status: reqwest::StatusCode,
    },
    /// The discovery document names no token endpoint.
    #[error("the discovery document at {url} names no token endpoint")]
    NoTokenEndpoint {
        /// The address the document came from.
        url: String,
    },
    /// The token request itself failed.
    #[error("the {grant} request to {url} failed")]
    Token {
        /// The address the request was sent to.
        url: String,
        /// The exchange that was attempted.
        grant: Grant,
        /// Why the request failed.
        #[source]
        source: reqwest::Error,
    },
    /// The token endpoint refused the exchange.
    #[error("{url} answered {status} to the {grant} request")]
    Refused {
        /// The address the request was sent to.
        url: String,
        /// The exchange that was attempted.
        grant: Grant,
        /// The status the server answered with.
        status: reqwest::StatusCode,
    },
    /// Neither grant the service admits is fully configured.
    #[error("no grant is configured for {name}")]
    NoUsableGrant {
        /// The source that has no usable grant.
        name: String,
    },
    /// A token lifetime could not be turned into an instant.
    #[error("a token lifetime of {seconds} seconds is not an instant from now")]
    Expiry {
        /// The lifetime that was added to the current instant.
        seconds: i64,
        /// Why the arithmetic failed.
        #[source]
        source: jiff::Error,
    },
}

impl AuthError {
    /// Whether the server refused the exchange rather than failing to answer.
    ///
    /// A refusal is the signal to try the next grant, and to log in again once
    /// a refresh token has run out; anything else is reported as it is.
    #[must_use]
    pub fn is_refusal(&self) -> bool {
        match self {
            Self::Refused { status, .. } => status.is_client_error(),
            _ => false,
        }
    }
}

/// The SMART configuration document, of which one field is read.
#[derive(Debug, serde::Deserialize)]
struct SmartConfiguration {
    #[serde(default)]
    token_endpoint: Option<String>,
}

/// A token response (RFC 6749 §5.1).
#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    refresh_token: Option<String>,
}

/// The tokens one exchange yielded.
#[derive(Clone)]
struct TokenSet {
    access_token: String,
    expires_at: jiff::Timestamp,
    refresh_token: Option<String>,
}

impl fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TokenSet(expires_at={}, refreshable={})",
            self.expires_at,
            self.refresh_token.is_some()
        )
    }
}

/// What one authenticator has learned so far.
#[derive(Debug, Default)]
struct AuthState {
    token_endpoint: Option<String>,
    tokens: Option<TokenSet>,
}

/// Adds `seconds` to `now`, naming the arithmetic that failed.
fn instant_after(now: jiff::Timestamp, seconds: i64) -> Result<jiff::Timestamp, AuthError> {
    let span = jiff::Span::new()
        .try_seconds(seconds)
        .map_err(|source| AuthError::Expiry { seconds, source })?;
    now.checked_add(span)
        .map_err(|source| AuthError::Expiry { seconds, source })
}

/// The token machinery of one service.
#[derive(Debug)]
pub struct Authenticator {
    name: String,
    discovery_url: String,
    client: reqwest::Client,
    credentials: Credentials,
    clock: Arc<dyn Clock>,
    state: tokio::sync::Mutex<AuthState>,
}

impl Authenticator {
    /// An authenticator for the service whose SMART configuration document is
    /// at `discovery_url`.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        discovery_url: impl Into<String>,
        client: reqwest::Client,
        credentials: Credentials,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            name: name.into(),
            discovery_url: discovery_url.into(),
            client,
            credentials,
            clock,
            state: tokio::sync::Mutex::new(AuthState::default()),
        }
    }

    /// An access token that is valid now.
    ///
    /// The held token is returned while more than [`EXPIRY_SKEW_SECONDS`] of
    /// its life are left. Past that it is refreshed, and a refused refresh
    /// becomes a fresh login from the stored credentials.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Discovery`], [`AuthError::DiscoveryStatus`], or
    /// [`AuthError::NoTokenEndpoint`] when the token endpoint cannot be found,
    /// [`AuthError::Token`] or [`AuthError::Refused`] when an exchange fails,
    /// [`AuthError::NoUsableGrant`] when nothing is configured to log in with,
    /// and [`AuthError::Expiry`] when an advertised lifetime is not an instant
    /// from now.
    pub async fn access_token(&self) -> Result<String, AuthError> {
        let mut state = self.state.lock().await;
        let now = self.clock.now();
        if let Some(tokens) = state.tokens.as_ref()
            && instant_after(now, EXPIRY_SKEW_SECONDS)? < tokens.expires_at
        {
            return Ok(tokens.access_token.clone());
        }
        let refresh_token = state
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.refresh_token.clone());
        let endpoint = self.token_endpoint(&mut state).await?;
        if let Some(token) = refresh_token {
            match self
                .exchange(&endpoint, Exchange::Refresh { token: &token }, now)
                .await
            {
                Ok(tokens) => {
                    let access_token = tokens.access_token.clone();
                    state.tokens = Some(tokens);
                    return Ok(access_token);
                }
                Err(error) if error.is_refusal() => {
                    tracing::info!(
                        source = %self.name,
                        reason = %error,
                        "the refresh token is spent; logging in again"
                    );
                    state.tokens = None;
                }
                Err(error) => return Err(error),
            }
        }
        let tokens = self.login(&endpoint, now).await?;
        let access_token = tokens.access_token.clone();
        state.tokens = Some(tokens);
        Ok(access_token)
    }

    /// The token endpoint, read from the discovery document once.
    async fn token_endpoint(&self, state: &mut AuthState) -> Result<String, AuthError> {
        if let Some(endpoint) = state.token_endpoint.as_ref() {
            return Ok(endpoint.clone());
        }
        let url = self.discovery_url.clone();
        let response =
            self.client
                .get(&url)
                .send()
                .await
                .map_err(|source| AuthError::Discovery {
                    url: url.clone(),
                    source,
                })?;
        let status = response.status();
        if !status.is_success() {
            return Err(AuthError::DiscoveryStatus { url, status });
        }
        let document: SmartConfiguration =
            response
                .json()
                .await
                .map_err(|source| AuthError::Discovery {
                    url: url.clone(),
                    source,
                })?;
        let Some(endpoint) = document.token_endpoint else {
            return Err(AuthError::NoTokenEndpoint { url });
        };
        tracing::debug!(source = %self.name, %endpoint, "the token endpoint was discovered");
        state.token_endpoint = Some(endpoint.clone());
        Ok(endpoint)
    }

    /// Logs in with the grants the credentials configure.
    ///
    /// The client-credentials grant is attempted first when a client secret is
    /// configured, and a refusal falls back to the documented password grant.
    async fn login(&self, endpoint: &str, now: jiff::Timestamp) -> Result<TokenSet, AuthError> {
        if self.credentials.has_client_credentials() {
            match self
                .exchange(endpoint, Exchange::ClientCredentials, now)
                .await
            {
                Ok(tokens) => return Ok(tokens),
                Err(error) if error.is_refusal() && self.credentials.has_password_grant() => {
                    tracing::warn!(
                        source = %self.name,
                        reason = %error,
                        "the client-credentials grant was refused; using the password grant"
                    );
                }
                Err(error) => return Err(error),
            }
        }
        let (Some(username), Some(password)) = (
            self.credentials.username.as_deref(),
            self.credentials.password.as_deref(),
        ) else {
            return Err(AuthError::NoUsableGrant {
                name: self.name.clone(),
            });
        };
        self.exchange(endpoint, Exchange::Password { username, password }, now)
            .await
    }

    /// Performs one token exchange.
    async fn exchange(
        &self,
        endpoint: &str,
        exchange: Exchange<'_>,
        now: jiff::Timestamp,
    ) -> Result<TokenSet, AuthError> {
        let grant = exchange.grant();
        let mut form: Vec<(&str, &str)> = vec![
            ("grant_type", grant.as_str()),
            ("client_id", self.credentials.client_id.as_str()),
        ];
        if let Some(secret) = self.credentials.client_secret.as_deref() {
            form.push(("client_secret", secret));
        }
        match exchange {
            Exchange::ClientCredentials => {}
            Exchange::Password { username, password } => {
                form.push(("username", username));
                form.push(("password", password));
            }
            Exchange::Refresh { token } => form.push(("refresh_token", token)),
        }
        let response = self
            .client
            .post(endpoint)
            .form(&form)
            .send()
            .await
            .map_err(|source| AuthError::Token {
                url: endpoint.to_owned(),
                grant,
                source,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(AuthError::Refused {
                url: endpoint.to_owned(),
                grant,
                status,
            });
        }
        let token: TokenResponse = response.json().await.map_err(|source| AuthError::Token {
            url: endpoint.to_owned(),
            grant,
            source,
        })?;
        // NOTE: RFC 6749 §5.1 only RECOMMENDS `expires_in`, so a response
        // without one is treated as expiring now and the next request
        // authenticates again rather than guessing a lifetime.
        let expires_at = instant_after(now, token.expires_in.unwrap_or_default())?;
        tracing::debug!(source = %self.name, %grant, %expires_at, "a token was issued");
        Ok(TokenSet {
            access_token: token.access_token,
            expires_at,
            refresh_token: token.refresh_token,
        })
    }
}
