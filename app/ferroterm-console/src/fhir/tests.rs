// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR client's tests: the addresses it builds and how it reads a refusal.

use super::answer::{excerpt, failure};
use super::*;

/// A refusal answered `status` with `body`.
fn refused(status: u16, body: &str) -> Answer {
    Answer {
        status,
        etag: None,
        location: None,
        body: body.to_owned(),
    }
}

#[test]
fn a_refusal_that_decodes_at_its_release_carries_its_outcome() {
    let error = failure(
        FhirVersion::R4B,
        &refused(
            404,
            r#"{"resourceType":"OperationOutcome","issue":[
                {"severity":"error","code":"not-found","diagnostics":"no such code"}]}"#,
        ),
        StatusCode::NOT_FOUND,
        "/r4b/CodeSystem/$lookup",
    );
    assert_eq!(
        error.outcome().map(|outcome| outcome.issues().len()),
        Some(1),
        "the server's own refusal travels with the error"
    );
}

#[test]
fn a_malformed_refusal_keeps_its_status_and_its_body() {
    for body in [
        r#"{"resourceType":"OperationOutcome","id":"x","text":{"div":"<p/>"}}"#,
        r#"{"resourceType":"OperationOutcome","issue":[{"code":"processing"}]}"#,
    ] {
        let error = failure(
            FhirVersion::R4B,
            &refused(422, body),
            StatusCode::UNPROCESSABLE_ENTITY,
            "/r4b/ValueSet/$expand",
        );
        assert!(
            matches!(&error, FhirError::Status { status, body: kept, .. }
                if *status == StatusCode::UNPROCESSABLE_ENTITY && kept == body),
            "a malformed refusal must not hide the status behind a decode error: {error:?}"
        );
    }
}

#[test]
fn a_version_base_is_the_root_plus_the_version_segment() {
    let client = FhirClient::new("https://tx.example.org".to_owned());
    assert_eq!(
        client.version_base(FhirVersion::R4B),
        "https://tx.example.org/r4b"
    );
}

#[test]
fn the_metadata_addresses_are_the_ones_a_reader_would_type() {
    let client = FhirClient::new("https://tx.example.org".to_owned());
    assert_eq!(
        client.metadata_url(FhirVersion::R5),
        "https://tx.example.org/r5/metadata"
    );
    assert_eq!(
        client.terminology_metadata_url(FhirVersion::R6),
        "https://tx.example.org/r6/metadata?mode=terminology"
    );
}

#[test]
fn a_code_system_search_encodes_the_canonical_it_asks_about() {
    let client = FhirClient::new("https://tx.example.org".to_owned());
    assert_eq!(
        client.code_system_search_url(
            FhirVersion::R4B,
            "https://terminology.example/x?edition=2031"
        ),
        "https://tx.example.org/r4b/CodeSystem?url=https%3A%2F%2Fterminology.example%2Fx%3Fedition%3D2031",
        "a canonical carrying its own query string cannot truncate the search"
    );
}

#[test]
fn an_expansion_address_carries_an_implicit_canonical_whole() {
    let client = FhirClient::new("https://tx.example.org".to_owned());
    let request = ExpandRequest {
        // An implicit value set canonical carries its own query string,
        // and an unencoded one would truncate the request around it.
        url: "http://snomed.info/sct?fhir_vs=isa/404684003".to_owned(),
        count: Some(20),
        offset: Some(40),
        ..ExpandRequest::default()
    };
    assert_eq!(
        client.expand_url(FhirVersion::R4B, &request),
        "https://tx.example.org/r4b/ValueSet/$expand\
         ?url=http%3A%2F%2Fsnomed.info%2Fsct%3Ffhir_vs%3Disa%2F404684003&count=20&offset=40",
        "the operation name survives the path and the canonical survives the query"
    );
}

#[test]
fn a_lookup_address_is_the_one_a_reader_would_type() {
    let client = FhirClient::new("https://tx.example.org".to_owned());
    let request = LookupRequest {
        system: "http://snomed.info/sct".to_owned(),
        code: "404684003".to_owned(),
        ..LookupRequest::default()
    };
    assert_eq!(
        client.lookup_url(FhirVersion::R5, &request),
        "https://tx.example.org/r5/CodeSystem/$lookup\
         ?system=http%3A%2F%2Fsnomed.info%2Fsct&code=404684003",
        "the operation name survives the path and the canonical survives the query"
    );
}

#[test]
fn an_inline_expansion_addresses_the_operation_and_carries_its_body() {
    let client = FhirClient::new("https://tx.example.org".to_owned());
    assert_eq!(
        client.expand_post_url(FhirVersion::R4B),
        "https://tx.example.org/r4b/ValueSet/$expand",
        "a POST carries its parameters in the body, so the address has no query"
    );
}

#[test]
fn a_search_sends_only_the_parameters_the_reader_named() {
    let client = FhirClient::new("https://tx.example.org".to_owned());
    assert_eq!(
        client.search_url(FhirVersion::R4B, VALUE_SET, &SearchFilter::default()),
        "https://tx.example.org/r4b/ValueSet",
        "an unfiltered search asks for everything the root holds"
    );
    assert_eq!(
        client.search_url(
            FhirVersion::R5,
            CONCEPT_MAP,
            &SearchFilter {
                url: "https://terminology.example/cm?a=b".to_owned(),
                version: "2031".to_owned(),
            }
        ),
        "https://tx.example.org/r5/ConceptMap\
         ?url=https%3A%2F%2Fterminology.example%2Fcm%3Fa%3Db&version=2031",
        "a canonical carrying its own query string cannot truncate the search"
    );
}

#[test]
fn a_resource_read_escapes_the_id_into_its_own_segment() {
    let client = FhirClient::new("https://tx.example.org".to_owned());
    assert_eq!(
        client.resource_url(FhirVersion::R6, VALUE_SET, "a/b"),
        "https://tx.example.org/r6/ValueSet/a%2Fb",
        "an id a reader typed cannot reach a route the viewer did not mean to ask for"
    );
}

#[test]
fn a_translation_sends_the_names_the_target_version_declares() {
    let client = FhirClient::new("https://tx.example.org".to_owned());
    let request = TranslateRequest {
        system: "http://snomed.info/sct".to_owned(),
        code: "404684003".to_owned(),
        ..TranslateRequest::default()
    };
    assert_eq!(
        client.translate_url(FhirVersion::R4B, &request),
        "https://tx.example.org/r4b/ConceptMap/$translate\
         ?system=http%3A%2F%2Fsnomed.info%2Fsct&code=404684003"
    );
    assert_eq!(
        client.translate_url(FhirVersion::R6, &request),
        "https://tx.example.org/r6/ConceptMap/$translate\
         ?sourceSystem=http%3A%2F%2Fsnomed.info%2Fsct&sourceCode=404684003",
        "the R6 ballot renamed `system` and `code`"
    );
}

#[test]
fn a_validation_addresses_the_resource_type_it_is_invoked_on() {
    let client = FhirClient::new("https://tx.example.org".to_owned());
    let request = ValidateRequest {
        url: "https://terminology.example/x?edition=2031".to_owned(),
        code: "cat".to_owned(),
        ..ValidateRequest::default()
    };
    assert_eq!(
        client.validate_code_url(FhirVersion::R4B, &request),
        "https://tx.example.org/r4b/CodeSystem/$validate-code\
         ?url=https%3A%2F%2Fterminology.example%2Fx%3Fedition%3D2031&code=cat",
        "the operation name survives the path and the canonical survives the query"
    );
    assert_eq!(
        client.validate_code_url(
            FhirVersion::R5,
            &ValidateRequest {
                on: validation::ValidateOn::ValueSet,
                id: "a/b".to_owned(),
                ..request
            }
        ),
        "https://tx.example.org/r5/ValueSet/a%2Fb/$validate-code?code=cat",
        "an id a reader typed cannot reach a route the viewer did not mean to ask for"
    );
}

#[test]
fn a_subsumption_addresses_the_code_system_and_carries_both_codes() {
    let client = FhirClient::new("https://tx.example.org".to_owned());
    let request = SubsumesRequest {
        system: "http://snomed.info/sct".to_owned(),
        code_a: "404684003".to_owned(),
        code_b: "64572001".to_owned(),
        ..SubsumesRequest::default()
    };
    assert_eq!(
        client.subsumes_url(FhirVersion::R6, &request),
        "https://tx.example.org/r6/CodeSystem/$subsumes\
         ?system=http%3A%2F%2Fsnomed.info%2Fsct&codeA=404684003&codeB=64572001"
    );
    assert_eq!(
        client.subsumes_url(
            FhirVersion::R4,
            &SubsumesRequest {
                id: "animals".to_owned(),
                system: String::new(),
                ..request
            }
        ),
        "https://tx.example.org/r4/CodeSystem/animals/$subsumes?codeA=404684003&codeB=64572001",
        "an instance run names the system in the path and nowhere else"
    );
}

#[test]
fn a_create_addresses_the_resource_type_and_a_history_the_instance() {
    let client = FhirClient::new("https://tx.example.org".to_owned());
    assert_eq!(
        client.create_url(FhirVersion::R5, CODE_SYSTEM),
        "https://tx.example.org/r5/CodeSystem",
        "a create posts to the type, which is what assigns the id"
    );
    assert_eq!(
        client.history_url(FhirVersion::R6, VALUE_SET, "a/b"),
        "https://tx.example.org/r6/ValueSet/a%2Fb/_history",
        "an id a reader typed cannot reach a route the viewer did not mean to ask for"
    );
}

#[test]
fn a_long_failure_body_is_cut_on_a_character_boundary() {
    let body = "é".repeat(BODY_EXCERPT_BYTES);
    let kept = excerpt(&body);
    assert!(
        kept.len() <= BODY_EXCERPT_BYTES + 1,
        "the excerpt stops at the bound, not past it: {} bytes",
        kept.len()
    );
    assert!(
        body.starts_with(&kept),
        "the excerpt is a prefix of the body the server sent"
    );
}

#[test]
fn a_bundle_of_one_shows_the_resource_and_of_several_an_array() {
    let one = r#"{"resourceType":"Bundle","entry":[{"resource":{"id":"a"}}]}"#;
    assert_eq!(resources_text(one), "{\n  \"id\": \"a\"\n}");
    let two = r#"{"entry":[{"resource":{"id":"a"}},{"resource":{"id":"b"}}]}"#;
    assert!(resources_text(two).starts_with('['));
    assert_eq!(resources_text("not json"), "not json");
}
