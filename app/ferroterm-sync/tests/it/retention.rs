// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Retention: the configured number of releases per code system stays, the
//! rest goes after the server has reloaded the new one.
//!
//! No specification governs retention, so these are our own design (#579).

use std::path::Path;

use ferroterm_sync::record::{Outcome, Trigger};

use crate::support::{Entry, Harness, TestClock};

/// Writes a built release into the index root, the way the build writes one.
fn release(root: &Path, edition: &str, date: &str) {
    let dir = root.join(format!("snomed-{edition}-{date}"));
    std::fs::create_dir_all(&dir).expect("the release directory");
    std::fs::write(
        dir.join("manifest.json"),
        format!(
            r#"{{"manifest":1,"system":"http://snomed.info/sct","edition":"http://snomed.info/sct/{edition}","version":"http://snomed.info/sct/{edition}/version/{date}","releaseDate":"{date}"}}"#
        ),
    )
    .expect("the manifest");
}

#[tokio::test]
async fn retention_keeps_the_configured_number_of_releases() {
    let mut harness = Harness::new().await;
    harness.config.retention = 2;
    release(&harness.index_root(), "11000146104", "20260331");
    release(&harness.index_root(), "11000146104", "20260630");
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

    assert_eq!(record.outcome, Outcome::Ok, "the run worked: {record:?}");
    assert_eq!(
        record.retention.len(),
        1,
        "one superseded release was removed: {:?}",
        record.retention
    );
    assert!(
        !harness
            .index_root()
            .join("snomed-11000146104-20260331")
            .exists(),
        "the oldest release is gone"
    );
    for kept in ["snomed-11000146104-20260630", "snomed-11000146104-20260930"] {
        assert!(
            harness.index_root().join(kept).is_dir(),
            "{kept} is kept for rollback"
        );
    }
    assert_eq!(
        harness.reload_requests().await,
        2,
        "the server was told about the new release and about the one that went"
    );
    assert!(record.bytes_used > 0, "the run reports the disk it uses");
}

#[tokio::test]
async fn a_release_the_service_already_holds_is_not_taken_again() {
    let mut harness = Harness::new().await;
    harness.config.retention = 1;
    release(&harness.index_root(), "11000146104", "20260930");
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

    assert_eq!(
        record.entries_taken, 0,
        "a release already under the index root is left alone"
    );
    assert!(
        harness
            .index_root()
            .join("snomed-11000146104-20260930")
            .is_dir(),
        "and it is still there"
    );
}
