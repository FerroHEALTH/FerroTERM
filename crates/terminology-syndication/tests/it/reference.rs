// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A reference [`Source`] implementation, the shape an add-on takes.
//!
//! It implements only the six methods the contract requires and takes
//! `list` and `fetch` as the trait provides them, so the tests exercise the
//! seam an add-on actually sits on.

use terminology_syndication::model::CategoryTerm;
use terminology_syndication::source::{Authorization, BoxFuture, Source, SourceError};

/// A source configured with fixed credentials, pointed at a mock server.
#[derive(Debug)]
pub(crate) struct ReferenceSource {
    name: String,
    feed_url: String,
    client: reqwest::Client,
    listing: Authorization,
    download: Authorization,
    yields: Vec<CategoryTerm>,
}

impl ReferenceSource {
    /// A source that authenticates neither the listing nor a download.
    pub(crate) fn open(name: &str, feed_url: &str) -> Self {
        Self {
            name: name.to_owned(),
            feed_url: feed_url.to_owned(),
            client: reqwest::Client::new(),
            listing: Authorization::Open,
            download: Authorization::Open,
            yields: CategoryTerm::NAMED.to_vec(),
        }
    }

    /// The same source with an authorization on the listing request.
    #[must_use]
    pub(crate) fn with_listing_auth(mut self, authorization: Authorization) -> Self {
        self.listing = authorization;
        self
    }

    /// The same source with an authorization on a download request.
    #[must_use]
    pub(crate) fn with_download_auth(mut self, authorization: Authorization) -> Self {
        self.download = authorization;
        self
    }
}

impl Source for ReferenceSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn feed_url(&self) -> &str {
        &self.feed_url
    }

    fn yields(&self) -> &[CategoryTerm] {
        &self.yields
    }

    fn client(&self) -> &reqwest::Client {
        &self.client
    }

    fn listing_auth(&self) -> BoxFuture<'_, Result<Authorization, SourceError>> {
        Box::pin(async move { Ok(self.listing.clone()) })
    }

    fn download_auth(&self) -> BoxFuture<'_, Result<Authorization, SourceError>> {
        Box::pin(async move { Ok(self.download.clone()) })
    }
}

/// A source whose credentials cannot be obtained.
#[derive(Debug)]
pub(crate) struct UnauthenticatedSource {
    name: String,
    client: reqwest::Client,
    feed_url: String,
    yields: Vec<CategoryTerm>,
}

impl UnauthenticatedSource {
    /// A source that fails to authenticate every request.
    pub(crate) fn new(feed_url: &str) -> Self {
        Self {
            name: String::from("reference"),
            client: reqwest::Client::new(),
            feed_url: feed_url.to_owned(),
            yields: Vec::new(),
        }
    }

    fn refuse(&self) -> SourceError {
        SourceError::Authentication {
            source_name: self.name.clone(),
            source: Box::new(std::io::Error::other("the credential store is empty")),
        }
    }
}

impl Source for UnauthenticatedSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn feed_url(&self) -> &str {
        &self.feed_url
    }

    fn yields(&self) -> &[CategoryTerm] {
        &self.yields
    }

    fn client(&self) -> &reqwest::Client {
        &self.client
    }

    fn listing_auth(&self) -> BoxFuture<'_, Result<Authorization, SourceError>> {
        Box::pin(async move { Err(self.refuse()) })
    }

    fn download_auth(&self) -> BoxFuture<'_, Result<Authorization, SourceError>> {
        Box::pin(async move { Err(self.refuse()) })
    }
}

/// Matches a request that carries no `Authorization` header.
pub(crate) struct NoAuthorization;

impl wiremock::Match for NoAuthorization {
    fn matches(&self, request: &wiremock::Request) -> bool {
        !request.headers.contains_key("authorization")
    }
}
