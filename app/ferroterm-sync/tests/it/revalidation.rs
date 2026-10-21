// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The post-activation check: what a new release did to the deployment's own
//! value sets and maps.
//!
//! No FHIR or SNOMED CT specification defines the check, so these are our own
//! design (#583).

use ferroterm_sync::record::{FindingKind, Outcome, Trigger};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use crate::support::{Entry, Harness, TestClock};

/// The `Parameters` a `$validate-code` answers with.
fn result(value: bool) -> String {
    format!(
        r#"{{"resourceType":"Parameters","parameter":[{{"name":"result","valueBoolean":{value}}}]}}"#
    )
}

/// A search `Bundle` carrying `resources`.
fn bundle(resources: &[&str]) -> String {
    let entries = resources
        .iter()
        .map(|resource| format!(r#"{{"resource":{resource}}}"#))
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"resourceType":"Bundle","type":"searchset","entry":[{entries}]}}"#)
}

/// The local value set every case here serves: two enumerated SNOMED codes.
const LOCAL_VALUE_SET: &str = r#"{"resourceType":"ValueSet","id":"local","url":"https://example.invalid/ValueSet/local","compose":{"include":[{"system":"http://snomed.info/sct","concept":[{"code":"11"},{"code":"22"}]}]}}"#;

#[tokio::test]
async fn a_release_that_changes_nothing_says_so() {
    let mut harness = Harness::new().await;
    harness.config.fhir_base_url = Some(format!("{}/r4b", harness.server.uri()));
    harness
        .publish(&[Entry::rf2(
            "11000146104",
            "20260930",
            "2026-09-30T09:00:00Z",
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    Mock::given(method("GET"))
        .and(path("/r4b/ValueSet"))
        .respond_with(ResponseTemplate::new(200).set_body_string(bundle(&[LOCAL_VALUE_SET])))
        .mount(&harness.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/r4b/ConceptMap"))
        .respond_with(ResponseTemplate::new(200).set_body_string(bundle(&[])))
        .mount(&harness.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/r4b/ValueSet/$validate-code"))
        .respond_with(ResponseTemplate::new(200).set_body_string(result(true)))
        .mount(&harness.server)
        .await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(record.outcome, Outcome::Ok, "the run worked: {record:?}");
    assert!(
        record.revalidation.findings.is_empty(),
        "nothing changed: {:?}",
        record.revalidation
    );
    assert_eq!(
        record.revalidation.resources, 1,
        "the local value set was read"
    );
    assert_eq!(
        record.revalidation.statement,
        "1 locally authored resources were revalidated and no local code changed",
        "a run with no findings says so"
    );
}

#[tokio::test]
async fn an_inactive_code_is_reported_with_the_release_that_changed_it() {
    let mut harness = Harness::new().await;
    harness.config.fhir_base_url = Some(format!("{}/r4b", harness.server.uri()));
    harness
        .publish(&[Entry::rf2(
            "11000146104",
            "20260930",
            "2026-09-30T09:00:00Z",
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    Mock::given(method("GET"))
        .and(path("/r4b/ValueSet"))
        .respond_with(ResponseTemplate::new(200).set_body_string(bundle(&[LOCAL_VALUE_SET])))
        .mount(&harness.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/r4b/ConceptMap"))
        .respond_with(ResponseTemplate::new(200).set_body_string(bundle(&[])))
        .mount(&harness.server)
        .await;
    // The code is a member, and it is not a member once inactive codes are
    // refused, which is what `activeOnly` asks for.
    Mock::given(method("GET"))
        .and(path("/r4b/ValueSet/$validate-code"))
        .and(query_param("code", "11"))
        .and(query_param("activeOnly", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_string(result(false)))
        .mount(&harness.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/r4b/ValueSet/$validate-code"))
        .respond_with(ResponseTemplate::new(200).set_body_string(result(true)))
        .mount(&harness.server)
        .await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(
        record.revalidation.findings.len(),
        1,
        "the retired code is the one finding: {:?}",
        record.revalidation
    );
    let finding = &record.revalidation.findings[0];
    assert_eq!(finding.kind, FindingKind::Inactive, "it went inactive");
    assert_eq!(finding.code, "11", "and it is named");
    assert_eq!(
        finding.system, "http://snomed.info/sct",
        "with the system it comes from"
    );
    assert_eq!(
        finding.release.as_deref(),
        Some("http://snomed.info/sct/11000146104/version/20260930"),
        "and the release this run activated"
    );
    assert_eq!(
        record.summary().revalidation_findings,
        1,
        "the webhook summary carries the finding count"
    );
    let text = service.metrics().exposition().expect("the metrics encode");
    assert!(
        text.contains(
            "ferroterm_sync_revalidation_findings{resource=\"ValueSet https://example.invalid/ValueSet/local\"} 1"
        ),
        "the scrape carries the count as a gauge per local resource: {text}"
    );
}

#[tokio::test]
async fn a_removed_code_is_reported_as_absent() {
    let mut harness = Harness::new().await;
    harness.config.fhir_base_url = Some(format!("{}/r4b", harness.server.uri()));
    harness
        .publish(&[Entry::rf2(
            "11000146104",
            "20260930",
            "2026-09-30T09:00:00Z",
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    Mock::given(method("GET"))
        .and(path("/r4b/ValueSet"))
        .respond_with(ResponseTemplate::new(200).set_body_string(bundle(&[LOCAL_VALUE_SET])))
        .mount(&harness.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/r4b/ConceptMap"))
        .respond_with(ResponseTemplate::new(200).set_body_string(bundle(&[])))
        .mount(&harness.server)
        .await;
    // The code is in neither the value set nor the code system at all.
    Mock::given(method("GET"))
        .and(path("/r4b/ValueSet/$validate-code"))
        .and(query_param("code", "22"))
        .respond_with(ResponseTemplate::new(200).set_body_string(result(false)))
        .mount(&harness.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/r4b/ValueSet/$validate-code"))
        .respond_with(ResponseTemplate::new(200).set_body_string(result(true)))
        .mount(&harness.server)
        .await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(
        record.revalidation.findings.len(),
        1,
        "the removed code is the one finding: {:?}",
        record.revalidation
    );
    assert_eq!(
        record.revalidation.findings[0].kind,
        FindingKind::Absent,
        "the release no longer carries it at all"
    );
}

#[tokio::test]
async fn the_check_is_reported_as_not_run_without_a_fhir_base() {
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
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(record.outcome, Outcome::Ok, "the run still worked");
    assert!(
        record
            .revalidation
            .skipped
            .as_deref()
            .is_some_and(|reason| reason.contains("fhir_base_url")),
        "the record says why the check did not run: {:?}",
        record.revalidation
    );
}

#[tokio::test]
async fn a_server_that_does_not_answer_leaves_the_release_served() {
    let mut harness = Harness::new().await;
    harness.config.fhir_base_url = Some(format!("{}/r4b", harness.server.uri()));
    harness
        .publish(&[Entry::rf2(
            "11000146104",
            "20260930",
            "2026-09-30T09:00:00Z",
        )])
        .await;
    harness.reloads_with(200).await;
    harness.accepts_webhooks().await;
    Mock::given(method("GET"))
        .and(path("/r4b/ValueSet"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&harness.server)
        .await;
    let service = harness.service(
        vec![harness.source()],
        TestClock::new("2026-10-01T03:00:00Z", 0),
    );

    let record = service.run(Trigger::RunOnce).await;

    assert_eq!(
        record.outcome,
        Outcome::Ok,
        "the release is served, so the run is not a failure: {record:?}"
    );
    assert!(
        record.revalidation.skipped.is_some(),
        "and the record says the check did not run: {:?}",
        record.revalidation
    );
    assert!(
        harness
            .index_root()
            .join("snomed-11000146104-20260930")
            .is_dir(),
        "the release stays served"
    );
}
