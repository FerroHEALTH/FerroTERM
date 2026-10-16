//! The issuer's OpenID Connect discovery document, and the HTTPS fetch under it.
//!
//! An OpenID Connect issuer publishes its metadata at
//! `{issuer}/.well-known/openid-configuration`, and the document names the
//! endpoints and the JWKS this server needs
//! (<https://openid.net/specs/openid-connect-discovery-1_0.html>, §4).

use std::time::Duration;

use axum::body::Body;
use http::{Request, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

/// The path an OpenID Connect issuer publishes its metadata at
/// (<https://openid.net/specs/openid-connect-discovery-1_0.html>, §4).
pub const DISCOVERY_PATH: &str = ".well-known/openid-configuration";

/// How long one fetch of the discovery document or the JWKS may take.
///
/// No specification governs the bound: our own design, sized so a slow issuer
/// refuses the start with a reason instead of hanging the process.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// The grant types an issuer that omits `grant_types_supported` runs.
///
/// OpenID Connect Discovery 1.0 §3: "If omitted, the default value is
/// `["authorization_code", "implicit"]`."
const DEFAULT_GRANT_TYPES: [&str; 2] = ["authorization_code", "implicit"];

/// The client authentication an issuer that omits
/// `token_endpoint_auth_methods_supported` runs.
///
/// OpenID Connect Discovery 1.0 §3: "If omitted, the default is
/// `client_secret_basic`".
const DEFAULT_TOKEN_ENDPOINT_AUTH_METHODS: [&str; 1] = ["client_secret_basic"];

/// The largest metadata document this server reads.
///
/// No specification governs the bound: our own design. A JWKS holds a handful
/// of public keys, so a megabyte is already generous.
const MAX_BODY: usize = 1 << 20;

/// A document the issuer did not give.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// The HTTPS client does not build, because the platform trust store does
    /// not open.
    #[error("cannot build the HTTPS client for the issuer")]
    Client {
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The URL does not parse.
    #[error("{url} is not a URL")]
    Url {
        /// The URL as given.
        url: String,
        /// The cause.
        #[source]
        source: http::Error,
    },
    /// The request did not complete.
    #[error("cannot reach {url}")]
    Request {
        /// The URL asked for.
        url: String,
        /// The cause.
        #[source]
        source: hyper_util::client::legacy::Error,
    },
    /// No answer arrived within [`TIMEOUT`].
    #[error("{url} did not answer within {timeout:?}")]
    Timeout {
        /// The URL asked for.
        url: String,
        /// How long the fetch waited.
        timeout: Duration,
    },
    /// The issuer answered something other than `200 OK`.
    #[error("{url} answered {status}")]
    Status {
        /// The URL asked for.
        url: String,
        /// The status it answered.
        status: StatusCode,
    },
    /// The body does not read, or exceeds the size this server accepts.
    #[error("cannot read the body of {url}")]
    Body {
        /// The URL asked for.
        url: String,
        /// The cause.
        #[source]
        source: axum::Error,
    },
    /// The body is not the JSON document this server expects.
    #[error("{url} did not answer the expected JSON document")]
    Parse {
        /// The URL asked for.
        url: String,
        /// The cause.
        #[source]
        source: serde_json::Error,
    },
    /// The document names an issuer other than the one it was fetched from.
    #[error("{url} declares the issuer `{declared}`, which is not `{configured}`")]
    IssuerMismatch {
        /// The URL asked for.
        url: String,
        /// The issuer the document declares.
        declared: String,
        /// The issuer the deployment configured.
        configured: String,
    },
    /// A URL that decides who may write arrives over cleartext HTTP.
    #[error("{url} is not an https URL, and only a loopback issuer may be plain HTTP")]
    NotSecure {
        /// The URL as given.
        url: String,
    },
}

/// `values` as owned strings.
fn owned(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

/// Whether `url` may be fetched: `https`, or plain HTTP on the loopback host.
///
/// OpenID Connect Discovery 1.0 §7.1 requires TLS on the issuer's endpoints,
/// and §3 says `jwks_uri` "MUST use the https scheme", because these bytes
/// decide which signatures this server trusts. A loopback issuer is the one
/// place the transport cannot be observed, and a test issuer is that. The
/// usage report holds its collector to the same rule.
pub(crate) fn secure(url: &str) -> bool {
    if url.starts_with("https://") {
        return true;
    }
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    // An IPv6 literal is bracketed, so its colons are not the port separator
    // (RFC 3986 §3.2.2).
    let host = match authority.split_once(']') {
        Some((bracketed, _)) => bracketed.strip_prefix('[').unwrap_or(bracketed),
        None => authority.split(':').next().unwrap_or(authority),
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// The issuer metadata this server reads.
///
/// Only the members the SMART discovery document and the token check need are
/// modelled; the issuer may publish more
/// (<https://openid.net/specs/openid-connect-discovery-1_0.html>, §3).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct IssuerMetadata {
    /// `issuer`: the issuer identifier every token must claim.
    pub issuer: String,
    /// `jwks_uri`: where the signing keys are published (RFC 7517).
    pub jwks_uri: String,
    /// `authorization_endpoint`, absent from an issuer that runs no
    /// authorization code flow.
    ///
    /// OpenID Connect Discovery 1.0 §3 makes it required, and an authorization
    /// server that only mints Backend Services tokens has none, so the absence
    /// is read rather than refused.
    #[serde(default)]
    pub authorization_endpoint: Option<String>,
    /// `token_endpoint`: where a client obtains an access token.
    pub token_endpoint: String,
    /// `registration_endpoint`, when the issuer registers clients dynamically.
    #[serde(default)]
    pub registration_endpoint: Option<String>,
    /// `introspection_endpoint` (RFC 7662), when the issuer offers one.
    #[serde(default)]
    pub introspection_endpoint: Option<String>,
    /// `revocation_endpoint` (RFC 7009), when the issuer offers one.
    #[serde(default)]
    pub revocation_endpoint: Option<String>,
    /// `grant_types_supported`.
    #[serde(default)]
    pub grant_types_supported: Vec<String>,
    /// `scopes_supported`.
    #[serde(default)]
    pub scopes_supported: Vec<String>,
    /// `response_types_supported`.
    #[serde(default)]
    pub response_types_supported: Vec<String>,
    /// `token_endpoint_auth_methods_supported`.
    #[serde(default)]
    pub token_endpoint_auth_methods_supported: Vec<String>,
    /// `code_challenge_methods_supported` (RFC 7636).
    #[serde(default)]
    pub code_challenge_methods_supported: Vec<String>,
}

/// The HTTPS client this server asks the issuer with.
///
/// It is built once, so the connection pool and the trust store survive across
/// the discovery fetch and every later JWKS refresh.
pub struct Http {
    client: Client<hyper_rustls::HttpsConnector<HttpConnector>, Body>,
}

impl std::fmt::Debug for Http {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Http").finish_non_exhaustive()
    }
}

impl Http {
    /// Builds the client over the platform's trust store.
    ///
    /// The trust store is the platform's, so a deployment whose issuer runs
    /// under a private certificate authority installs that authority in the
    /// image and needs nothing else here.
    ///
    /// # Errors
    ///
    /// Returns [`FetchError::Client`] when the trust store does not open.
    pub fn new() -> Result<Self, FetchError> {
        let roots = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(|source| FetchError::Client { source })?;
        // NOTE: `https_or_http` keeps a plain-HTTP issuer reachable, which is
        // what a test issuer and a loopback issuer are
        // (<https://docs.rs/hyper-rustls/0.27/hyper_rustls/struct.HttpsConnectorBuilder.html>).
        let connector = roots.https_or_http().enable_http1().build();
        Ok(Self {
            client: Client::builder(TokioExecutor::new()).build(connector),
        })
    }

    /// Fetches `url` and parses the JSON document it answers.
    ///
    /// # Errors
    ///
    /// Returns [`FetchError`] when the URL is not `https` (or loopback), does
    /// not parse, the request fails or exceeds [`TIMEOUT`], the answer is not
    /// `200 OK`, or the body is not the document this server expects.
    pub async fn json<T>(&self, url: &str) -> Result<T, FetchError>
    where
        T: serde::de::DeserializeOwned,
    {
        if !secure(url) {
            return Err(FetchError::NotSecure {
                url: url.to_owned(),
            });
        }
        let request = Request::get(url)
            .header(http::header::ACCEPT, "application/json")
            .body(Body::empty())
            .map_err(|source| FetchError::Url {
                url: url.to_owned(),
                source,
            })?;
        let response = tokio::time::timeout(TIMEOUT, self.client.request(request))
            .await
            .map_err(|_elapsed| FetchError::Timeout {
                url: url.to_owned(),
                timeout: TIMEOUT,
            })?
            .map_err(|source| FetchError::Request {
                url: url.to_owned(),
                source,
            })?;
        let status = response.status();
        if status != StatusCode::OK {
            return Err(FetchError::Status {
                url: url.to_owned(),
                status,
            });
        }
        let bytes = axum::body::to_bytes(Body::new(response.into_body()), MAX_BODY)
            .await
            .map_err(|source| FetchError::Body {
                url: url.to_owned(),
                source,
            })?;
        serde_json::from_slice(&bytes).map_err(|source| FetchError::Parse {
            url: url.to_owned(),
            source,
        })
    }

    /// Fetches the discovery document of `issuer` and checks it names `issuer`.
    ///
    /// The issuer identifier in the document has to be the one the document was
    /// fetched under, so a redirected or mistyped issuer refuses the start
    /// instead of validating tokens from somewhere else
    /// (<https://openid.net/specs/openid-connect-discovery-1_0.html>, §4.3).
    ///
    /// # Errors
    ///
    /// Returns [`FetchError`] when the document does not arrive, does not
    /// parse, or declares another issuer.
    pub async fn discover(&self, issuer: &str) -> Result<IssuerMetadata, FetchError> {
        let url = format!("{}/{DISCOVERY_PATH}", issuer.trim_end_matches('/'));
        let mut metadata: IssuerMetadata = self.json(&url).await?;
        if metadata.issuer.trim_end_matches('/') != issuer.trim_end_matches('/') {
            return Err(FetchError::IssuerMismatch {
                url,
                declared: metadata.issuer,
                configured: issuer.to_owned(),
            });
        }
        // NOTE: an omitted member means its documented default, not an issuer
        // that supports nothing (OpenID Connect Discovery 1.0 §3).
        if metadata.grant_types_supported.is_empty() {
            metadata.grant_types_supported = owned(&DEFAULT_GRANT_TYPES);
        }
        if metadata.token_endpoint_auth_methods_supported.is_empty() {
            metadata.token_endpoint_auth_methods_supported =
                owned(&DEFAULT_TOKEN_ENDPOINT_AUTH_METHODS);
        }
        Ok(metadata)
    }
}
