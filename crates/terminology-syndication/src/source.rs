// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The [`Source`] seam: one syndication service, with its own authentication.
//!
//! A source is configuration plus an authentication scheme. The Atom dialect,
//! the selection rules, and the checksum-verified download are shared, so an
//! add-on states where its feed is, what it is expected to offer, and how to
//! authorize a listing and a download. The two are separate because the
//! services differ: some challenge the listing itself, some serve the listing
//! openly and challenge only the download, and some challenge neither.
//!
//! # The add-on contract
//!
//! An add-on implements [`Source::name`], [`Source::feed_url`],
//! [`Source::yields`], [`Source::client`], [`Source::listing_auth`], and
//! [`Source::download_auth`], and takes [`Source::list`] and
//! [`Source::fetch`] as they are. It overrides either of those two only when
//! its service departs from the dialect, and says so in its own
//! documentation. An add-on whose service keeps FHIR resources behind its
//! FHIR API rather than in the feed answers [`Source::fhir_api_url`], and
//! takes [`Source::list_api`] and [`Source::fetch_api`] as they are. The trait is object-safe, so a caller holds every configured
//! add-on in one `Vec<Box<dyn Source>>`.
//!
//! An add-on obtains and refreshes its credentials inside
//! [`Source::listing_auth`] and [`Source::download_auth`], which are called
//! once per request, so a token that expires mid-run is renewed without the
//! caller knowing that tokens exist. Credentials come from the deployment's
//! own configuration; nothing in this crate reads a file or an environment
//! variable.

use core::fmt;
use core::future::Future;
use core::pin::Pin;
use std::path::Path;

use crate::download::{DownloadError, Fetched};
use crate::model::{CategoryTerm, ContentLink, Feed};
use crate::parse::ParseError;

/// A future a [`Source`] method returns.
///
/// The methods are spelled as boxed futures rather than `async fn` because an
/// `async fn` in a trait makes the trait dyn-incompatible, and a caller has to
/// hold its configured sources as trait objects
/// (<https://doc.rust-lang.org/reference/items/traits.html#dyn-compatibility>).
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What a request carries to prove it may read.
///
/// The `Debug` rendering names the scheme and never the secret, so a source
/// can be logged whole.
#[derive(Clone, PartialEq, Eq, Default)]
pub enum Authorization {
    /// The request carries nothing; the service is open.
    #[default]
    Open,
    /// The request carries an `Authorization: Bearer` token.
    Bearer(String),
    /// The request carries HTTP basic credentials.
    Basic {
        /// The account name.
        user: String,
        /// The account's secret.
        password: String,
    },
}

impl Authorization {
    /// Applies the authorization to a request.
    pub fn apply(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self {
            Self::Open => request,
            Self::Bearer(token) => request.bearer_auth(token),
            Self::Basic { user, password } => request.basic_auth(user, Some(password)),
        }
    }

    /// The scheme's name, for a log line or a run record.
    #[must_use]
    pub const fn scheme(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Bearer(_) => "bearer",
            Self::Basic { .. } => "basic",
        }
    }
}

impl fmt::Debug for Authorization {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Authorization({})", self.scheme())
    }
}

/// A source that could not be listed or fetched from.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// The source could not obtain the credentials a request needs.
    #[error("{source_name} could not authenticate")]
    Authentication {
        /// The source that failed to authenticate.
        source_name: String,
        /// Why authentication failed.
        #[source]
        source: Box<dyn core::error::Error + Send + Sync>,
    },
    /// The listing request failed.
    #[error("the listing request to {url} failed")]
    Request {
        /// The address the request was sent to.
        url: String,
        /// Why the request failed.
        #[source]
        source: reqwest::Error,
    },
    /// The listing answered with a status that is not a success.
    #[error("{url} answered {status}")]
    Status {
        /// The address the request was sent to.
        url: String,
        /// The status the server answered with.
        status: reqwest::StatusCode,
    },
    /// The listing could not be read as a syndication document.
    #[error("the listing at {url} could not be read")]
    Parse {
        /// The address the listing came from.
        url: String,
        /// Why the listing could not be read.
        #[source]
        source: ParseError,
    },
    /// The entry advertises no digest, so its bytes cannot be verified.
    #[error("the entry linking {url} advertises no checksum")]
    NoChecksum {
        /// The address the entry links.
        url: String,
    },
    /// The content item could not be fetched.
    #[error("a content item could not be fetched")]
    Download(#[from] DownloadError),
    /// A page of the FHIR API listing is not a JSON bundle.
    #[error("the FHIR API page at {url} could not be read")]
    ApiParse {
        /// The address the page came from.
        url: String,
        /// Why the page could not be read.
        #[source]
        source: serde_json::Error,
    },
    /// The source offers no FHIR API to list.
    #[error("{source_name} offers no FHIR API")]
    NoFhirApi {
        /// The source that was asked.
        source_name: String,
    },
}

/// One syndication service an add-on speaks to.
///
/// The contract an add-on implements is described at the module level.
pub trait Source: fmt::Debug + Send + Sync {
    /// The source's name, as a run record and a log line name it.
    fn name(&self) -> &str;

    /// The address of the source's Atom feed.
    fn feed_url(&self) -> &str;

    /// The category terms the source is expected to offer.
    ///
    /// A run compares this with what the feed actually carries, so a service
    /// that stops publishing a kind of content is noticed.
    fn yields(&self) -> &[CategoryTerm];

    /// The HTTP client the source's requests go through.
    fn client(&self) -> &reqwest::Client;

    /// The authorization the listing request carries.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::Authentication`] when the source cannot obtain
    /// the credentials the listing needs.
    fn listing_auth(&self) -> BoxFuture<'_, Result<Authorization, SourceError>>;

    /// The authorization a download request carries.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::Authentication`] when the source cannot obtain
    /// the credentials a download needs.
    fn download_auth(&self) -> BoxFuture<'_, Result<Authorization, SourceError>>;

    /// Reads the source's feed.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::Authentication`] when the listing cannot be
    /// authorized, [`SourceError::Request`] when the request fails,
    /// [`SourceError::Status`] when the server answers with a status that is
    /// not a success, and [`SourceError::Parse`] when the answer is not a
    /// syndication document.
    fn list(&self) -> BoxFuture<'_, Result<Feed, SourceError>> {
        Box::pin(async move {
            let url = self.feed_url().to_owned();
            let authorization = self.listing_auth().await?;
            let response = authorization
                .apply(self.client().get(&url))
                .send()
                .await
                .map_err(|source| SourceError::Request {
                    url: url.clone(),
                    source,
                })?;
            let status = response.status();
            if !status.is_success() {
                return Err(SourceError::Status { url, status });
            }
            let body = response
                .text()
                .await
                .map_err(|source| SourceError::Request {
                    url: url.clone(),
                    source,
                })?;
            crate::parse::feed(&body).map_err(|source| SourceError::Parse { url, source })
        })
    }

    /// The FHIR endpoint whose resources the source lists beside its feed.
    ///
    /// `None`, the default, is a source whose feed carries everything.
    fn fhir_api_url(&self) -> Option<&str> {
        None
    }

    /// Lists the source's FHIR API as a feed.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::NoFhirApi`] when the source offers no FHIR API,
    /// [`SourceError::Authentication`] when the listing cannot be authorized,
    /// and the errors of [`crate::fhir_api::list`] when a page cannot be read.
    fn list_api(&self) -> BoxFuture<'_, Result<Feed, SourceError>> {
        Box::pin(async move {
            let Some(base_url) = self.fhir_api_url() else {
                return Err(SourceError::NoFhirApi {
                    source_name: self.name().to_owned(),
                });
            };
            let authorization = self.listing_auth().await?;
            crate::fhir_api::list(self.client(), base_url, &authorization).await
        })
    }

    /// Fetches one resource the FHIR API lists to `destination`.
    ///
    /// The API advertises no digest, so the digest of what arrived is computed
    /// and answered rather than verified.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::Authentication`] when the download cannot be
    /// authorized and [`SourceError::Download`] when the bytes do not arrive.
    fn fetch_api<'a>(
        &'a self,
        link: &'a ContentLink,
        destination: &'a Path,
    ) -> BoxFuture<'a, Result<Fetched, SourceError>> {
        Box::pin(async move {
            let authorization = self.download_auth().await?;
            let request = authorization.apply(
                self.client()
                    .get(&link.href)
                    .header(reqwest::header::ACCEPT, crate::fhir_api::FHIR_JSON),
            );
            Ok(crate::download::to_file_unverified(request, destination).await?)
        })
    }

    /// Fetches one content link to `destination`, verifying its digest.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::Authentication`] when the download cannot be
    /// authorized, [`SourceError::NoChecksum`] when the entry advertises no
    /// digest, and [`SourceError::Download`] when the bytes do not arrive or
    /// do not verify.
    fn fetch<'a>(
        &'a self,
        link: &'a ContentLink,
        destination: &'a Path,
    ) -> BoxFuture<'a, Result<Fetched, SourceError>> {
        Box::pin(async move {
            let Some(expected) = link.checksum.as_ref() else {
                return Err(SourceError::NoChecksum {
                    url: link.href.clone(),
                });
            };
            let authorization = self.download_auth().await?;
            let request = authorization.apply(self.client().get(&link.href));
            Ok(crate::download::to_file(request, destination, expected).await?)
        })
    }
}
