// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The two lanes: an RF2 release becomes a built artifact, a FHIR resource
//! becomes a file, and what the deployment already holds is left alone.
//!
//! No FHIR or SNOMED CT specification governs a run, so these are our own
//! design (#579).

use ferroterm_sync::record::{Outcome, Trigger};

use crate::support::{Entry, Harness, TestClock};

#[tokio::test]
async fn an_rf2_release_is_built_and_served() {
    let harness = Harness::new().await;
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

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(record.outcome, Outcome::Ok, "the run worked: {record:?}");
    assert_eq!(record.entries_taken, 1, "the one entry was taken");
    let served = harness.index_root().join("snomed-11000146104-20260930");
    assert!(
        served.join("manifest.json").is_file(),
        "the built release sits under the index root"
    );
    assert_eq!(
        harness.reload_requests().await,
        1,
        "the server was asked to reload once"
    );
    assert_eq!(
        record.activation.activated.len(),
        1,
        "the record names what reached the server"
    );
    assert_eq!(
        record.activation.staged, 0,
        "nothing is left waiting in auto activation"
    );
}

#[tokio::test]
async fn a_new_release_lands_beside_the_previous_one() {
    let harness = Harness::new().await;
    harness
        .publish(&[
            Entry::rf2("11000146104", "20260630", "2026-06-30T09:00:00Z"),
            Entry::rf2("11000146104", "20260930", "2026-09-30T09:00:00Z"),
        ])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let clock = TestClock::new("2026-10-01T03:00:00Z", 0);
    let service = harness.service(vec![harness.source()], clock);

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(record.outcome, Outcome::Ok, "the run worked: {record:?}");
    for release in ["snomed-11000146104-20260630", "snomed-11000146104-20260930"] {
        assert!(
            harness.index_root().join(release).is_dir(),
            "{release} is served, so the previous release is there to roll back to"
        );
    }
}

#[tokio::test]
async fn a_fhir_resource_lands_in_the_managed_directory() {
    let harness = Harness::new().await;
    let body =
        r#"{"resourceType":"ValueSet","url":"https://example.invalid/ValueSet/a","version":"1"}"#;
    harness
        .publish(&[Entry::value_set(
            "https://example.invalid/ValueSet/a",
            "1",
            "2026-09-30T09:00:00Z",
            body,
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let clock = TestClock::new("2026-10-01T03:00:00Z", 0);
    let service = harness.service(vec![harness.source()], clock);

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(record.outcome, Outcome::Ok, "the run worked: {record:?}");
    let file = harness
        .resources()
        .join("https-example-invalid-valueset-a-1.json");
    assert_eq!(
        std::fs::read_to_string(&file).expect("the resource was written"),
        body,
        "a resource that needs no correction lands byte-identical"
    );
}

#[tokio::test]
async fn a_correction_is_applied_and_recorded() {
    let harness = Harness::new().await;
    let body = r#"{"resourceType":"ConceptMap","url":"https://example.invalid/ConceptMap/a","version":"1","experimental":"true"}"#;
    harness
        .publish(&[Entry::value_set(
            "https://example.invalid/ConceptMap/a",
            "1",
            "2026-09-30T09:00:00Z",
            body,
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let clock = TestClock::new("2026-10-01T03:00:00Z", 0);
    let service = harness.service(vec![harness.source_with_fixups()], clock);

    let record = service.run(Trigger::RunOnce).await;

    let taken = &record.sources[0].taken[0];
    assert_eq!(
        taken.fixups.len(),
        1,
        "the correction the source needed is recorded: {taken:?}"
    );
    let file = harness
        .resources()
        .join("https-example-invalid-conceptmap-a-1.json");
    let written = std::fs::read_to_string(&file).expect("the resource was written");
    assert!(
        written.contains("\"experimental\":true"),
        "the corrected bytes are what is served: {written}"
    );
}

#[tokio::test]
async fn an_entry_at_the_same_date_is_skipped_on_the_next_run() {
    let harness = Harness::new().await;
    let body =
        r#"{"resourceType":"ValueSet","url":"https://example.invalid/ValueSet/a","version":"1"}"#;
    harness
        .publish(&[Entry::value_set(
            "https://example.invalid/ValueSet/a",
            "1",
            "2026-09-30T09:00:00Z",
            body,
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let clock = TestClock::new("2026-10-01T03:00:00Z", 0);
    let service = harness.service(vec![harness.source()], clock);

    let first = service.run(Trigger::RunOnce).await;
    assert_eq!(first.entries_taken, 1, "the first run takes it");
    let second = service.run(Trigger::RunOnce).await;

    assert_eq!(
        second.entries_taken, 0,
        "the same canonical and version at the same date is not taken again"
    );
    let skipped = &second.sources[0].skipped;
    assert_eq!(skipped.len(), 1, "it is reported as skipped: {skipped:?}");
    assert!(
        skipped[0].reason.contains("2026-09-30"),
        "the reason names the dates it compared: {:?}",
        skipped[0].reason
    );
}

#[tokio::test]
async fn an_older_republish_is_skipped() {
    let harness = Harness::new().await;
    let body =
        r#"{"resourceType":"ValueSet","url":"https://example.invalid/ValueSet/a","version":"1"}"#;
    harness
        .publish(&[Entry::value_set(
            "https://example.invalid/ValueSet/a",
            "1",
            "2026-09-30T09:00:00Z",
            body,
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    let clock = TestClock::new("2026-10-01T03:00:00Z", 0);
    let service = harness.service(vec![harness.source()], clock);
    let first = service.run(Trigger::RunOnce).await;
    assert_eq!(first.entries_taken, 1, "the first run takes it");

    let older = Harness::new().await;
    older
        .publish(&[Entry::value_set(
            "https://example.invalid/ValueSet/a",
            "1",
            "2026-01-01T09:00:00Z",
            body,
        )])
        .await;
    let service = harness.service(
        vec![older.source()],
        TestClock::new("2026-10-02T03:00:00Z", 0),
    );

    let second = service.run(Trigger::RunOnce).await;

    assert_eq!(
        second.entries_taken, 0,
        "an entry dated before the copy already served is left behind"
    );
}

#[tokio::test]
async fn a_run_touches_nothing_it_did_not_write() {
    let harness = Harness::new().await;
    harness
        .publish(&[Entry::rf2(
            "11000146104",
            "20260930",
            "2026-09-30T09:00:00Z",
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    // The server's own write store and a resource an operator wrote by hand,
    // both outside every directory the service manages.
    let store = harness.dir.path().join("write-store");
    std::fs::create_dir_all(&store).expect("the write store");
    let authored = store.join("locally-authored.json");
    std::fs::write(&authored, "the deployment's own resource").expect("the local resource");
    let by_hand = harness.resources().join("by-hand.json");
    std::fs::write(
        &by_hand,
        r#"{"resourceType":"CodeSystem","url":"urn:x","version":"1"}"#,
    )
    .expect("the hand-written resource");
    let before = (
        std::fs::read(&authored).expect("read"),
        std::fs::read(&by_hand).expect("read"),
    );
    let clock = TestClock::new("2026-10-01T03:00:00Z", 0);
    let service = harness.service(vec![harness.source()], clock);

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(record.outcome, Outcome::Ok, "the run worked: {record:?}");
    assert_eq!(
        (
            std::fs::read(&authored).expect("read"),
            std::fs::read(&by_hand).expect("read")
        ),
        before,
        "every file the service did not write is byte-identical"
    );
}
