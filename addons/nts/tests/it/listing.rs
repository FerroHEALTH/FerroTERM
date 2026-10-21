// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading the challenged listing, and what a run does with what it finds.

use terminology_syndication::model::CategoryTerm;
use terminology_syndication::select::{Holdings, SkipReason};
use terminology_syndication::source::Source;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support;

/// A service that challenges its listing and answers the token it issued.
async fn service() -> MockServer {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("password")
        .respond_with(support::issued("access-first", 300, "refresh-first"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/synd/syndication.xml"))
        .and(header("authorization", "Bearer access-first"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(support::feed_xml(), "application/atom+xml"),
        )
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn the_listing_is_read_with_the_token_the_service_issued() {
    let server = service().await;
    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::password_account(), clock);

    let feed = source.list().await.expect("the listing is served");
    assert_eq!(feed.entries.len(), 5, "every entry is read");
    assert_eq!(
        source.name(),
        "nts",
        "the run record names the source it read"
    );
}

#[tokio::test]
async fn an_unsubscribed_system_is_reported_as_skipped() {
    let server = service().await;
    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::password_account(), clock);
    let feed = source.list().await.expect("the listing is served");

    let selection = source.select(&feed, &Holdings::new());
    let skipped = selection
        .skipped
        .iter()
        .find(|skipped| {
            matches!(
                &skipped.reason,
                SkipReason::SystemNotSubscribed { canonical }
                    if canonical == "https://example.invalid/fhir/ValueSet/other"
            )
        })
        .expect("the unsubscribed entry is in the run record");
    assert_eq!(
        skipped.title, "A Value Set Nothing Subscribes To",
        "the record reads without the feed beside it"
    );
    assert!(
        selection
            .taken
            .iter()
            .all(|taken| taken.entry.content_item_identifier.as_deref()
                != Some("https://example.invalid/fhir/ValueSet/other")),
        "an unsubscribed system is never fetched"
    );
}

#[tokio::test]
async fn a_system_offered_only_as_the_binary_index_is_not_syndicable() {
    let server = service().await;
    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::password_account(), clock);
    let feed = source.list().await.expect("the listing is served");

    let refused = source.not_syndicable(&feed);
    assert_eq!(
        refused.len(),
        1,
        "only the system with no other form is reported: {refused:?}"
    );
    let entry = refused.first().expect("the reported system");
    assert_eq!(entry.canonical, "http://loinc.org");
    assert_eq!(
        entry.term,
        CategoryTerm::BinaryIndex,
        "the run record names the category the system arrives in"
    );
    assert_eq!(
        entry.to_string(),
        "http://loinc.org is offered only as BINARY, which is not syndicable"
    );

    let selection = source.select(&feed, &Holdings::new());
    assert!(
        selection
            .skipped
            .iter()
            .any(|skipped| skipped.reason == SkipReason::BinaryIndex),
        "the binary entries are in the run record too"
    );
    assert!(
        source
            .not_syndicable(&feed)
            .iter()
            .all(|refused| refused.canonical != "http://snomed.info/sct"),
        "a system that also arrives as RF2 is syndicable"
    );
}

#[tokio::test]
async fn the_subscribed_releases_are_taken() {
    let server = service().await;
    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::password_account(), clock);
    let feed = source.list().await.expect("the listing is served");

    let selection = source.select(&feed, &Holdings::new());
    let taken: Vec<&str> = selection
        .taken
        .iter()
        .filter_map(|taken| taken.entry.content_item_identifier.as_deref())
        .collect();
    assert_eq!(
        taken,
        vec!["http://snomed.info/sct", "http://unitsofmeasure.org"],
        "the snapshot and the FHIR code system are taken, in feed order"
    );
}

#[tokio::test]
async fn a_release_already_served_is_not_taken_again() {
    let server = service().await;
    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::password_account(), clock);
    let feed = source.list().await.expect("the listing is served");

    let mut holdings = Holdings::new();
    holdings.record(
        "http://snomed.info/sct",
        "http://snomed.info/sct/11000146104/version/20260331",
        "2026-03-31T09:00:00Z".parse().expect("the date reads"),
    );
    let selection = source.select(&feed, &holdings);
    assert!(
        selection
            .skipped
            .iter()
            .any(|skipped| matches!(skipped.reason, SkipReason::NotNewer { .. })),
        "the replace rule keeps the served copy"
    );
}
