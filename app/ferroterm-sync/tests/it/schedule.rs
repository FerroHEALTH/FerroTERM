// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The schedule: a run starts at its due time, and a restart between two runs
//! does not replay the one that already happened.
//!
//! No specification governs the schedule, so these are our own design (#579).

use ferroterm_sync::record::Trigger;

use crate::support::{Entry, Harness, TestClock, every};

#[tokio::test]
async fn a_scheduled_run_starts_at_its_due_time() {
    let mut harness = Harness::new().await;
    harness.config.schedule.at = Some(String::from("03:00"));
    harness
        .publish(&[Entry::rf2(
            "11000146104",
            "20260930",
            "2026-09-30T09:00:00Z",
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let clock = TestClock::new("2026-10-01T01:00:00Z", 1);
    let service = harness.service(vec![harness.source()], clock);

    let record = service
        .wait_and_run()
        .await
        .expect("a schedule is configured, so a run is due");

    assert_eq!(
        record.trigger, "schedule",
        "the run the schedule started says so"
    );
    assert_eq!(
        record.started,
        "2026-10-01T03:00:00Z"
            .parse::<jiff::Timestamp>()
            .expect("a timestamp"),
        "the run started at the configured time of day"
    );
    assert!(
        harness
            .index_root()
            .join("snomed-11000146104-20260930")
            .is_dir(),
        "the scheduled run did the work"
    );
}

#[tokio::test]
async fn a_restart_between_runs_does_not_replay() {
    let mut harness = Harness::new().await;
    harness.config.schedule = every("24h");
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
    let first = service.run(Trigger::Schedule).await;
    drop(service);

    // The process stops and starts again an hour later, reading the same
    // state directory.
    let restarted = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T04:00:00Z", 0),
    );
    let due = restarted
        .next_due()
        .await
        .expect("the due time is computable");

    assert_eq!(
        due,
        Some(
            first
                .finished
                .checked_add(jiff::SignedDuration::from_hours(24))
                .expect("a day after the last run")
        ),
        "the next run is a day after the last one ended, not now"
    );
    assert_eq!(
        restarted.records().summaries().expect("the records").len(),
        1,
        "the restart performed no run of its own"
    );
}

#[tokio::test]
async fn a_service_with_no_schedule_has_no_due_time() {
    let harness = Harness::new().await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );

    assert_eq!(
        service.next_due().await.expect("computable"),
        None,
        "with no schedule the service runs only when it is asked to"
    );
    assert!(
        service.wait_and_run().await.is_none(),
        "and waiting for a scheduled run answers nothing"
    );
}
