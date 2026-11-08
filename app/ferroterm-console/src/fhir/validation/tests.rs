// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `$validate-code` and `$subsumes` tests: the requests each run sends,
//! and what is read of each answer.

use super::*;

use crate::fhir::operation::tests::{query, read};

/// `json`, read as the R4B answer of a value set `$validate-code`.
fn validated(json: &str) -> Result<ValidationAnswer, FhirError> {
    let request = value_set_run().request().expect("the run names its system");
    ValidationAnswer::read(
        read(FhirVersion::R4B, &request, json)?,
        "",
        FhirVersion::R4B,
    )
}

/// `json`, read as the R4B answer of `$validate-code`.
fn parse(json: &str) -> ValidationAnswer {
    validated(json).expect("the fixture is a valid answer at R4B")
}

/// `json`, read as the R4B answer of `$subsumes`.
fn subsumed(json: &str) -> Result<SubsumesAnswer, FhirError> {
    let request = SubsumesRequest {
        system: "https://terminology.example/animals".to_owned(),
        code_a: "cat".to_owned(),
        code_b: "kitten".to_owned(),
        ..SubsumesRequest::default()
    }
    .request();
    SubsumesAnswer::read(
        &read(FhirVersion::R4B, &request, json)?,
        "",
        FhirVersion::R4B,
    )
}

/// The query `request` is sent with at R4B.
fn sent(request: &ValidateRequest) -> String {
    query(
        FhirVersion::R4B,
        request.request().expect("the run names what it needs"),
    )
}

fn value_set_run() -> ValidateRequest {
    ValidateRequest {
        on: ValidateOn::ValueSet,
        url: "https://terminology.example/vs".to_owned(),
        system: "https://terminology.example/animals".to_owned(),
        code: "cat".to_owned(),
        ..ValidateRequest::default()
    }
}

fn code_system_run() -> ValidateRequest {
    ValidateRequest {
        on: ValidateOn::CodeSystem,
        url: "https://terminology.example/animals".to_owned(),
        resource_version: "2.0".to_owned(),
        code: "cat".to_owned(),
        display: "Dog".to_owned(),
        ..ValidateRequest::default()
    }
}

#[test]
fn a_code_system_run_sends_the_names_that_operation_declares() {
    assert_eq!(
        sent(&code_system_run()),
        "?url=https%3A%2F%2Fterminology.example%2Fanimals&code=cat&version=2.0&display=Dog",
        "the code system's own version travels as `version`"
    );
}

#[test]
fn a_value_set_run_names_two_versions_under_their_own_parameters() {
    let request = ValidateRequest {
        on: ValidateOn::ValueSet,
        url: "https://terminology.example/vs".to_owned(),
        resource_version: "1.2".to_owned(),
        system: "https://terminology.example/animals".to_owned(),
        system_version: "2.0".to_owned(),
        code: "cat".to_owned(),
        display_language: "nl-NL".to_owned(),
        ..ValidateRequest::default()
    };
    assert_eq!(
        sent(&request),
        "?url=https%3A%2F%2Fterminology.example%2Fvs&valueSetVersion=1.2\
         &code=cat&system=https%3A%2F%2Fterminology.example%2Fanimals&systemVersion=2.0\
         &displayLanguage=nl-NL",
        "the value set's version and the code system's version are two parameters"
    );
}

#[test]
fn an_instance_run_leaves_the_canonical_out_of_the_query() {
    let request = ValidateRequest {
        id: "animals".to_owned(),
        ..code_system_run()
    };
    assert!(request.instance());
    assert_eq!(
        sent(&request),
        "?code=cat&version=2.0&display=Dog",
        "the address names the resource, so naming it again in `url` would be a second claim"
    );
}

#[test]
fn a_half_named_run_is_not_sent_at_all() {
    assert!(!ValidateRequest::default().runnable(), "nothing was named");
    assert!(
        !ValidateRequest {
            code: String::new(),
            ..code_system_run()
        }
        .runnable(),
        "a system with no code asks nothing"
    );
    assert!(
        !ValidateRequest {
            on: ValidateOn::ValueSet,
            system: String::new(),
            ..code_system_run()
        }
        .runnable(),
        "a code alone does not say which system it is from"
    );
    assert!(code_system_run().runnable());
}

#[test]
fn a_subsumes_run_sends_both_codes_and_the_system_they_are_from() {
    let request = SubsumesRequest {
        system: "https://terminology.example/animals".to_owned(),
        version: "2.0".to_owned(),
        code_a: "cat".to_owned(),
        code_b: "kitten".to_owned(),
        ..SubsumesRequest::default()
    };
    assert!(request.runnable());
    assert_eq!(
        query(FhirVersion::R4B, request.request()),
        "?codeA=cat&codeB=kitten&system=https%3A%2F%2Fterminology.example%2Fanimals&version=2.0"
    );
    assert!(
        !SubsumesRequest {
            code_b: String::new(),
            ..request.clone()
        }
        .runnable(),
        "one code is not a comparison"
    );
    assert_eq!(
        query(
            FhirVersion::R4B,
            SubsumesRequest {
                id: "animals".to_owned(),
                system: String::new(),
                ..request
            }
            .request()
        ),
        "?codeA=cat&codeB=kitten&version=2.0",
        "an instance run takes the system from the address"
    );
}

#[test]
fn a_wrong_display_comes_back_corrected_with_its_classification() {
    let answer = parse(
        r#"{"resourceType":"Parameters","parameter":[
            {"name":"result","valueBoolean":false},
            {"name":"message","valueString":"Wrong display name"},
            {"name":"display","valueString":"Cat"},
            {"name":"code","valueCode":"cat"},
            {"name":"system","valueUri":"https://terminology.example/animals"},
            {"name":"version","valueString":"2.0"},
            {"name":"issues","resource":{"resourceType":"OperationOutcome","issue":[
              {"severity":"error","code":"invalid",
               "details":{"coding":[{"system":"https://example.org/tx-issue-type",
                 "code":"invalid-display"}],"text":"Wrong display name"},
               "expression":["Coding.display"]}]}}]}"#,
    );
    let read = answer.validation(&ferrohealth_ui::i18n::Locale::default());
    assert_eq!(read.result, Some(false));
    assert_eq!(
        read.display.as_deref(),
        Some("Cat"),
        "the display the system prefers comes back beside the refusal"
    );
    assert_eq!(read.version.as_deref(), Some("2.0"));
    assert_eq!(
        read.issues,
        vec![ValidationIssue {
            severity: "error".to_owned(),
            classifications: vec![Classification {
                code: "invalid-display".to_owned(),
                system: "https://example.org/tx-issue-type".to_owned(),
            }],
            issue_code: "invalid".to_owned(),
            text: "Wrong display name".to_owned(),
            expressions: vec!["Coding.display".to_owned()],
        }],
        "the coding leads, and the IssueType code travels beside it"
    );
}

#[test]
fn an_inactive_concept_is_an_answer_rather_than_a_failure() {
    let read = parse(
        r#"{"parameter":[{"name":"result","valueBoolean":true},
            {"name":"code","valueCode":"dodo"},
            {"name":"inactive","valueBoolean":true},
            {"name":"status","valueCode":"retired"},
            {"name":"message","valueString":"The concept is retired."}]}"#,
    )
    .validation(&ferrohealth_ui::i18n::Locale::default());
    assert_eq!(read.result, Some(true));
    assert_eq!(read.inactive, Some(true));
    assert_eq!(read.status.as_deref(), Some("retired"));
}

#[test]
fn every_unknown_system_the_answer_names_is_kept() {
    let read = parse(
        r#"{"parameter":[{"name":"result","valueBoolean":false},
            {"name":"x-caused-by-unknown-system","valueCanonical":"https://a.example"},
            {"name":"x-unknown-system","valueCanonical":"https://b.example"},
            {"name":"x-caused-by-unknown-system","valueCanonical":"https://c.example"}]}"#,
    )
    .validation(&ferrohealth_ui::i18n::Locale::default());
    assert_eq!(
        read.unknown_systems,
        vec![
            "https://a.example".to_owned(),
            "https://c.example".to_owned(),
            "https://b.example".to_owned(),
        ],
        "a validator is told every system this server does not hold, each parameter in \
         the server's order and the systems the result turned on first"
    );
}

#[test]
fn a_codeable_concept_answer_keeps_its_text_and_its_codings() {
    let read = parse(
        r#"{"parameter":[{"name":"result","valueBoolean":true},
            {"name":"codeableConcept","valueCodeableConcept":{
               "text":"a cat",
               "coding":[{"system":"https://terminology.example/animals","version":"2.0",
                          "code":"cat","display":"Cat"}]}}]}"#,
    )
    .validation(&ferrohealth_ui::i18n::Locale::default());
    let concept = read.concept.expect("the answer echoed the concept");
    assert_eq!(concept.text.as_deref(), Some("a cat"));
    assert_eq!(
        concept.codings.first().map(Coding::rendered),
        Some("https://terminology.example/animals|cat (Cat)".to_owned())
    );
    assert_eq!(
        concept
            .codings
            .first()
            .and_then(|coding| coding.version.clone()),
        Some("2.0".to_owned()),
        "the version the coding was validated in is kept"
    );
}

#[test]
fn an_answer_with_no_issues_reads_as_an_empty_list_rather_than_a_failure() {
    let read = parse(r#"{"parameter":[{"name":"result","valueBoolean":true}]}"#)
        .validation(&ferrohealth_ui::i18n::Locale::default());
    assert_eq!(read.issues, Vec::<ValidationIssue>::new());
}

#[test]
fn an_answer_the_viewer_cannot_read_states_that_rather_than_inventing_one() {
    let unread = validated(r#"{"resourceType":"Parameters"}"#);
    assert!(
        matches!(unread, Err(FhirError::Decode { .. })),
        "`result` is 1..1, so an answer without it is refused, never read as a verdict: \
         {unread:?}"
    );
}

#[test]
fn an_issue_with_no_wording_still_produces_a_line() {
    let read = parse(
        r#"{"parameter":[{"name":"result","valueBoolean":false},
            {"name":"issues","resource":{"resourceType":"OperationOutcome",
            "issue":[{"severity":"error","code":"processing"}]}}]}"#,
    )
    .validation(&ferrohealth_ui::i18n::Locale::default());
    assert_eq!(
        read.issues,
        vec![ValidationIssue {
            severity: "error".to_owned(),
            classifications: Vec::new(),
            issue_code: "processing".to_owned(),
            text: "the server sent no diagnostic".to_owned(),
            expressions: Vec::new(),
        }],
        "a refusal never renders as nothing"
    );
}

#[test]
fn every_subsumption_outcome_is_a_sentence_as_well_as_a_code() {
    assert_eq!(
        subsumption_sentence(
            &ferrohealth_ui::i18n::Locale::default(),
            "equivalent",
            "a",
            "b"
        ),
        "a and b are the same concept."
    );
    assert_eq!(
        subsumption_sentence(
            &ferrohealth_ui::i18n::Locale::default(),
            "subsumes",
            "a",
            "b"
        ),
        "a subsumes b: b is a descendant of a."
    );
    assert_eq!(
        subsumption_sentence(
            &ferrohealth_ui::i18n::Locale::default(),
            "subsumed-by",
            "a",
            "b"
        ),
        "a is subsumed by b: a is a descendant of b."
    );
    assert_eq!(
        subsumption_sentence(
            &ferrohealth_ui::i18n::Locale::default(),
            "not-subsumed",
            "a",
            "b"
        ),
        "Neither a nor b subsumes the other."
    );
    assert_eq!(
        subsumption_sentence(
            &ferrohealth_ui::i18n::Locale::default(),
            "mystery",
            "a",
            "b"
        ),
        "The server answered the outcome `mystery`, which is not one of the four this console has wording for.",
        "an answer the viewer does not know is stated rather than guessed at"
    );
}

#[test]
fn the_subsumption_outcome_is_read_from_the_parameter_that_carries_it() {
    assert_eq!(
        subsumed(r#"{"parameter":[{"name":"outcome","valueCode":"subsumes"}]}"#)
            .expect("a valid answer")
            .subsumption(),
        Some("subsumes".to_owned())
    );
    let unread = subsumed(r#"{"parameter":[{"name":"result","valueBoolean":true}]}"#);
    assert!(
        matches!(unread, Err(FhirError::Decode { .. })),
        "`outcome` is 1..1, so an answer carrying none is refused rather than read as one: \
         {unread:?}"
    );
}
