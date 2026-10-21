// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR R6 ballot under `/r6`: the 6.0.0-ballot5 resources, the shapes
//! the ballot shares with R5, and the inputs it renames or adds
//! (<https://hl7.org/fhir/6.0.0-ballot5/terminology-service.html>).

use ferroterm_testkit::fhir::{ANIMALS, CM_ANIMALS_COLOURS, COLOURS, VS_ALL, VS_PETS};
use fhir_types::codec::{Json, Path, expect_object};
use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::{Server, parameter};

fn part<'a>(m: &'a Value, name: &str) -> Option<&'a Value> {
    m["part"].as_array()?.iter().find(|p| p["name"] == name)
}

#[tokio::test]
async fn the_metadata_resources_are_the_r6_ballot_and_say_so() {
    let server = Server::start();
    let (status, body) = server.get("/r6/metadata").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["fhirVersion"], "6.0.0-ballot5");
    assert_eq!(
        body["url"],
        "https://ferroterm.eu/fhir/CapabilityStatement/ferroterm-r6"
    );
    let description = body["description"].as_str().expect("the ballot note");
    assert!(description.contains("6.0.0-ballot5"), "{description}");
    assert!(description.contains("re-verified"), "{description}");
    let mut path = Path::root("CapabilityStatement");
    let carried = crate::fixture::document(&body);
    let object = expect_object(&carried, &path).expect("object");
    let decoded =
        fhir_types::r6::capability_statement::CapabilityStatement::from_json(object, &mut path)
            .expect("an R6 CapabilityStatement");
    assert_eq!(
        crate::fixture::written(&decoded.to_json().expect("encodes")),
        body
    );
    let (status, body) = server.get("/r6/metadata?mode=terminology").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["description"]
            .as_str()
            .is_some_and(|d| d.contains("ballot")),
        "{body}"
    );
    // R6 renames `codeSystem.version.code` to `value` and makes `content` optional.
    assert_eq!(body["codeSystem"][0]["content"], "not-present");
    let mut path = Path::root("TerminologyCapabilities");
    let carried = crate::fixture::document(&body);
    let object = expect_object(&carried, &path).expect("object");
    fhir_types::r6::terminology_capabilities::TerminologyCapabilities::from_json(object, &mut path)
        .expect("an R6 TerminologyCapabilities");
    let (status, body) = server.get("/r6/$versions").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(parameter(&body, "version").unwrap()["valueCode"], "6.0");
    // R5 carries no ballot note.
    let (status, body) = server.get("/r5/metadata").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("description").is_none(), "{body}");
}

#[tokio::test]
async fn the_operations_answer_in_the_r5_family_shapes_under_r6() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .get(&format!(
            "/r6/CodeSystem/$lookup?system={ANIMALS}&code=animal"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        parameter(&body, "definition").unwrap()["valueString"],
        "A living thing that is not a plant."
    );
    let request = json!({"resourceType": "Parameters", "parameter": [
        {"name": "url", "valueUri": ANIMALS},
        {"name": "coding", "valueCoding": {"system": ANIMALS, "code": "cat", "display": "Dog"}}
    ]});
    let (status, body) = server.post("/r6/CodeSystem/$validate-code", &request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], false);
    assert_eq!(parameter(&body, "code").unwrap()["valueCode"], "cat");
    let issues = &parameter(&body, "issues").expect("issues")["resource"];
    assert_eq!(
        issues["issue"][0]["details"]["coding"][0]["code"],
        "invalid-display"
    );
    let (status, body) = server
        .get(&format!("/r6/ValueSet/$expand?url={VS_PETS}&property=legs"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut path = Path::root("ValueSet");
    let carried = crate::fixture::document(&body);
    let object = expect_object(&carried, &path).expect("object");
    fhir_types::r6::value_set::ValueSet::from_json(object, &mut path).expect("an R6 ValueSet");
    assert_eq!(body["expansion"]["property"][0]["code"], "legs");
    let (status, body) = server
        .get(&format!(
            "/r6/CodeSystem/$subsumes?system={ANIMALS}&codeA=animal&codeB=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        parameter(&body, "outcome").unwrap()["valueCode"],
        "subsumes"
    );
    let (status, body) = server.get(&format!("/r6/ValueSet?url={VS_ALL}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["resourceType"], "Bundle");
}

#[tokio::test]
async fn translate_under_r6_takes_only_the_source_names_and_the_target_inputs() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .get(&format!(
            "/r6/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&sourceSystem={ANIMALS}&sourceCode=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], true);
    let m = parameter(&body, "match").expect("match");
    assert_eq!(
        part(m, "relationship").expect("relationship")["valueCode"],
        "equivalent"
    );
    assert_eq!(
        part(m, "originMap").expect("originMap")["valueCanonical"],
        format!("{CM_ANIMALS_COLOURS}|1.0")
    );
    // R6 declares no `system`: the R5 name is refused.
    let (status, body) = server
        .get(&format!(
            "/r6/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&system={ANIMALS}&sourceCode=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = server
        .get(&format!(
            "/r6/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&targetSystem={COLOURS}&targetCode=RED"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let m = parameter(&body, "match").expect("match");
    assert_eq!(
        part(m, "sourceConcept").expect("sourceConcept")["valueCoding"]["code"],
        "cat"
    );
}

#[tokio::test]
async fn r6_takes_a_repeated_display_language_and_refuses_what_it_does_not_implement() {
    let server = Server::start_with_resources();
    // R6 declares `displayLanguage` 0..* on ValueSet/$validate-code: the list is
    // one BCP 47 range list.
    let (status, body) = server
        .post(
            "/r6/ValueSet/$validate-code",
            &json!({"resourceType": "Parameters", "parameter": [
                {"name": "url", "valueUri": VS_ALL},
                {"name": "coding", "valueCoding": {"system": ANIMALS, "code": "cat", "display": "Katze"}},
                {"name": "displayLanguage", "valueCode": "nl"},
                {"name": "displayLanguage", "valueCode": "de"}
            ]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        parameter(&body, "result").unwrap()["valueBoolean"],
        true,
        "{body}"
    );
    // Declared by the ballot, not implemented: refused, never absorbed.
    for (path, query) in [
        (
            "/r6/ValueSet/$expand",
            format!("url={VS_PETS}&filterProperty=legs"),
        ),
        (
            "/r6/ValueSet/$expand",
            format!("url={VS_PETS}&manifest=http://example.org/manifest"),
        ),
        (
            "/r6/ValueSet/$validate-code",
            format!("url={VS_PETS}&system={ANIMALS}&code=cat&manifest=http://example.org/manifest"),
        ),
    ] {
        let (status, body) = server.get(&format!("{path}?{query}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}?{query}: {body}");
        assert_eq!(body["issue"][0]["code"], "not-supported", "{body}");
    }
}

/// `handle-unclosed-expansion` is declared by the R6 `OperationDefinition`
/// alone (`in 0..1 boolean`); R4, R4B, and R5 declare no such input, so they
/// refuse the name as undeclared
/// (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-expand.html>,
/// <https://hl7.org/fhir/R5/valueset-operation-expand.html>).
#[tokio::test]
async fn handle_unclosed_expansion_is_taken_under_r6_and_refused_under_the_earlier_versions() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .get(&format!(
            "/r6/ValueSet/$expand?url={VS_PETS}&handle-unclosed-expansion=true"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let echoed = body["expansion"]["parameter"]
        .as_array()
        .expect("the echo")
        .iter()
        .find(|p| p["name"] == "handle-unclosed-expansion")
        .expect("the parameter is echoed");
    assert_eq!(echoed["valueBoolean"], true, "{body}");
    // The pets value set draws on a `complete` system, so it is closed and
    // `false` is answered with the expansion.
    let (status, body) = server
        .get(&format!(
            "/r6/ValueSet/$expand?url={VS_PETS}&handle-unclosed-expansion=false"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["expansion"].get("extension").is_none(),
        "a closed expansion carries no mark: {body}"
    );
    for version in ["r4", "r4b", "r5"] {
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$expand?url={VS_PETS}&handle-unclosed-expansion=true"
            ))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "/{version}: {body}");
        assert_eq!(body["issue"][0]["code"], "invalid", "/{version}: {body}");
        let diagnostics = body["issue"][0]["diagnostics"]
            .as_str()
            .expect("diagnostics");
        assert!(
            diagnostics.contains("does not declare a parameter `handle-unclosed-expansion`"),
            "/{version}: {diagnostics}"
        );
    }
}
