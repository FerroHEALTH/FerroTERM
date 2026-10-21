// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FHIR R5 under `/r5`: the 5.0.0 resources and the operation shapes R5 adds
//! (<https://hl7.org/fhir/R5/terminology-service.html>), and the R4-family
//! endpoints staying within their declared outputs.

use ferroterm_testkit::fhir::{ANIMALS, ANIMALS_NL, CM_ANIMALS_COLOURS, COLOURS, VS_ALL, VS_PETS};
use ferroterm_testkit::snomed::{CAT, VERSION, item, sctid};
use fhir_types::codec::{Json, Path, expect_object};
use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::{Server, parameter};

const SCT: &str = "http://snomed.info/sct";

fn part<'a>(m: &'a Value, name: &str) -> Option<&'a Value> {
    m["part"].as_array()?.iter().find(|p| p["name"] == name)
}

#[tokio::test]
async fn the_metadata_resources_are_r5() {
    let server = Server::start();
    let (status, body) = server.get("/r5/metadata").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["fhirVersion"], "5.0.0");
    assert_eq!(
        body["url"],
        "https://ferroterm.eu/fhir/CapabilityStatement/ferroterm-r5"
    );
    let mut path = Path::root("CapabilityStatement");
    let carried = crate::fixture::document(&body);
    let object = expect_object(&carried, &path).expect("object");
    let decoded =
        fhir_types::r5::capability_statement::CapabilityStatement::from_json(object, &mut path)
            .expect("an R5 CapabilityStatement");
    assert_eq!(
        crate::fixture::written(&decoded.to_json().expect("encodes")),
        body
    );
    let (status, body) = server.get("/r5/metadata?mode=terminology").await;
    assert_eq!(status, StatusCode::OK);
    // R5 makes codeSystem.content mandatory; the R4 family has no such element.
    assert_eq!(body["codeSystem"][0]["content"], "not-present");
    let mut path = Path::root("TerminologyCapabilities");
    let carried = crate::fixture::document(&body);
    let object = expect_object(&carried, &path).expect("object");
    fhir_types::r5::terminology_capabilities::TerminologyCapabilities::from_json(object, &mut path)
        .expect("an R5 TerminologyCapabilities");
    let (status, body) = server.get("/r5/$versions").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(parameter(&body, "version").unwrap()["valueCode"], "5.0");
}

#[tokio::test]
async fn lookup_answers_definition_under_r5_only() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .get(&format!(
            "/r5/CodeSystem/$lookup?system={ANIMALS}&code=animal"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        parameter(&body, "definition").unwrap()["valueString"],
        "A living thing that is not a plant."
    );
    let property_codes = |body: &Value| -> Vec<String> {
        body["parameter"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["name"] == "property")
            .map(|p| p["part"][0]["valueCode"].as_str().unwrap().to_owned())
            .collect()
    };
    assert!(
        !property_codes(&body).contains(&String::from("definition")),
        "R5 answers definition once, as the named output: {body}"
    );
    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/$lookup?system={ANIMALS}&code=animal"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        parameter(&body, "definition").is_none(),
        "R4B declares definition as a property, not an output: {body}"
    );
    assert!(
        property_codes(&body).contains(&String::from("definition")),
        "R4B answers definition in the property group: {body}"
    );
    let (status, body) = server
        .get(&format!(
            "/r5/CodeSystem/$lookup?system={SCT}&code={}&useSupplement={ANIMALS_NL}",
            sctid(item(CAT))
        ))
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "useSupplement names a loaded supplement: {body}"
    );
    assert_eq!(parameter(&body, "version").unwrap()["valueString"], VERSION);
    // NOTE: a supplement the server has not loaded is refused (#184).
    let (status, body) = server
        .get(&format!(
            "/r5/CodeSystem/$lookup?system={SCT}&code={}&useSupplement=http://example.org/none",
            sctid(item(CAT))
        ))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["issue"][0]["code"], "not-found");
}

#[tokio::test]
async fn validate_code_itemises_issues_under_r5_and_under_the_overlay_on_r4b() {
    let server = Server::start_with_resources();
    let request = json!({"resourceType": "Parameters", "parameter": [
        {"name": "url", "valueUri": ANIMALS},
        {"name": "coding", "valueCoding": {"system": ANIMALS, "code": "cat", "display": "Dog"}}
    ]});
    let (status, body) = server.post("/r5/CodeSystem/$validate-code", &request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], false);
    assert_eq!(parameter(&body, "code").unwrap()["valueCode"], "cat");
    assert_eq!(parameter(&body, "system").unwrap()["valueUri"], ANIMALS);
    assert_eq!(parameter(&body, "version").unwrap()["valueString"], "2.0");
    let issues = &parameter(&body, "issues").expect("issues")["resource"];
    assert_eq!(issues["resourceType"], "OperationOutcome");
    assert_eq!(
        issues["issue"][0]["details"]["coding"][0]["code"],
        "invalid-display"
    );
    assert_eq!(issues["issue"][0]["expression"][0], "Coding.display");
    let (status, body) = server
        .post("/r4b/CodeSystem/$validate-code", &request)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], false);
    // NOTE: R4B declares none of these; the terminology ecosystem overlay pre-adopts
    // them from R6 (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
    for adopted in ["code", "system", "version", "issues"] {
        assert!(
            parameter(&body, adopted).is_some(),
            "R4B answers the pre-adopted `{adopted}` output: {body}"
        );
    }
    assert_eq!(
        parameter(&body, "issues").unwrap()["resource"]["issue"][0]["details"]["coding"][0]["code"],
        "invalid-display"
    );
    let (status, body) = server
        .get(&format!(
            "/r5/ValueSet/$validate-code?url={VS_PETS}&system={ANIMALS}&code=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], false);
    let issues = &parameter(&body, "issues").expect("issues")["resource"];
    assert_eq!(
        issues["issue"][0]["details"]["coding"][0]["code"],
        "not-in-vs"
    );
    let (status, body) = server
        .get(&format!(
            "/r4b/ValueSet/$validate-code?url={VS_PETS}&system={ANIMALS}&code=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let issues = &parameter(&body, "issues").expect("the overlay's issues on R4B")["resource"];
    assert_eq!(
        issues["issue"][0]["details"]["coding"][0]["code"], "not-in-vs",
        "{body}"
    );
}

#[tokio::test]
async fn expand_returns_properties_under_r5() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .get(&format!("/r5/ValueSet/$expand?url={VS_PETS}&property=legs"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut path = Path::root("ValueSet");
    let carried = crate::fixture::document(&body);
    let object = expect_object(&carried, &path).expect("object");
    fhir_types::r5::value_set::ValueSet::from_json(object, &mut path).expect("an R5 ValueSet");
    assert_eq!(body["expansion"]["property"][0]["code"], "legs");
    assert_eq!(
        body["expansion"]["property"][0]["uri"],
        "http://example.org/legs"
    );
    // `VS_PETS` is one `is-a` include, so the expansion nests and `kitten` is a child.
    let kitten =
        crate::ecosystem::contained(&body["expansion"]["contains"], "kitten").expect("kitten");
    assert_eq!(kitten["property"][0]["code"], "legs");
    assert_eq!(kitten["property"][0]["valueInteger"], 4);
    let (status, body) = server
        .get(&format!("/r5/ValueSet/$expand?url={VS_PETS}"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["expansion"].get("property").is_none(), "{body}");
    // R4B declares no `property` on $expand; the ecosystem overlay pre-adopts it, and the
    // R4B resource has no `expansion.property` element, so the answer travels as the
    // cross-version extension (<https://hl7.org/fhir/R5/versions.html#extensions>).
    let (status, body) = server
        .get(&format!(
            "/r4b/ValueSet/$expand?url={VS_PETS}&property=legs"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["expansion"].get("property").is_none(), "{body}");
    assert_eq!(
        body["expansion"]["extension"][0]["url"],
        "http://hl7.org/fhir/5.0/StructureDefinition/extension-ValueSet.expansion.property"
    );
}

#[tokio::test]
async fn translate_speaks_the_r5_parameter_names() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .get(&format!(
            "/r5/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&system={ANIMALS}&sourceCode=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], true);
    let m = parameter(&body, "match").expect("match");
    assert_eq!(
        part(m, "relationship").expect("relationship")["valueCode"],
        "equivalent"
    );
    assert!(
        part(m, "equivalence").is_none(),
        "R5 has no equivalence part"
    );
    assert_eq!(
        part(m, "concept").expect("concept")["valueCoding"]["code"],
        "RED"
    );
    // R5 declares `originMap` as a uri; the overlay takes R6's canonical.
    assert_eq!(
        part(m, "originMap").expect("originMap")["valueCanonical"],
        format!("{CM_ANIMALS_COLOURS}|1.0")
    );
    // `code` is the R4 name; R5 declares `sourceCode`.
    let (status, body) = server
        .get(&format!(
            "/r5/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&system={ANIMALS}&code=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    // A targetCode is translated in reverse.
    let (status, body) = server
        .get(&format!(
            "/r5/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&system={COLOURS}&targetCode=RED"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], true);
    // The match is reported source to target: `concept` is the target code.
    let m = parameter(&body, "match").expect("match");
    assert_eq!(
        part(m, "concept").expect("concept")["valueCoding"]["code"],
        "RED"
    );
    assert_eq!(
        part(m, "sourceConcept").expect("sourceConcept")["valueCoding"]["code"],
        "cat"
    );
    // R4B emits its declared match parts plus the overlay's `originMap`, as a
    // canonical (pre-adopted from R6), and no `relationship`.
    let (status, body) = server
        .get(&format!(
            "/r4b/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&system={ANIMALS}&code=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let m = parameter(&body, "match").expect("match");
    assert_eq!(
        part(m, "originMap").expect("originMap")["valueCanonical"],
        format!("{CM_ANIMALS_COLOURS}|1.0"),
        "{body}"
    );
    assert!(part(m, "relationship").is_none(), "{body}");
    assert_eq!(
        part(m, "equivalence").expect("equivalence")["valueCode"],
        "equivalent"
    );
}

#[tokio::test]
async fn expand_reads_a_property_sent_as_a_code() {
    let server = Server::start_with_resources();
    // `property` is declared as a string; a `code` specializes `string`
    // (<https://hl7.org/fhir/R5/datatypes.html#primitive>).
    let (status, body) = server
        .post(
            "/r5/ValueSet/$expand",
            &json!({"resourceType": "Parameters", "parameter": [
                {"name": "url", "valueCanonical": VS_ALL},
                {"name": "property", "valueCode": "legs"}
            ]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let cat = body["expansion"]["contains"]
        .as_array()
        .expect("contains")
        .iter()
        .find(|c| c["code"] == "cat")
        .expect("cat");
    assert_eq!(cat["property"][0]["code"], "legs");
    // An integer sent for a string parameter is still refused.
    let (status, body) = server
        .post(
            "/r5/ValueSet/$expand",
            &json!({"resourceType": "Parameters", "parameter": [
                {"name": "url", "valueUri": VS_ALL},
                {"name": "property", "valueInteger": 4}
            ]}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn code_system_validate_code_takes_lenient_display_validation() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .get(&format!(
            "/r5/CodeSystem/$validate-code?url={ANIMALS}&code=dog&display=Hound"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], false);
    let (status, body) = server
        .get(&format!(
            "/r5/CodeSystem/$validate-code?url={ANIMALS}&code=dog&display=Hound&lenient-display-validation=true"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], true);
    assert_eq!(
        parameter(&body, "message").unwrap()["valueString"],
        "'Hound' is no longer considered a correct display for code 'dog' (status = withdrawn). The correct display is 'Dog'"
    );
    // A deprecated concept: a `status` output and no `inactive`.
    let (status, body) = server
        .get(&format!(
            "/r5/CodeSystem/$validate-code?url={ANIMALS}&code=plant"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], true);
    assert_eq!(
        parameter(&body, "status").unwrap()["valueCode"],
        "deprecated"
    );
    assert!(parameter(&body, "inactive").is_none(), "{body}");
}
