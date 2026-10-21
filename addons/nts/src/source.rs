// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The add-on itself: one configured service behind a bearer challenge.
//!
//! The service challenges both the listing and a download, and both carry the
//! same access token, so [`Source::listing_auth`] and [`Source::download_auth`]
//! answer from one authenticator. Reading the feed, applying the selection
//! rules, and verifying a download are the shared crate's; this type adds the
//! service's address, its authentication, its subscription, and the one
//! content correction its files need.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use terminology_syndication::download::Fetched;
use terminology_syndication::model::{CategoryTerm, ContentLink, Feed};
use terminology_syndication::select::{Holdings, Selection, Subscription};
use terminology_syndication::source::{Authorization, BoxFuture, Source, SourceError};

use crate::auth::{Authenticator, Clock, SystemClock};
use crate::config::NtsConfig;
use crate::credentials::{CredentialError, Credentials};
use crate::fixup::{Fixup, FixupError};

/// The add-on could not be built, or a content item could not be taken.
#[derive(Debug, thiserror::Error)]
pub enum NtsError {
    /// The credentials could not be read.
    #[error("the credentials could not be read")]
    Credentials(#[from] CredentialError),
    /// The HTTP client could not be built.
    #[error("the HTTP client could not be built")]
    Client {
        /// Why the client could not be built.
        #[source]
        source: reqwest::Error,
    },
    /// The feed could not be listed, or the item could not be fetched.
    #[error("the syndication request failed")]
    Syndication(#[from] SourceError),
    /// The fetched file could not be read or written.
    #[error("{path} could not be read or written")]
    File {
        /// The file the run was working on.
        path: std::path::PathBuf,
        /// Why the file could not be read or written.
        #[source]
        source: std::io::Error,
    },
    /// The fetched resource could not be corrected.
    #[error("the fetched resource could not be corrected")]
    Fixup(#[from] FixupError),
}

/// A subscribed system the service offers in no form a run can take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotSyndicable {
    /// The canonical identifier of the system.
    pub canonical: String,
    /// The only category term the system is offered under.
    pub term: CategoryTerm,
}

impl core::fmt::Display for NotSyndicable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} is offered only as {}, which is not syndicable",
            self.canonical, self.term
        )
    }
}

/// A FHIR resource file that arrived, with the corrections it needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedResource {
    /// Where the bytes were written, and the digest that was verified.
    pub fetched: Fetched,
    /// The corrections applied after the digest was verified.
    pub fixups: Vec<Fixup>,
}

/// The Nationale Terminologie Server as a syndication source.
#[derive(Debug)]
pub struct NtsSource {
    name: String,
    feed_url: String,
    fhir_api_url: Option<String>,
    client: reqwest::Client,
    authenticator: Authenticator,
    subscription: Subscription,
    yields: Vec<CategoryTerm>,
}

impl NtsSource {
    /// The add-on configured from `config`, with the credentials it names.
    ///
    /// # Errors
    ///
    /// Returns [`NtsError::Credentials`] when the credentials cannot be read
    /// or configure no usable grant, and [`NtsError::Client`] when the HTTP
    /// client cannot be built.
    pub fn new(config: &NtsConfig) -> Result<Self, NtsError> {
        let credentials = Credentials::load(&config.credentials)?;
        let client = reqwest::Client::builder()
            .build()
            .map_err(|source| NtsError::Client { source })?;
        Ok(Self::with_parts(
            config,
            credentials,
            client,
            Arc::new(SystemClock),
        ))
    }

    /// The add-on with its HTTP client, credentials, and clock supplied.
    #[must_use]
    pub fn with_parts(
        config: &NtsConfig,
        credentials: Credentials,
        client: reqwest::Client,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let authenticator = Authenticator::new(
            config.name.clone(),
            config.discovery_url(),
            client.clone(),
            credentials,
            clock,
        );
        Self {
            name: config.name.clone(),
            feed_url: config.feed_url(),
            fhir_api_url: config.fhir_api_url(),
            client,
            authenticator,
            subscription: config.subscription(),
            yields: config.yields(),
        }
    }

    /// The subscription this source runs under.
    #[must_use]
    pub fn subscription(&self) -> &Subscription {
        &self.subscription
    }

    /// What a run takes from `feed`, and what it leaves behind with a reason.
    #[must_use]
    pub fn select(&self, feed: &Feed, holdings: &Holdings) -> Selection {
        terminology_syndication::select::select(feed, &self.subscription, holdings)
    }

    /// The subscribed systems `feed` offers in no form a run can take.
    ///
    /// Ontoserver's binary index is an internal package rather than a release,
    /// so a system offered only that way is reported by name instead of going
    /// missing from the run record.
    #[must_use]
    pub fn not_syndicable(&self, feed: &Feed) -> Vec<NotSyndicable> {
        let mut offered: BTreeMap<&str, BTreeSet<CategoryTerm>> = BTreeMap::new();
        for entry in &feed.entries {
            let Some(canonical) = entry.content_item_identifier.as_deref() else {
                continue;
            };
            let version = entry.content_item_version.as_deref().unwrap_or_default();
            if !self.subscription.systems.admits(canonical, version) {
                continue;
            }
            offered.entry(canonical).or_default().insert(entry.term());
        }
        offered
            .into_iter()
            .filter(|(_, terms)| {
                !terms.is_empty() && terms.iter().all(|term| *term == CategoryTerm::BinaryIndex)
            })
            .map(|(canonical, _)| NotSyndicable {
                canonical: canonical.to_owned(),
                term: CategoryTerm::BinaryIndex,
            })
            .collect()
    }

    /// Fetches one FHIR resource file and corrects it for this service.
    ///
    /// The digest the feed advertises is verified against the bytes that
    /// arrived, and the correction runs after that, so a corrected file no
    /// longer carries the advertised digest and the returned list says why.
    ///
    /// # Errors
    ///
    /// Returns [`NtsError::Syndication`] when the item cannot be fetched or
    /// verified, [`NtsError::File`] when the file cannot be read or written,
    /// and [`NtsError::Fixup`] when the resource cannot be corrected.
    pub async fn fetch_resource(
        &self,
        link: &ContentLink,
        destination: &Path,
    ) -> Result<FetchedResource, NtsError> {
        let fetched = self.fetch(link, destination).await?;
        let arrived = tokio::fs::read(&fetched.path)
            .await
            .map_err(|source| NtsError::File {
                path: fetched.path.clone(),
                source,
            })?;
        let corrected = crate::fixup::normalize(&arrived)?;
        if !corrected.fixups.is_empty() {
            tokio::fs::write(&fetched.path, &corrected.bytes)
                .await
                .map_err(|source| NtsError::File {
                    path: fetched.path.clone(),
                    source,
                })?;
            for fixup in &corrected.fixups {
                tracing::info!(source = %self.name, path = %fetched.path.display(), %fixup, "a resource was corrected");
            }
        }
        Ok(FetchedResource {
            fetched,
            fixups: corrected.fixups,
        })
    }

    /// The bearer token every request to this service carries.
    fn bearer(&self) -> BoxFuture<'_, Result<Authorization, SourceError>> {
        Box::pin(async move {
            let token = self.authenticator.access_token().await.map_err(|source| {
                SourceError::Authentication {
                    source_name: self.name.clone(),
                    source: Box::new(source),
                }
            })?;
            Ok(Authorization::Bearer(token))
        })
    }
}

impl Source for NtsSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn feed_url(&self) -> &str {
        &self.feed_url
    }

    fn fhir_api_url(&self) -> Option<&str> {
        self.fhir_api_url.as_deref()
    }

    fn yields(&self) -> &[CategoryTerm] {
        &self.yields
    }

    fn client(&self) -> &reqwest::Client {
        &self.client
    }

    fn listing_auth(&self) -> BoxFuture<'_, Result<Authorization, SourceError>> {
        self.bearer()
    }

    fn download_auth(&self) -> BoxFuture<'_, Result<Authorization, SourceError>> {
        self.bearer()
    }
}
