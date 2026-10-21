// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR API lane: a service that keeps its resources behind its FHIR API
//! rather than in the feed is listed there, and what it lists goes through
//! the resource lane like a feed entry.
//!
//! No FHIR or SNOMED CT specification governs a run (our own design, #692).

use ferroterm_sync::record::{Origin, Outcome, Trigger};
use terminology_syndication::model::CategoryTerm;
use terminology_syndication::select::Subscription;

use crate::support::{Harness, TestClock};

fn value_set(id: &str, canonical: &str, version: &str, updated: &str) -> serde_json::Value {
    serde_json::json!({
        "resourceType": "ValueSet", "id": id, "url": canonical, "version": version,
        "status": "active", "meta": {"lastUpdated": updated},
        "compose": {"include": [{"system": "http://example.invalid/cs"}]}
    })
}

#[tokio::test]
async fn a_resource_the_api_lists_lands_in_the_managed_directory() {
    let harness = Harness::new().await;
    harness.publish(&[]).await;
    harness
        .publish_api(&[value_set(
            "a1",
            "https://example.invalid/ValueSet/a",
            "1",
            "2026-09-01T00:00:00Z",
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let clock = TestClock::new("2026-10-01T03:00:00Z", 0);
    let subscription = Subscription::new()
        .with_system("https://example.invalid/ValueSet/a")
        .with_category(CategoryTerm::FhirValueSet);
    let service = harness.service(vec![harness.source_with_api(subscription)], clock);

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(record.outcome, Outcome::Ok, "the run worked: {record:?}");
    let source = &record.sources[0];
    assert_eq!(
        source.fhir_api_url.as_deref(),
        Some(harness.fhir_api_url().as_str()),
        "the record names the API that was listed"
    );
    assert_eq!(source.entries_seen, 0, "the feed offered nothing");
    assert_eq!(
        source.api_entries_seen, 1,
        "the API listed the one resource"
    );
    assert_eq!(source.taken.len(), 1, "it was taken");
    assert_eq!(
        source.taken[0].origin,
        Origin::FhirApi,
        "the record says where it came from"
    );
    assert_eq!(
        source.taken[0].entry_id, "ValueSet/a1",
        "the entry is the resource's address"
    );
    assert!(
        source.not_visible.is_empty(),
        "the subscribed canonical was visible"
    );
    let served = harness
        .resources()
        .join("https-example-invalid-valueset-a-1.json");
    assert!(
        served.is_file(),
        "the resource sits in the managed directory"
    );
    let body: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&served).expect("the file reads"))
            .expect("the file is JSON");
    assert_eq!(
        body["id"], "a1",
        "the full resource was fetched by id, not the summary"
    );
}

#[tokio::test]
async fn a_resource_already_served_is_not_taken_from_the_api_again() {
    let harness = Harness::new().await;
    harness.publish(&[]).await;
    harness
        .publish_api(&[value_set(
            "a1",
            "https://example.invalid/ValueSet/a",
            "1",
            "2026-09-01T00:00:00Z",
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let clock = TestClock::new("2026-10-01T03:00:00Z", 0);
    let subscription = Subscription::new().with_category(CategoryTerm::FhirValueSet);
    let service = harness.service(vec![harness.source_with_api(subscription)], clock);

    let first = service.run(Trigger::RunOnce).await;
    let second = service.run(Trigger::Manual).await;

    assert_eq!(first.entries_taken, 1, "the first run took the resource");
    assert_eq!(second.entries_taken, 0, "the second run holds it already");
    assert!(
        second.sources[0]
            .skipped
            .iter()
            .any(|skipped| skipped.origin == Origin::FhirApi
                && skipped.reason.contains("the served copy is dated")),
        "the record says why: {:?}",
        second.sources[0].skipped
    );
}

#[tokio::test]
async fn a_subscribed_system_no_listing_carries_is_reported_as_not_visible() {
    let harness = Harness::new().await;
    harness.publish(&[]).await;
    harness.publish_api(&[]).await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let clock = TestClock::new("2026-10-01T03:00:00Z", 0);
    let subscription = Subscription::new()
        .with_system("http://loinc.org")
        .with_category(CategoryTerm::FhirCodeSystem);
    let service = harness.service(vec![harness.source_with_api(subscription)], clock);

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(
        record.outcome,
        Outcome::Ok,
        "an invisible system fails nothing: {record:?}"
    );
    assert_eq!(
        record.sources[0].not_visible,
        vec![String::from("http://loinc.org")],
        "the record names the system the account could not see"
    );
}

#[tokio::test]
async fn an_api_that_does_not_answer_fails_the_run_and_keeps_the_feed_lane() {
    let harness = Harness::new().await;
    harness.publish(&[]).await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let clock = TestClock::new("2026-10-01T03:00:00Z", 0);
    let subscription = Subscription::new().with_category(CategoryTerm::FhirValueSet);
    let service = harness.service(vec![harness.source_with_api(subscription)], clock);

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(
        record.outcome,
        Outcome::Failed,
        "an API that does not list is an error"
    );
    assert!(
        record.sources[0]
            .errors
            .iter()
            .any(|error| error.contains("/fhir/CodeSystem")),
        "the error names the page that did not answer: {:?}",
        record.sources[0].errors
    );
    assert!(
        record.sources[0].not_visible.is_empty(),
        "nothing is called invisible when a listing failed"
    );
}

// A service's FHIR API lists every resource it holds, so an entry left behind
// only because the subscription does not name it is counted, not listed, while
// every other reason keeps its line (#695).
#[tokio::test]
async fn resources_outside_the_subscription_are_counted_not_listed() {
    let harness = Harness::new().await;
    harness.publish(&[]).await;
    harness
        .publish_api(&[
            value_set(
                "a1",
                "https://example.invalid/ValueSet/a",
                "1",
                "2026-09-01T00:00:00Z",
            ),
            value_set(
                "b1",
                "https://example.invalid/ValueSet/b",
                "1",
                "2026-09-01T00:00:00Z",
            ),
            value_set(
                "c1",
                "https://example.invalid/ValueSet/c",
                "1",
                "2026-09-01T00:00:00Z",
            ),
        ])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let clock = TestClock::new("2026-10-01T03:00:00Z", 0);
    let subscription = Subscription::new()
        .with_system("https://example.invalid/ValueSet/a")
        .with_category(CategoryTerm::FhirValueSet);
    let service = harness.service(vec![harness.source_with_api(subscription)], clock);

    let first = service.run(Trigger::RunOnce).await;
    let second = service.run(Trigger::Manual).await;

    let source = &first.sources[0];
    assert_eq!(source.taken.len(), 1, "the subscribed resource was taken");
    assert!(
        source.skipped.is_empty(),
        "no unsubscribed resource is listed: {:?}",
        source.skipped
    );
    assert_eq!(
        source.not_subscribed,
        vec![ferroterm_sync::record::NotSubscribed {
            origin: Origin::FhirApi,
            entries: 2,
        }],
        "the two unsubscribed resources are one count"
    );
    assert_eq!(first.summary().entries_not_subscribed, 2);
    assert_eq!(first.summary().entries_skipped, 0);

    let again = &second.sources[0];
    assert_eq!(
        again.skipped.len(),
        1,
        "a resource held already keeps its line: {:?}",
        again.skipped
    );
    assert_eq!(second.summary().entries_skipped, 1);
    assert_eq!(second.summary().entries_not_subscribed, 2);
}
