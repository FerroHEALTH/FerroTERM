// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Synchronisation screens' tests.

use super::summary::{activation_line, left_line, revalidation_line, source_lines, taken_lines};
use super::*;

/// The English locale the tests read the words in.
fn english() -> Locale {
    Locale::default()
}

#[test]
fn a_duration_reads_in_minutes_and_seconds() {
    assert_eq!(duration(&english(), 252_000), "4 min 12 s");
    assert_eq!(duration(&english(), 9_400), "9 s");
    assert_eq!(duration(&english(), 0), "0 s");
}

#[test]
fn a_run_says_what_it_did_or_why_it_failed() {
    let quiet = RunSummary {
        outcome: RunOutcome::Ok,
        ..RunSummary::default()
    };
    assert_eq!(did(&english(), &quiet), "Nothing new");
    let took = RunSummary {
        outcome: RunOutcome::Ok,
        entries_taken: 2,
        activated: 1,
        ..RunSummary::default()
    };
    assert_eq!(did(&english(), &took), "2 releases taken, 1 activated");
    let failed = RunSummary {
        outcome: RunOutcome::Failed,
        errors: vec![String::from("the feed answered 503")],
        ..RunSummary::default()
    };
    assert_eq!(did(&english(), &failed), "the feed answered 503");
}

#[test]
fn every_kind_the_service_records_reads_as_a_sentence() {
    assert_eq!(
        kind_words(&english(), "inactive"),
        "The release made this code inactive"
    );
    assert_eq!(
        kind_words(&english(), "absent"),
        "The release no longer carries this code"
    );
    assert_eq!(
        kind_words(&english(), "outside-value-set"),
        "The code no longer falls inside the value set it was included through"
    );
    assert_eq!(
        kind_words(&english(), "something-new"),
        "something-new",
        "a kind this console predates is shown as the service wrote it"
    );
}

#[test]
fn a_record_reads_as_its_summary_lines() {
    let record: RunRecord = serde_json::from_str(
        r#"{"sources":[{"source":"nts","entries_seen":412,"fhir_api_url":"https://api.example",
            "api_entries_seen":1920,"taken":[{"title":"Findings","version":"2026-10"},
            {"title":"Labs","version":"7","error":"the digest did not match"}],
            "skipped":[{},{}],"not_subscribed":[{"entries":388},{"entries":1}]}],
            "activation":{"activated":[{}],"staged":0,"reloads":[{"status":200}]},
            "revalidation":{"resources":12,"checked":214,"statement":"1 finding"}}"#,
    )
    .expect("the record parses");
    assert_eq!(
        source_lines(&english(), &record),
        ["nts: the feed offered 412 entries, and its FHIR API listed 1,920 resources"]
    );
    assert_eq!(
        taken_lines(&english(), &record),
        ["Findings 2026-10", "Labs 7: the digest did not match"]
    );
    assert_eq!(
        left_line(&english(), &record),
        "2 entries for another reason, 389 outside your subscription"
    );
    assert_eq!(
        activation_line(&english(), &record),
        "1 activated, 0 still staged, 1 reloads asked of the server"
    );
    assert_eq!(
        revalidation_line(&english(), &record),
        "12 locally written resources read, 214 codes checked: 1 finding"
    );
}

#[test]
fn a_failed_activation_and_a_skipped_revalidation_say_why() {
    let record: RunRecord = serde_json::from_str(
        r#"{"activation":{"activated":[],"error":"the server refused the reload"},
            "revalidation":{"skipped":"no resource store"}}"#,
    )
    .expect("the record parses");
    assert_eq!(
        activation_line(&english(), &record),
        "0 activated, then it failed: the server refused the reload"
    );
    assert_eq!(
        revalidation_line(&english(), &record),
        "Not run: no resource store"
    );
}

#[test]
fn a_run_id_is_one_path_segment() {
    assert_eq!(
        run_link("20261021T030000Z", false),
        "/sync/20261021T030000Z"
    );
    assert_eq!(run_link("a/b", true), "/sync/a%2Fb/findings");
}
