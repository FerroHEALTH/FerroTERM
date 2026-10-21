// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The container health probe: one `GET /health` on the admin listener.
//!
//! The image has no shell and no HTTP client, so the probe is the binary
//! itself. Without an address it follows the configured listen address, on the
//! loopback when the service listens on every interface.
//!
//! No FHIR specification governs this: our own design.

use std::net::SocketAddr;

/// A probe that did not answer `200 OK`.
#[derive(Debug, thiserror::Error)]
pub enum HealthError {
    /// The request did not reach the service.
    #[error("the probe to {url} failed")]
    Request {
        /// The address the probe was sent to.
        url: String,
        /// Why the request failed.
        #[source]
        source: reqwest::Error,
    },
    /// The service answered with a status that is not a success.
    #[error("{url} answered {status}")]
    Status {
        /// The address the probe was sent to.
        url: String,
        /// The status the service answered with.
        status: u16,
    },
}

/// The address `GET /health` is probed at, for a service listening on `listen`.
///
/// A service listening on every interface is probed on the loopback, which is
/// the one address a container can always reach itself on.
#[must_use]
pub fn url_for(listen: SocketAddr) -> String {
    if listen.ip().is_unspecified() {
        return format!("http://127.0.0.1:{}/health", listen.port());
    }
    format!("http://{listen}/health")
}

/// Probes `url`, answering `Ok` on `200 OK`.
///
/// # Errors
///
/// Returns [`HealthError::Request`] when the request does not reach the
/// service and [`HealthError::Status`] when the answer is not a success.
pub async fn probe(url: &str) -> Result<(), HealthError> {
    let response = reqwest::get(url)
        .await
        .map_err(|source| HealthError::Request {
            url: url.to_owned(),
            source,
        })?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    Err(HealthError::Status {
        url: url.to_owned(),
        status: status.as_u16(),
    })
}

#[cfg(test)]
mod tests {
    use super::url_for;

    #[test]
    fn a_service_on_every_interface_is_probed_on_the_loopback() {
        assert_eq!(
            url_for("0.0.0.0:8181".parse().expect("an address")),
            "http://127.0.0.1:8181/health",
            "a container reaches itself on the loopback"
        );
    }

    #[test]
    fn a_named_address_is_probed_as_it_is() {
        assert_eq!(
            url_for("127.0.0.1:9000".parse().expect("an address")),
            "http://127.0.0.1:9000/health",
            "the configured address is the one probed"
        );
    }
}
