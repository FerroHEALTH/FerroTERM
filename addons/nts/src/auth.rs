// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Obtaining and keeping an OAuth 2 access token, without a person present.
//!
//! The service challenges the listing and every download, so each request a
//! run makes carries a bearer token. The token comes from the OAuth 2.0
//! client credentials grant (RFC 6749 §4.4) through `ferrohealth-smart`'s
//! `TokenSource`, which reads the token endpoint from the issuer's metadata,
//! caches the token and fetches a new one before it expires. The service's
//! issuer publishes `client_credentials` among its grants, and its own
//! authentication examples use it with a confidential client
//! (<https://github.com/terminologieserver/ontoserver_authentication_examples>).

use std::sync::Arc;
use std::time::Duration;

use ferrohealth_smart::client::cache::{Clock, TokenSource};
use ferrohealth_smart::client::{ClientAuth, ClientCredentials, Endpoint, TokenError};
use reqwest::header::HeaderValue;

use crate::credentials::Credentials;

/// How long before an access token expires the add-on fetches a new one.
///
/// A token that expires between the decision to use it and the server reading
/// it would fail a run for no reason, so the renewal happens early.
pub const REFRESH_MARGIN: Duration = Duration::from_secs(60);

/// Why no access token could be obtained.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// The token source could not be built from the configuration.
    #[error("the token source for {name} could not be built")]
    Setup {
        /// The source's configured name.
        name: String,
        /// What `ferrohealth-smart` reported.
        #[source]
        source: Box<TokenError>,
    },
    /// The issuer's metadata or its token endpoint gave no token.
    #[error("{name} obtained no access token")]
    Token {
        /// The source's configured name.
        name: String,
        /// What `ferrohealth-smart` reported, shared by every caller that
        /// waited on the same fetch.
        #[source]
        source: Arc<TokenError>,
    },
}

/// The access token of one configured source, cached and renewed.
#[derive(Debug)]
pub struct Authenticator {
    name: String,
    tokens: TokenSource,
}

impl Authenticator {
    /// Returns the authenticator of the source `name`, which asks the issuer
    /// `issuer` for its tokens with `credentials` and reads their expiry
    /// against `clock`.
    ///
    /// The client authenticates with `client_secret_post`, the client secret
    /// in the request body, which is how the service's own examples send it
    /// (RFC 6749 §2.3.1).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Setup`] when the HTTP engine of the token source
    /// cannot be built.
    pub fn new(
        name: impl Into<String>,
        issuer: &str,
        credentials: &Credentials,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AuthError> {
        let name = name.into();
        let mut grant = ClientCredentials::new(
            Endpoint::Issuer(issuer.to_owned()),
            credentials.client_id.clone(),
            credentials.client_secret.to_secret_string(),
        );
        grant.client_auth = ClientAuth::ClientSecretPost;
        grant.refresh_margin = REFRESH_MARGIN;
        let tokens = TokenSource::with_clock(grant, clock).map_err(|source| AuthError::Setup {
            name: name.clone(),
            source: Box::new(source),
        })?;
        Ok(Self { name, tokens })
    }

    /// Returns the `Authorization` field value the next request carries
    /// (RFC 6750 §2.1).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Token`] when the issuer's metadata cannot be read
    /// or the token endpoint issues no token.
    pub async fn authorization(&self) -> Result<HeaderValue, AuthError> {
        let token = self
            .tokens
            .token()
            .await
            .map_err(|source| AuthError::Token {
                name: self.name.clone(),
                source,
            })?;
        Ok(token.header().clone())
    }
}
