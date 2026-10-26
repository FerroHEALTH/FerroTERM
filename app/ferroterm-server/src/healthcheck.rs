// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The health probe the container runs against itself.
//!
//! The image has no shell and no HTTP client, so `ferroterm healthcheck` is its
//! `HEALTHCHECK` command: one `GET /health` with a short timeout, exit 0 on
//! `200 OK`. The binary binds its listener only after every artifact is open
//! (`main.rs` loads, then binds), so a server that answers at all serves every
//! code system it was configured with, and healthy means it answers terminology
//! requests.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use axum::body::Body;
use http::{Request, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

/// The route the probe asks for.
pub const PATH: &str = "/health";

/// How long the probe waits for an answer.
///
/// Shorter than the image's `HEALTHCHECK --timeout=5s`, so the probe reports
/// the reason itself instead of being killed.
pub const TIMEOUT: Duration = Duration::from_secs(3);

/// Why the probe did not get a `200 OK`.
#[derive(Debug, thiserror::Error)]
pub enum HealthcheckError {
    /// The URL does not parse.
    #[error("{url} is not a URL")]
    Url {
        /// The URL as given.
        url: String,
        /// The cause.
        #[source]
        source: http::Error,
    },
    /// The request did not complete: nothing listens, the connection was
    /// refused or reset, or the answer was not HTTP.
    #[error("cannot reach {url}")]
    Request {
        /// The URL probed.
        url: String,
        /// The cause.
        #[source]
        source: hyper_util::client::legacy::Error,
    },
    /// No answer arrived within [`TIMEOUT`].
    #[error("{url} did not answer within {timeout:?}")]
    Timeout {
        /// The URL probed.
        url: String,
        /// How long the probe waited.
        timeout: Duration,
    },
    /// The server answered something other than `200 OK`.
    #[error("{url} answered {status}")]
    Status {
        /// The URL probed.
        url: String,
        /// The status it answered.
        status: StatusCode,
    },
}

/// The URL to probe for a server listening on `listen`.
///
/// An unspecified address (`0.0.0.0`, `[::]`) is reached on the loopback of the
/// same family, since that is where the probe runs; any other address is used
/// as written.
#[must_use]
pub fn url_of(listen: &str) -> String {
    // NOTE: a listen value that is no socket address is a host name and port
    // (`localhost:8080`), which is legitimately not of this form and is used
    // as written; the server refused anything else at bind time.
    let authority = match listen.parse::<SocketAddr>() {
        Ok(address) if address.ip().is_unspecified() => {
            let loopback = match address.ip() {
                IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
                IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
            };
            SocketAddr::new(loopback, address.port()).to_string()
        }
        Ok(address) => address.to_string(),
        Err(_not_a_socket_address) => listen.to_owned(),
    };
    format!("http://{authority}{PATH}")
}

/// Asks `url` once and returns when it answered `200 OK`.
///
/// # Errors
///
/// Returns [`HealthcheckError`] when the URL does not parse, the request fails
/// or exceeds [`TIMEOUT`], or the answer is not `200 OK`.
pub async fn probe(url: &str) -> Result<(), HealthcheckError> {
    let request =
        Request::get(url)
            .body(Body::empty())
            .map_err(|source| HealthcheckError::Url {
                url: url.to_owned(),
                source,
            })?;
    let client: Client<HttpConnector, Body> = Client::builder(TokioExecutor::new()).build_http();
    let response = tokio::time::timeout(TIMEOUT, client.request(request))
        .await
        .map_err(|_elapsed| HealthcheckError::Timeout {
            url: url.to_owned(),
            timeout: TIMEOUT,
        })?
        .map_err(|source| HealthcheckError::Request {
            url: url.to_owned(),
            source,
        })?;
    match response.status() {
        StatusCode::OK => Ok(()),
        status => Err(HealthcheckError::Status {
            url: url.to_owned(),
            status,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::url_of;

    #[test]
    fn an_unspecified_listen_address_is_probed_on_the_loopback() {
        assert_eq!(url_of("0.0.0.0:8080"), "http://127.0.0.1:8080/health");
        assert_eq!(url_of("[::]:8080"), "http://[::1]:8080/health");
    }

    #[test]
    fn a_specific_address_or_a_host_name_is_probed_as_written() {
        assert_eq!(url_of("127.0.0.1:9090"), "http://127.0.0.1:9090/health");
        assert_eq!(url_of("10.0.0.5:8080"), "http://10.0.0.5:8080/health");
        assert_eq!(url_of("[fd00::5]:8080"), "http://[fd00::5]:8080/health");
        assert_eq!(url_of("localhost:8080"), "http://localhost:8080/health");
    }
}
