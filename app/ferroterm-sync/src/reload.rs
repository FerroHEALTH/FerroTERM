// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Asking the server to reload the set it serves.
//!
//! The server re-scans its index root and its resource directory on
//! `POST /reload`, served by the admin listener its `FERROTERM__ADMIN__LISTEN`
//! binds, and answers with the systems it now serves. A reload it refuses
//! leaves the old set answering, which is what makes a failed run harmless.
//!
//! The call is made through the `[server_admin]` endpoint table: its
//! timeout, TLS and retry, and its credential, an OAuth 2.0 client
//! credentials token (RFC 6749 §4.4) that `ferrohealth-smart` fetches, caches
//! and renews once on a `401`.
//!
//! No FHIR specification governs the call: our own design.

use ferrohealth_smart::client::outbound::{OutboundClient, OutboundError, SendError};
use ferrohealth_smart::config::ServerTls;
use ferrohealth_smart::config::endpoint::EndpointSection;

use crate::record::ReloadReply;

/// A reload the server did not perform.
#[derive(Debug, thiserror::Error)]
pub enum ReloadError {
    /// The request never reached the server, or no token could be obtained
    /// for it.
    #[error("the reload request to {url} failed")]
    Request {
        /// The address the request was sent to.
        url: String,
        /// Why the request failed.
        #[source]
        source: Box<SendError>,
    },
    /// The answer could not be read.
    #[error("the answer to the reload request to {url} could not be read")]
    Body {
        /// The address the request was sent to.
        url: String,
        /// Why the answer did not read.
        #[source]
        source: reqwest::Error,
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

/// The server's admin listener, as this service speaks to it.
#[derive(Debug, Clone)]
pub struct AdminClient {
    outbound: OutboundClient,
    reload: url::Url,
}

impl AdminClient {
    /// The client of the admin listener `endpoint` names.
    ///
    /// # Errors
    ///
    /// Returns [`OutboundError`] when the table, its TLS or its credential
    /// is refused.
    pub fn new(endpoint: &EndpointSection) -> Result<Self, OutboundError> {
        let outbound = OutboundClient::build(endpoint, "server_admin", &ServerTls)?;
        let mut reload = outbound.url().clone();
        let base = reload.path().trim_end_matches('/').to_owned();
        reload.set_path(&format!("{base}/reload"));
        Ok(Self { outbound, reload })
    }

    /// The address `POST /reload` is sent to.
    #[must_use]
    pub fn reload_url(&self) -> String {
        self.reload.to_string()
    }

    /// Asks the server to reload the set it serves.
    ///
    /// # Errors
    ///
    /// Returns [`ReloadError::Request`] when the request does not reach the
    /// server or no token could be obtained for it, [`ReloadError::Body`]
    /// when the answer does not read, and [`ReloadError::Refused`] when the
    /// server answers with a status that is not a success, in which case the
    /// old set still answers.
    pub async fn reload(&self) -> Result<ReloadReply, ReloadError> {
        let url = self.reload_url();
        let response = self
            .outbound
            .send(&reqwest::Method::POST, &self.reload, |call| call)
            .await
            .map_err(|source| ReloadError::Request {
                url: url.clone(),
                source: Box::new(source),
            })?;
        let status = response.status();
        let body = response.text().await.map_err(|source| ReloadError::Body {
            url: url.clone(),
            source,
        })?;
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
    use ferrohealth_secret::secret_url::SecretUrl;
    use ferrohealth_smart::config::endpoint::EndpointSection;

    use super::AdminClient;

    #[test]
    fn the_reload_address_sits_under_the_base() {
        let client = AdminClient::new(&EndpointSection {
            url: SecretUrl::new("http://server:8081/"),
            ..EndpointSection::default()
        })
        .expect("the table builds");
        assert_eq!(
            client.reload_url(),
            "http://server:8081/reload",
            "a trailing slash in the configuration does not double up"
        );
    }
}
