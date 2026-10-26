// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The sources a run reads, and the add-on each one comes from.
//!
//! A source is an add-on from `addons/*` implementing
//! [`terminology_syndication::source::Source`], the subscription it runs
//! under, and the corrections its service needs on a FHIR resource file. The
//! service holds them as trait objects, so adding a national terminology
//! service is a new add-on crate and one arm of [`configure`].
//!
//! No FHIR or SNOMED CT specification governs this: our own design.

use core::fmt;

use terminology_syndication::select::{Holdings, Selection, Subscription};
use terminology_syndication::source::Source;

use crate::config::{NtsSection, SourceConfig};

/// A FHIR resource file a service publishes with a correction it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corrected {
    /// The bytes to write.
    pub bytes: Vec<u8>,
    /// What was corrected, one line each, for the run record.
    pub applied: Vec<String>,
}

/// A correction that could not be applied.
#[derive(Debug, thiserror::Error)]
#[error("the resource could not be corrected")]
pub struct FixupError {
    #[source]
    source: Box<dyn core::error::Error + Send + Sync>,
}

impl FixupError {
    /// The error carrying `source` as its cause.
    #[must_use]
    pub fn new(source: Box<dyn core::error::Error + Send + Sync>) -> Self {
        Self { source }
    }
}

/// The corrections one service's files need before they are served.
///
/// A service that publishes a resource its own specification refuses is
/// corrected by its add-on, which records what it changed. A file that needs
/// nothing comes back byte-identical with an empty list.
pub trait Fixups: fmt::Debug + Send + Sync {
    /// Corrects one resource file, naming what it changed.
    ///
    /// # Errors
    ///
    /// Returns [`FixupError`] when the file cannot be corrected, which leaves
    /// the entry failed and the served set untouched.
    fn apply(&self, bytes: &[u8]) -> Result<Corrected, FixupError>;
}

/// One configured source: the add-on, its subscription, and its corrections.
#[derive(Debug)]
pub struct ConfiguredSource {
    source: Box<dyn Source>,
    subscription: Subscription,
    fixups: Option<Box<dyn Fixups>>,
}

impl ConfiguredSource {
    /// The source `source`, taking what `subscription` names.
    #[must_use]
    pub fn new(source: Box<dyn Source>, subscription: Subscription) -> Self {
        Self {
            source,
            subscription,
            fixups: None,
        }
    }

    /// The same source with the corrections its service needs.
    #[must_use]
    pub fn with_fixups(mut self, fixups: Box<dyn Fixups>) -> Self {
        self.fixups = Some(fixups);
        self
    }

    /// The add-on itself.
    #[must_use]
    pub fn source(&self) -> &dyn Source {
        self.source.as_ref()
    }

    /// The corrections this service's files need, when it needs any.
    #[must_use]
    pub fn fixups(&self) -> Option<&dyn Fixups> {
        self.fixups.as_deref()
    }

    /// The subscribed canonicals none of `feeds` lists.
    #[must_use]
    pub fn unseen(&self, feeds: &[&terminology_syndication::model::Feed]) -> Vec<String> {
        terminology_syndication::select::unseen(&self.subscription, feeds)
    }

    /// What a run takes from `feed`, and what it leaves behind with a reason.
    #[must_use]
    pub fn select(
        &self,
        feed: &terminology_syndication::model::Feed,
        held: &Holdings,
    ) -> Selection {
        terminology_syndication::select::select(feed, &self.subscription, held)
    }
}

/// A source the service cannot build.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// No add-on compiled into this service reads that kind of service.
    #[error("`{kind}` is not a source this service reads; it reads {known}")]
    UnknownKind {
        /// The kind the configuration named.
        kind: String,
        /// The kinds the service does read.
        known: String,
    },
    /// The add-on's own configuration block does not read.
    #[error("the configuration of the `{kind}` source does not read")]
    Config {
        /// The kind whose block did not read.
        kind: String,
        /// What TOML found wrong with it.
        #[source]
        source: toml::de::Error,
    },
    /// A secret of the account the add-on authenticates with does not read.
    #[error("the account of the `{kind}` source does not read")]
    Credentials {
        /// The kind whose account did not read.
        kind: String,
        /// Why it did not read.
        #[source]
        source: ferrohealth_secret::file::SecretError,
    },
    /// The add-on could not be built from its configuration.
    #[error("the `{kind}` source could not be built")]
    Build {
        /// The kind that could not be built.
        kind: String,
        /// Why it could not be built.
        #[source]
        source: Box<dyn core::error::Error + Send + Sync>,
    },
}

/// The kinds of service this build reads, in the order they are registered.
pub const KINDS: [&str; 1] = ["nts"];

/// Builds every configured source.
///
/// # Errors
///
/// Returns [`RegistryError::UnknownKind`] when no add-on reads that kind,
/// [`RegistryError::Config`] when the add-on's own block does not read, and
/// [`RegistryError::Credentials`] when a secret of the account `nts` names
/// does not read, and [`RegistryError::Build`] when the add-on cannot be
/// built, which is what missing credentials look like.
pub fn configure(
    sources: &[SourceConfig],
    nts: &NtsSection,
) -> Result<Vec<ConfiguredSource>, RegistryError> {
    let mut out = Vec::with_capacity(sources.len());
    for source in sources {
        out.push(match source.kind.as_str() {
            "nts" => nts_source(source, nts)?,
            other => {
                return Err(RegistryError::UnknownKind {
                    kind: other.to_owned(),
                    known: KINDS.join(", "),
                });
            }
        });
    }
    Ok(out)
}

/// The Nationale Terminologie Server add-on, configured from its own block
/// and authenticating with the account `account` names.
fn nts_source(
    source: &SourceConfig,
    account: &NtsSection,
) -> Result<ConfiguredSource, RegistryError> {
    let config: addon_nts::config::NtsConfig =
        source
            .config
            .clone()
            .try_into()
            .map_err(|source| RegistryError::Config {
                kind: String::from("nts"),
                source,
            })?;
    let credentials = account
        .credentials()
        .map_err(|source| RegistryError::Credentials {
            kind: String::from("nts"),
            source,
        })?;
    let built = addon_nts::source::NtsSource::new(&config, credentials).map_err(|error| {
        RegistryError::Build {
            kind: String::from("nts"),
            source: Box::new(error),
        }
    })?;
    let subscription = built.subscription().clone();
    Ok(ConfiguredSource::new(Box::new(built), subscription).with_fixups(Box::new(NtsFixups)))
}

/// The corrections the Nationale Terminologie Server's files need.
///
/// The add-on owns what a correction is; this is the seam the service calls it
/// through, so the lane that writes a resource file stays the same for every
/// service.
#[derive(Debug, Clone, Copy)]
struct NtsFixups;

impl Fixups for NtsFixups {
    fn apply(&self, bytes: &[u8]) -> Result<Corrected, FixupError> {
        let corrected =
            addon_nts::fixup::normalize(bytes).map_err(|error| FixupError::new(Box::new(error)))?;
        Ok(Corrected {
            bytes: corrected.bytes,
            applied: corrected.fixups.iter().map(ToString::to_string).collect(),
        })
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
mod tests {
    use super::{Fixups, NtsFixups, configure};
    use crate::config::{NtsSection, SourceConfig};

    #[test]
    fn an_unknown_kind_names_the_kinds_this_build_reads() {
        let configured = configure(
            &[SourceConfig {
                kind: String::from("mlds"),
                config: toml::Table::new(),
            }],
            &NtsSection::default(),
        );
        match configured {
            Ok(_) => panic!("a service this build cannot read is refused at start-up"),
            Err(error) => assert!(
                error.to_string().contains("nts"),
                "the refusal names what this build does read: {error}"
            ),
        }
    }

    #[test]
    fn a_file_that_needs_no_correction_comes_back_byte_identical()
    -> Result<(), Box<dyn core::error::Error>> {
        let bytes =
            br#"{"resourceType":"ValueSet","url":"https://example.invalid/vs","version":"1"}"#;
        let corrected = NtsFixups.apply(bytes)?;
        assert_eq!(
            corrected.bytes, bytes,
            "a resource that needs nothing is written as it arrived"
        );
        assert!(
            corrected.applied.is_empty(),
            "nothing was corrected, so nothing is reported"
        );
        Ok(())
    }
}
