// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Manual activation: a run stages and stops, and `POST /activate` is what
//! puts the release in front of the server.
//!
//! No specification governs activation, so these are our own design (#579).

use ferroterm_sync::config::Activation;
use ferroterm_sync::record::{Outcome, Trigger};

use crate::support::{Entry, Harness, TestClock};

#[tokio::test]
async fn manual_activation_stages_without_serving() {
    let mut harness = Harness::new().await;
    harness.config.activation = Activation::Manual;
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

    assert_eq!(record.outcome, Outcome::Ok, "staging worked: {record:?}");
    assert_eq!(record.activation.mode, "manual", "the mode is recorded");
    assert_eq!(record.activation.staged, 1, "one release waits");
    assert!(
        record.activation.activated.is_empty(),
        "nothing reached the server"
    );
    assert_eq!(
        harness.reload_requests().await,
        0,
        "the server was not asked to reload"
    );
    assert!(
        !harness
            .index_root()
            .join("snomed-11000146104-20260930")
            .exists(),
        "and the index root is untouched"
    );

    let activation = service.activate().await;

    assert_eq!(
        activation.outcome,
        Outcome::Ok,
        "the activation worked: {activation:?}"
    );
    assert_eq!(
        activation.trigger, "activate",
        "the record says what started it"
    );
    assert!(
        harness
            .index_root()
            .join("snomed-11000146104-20260930")
            .join("manifest.json")
            .is_file(),
        "the staged release is now served"
    );
    assert_eq!(
        harness.reload_requests().await,
        1,
        "and the server was asked to reload"
    );
}

#[tokio::test]
async fn an_activation_does_not_move_the_schedule() {
    let mut harness = Harness::new().await;
    harness.config.activation = Activation::Manual;
    harness.config.schedule = crate::support::every("24h");
    harness
        .publish(&[Entry::rf2(
            "11000146104",
            "20260930",
            "2026-09-30T09:00:00Z",
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let clock = TestClock::new("2026-10-01T03:00:00Z", 0);
    let service = harness.service(vec![harness.source()], clock);
    let run = service.run(Trigger::RunOnce).await;
    let due = service.next_due().await.expect("computable");

    let activation = service.activate().await;

    assert_eq!(activation.outcome, Outcome::Ok, "the activation worked");
    assert_eq!(
        service.next_due().await.expect("computable"),
        due,
        "an activation is not a run, so the next run stays a day after {}",
        run.finished
    );
}

#[tokio::test]
async fn a_staged_release_is_not_fetched_again() {
    let mut harness = Harness::new().await;
    harness.config.activation = Activation::Manual;
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

    let first = service.run(Trigger::RunOnce).await;
    let second = service.run(Trigger::RunOnce).await;

    assert_eq!(first.entries_taken, 1, "the first run staged it");
    assert_eq!(
        second.entries_taken, 0,
        "a release already staged is not built a second time"
    );
    assert_eq!(
        second.activation.staged, 1,
        "and it is still waiting for activation"
    );
}
