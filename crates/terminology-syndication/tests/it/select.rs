// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The selection rules over the synthetic feed.

use terminology_syndication::model::{CategoryTerm, Feed};
use terminology_syndication::parse;
use terminology_syndication::select::{self, Holdings, SkipReason, Subscription, Systems};

use crate::fixtures;

const EDITION: &str = "http://snomed.info/sct/11000001107";
const EXTENSION: &str = "http://snomed.info/sct/21000001104";
const VALUE_SET: &str = "https://example.invalid/fhir/ValueSet/example";

fn feed() -> Feed {
    parse::feed(&fixtures::synthetic("full-feed.xml")).expect("the fixture is a well-formed feed")
}

/// A subscription taking everything the crate names, from every system.
fn everything() -> Subscription {
    CategoryTerm::NAMED
        .into_iter()
        .fold(Subscription::new(), Subscription::with_category)
}

fn reason_for(
    feed: &Feed,
    subscription: &Subscription,
    holdings: &Holdings,
    title: &str,
) -> SkipReason {
    select::select(feed, subscription, holdings)
        .skipped
        .into_iter()
        .find(|skipped| skipped.title == title)
        .unwrap_or_else(|| panic!("{title} is skipped"))
        .reason
}

fn taken_titles(feed: &Feed, subscription: &Subscription, holdings: &Holdings) -> Vec<String> {
    select::select(feed, subscription, holdings)
        .taken
        .into_iter()
        .map(|taken| taken.entry.title)
        .collect()
}

#[test]
fn an_unsubscribed_system_is_skipped_by_name() {
    let subscription = everything().with_system(EDITION);
    let reason = reason_for(
        &feed(),
        &subscription,
        &Holdings::new(),
        "Example Value Set",
    );
    assert_eq!(
        reason,
        SkipReason::SystemNotSubscribed {
            canonical: String::from(VALUE_SET),
        }
    );
}

#[test]
fn a_subscribed_system_is_taken() {
    let subscription = everything().with_system(EDITION);
    assert_eq!(
        taken_titles(&feed(), &subscription, &Holdings::new()),
        ["Example Edition 31 January 2026 (RF2 SNAPSHOT)"]
    );
}

#[test]
fn an_unsubscribed_category_is_skipped_by_term() {
    let subscription = Subscription::new().with_category(CategoryTerm::SnomedRf2Snapshot);
    let reason = reason_for(
        &feed(),
        &subscription,
        &Holdings::new(),
        "Example Value Set",
    );
    assert_eq!(
        reason,
        SkipReason::CategoryNotSubscribed {
            term: CategoryTerm::FhirValueSet,
        }
    );
}

#[test]
fn a_delta_distribution_is_skipped_because_a_build_reads_the_snapshot() {
    let reason = reason_for(
        &feed(),
        &everything(),
        &Holdings::new(),
        "Example Edition 31 January 2026 (RF2 DELTA)",
    );
    assert_eq!(
        reason,
        SkipReason::NotSnapshot {
            term: CategoryTerm::SnomedRf2Delta,
        }
    );
    assert_eq!(
        reason.to_string(),
        "SCT_RF2_DELTA is not a snapshot, and a build reads the snapshot"
    );
}

#[test]
fn a_full_distribution_is_skipped_because_a_build_reads_the_snapshot() {
    let reason = reason_for(
        &feed(),
        &everything(),
        &Holdings::new(),
        "Example Edition 31 January 2026 (RF2 FULL)",
    );
    assert_eq!(
        reason,
        SkipReason::NotSnapshot {
            term: CategoryTerm::SnomedRf2Full,
        }
    );
}

#[test]
fn a_distribution_of_every_release_type_is_taken_because_it_carries_the_snapshot() {
    let subscription = everything().with_system(EXTENSION);
    assert_eq!(
        taken_titles(&feed(), &subscription, &Holdings::new()),
        ["Example Extension 15 December 2025 v1.0"]
    );
}

#[test]
fn the_binary_index_is_refused_with_its_own_reason() {
    let reason = reason_for(
        &feed(),
        &everything(),
        &Holdings::new(),
        "Example Binary Index 2.0.13",
    );
    assert_eq!(reason, SkipReason::BinaryIndex);
    assert_eq!(
        reason.to_string(),
        "the binary index is an Ontoserver-internal package, not a syndicable release"
    );
}

#[test]
fn the_binary_index_is_refused_even_when_its_system_and_category_are_subscribed() {
    let subscription = everything().with_system(EDITION);
    let reason = reason_for(
        &feed(),
        &subscription,
        &Holdings::new(),
        "Example Binary Index 2.0.13",
    );
    assert_eq!(
        reason,
        SkipReason::BinaryIndex,
        "a system that only arrives as a binary index shows up as not syndicable"
    );
}

#[test]
fn an_entry_with_no_content_link_is_skipped() {
    let subscription = everything().with_system("https://example.invalid/fhir/ValueSet/linkless");
    let reason = reason_for(
        &feed(),
        &subscription,
        &Holdings::new(),
        "Example Entry Without A Content Link",
    );
    assert_eq!(reason, SkipReason::NoContentLink);
}

#[test]
fn a_held_copy_at_the_same_date_is_not_replaced() {
    let mut holdings = Holdings::new();
    let offered: jiff::Timestamp = "2026-01-31T09:00:00Z".parse().expect("a fixed timestamp");
    holdings.record(EDITION, format!("{EDITION}/version/20260131"), offered);
    let subscription = everything().with_system(EDITION);
    let reason = reason_for(
        &feed(),
        &subscription,
        &holdings,
        "Example Edition 31 January 2026 (RF2 SNAPSHOT)",
    );
    assert_eq!(
        reason,
        SkipReason::NotNewer {
            held: offered,
            offered,
        }
    );
}

#[test]
fn a_held_copy_at_a_later_date_is_not_replaced() {
    let mut holdings = Holdings::new();
    let held: jiff::Timestamp = "2026-02-01T09:00:00Z".parse().expect("a fixed timestamp");
    holdings.record(EDITION, format!("{EDITION}/version/20260131"), held);
    let subscription = everything().with_system(EDITION);
    let reason = reason_for(
        &feed(),
        &subscription,
        &holdings,
        "Example Edition 31 January 2026 (RF2 SNAPSHOT)",
    );
    let offered: jiff::Timestamp = "2026-01-31T09:00:00Z".parse().expect("a fixed timestamp");
    assert_eq!(reason, SkipReason::NotNewer { held, offered });
}

#[test]
fn a_held_copy_at_an_earlier_date_is_replaced() {
    let mut holdings = Holdings::new();
    let held: jiff::Timestamp = "2026-01-01T09:00:00Z".parse().expect("a fixed timestamp");
    holdings.record(EDITION, format!("{EDITION}/version/20260131"), held);
    let subscription = everything().with_system(EDITION);
    assert_eq!(
        taken_titles(&feed(), &subscription, &holdings),
        ["Example Edition 31 January 2026 (RF2 SNAPSHOT)"],
        "the incoming entry is later, so it replaces the served copy"
    );
}

#[test]
fn a_held_copy_of_another_version_does_not_block_a_new_version() {
    let mut holdings = Holdings::new();
    let held: jiff::Timestamp = "2026-02-01T09:00:00Z".parse().expect("a fixed timestamp");
    holdings.record(EDITION, format!("{EDITION}/version/20251231"), held);
    let subscription = everything().with_system(EDITION);
    assert_eq!(
        taken_titles(&feed(), &subscription, &holdings),
        ["Example Edition 31 January 2026 (RF2 SNAPSHOT)"],
        "a different version is a new instance beside the old one"
    );
}

#[test]
fn every_entry_of_the_feed_is_either_taken_or_named_as_skipped() {
    let feed = feed();
    let selection = select::select(&feed, &everything(), &Holdings::new());
    assert_eq!(
        selection.taken.len() + selection.skipped.len(),
        feed.entries.len(),
        "nothing is dropped in silence"
    );
}

#[test]
fn a_subscription_with_no_system_filter_admits_every_system() {
    assert_eq!(Subscription::new().systems, Systems::Any);
    assert!(Subscription::new().systems.admits(VALUE_SET, ""));
    let listed = Subscription::new().with_system(EDITION);
    assert!(listed.systems.admits(EDITION, ""));
    assert!(!listed.systems.admits(VALUE_SET, ""));
}

#[test]
fn a_listed_edition_admits_a_release_identified_by_the_bare_code_system() {
    let listed = Subscription::new().with_system(EDITION);
    let release = format!("{EDITION}/version/20260131");
    assert!(
        listed.systems.admits("http://snomed.info/sct", &release),
        "Ontoserver identifies an edition release by the code system, the edition in the version"
    );
    assert!(
        !listed.systems.admits(
            "http://snomed.info/sct",
            "http://snomed.info/sct/900000000000207008/version/20260101"
        ),
        "a release of another edition is not admitted"
    );
    assert!(
        !listed.systems.admits(
            "http://snomed.info/sct",
            &format!("{EDITION}0/version/20260131")
        ),
        "an edition whose module id merely starts with the listed one is not admitted"
    );
}

#[test]
fn holdings_report_what_they_hold() {
    let mut holdings = Holdings::new();
    assert!(holdings.is_empty());
    let date: jiff::Timestamp = "2026-01-01T00:00:00Z".parse().expect("a fixed timestamp");
    holdings.record(VALUE_SET, "1.0.0", date);
    assert_eq!(holdings.len(), 1);
    assert_eq!(holdings.date_of(VALUE_SET, "1.0.0"), Some(date));
    assert_eq!(holdings.date_of(VALUE_SET, "2.0.0"), None);
}
