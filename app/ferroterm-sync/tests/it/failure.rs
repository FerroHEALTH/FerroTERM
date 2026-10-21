// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a failure leaves behind: the served set unchanged, the run marked
//! failed, and the webhook fired either way.
//!
//! No specification governs a run, so these are our own design (#579).

use ferroterm_sync::record::{Outcome, Trigger};

use crate::support::{Entry, Harness, TestClock, failing_build};

#[tokio::test]
async fn a_build_failure_leaves_the_index_root_alone() {
    let mut harness = Harness::new().await;
    harness.config.build_command = failing_build(harness.dir.path());
    harness
        .publish(&[Entry::rf2(
            "11000146104",
            "20260930",
            "2026-09-30T09:00:00Z",
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(record.outcome, Outcome::Failed, "the run failed");
    assert!(
        record.errors.iter().any(|error| error.contains("exit")),
        "the record says the build exited with a failure: {:?}",
        record.errors
    );
    assert_eq!(
        std::fs::read_dir(harness.index_root())
            .expect("the index root lists")
            .count(),
        0,
        "nothing reached the index root"
    );
    assert_eq!(
        harness.reload_requests().await,
        0,
        "and the server was never asked to reload"
    );
    assert_eq!(
        harness.webhook_deliveries().await,
        1,
        "the webhook fired for the failure"
    );
}

#[tokio::test]
async fn a_refused_reload_is_rolled_back() {
    let harness = Harness::new().await;
    harness
        .publish(&[Entry::rf2(
            "11000146104",
            "20260930",
            "2026-09-30T09:00:00Z",
        )])
        .await;
    harness.reloads_with(500).await;
    harness.accepts_webhooks().await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(record.outcome, Outcome::Failed, "the run failed");
    assert!(
        record.activation.rolled_back,
        "the activation was taken back: {:?}",
        record.activation
    );
    assert_eq!(
        record.activation.reloads.len(),
        1,
        "the record carries what the server answered"
    );
    assert_eq!(
        record.activation.reloads[0].status, 500,
        "including its status"
    );
    assert_eq!(
        std::fs::read_dir(harness.index_root())
            .expect("the index root lists")
            .count(),
        0,
        "the index root is exactly as it was"
    );
    assert!(
        harness
            .config
            .staging
            .join("snomed-11000146104-20260930")
            .is_dir(),
        "and the build is back in staging"
    );
    assert_eq!(
        harness.webhook_deliveries().await,
        1,
        "the webhook fired for the failure"
    );
}

#[tokio::test]
async fn a_refused_reload_puts_back_the_resource_it_replaced() {
    let mut harness = Harness::new().await;
    let served = r#"{"resourceType":"ValueSet","url":"https://example.invalid/ValueSet/a","version":"1","title":"served"}"#;
    let offered = r#"{"resourceType":"ValueSet","url":"https://example.invalid/ValueSet/a","version":"1","title":"offered"}"#;
    harness
        .publish(&[Entry::value_set(
            "https://example.invalid/ValueSet/a",
            "1",
            "2026-09-01T09:00:00Z",
            served,
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-09-02T03:00:00Z", 0),
    );
    let first = service.run(Trigger::RunOnce).await;
    assert_eq!(first.outcome, Outcome::Ok, "the first run served it");
    let file = harness
        .resources()
        .join("https-example-invalid-valueset-a-1.json");
    assert_eq!(
        std::fs::read_to_string(&file).expect("the resource was written"),
        served,
        "and wrote what the feed offered"
    );
    drop(service);

    // The same canonical and version, republished at a later date, meets a
    // server that refuses the reload.
    let republished = Harness::new().await;
    republished
        .publish(&[Entry::value_set(
            "https://example.invalid/ValueSet/a",
            "1",
            "2026-09-30T09:00:00Z",
            offered,
        )])
        .await;
    republished.reloads_with(500).await;
    harness.config.server_admin_url = republished.server.uri();
    let service = harness.service(
        vec![republished.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(record.outcome, Outcome::Failed, "the second run failed");
    assert!(
        record.activation.rolled_back,
        "the activation was taken back: {:?}",
        record.activation
    );
    assert_eq!(
        std::fs::read_to_string(&file).expect("the resource is still there"),
        served,
        "the resource the deployment served is byte-identical"
    );
}

#[tokio::test]
async fn a_feed_that_does_not_answer_fails_the_run() {
    let harness = Harness::new().await;
    harness.accepts_webhooks().await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(record.outcome, Outcome::Failed, "the run failed");
    assert_eq!(record.sources.len(), 1, "the source is still in the record");
    assert!(
        !record.sources[0].errors.is_empty(),
        "with the reason the feed gave: {:?}",
        record.sources[0]
    );
    assert_eq!(
        harness.webhook_deliveries().await,
        1,
        "the webhook fired for the failure"
    );
}
