// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Asking the server to reload the set it serves.
//!
//! The server re-scans its index root and its resource directory on
//! `POST /reload`, served by the admin listener its `FERROTERM_ADMIN_LISTEN`
//! binds, and answers with the systems it now serves. A reload it refuses
//! leaves the old set answering, which is what makes a failed run harmless.
//!
//! The call carries the credential `[server_admin_credentials]` names: HTTP
//! Basic (RFC 7617) or an OAuth 2.0 client credentials token (RFC 6749 §4.4)
//! that `ferrohealth-smart` fetches, caches and renews once on a `401`.
//!
//! No FHIR specification governs the call: our own design.

use std::sync::Arc;

use ferrohealth_smart::client::cache::TokenSource;
use ferrohealth_smart::client::{CallError, RenewalError, TokenError};
use ferrohealth_smart::config::Outbound;
use reqwest::header::HeaderValue;

use crate::record::ReloadReply;

/// A reload the server did not perform.
#[derive(Debug, thiserror::Error)]
pub enum ReloadError {
    /// The request never reached the server.
    #[error("the reload request to {url} failed")]
    Request {
        /// The address the request was sent to.
        url: String,
        /// Why the request failed.
        #[source]
        source: reqwest::Error,
    },
    /// No token could be obtained for the call.
    #[error("no token could be obtained for the reload request to {url}")]
    Token {
        /// The address the request was meant for.
        url: String,
        /// Why the token could not be obtained.
        #[source]
        source: Arc<TokenError>,
    },
    /// The server refused the token and no new one could be obtained.
    #[error("{url} refused the token of the reload request")]
    Renewal {
        /// The address the request was sent to.
        url: String,
        /// Why the new token could not be obtained.
        #[source]
        source: RenewalError,
    },
    /// The server answered, and refused.
    #[error("{url} answered {status}: {body}")]
    Refused {
        /// The address the request was sent to.
        url: String,
        /// The status the server answered with.
        status: u16,
        /// The body it answered with.
        body: String,
    },
}

/// The credential the reload call carries.
#[derive(Debug, Clone)]
enum Credential {
    /// None: the server authenticates no caller.
    None,
    /// The `Authorization` value of an HTTP Basic credential.
    Basic(HeaderValue),
    /// The client credentials token.
    Token(TokenSource),
}

/// The server's admin listener, as this service speaks to it.
#[derive(Debug, Clone)]
pub struct AdminClient {
    client: reqwest::Client,
    base_url: String,
    credential: Credential,
}

impl AdminClient {
    /// The client for the admin listener at `base_url`, calling it with no
    /// credential.
    #[must_use]
    pub fn new(client: reqwest::Client, base_url: &str) -> Self {
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_owned(),
            credential: Credential::None,
        }
    }

    /// This client calling with `outbound`, the credential
    /// `[server_admin_credentials]` resolved to.
    ///
    /// # Errors
    ///
    /// Returns [`TokenError`] when the client credentials grant cannot build
    /// its token client.
    pub fn with_credentials(mut self, outbound: Outbound) -> Result<Self, TokenError> {
        self.credential = match outbound {
            Outbound::Basic(credential) => Credential::Basic(credential.header_value()),
            Outbound::Oauth(grant) => Credential::Token(TokenSource::new(*grant)?),
        };
        Ok(self)
    }

    /// The address `POST /reload` is sent to.
    #[must_use]
    pub fn reload_url(&self) -> String {
        format!("{}/reload", self.base_url)
    }

    /// Sends `POST /reload` carrying `authorization`, when given.
    async fn send(
        &self,
        url: &str,
        authorization: Option<HeaderValue>,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let mut request = self.client.post(url);
        if let Some(value) = authorization {
            request = request.header(reqwest::header::AUTHORIZATION, value);
        }
        request.send().await
    }

    /// Asks the server to reload the set it serves.
    ///
    /// # Errors
    ///
    /// Returns [`ReloadError::Request`] when the request does not reach the
    /// server, [`ReloadError::Token`] when no token could be obtained for it,
    /// [`ReloadError::Renewal`] when the server refused the token and no new
    /// one could be obtained, and [`ReloadError::Refused`] when the server
    /// answers with a status that is not a success, in which case the old set
    /// still answers.
    pub async fn reload(&self) -> Result<ReloadReply, ReloadError> {
        let url = self.reload_url();
        let request = |source| ReloadError::Request {
            url: url.clone(),
            source,
        };
        let response = match &self.credential {
            Credential::None => self.send(&url, None).await.map_err(request)?,
            Credential::Basic(value) => self
                .send(&url, Some(value.clone()))
                .await
                .map_err(request)?,
            Credential::Token(source) => source
                .call(|header| self.send(&url, Some(header)))
                .await
                .map_err(|error| match error {
                    CallError::Token { source } => ReloadError::Token {
                        url: url.clone(),
                        source,
                    },
                    CallError::Renewal(source) => ReloadError::Renewal {
                        url: url.clone(),
                        source,
                    },
                    CallError::Call { source } => request(source),
                    _ => ReloadError::Token {
                        url: url.clone(),
                        source: Arc::new(TokenError::Abandoned),
                    },
                })?,
        };
        let status = response.status();
        let body = response.text().await.map_err(request)?;
        if !status.is_success() {
            return Err(ReloadError::Refused {
                url,
                status: status.as_u16(),
                body,
            });
        }
        Ok(ReloadReply {
            status: status.as_u16(),
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AdminClient;

    #[test]
    fn the_reload_address_sits_under_the_base() {
        let client = AdminClient::new(reqwest::Client::new(), "http://server:8081/");
        assert_eq!(
            client.reload_url(),
            "http://server:8081/reload",
            "a trailing slash in the configuration does not double up"
        );
    }
}
