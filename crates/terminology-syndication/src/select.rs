// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Deciding which feed entries a run takes, and naming why it leaves the rest.
//!
//! Every entry a run leaves behind carries a [`SkipReason`], so a run record
//! can state what the feed offered and what was done with it. Nothing is
//! dropped in silence.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::model::{CategoryTerm, ContentLink, Entry, Feed};

/// Which systems a subscription takes from a feed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Systems {
    /// Every system the feed offers.
    #[default]
    Any,
    /// Only these canonical identifiers (`ncts:contentItemIdentifier`).
    Listed(BTreeSet<String>),
}

impl Systems {
    /// Whether the subscription takes the content item `canonical` at
    /// `version`.
    ///
    /// A listed identifier admits the item that carries it. A listed SNOMED CT
    /// edition URI also admits an item identified by the bare code system
    /// whose version URI sits under that edition, which is how Ontoserver
    /// writes an edition release.
    #[must_use]
    pub fn admits(&self, canonical: &str, version: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Listed(listed) => {
                listed.contains(canonical)
                    || listed.iter().any(|edition| is_release_of(version, edition))
            }
        }
    }
}

// NOTE: the SNOMED CT URI Standard forms a versioned edition as
// `{edition URI}/version/{date}` (https://confluence.ihtsdotools.org/display/DOCURI),
// so a version URI under an edition names a release of that edition.
fn is_release_of(version: &str, edition: &str) -> bool {
    version
        .strip_prefix(edition)
        .is_some_and(|rest| rest.starts_with("/version/"))
}

/// What one run takes from one feed: which systems, and which entry kinds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Subscription {
    /// The systems the run takes.
    pub systems: Systems,
    /// The category terms the run takes.
    pub categories: BTreeSet<CategoryTerm>,
}

impl Subscription {
    /// An empty subscription: every system, no category.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Restricts the subscription to one more canonical identifier.
    #[must_use]
    pub fn with_system(mut self, canonical: impl Into<String>) -> Self {
        match &mut self.systems {
            Systems::Any => {
                self.systems = Systems::Listed(BTreeSet::from([canonical.into()]));
            }
            Systems::Listed(listed) => {
                listed.insert(canonical.into());
            }
        }
        self
    }

    /// Adds one category term to the subscription.
    #[must_use]
    pub fn with_category(mut self, term: CategoryTerm) -> Self {
        self.categories.insert(term);
        self
    }
}

/// The listed canonicals that no entry of any of `feeds` carries.
///
/// A service that scopes its listing to the account's licences shows an
/// unlicensed system as absent, so a subscribed canonical that appears in no
/// listing is reported by name: the run record then says the system was not
/// visible to the account, which is a licence question and never proof that
/// the service lacks it. A subscription to every system lists nothing.
#[must_use]
pub fn unseen(subscription: &Subscription, feeds: &[&Feed]) -> Vec<String> {
    let Systems::Listed(listed) = &subscription.systems else {
        return Vec::new();
    };
    listed
        .iter()
        .filter(|canonical| {
            !feeds.iter().any(|feed| {
                feed.entries.iter().any(|entry| {
                    let version = entry.content_item_version.as_deref().unwrap_or_default();
                    entry.content_item_identifier.as_deref() == Some(canonical.as_str())
                        || is_release_of(version, canonical)
                })
            })
        })
        .cloned()
        .collect()
}

/// The dates of the content items a deployment already serves.
///
/// The key is the canonical identifier and the version identifier of a content
/// item, which is the identity Ontoserver's update semantics compare
/// (<https://www.ontoserver.csiro.au/docs/6.22.5/syndication.html>).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Holdings {
    dates: BTreeMap<(String, String), jiff::Timestamp>,
}

impl Holdings {
    /// An empty set of holdings: nothing is served yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `canonical` at `version` is served, as of `date`.
    pub fn record(
        &mut self,
        canonical: impl Into<String>,
        version: impl Into<String>,
        date: jiff::Timestamp,
    ) {
        self.dates.insert((canonical.into(), version.into()), date);
    }

    /// The date of the held content item, when one with that identity is held.
    #[must_use]
    pub fn date_of(&self, canonical: &str, version: &str) -> Option<jiff::Timestamp> {
        self.dates
            .get(&(canonical.to_owned(), version.to_owned()))
            .copied()
    }

    /// How many content items are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.dates.len()
    }

    /// Whether nothing is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dates.is_empty()
    }
}

/// One entry the run takes, with the link its bytes come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Taken {
    /// The entry itself.
    pub entry: Entry,
    /// The entry's content link.
    pub link: ContentLink,
}

/// One entry the run leaves behind, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// The entry's identifier.
    pub entry_id: String,
    /// The entry's title, so a run record reads without the feed beside it.
    pub title: String,
    /// Why the entry was left behind.
    pub reason: SkipReason,
}

/// Why a run leaves a feed entry behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The entry is Ontoserver's precomputed binary index.
    BinaryIndex,
    /// The entry is an RF2 distribution that is not a snapshot.
    NotSnapshot {
        /// The release type the entry offers.
        term: CategoryTerm,
    },
    /// The entry names no canonical identifier, so it cannot be subscribed to.
    NoCanonical,
    /// The entry's system is outside the subscription.
    SystemNotSubscribed {
        /// The canonical identifier the entry names.
        canonical: String,
    },
    /// The entry's category is outside the subscription.
    CategoryNotSubscribed {
        /// The category term the entry declares.
        term: CategoryTerm,
    },
    /// The entry offers no `alternate` link, so there are no bytes to fetch.
    NoContentLink,
    /// The entry carries no date, so the replace rule cannot be applied.
    NoDate,
    /// The same canonical and version is already served, at the same date or a
    /// later one.
    NotNewer {
        /// The date of the content item already served.
        held: jiff::Timestamp,
        /// The date the entry offers.
        offered: jiff::Timestamp,
    },
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BinaryIndex => f.write_str(
                "the binary index is an Ontoserver-internal package, not a syndicable release",
            ),
            Self::NotSnapshot { term } => {
                write!(
                    f,
                    "{term} is not a snapshot, and a build reads the snapshot"
                )
            }
            Self::NoCanonical => f.write_str("the entry names no canonical identifier"),
            Self::SystemNotSubscribed { canonical } => {
                write!(f, "{canonical} is not subscribed")
            }
            Self::CategoryNotSubscribed { term } => write!(f, "{term} is not subscribed"),
            Self::NoContentLink => f.write_str("the entry offers no alternate link to fetch"),
            Self::NoDate => f.write_str("the entry carries no date to compare"),
            Self::NotNewer { held, offered } => write!(
                f,
                "the served copy is dated {held} and the entry offers {offered}"
            ),
        }
    }
}

/// What a run takes from one feed, and what it leaves behind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// The entries to fetch, in feed order.
    pub taken: Vec<Taken>,
    /// The entries left behind, in feed order, each with its reason.
    pub skipped: Vec<Skipped>,
}

/// Decides what to take from `feed` under `subscription`, given what is held.
///
/// The rules, in the order they are applied to one entry:
///
/// 1. Ontoserver's binary index is refused: it is an internal package format,
///    and a system that only arrives that way is reported rather than missed.
/// 2. An RF2 delta or full distribution is refused; a build reads the
///    snapshot.
/// 3. The entry's canonical identifier must be subscribed, and so must its
///    category.
/// 4. The entry must offer an `alternate` link.
/// 5. The replace rule: when the same canonical and version is already served,
///    the entry is taken only if its date is later, which is Ontoserver's
///    update semantics
///    (<https://www.ontoserver.csiro.au/docs/6.22.5/syndication.html>).
#[must_use]
pub fn select(feed: &Feed, subscription: &Subscription, holdings: &Holdings) -> Selection {
    let mut selection = Selection::default();
    for entry in &feed.entries {
        match decide(entry, subscription, holdings) {
            Ok(link) => selection.taken.push(Taken {
                entry: entry.clone(),
                link,
            }),
            Err(reason) => selection.skipped.push(Skipped {
                entry_id: entry.id.clone(),
                title: entry.title.clone(),
                reason,
            }),
        }
    }
    selection
}

fn decide(
    entry: &Entry,
    subscription: &Subscription,
    holdings: &Holdings,
) -> Result<ContentLink, SkipReason> {
    let term = entry.term();
    if term == CategoryTerm::BinaryIndex {
        return Err(SkipReason::BinaryIndex);
    }
    // NOTE: an RF2 distribution carrying every release type also carries the
    // Snapshot the build reads, so ALL is a snapshot source and DELTA and FULL
    // are not (no FHIR/SNOMED spec governs this selection: our own design).
    if matches!(
        term,
        CategoryTerm::SnomedRf2Delta | CategoryTerm::SnomedRf2Full
    ) {
        return Err(SkipReason::NotSnapshot { term });
    }
    let Some(canonical) = entry.content_item_identifier.as_deref() else {
        return Err(SkipReason::NoCanonical);
    };
    let version = entry.content_item_version.as_deref().unwrap_or_default();
    if !subscription.systems.admits(canonical, version) {
        return Err(SkipReason::SystemNotSubscribed {
            canonical: canonical.to_owned(),
        });
    }
    if !subscription.categories.contains(&term) {
        return Err(SkipReason::CategoryNotSubscribed { term });
    }
    let Some(link) = entry.content_link() else {
        return Err(SkipReason::NoContentLink);
    };
    let Some(offered) = entry.updated else {
        return Err(SkipReason::NoDate);
    };
    if let Some(held) = holdings.date_of(canonical, version)
        && offered <= held
    {
        return Err(SkipReason::NotNewer { held, offered });
    }
    Ok(link.clone())
}
