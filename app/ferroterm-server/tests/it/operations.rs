// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `CodeSystem` operations over HTTP: GET and POST, type and instance level,
//! and every refusal as an `OperationOutcome`.

use ferroterm_testkit::snomed::{ANIMAL, CAT, DOG, FISH, Item, VERSION, item, sctid};
use http::StatusCode;
use serde_json::json;

use crate::fixture::{Server, parameter, parameters};
use ferroterm_testkit::fhir::{ANIMALS, CM_ANIMALS_COLOURS, CM_FALLBACK, VS_ALL, VS_PETS};

const SCT: &str = "http://snomed.info/sct";

#[tokio::test]
async fn lookup_by_get_and_post() {
    let server = Server::start();
    let cat = sctid(item(CAT));
    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/$lookup?system={SCT}&code={cat}&displayLanguage=nl"
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["resourceType"], "Parameters");
    assert_eq!(
        parameter(&body, "name").unwrap()["valueString"],
        "SNOMED CT"
    );
    assert_eq!(parameter(&body, "version").unwrap()["valueString"], VERSION);
    assert_eq!(parameter(&body, "display").unwrap()["valueString"], "Kat");
    let designations = body["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"] == "designation")
        .count();
    assert_eq!(designations, 5);
    let inactive = body["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "property" && p["part"][0]["valueCode"] == "inactive")
        .expect("inactive property");
    assert_eq!(inactive["part"][1]["valueBoolean"], false);

    let (status, body) = server
        .post(
            "/r4b/CodeSystem/$lookup",
            &parameters(&[
                (
                    "coding",
                    json!({"valueCoding": {"system": SCT, "code": cat}}),
                ),
                ("property", json!({"valueCode": "sufficientlyDefined"})),
                ("property", json!({"valueCode": "lang.en"})),
            ]),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let properties: Vec<&str> = body["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"] == "property")
        .map(|p| p["part"][0]["valueCode"].as_str().unwrap())
        .collect();
    assert_eq!(properties, ["sufficientlyDefined"]);
    let english = body["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"] == "designation")
        .count();
    assert_eq!(english, 3, "lang.en keeps the English designations only");
}

/// A parameter the operation does not declare is refused on both routes.
///
/// A `GET` is checked while its query is decoded, and a `POST` carries a
/// `Parameters` the codec reads without consulting the operation. The two have
/// to agree, or a server silently accepts an input it then ignores
/// (<https://hl7.org/fhir/R4B/operations.html#3.2.0.6>).
#[tokio::test]
async fn an_undeclared_parameter_is_refused_on_get_and_on_post() {
    let server = Server::start();
    let cat = sctid(item(CAT));

    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/$lookup?system={SCT}&code={cat}&colour=red"
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["issue"][0]["code"], "invalid");

    let (status, body) = server
        .post(
            "/r4b/CodeSystem/$lookup",
            &parameters(&[
                ("system", json!({"valueUri": SCT})),
                ("code", json!({"valueCode": cat})),
                ("colour", json!({"valueString": "red"})),
            ]),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["issue"][0]["code"], "invalid");
    assert!(
        body["issue"][0]["details"]["text"]
            .as_str()
            .is_some_and(|text| text.contains("colour")),
        "the refusal names the parameter: {body}"
    );
}

#[tokio::test]
async fn lookup_refusals_on_the_wire() {
    let server = Server::start();
    let cat = sctid(item(CAT));
    // Undeclared query parameter.
    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/$lookup?system={SCT}&code={cat}&colour=red"
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["issue"][0]["code"], "invalid");
    // A complex parameter on GET.
    let (status, body) = server.get("/r4b/CodeSystem/$lookup?coding=x").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["issue"][0]["code"], "not-supported");
    // No system.
    let (status, body) = server
        .get(&format!("/r4b/CodeSystem/$lookup?code={cat}"))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["issue"][0]["code"], "required");
    // Unknown code: 400 code-invalid. The code "was not valid in the context",
    // where `not-found` is for a reference that could not be resolved
    // (<https://hl7.org/fhir/R4B/valueset-issue-type.html>); the system below
    // is such a reference.
    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/$lookup?system={SCT}&code={}",
            sctid(Item::raw(4242))
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["issue"][0]["code"], "code-invalid");
    // Unknown system: 404.
    let (status, _) = server
        .get(&format!(
            "/r4b/CodeSystem/$lookup?system=http://loinc.org&code={cat}"
        ))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Instance-level $lookup is not an R4B invocation: the route does not exist.
    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/{}/$lookup?code={cat}",
            server.snomed_id()
        ))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["resourceType"], "OperationOutcome");
    // A repeated singular parameter in a POST body.
    let (status, body) = server
        .post(
            "/r4b/CodeSystem/$lookup",
            &parameters(&[
                ("system", json!({"valueUri": SCT})),
                ("code", json!({"valueCode": cat})),
                ("code", json!({"valueCode": cat})),
            ]),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["issue"][0]["code"], "invalid");
    // Not JSON, and not FHIR JSON.
    let (status, body) = server
        .post_raw(
            "/r4b/CodeSystem/$lookup",
            "application/fhir+json",
            "{not json",
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["issue"][0]["code"], "structure");
    let (status, _) = server
        .post_raw("/r4b/CodeSystem/$lookup", "text/plain", "{}")
        .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let (status, body) = server
        .post_raw(
            "/r4b/CodeSystem/$lookup",
            "application/fhir+json",
            r#"{"resourceType":"Patient"}"#,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["issue"][0]["code"], "structure");
}

#[tokio::test]
async fn validate_code_at_type_and_instance_level() {
    let server = Server::start();
    let cat = sctid(item(CAT));
    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/$validate-code?url={SCT}&code={cat}&display=Kat"
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], true);
    assert_eq!(parameter(&body, "display").unwrap()["valueString"], "Cat");
    // Wrong display: false with the correct display.
    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/$validate-code?url={SCT}&code={cat}&display=Dog"
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], false);
    assert_eq!(parameter(&body, "display").unwrap()["valueString"], "Cat");
    // Inactive: true with a message.
    let (_, body) = server
        .get(&format!(
            "/r4b/CodeSystem/$validate-code?url={SCT}&code={}",
            sctid(item(FISH))
        ))
        .await;
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], true);
    assert!(
        parameter(&body, "message").unwrap()["valueString"]
            .as_str()
            .unwrap()
            .contains("inactive")
    );
    // Instance level: the instance is the system; `system` is not an R4B parameter.
    let id = server.snomed_id();
    let (status, body) = server
        .get(&format!("/r4b/CodeSystem/{id}/$validate-code?code={cat}"))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], true);
    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/{id}/$validate-code?system={SCT}&code={cat}"
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["issue"][0]["diagnostics"]
            .as_str()
            .unwrap()
            .contains("system")
    );
    let (status, _) = server
        .get(&format!("/r4b/CodeSystem/nope/$validate-code?code={cat}"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // POST with a codeableConcept.
    let (status, body) = server
        .post(
            &format!("/r4b/CodeSystem/{id}/$validate-code"),
            &parameters(&[(
                "codeableConcept",
                json!({"valueCodeableConcept": {"coding": [
                    {"system": "http://loinc.org", "code": "1234-5"},
                    {"system": SCT, "code": sctid(item(DOG))}
                ]}}),
            )]),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], true);
    assert_eq!(parameter(&body, "display").unwrap()["valueString"], "Dog");
}

#[tokio::test]
async fn subsumes_at_type_and_instance_level() {
    let server = Server::start();
    let (animal, cat, dog) = (sctid(item(ANIMAL)), sctid(item(CAT)), sctid(item(DOG)));
    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/$subsumes?system={SCT}&codeA={animal}&codeB={cat}"
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        parameter(&body, "outcome").unwrap()["valueCode"],
        "subsumes"
    );
    let id = server.snomed_id();
    let (_, body) = server
        .get(&format!(
            "/r4b/CodeSystem/{id}/$subsumes?codeA={cat}&codeB={dog}"
        ))
        .await;
    assert_eq!(
        parameter(&body, "outcome").unwrap()["valueCode"],
        "not-subsumed"
    );
    let (status, body) = server
        .post(
            "/r4b/CodeSystem/$subsumes",
            &parameters(&[
                (
                    "codingA",
                    json!({"valueCoding": {"system": SCT, "code": cat}}),
                ),
                (
                    "codingB",
                    json!({"valueCoding": {"system": SCT, "code": animal}}),
                ),
            ]),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        parameter(&body, "outcome").unwrap()["valueCode"],
        "subsumed-by"
    );
    // An unknown code is an error, never not-subsumed, and the error is
    // `code-invalid` rather than `not-found`: the system resolved and the code
    // did not (<https://hl7.org/fhir/R4B/valueset-issue-type.html>, and the
    // ecosystem's `simple-subsumes-unknown-code`).
    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/$subsumes?system={SCT}&codeA={cat}&codeB={}",
            sctid(Item::raw(4242))
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["issue"][0]["code"], "code-invalid");
    // No system at type level.
    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/$subsumes?codeA={cat}&codeB={dog}"
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["issue"][0]["code"], "required");
}

/// `Accept-Language` picks the display language when `displayLanguage` is
/// absent, the parameter wins when both are given, and the header's quality
/// order and wildcard are honoured
/// (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/languages.html>).
#[tokio::test]
async fn accept_language_selects_the_display_when_no_parameter_does() {
    let server = Server::start();
    let cat = sctid(item(CAT));
    let uri = format!("/r4b/CodeSystem/$lookup?system={SCT}&code={cat}&property=display");
    let (status, body) = server.get_with_header(&uri, "Accept-Language", "nl").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(parameter(&body, "display").unwrap()["valueString"], "Kat");
    let (_, body) = server
        .get_with_header(
            &format!("{uri}&displayLanguage=en"),
            "Accept-Language",
            "nl",
        )
        .await;
    assert_eq!(
        parameter(&body, "display").unwrap()["valueString"],
        "Cat",
        "the parameter wins"
    );
    let (_, body) = server
        .get_with_header(&uri, "Accept-Language", "fr;q=0.9, nl;q=0.8, en;q=0.7")
        .await;
    assert_eq!(
        parameter(&body, "display").unwrap()["valueString"],
        "Kat",
        "French is not carried, Dutch is the next by quality"
    );
    let (_, body) = server
        .get_with_header(&uri, "Accept-Language", "en, nl;q=0.4")
        .await;
    assert_eq!(parameter(&body, "display").unwrap()["valueString"], "Cat");
    let (_, body) = server.get_with_header(&uri, "Accept-Language", "*").await;
    assert_eq!(
        parameter(&body, "display").unwrap()["valueString"],
        "Cat",
        "any language is the system's own"
    );
    let (status, body) = server
        .post_with_header(
            "/r4b/CodeSystem/$validate-code",
            &parameters(&[
                ("url", json!({"valueUri": SCT})),
                ("code", json!({"valueCode": cat})),
                ("display", json!({"valueString": "Cat"})),
            ]),
            "Accept-Language",
            "nl",
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        parameter(&body, "display").unwrap()["valueString"],
        "Kat",
        "the header picks the display returned on POST too"
    );
    let (status, body) = server
        .get_with_header(
            &format!(
                "/r4b/CodeSystem/$subsumes?system={SCT}&codeA={}&codeB={cat}",
                sctid(item(ANIMAL))
            ),
            "Accept-Language",
            "nl",
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an operation without displayLanguage ignores the header"
    );
    assert_eq!(
        parameter(&body, "outcome").unwrap()["valueCode"],
        "subsumes"
    );
}

/// The `sourceCode` spelling of the version served under `base`.
///
/// R5 renames `$translate`'s `code` to `sourceCode`, and the R6 ballot keeps
/// that name (<https://hl7.org/fhir/R5/conceptmap-operation-translate.html>).
fn source_code(base: &str) -> &'static str {
    if matches!(base, "r5" | "r6") {
        "sourceCode"
    } else {
        "code"
    }
}

#[tokio::test]
async fn expand_answers_at_the_instance_level_on_every_version() {
    let server = Server::start_with_resources();
    let id = server.value_set_id_of(VS_PETS);
    // Every served version's `OperationDefinition` declares `$expand` at the
    // instance level, where the operation runs on that value set
    // (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>,
    // <https://hl7.org/fhir/R4B/operations.html#request>).
    for base in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server.get(&format!("/{base}/ValueSet/{id}/$expand")).await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body["resourceType"], "ValueSet", "{base}");
        assert_eq!(body["url"], VS_PETS, "{base}: {body}");
        assert!(
            body["expansion"]["contains"]
                .as_array()
                .is_some_and(|members| !members.is_empty()),
            "{base}: the instance expanded: {body}"
        );

        let (status, posted) = server
            .post(
                &format!("/{base}/ValueSet/{id}/$expand"),
                &json!({"resourceType": "Parameters", "parameter": [
                    {"name": "count", "valueInteger": 10}
                ]}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {posted}");
        assert_eq!(posted["url"], VS_PETS, "{base}: {posted}");
    }

    // The instance form binds the resource, so a `url` naming another one
    // contradicts the invocation (<https://hl7.org/fhir/R4B/operations.html#request>).
    let (status, body) = server
        .get(&format!("/r4b/ValueSet/{id}/$expand?url={VS_ALL}"))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["issue"][0]["code"], "invalid");

    let (status, body) = server.get("/r4b/ValueSet/no-such-set/$expand").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["issue"][0]["code"], "not-found");
}

#[tokio::test]
async fn translate_answers_at_the_instance_level_on_every_version() {
    let server = Server::start_with_resources();
    let id = server.concept_map_id_of(CM_ANIMALS_COLOURS);
    // `$translate` is declared at the instance level on every served version
    // (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>).
    for base in ["r4", "r4b", "r5", "r6"] {
        let code = source_code(base);
        let (status, body) = server
            .get(&format!(
                "/{base}/ConceptMap/{id}/$translate?sourceSystem={ANIMALS}&{code}=cat"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body["resourceType"], "Parameters", "{base}");
        assert_eq!(
            parameter(&body, "result").map(|p| &p["valueBoolean"]),
            Some(&json!(true)),
            "{base}: {body}"
        );

        let (status, posted) = server
            .post(
                &format!("/{base}/ConceptMap/{id}/$translate"),
                &json!({"resourceType": "Parameters", "parameter": [
                    {"name": "sourceSystem", "valueUri": ANIMALS},
                    {"name": code, "valueCode": "cat"}
                ]}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {posted}");
        assert_eq!(
            parameter(&posted, "result").map(|p| &p["valueBoolean"]),
            Some(&json!(true)),
            "{base}: {posted}"
        );
    }

    let (status, body) = server
        .get(&format!(
            "/r4b/ConceptMap/{id}/$translate?url={CM_FALLBACK}&sourceSystem={ANIMALS}&code=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["issue"][0]["code"], "invalid");

    let (status, body) = server
        .get(&format!(
            "/r4b/ConceptMap/no-such-map/$translate?system={ANIMALS}&code=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn a_persisted_value_set_expands_at_its_instance_id() {
    let server = Server::start_persisting();
    let set = json!({
        "resourceType": "ValueSet",
        "url": "http://ferroterm.test/ValueSet/instance-expand",
        "version": "1.0", "status": "active",
        "compose": {"include": [{"system": ANIMALS, "concept": [{"code": "cat"}]}]}
    });
    let response = server.put("/r4b/ValueSet/instance-expand", &set).await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let (status, body) = server.get("/r4b/ValueSet/instance-expand/$expand").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["expansion"]["contains"][0]["code"], "cat", "{body}");
}

#[tokio::test]
async fn a_persisted_value_set_without_a_canonical_expands_at_its_instance_id() {
    // "If the operation is not called at the instance level, one of the in
    // parameters url, context or valueSet must be provided"
    // (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>), so an
    // instance needs no canonical of its own. `ValueSet.url` is 0..1.
    let server = Server::start_persisting();
    let set = json!({
        "resourceType": "ValueSet",
        "version": "1.0", "status": "active",
        "compose": {"include": [{"system": ANIMALS, "concept": [{"code": "dog"}]}]}
    });
    let response = server.put("/r4b/ValueSet/local-only", &set).await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let (status, body) = server.get("/r4b/ValueSet/local-only").await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = server.get("/r4b/ValueSet/local-only/$expand").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["expansion"]["contains"][0]["code"], "dog", "{body}");
}

#[tokio::test]
async fn value_set_validate_code_answers_at_the_instance_level_on_every_version() {
    let server = Server::start_with_resources();
    let id = server.value_set_id_of(VS_PETS);
    // Every served version's `OperationDefinition` declares
    // `ValueSet/$validate-code` at the instance level
    // (<https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>).
    for base in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server
            .get(&format!(
                "/{base}/ValueSet/{id}/$validate-code?system={ANIMALS}&code=kitten"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(
            parameter(&body, "result").map(|p| &p["valueBoolean"]),
            Some(&json!(true)),
            "{base}: {body}"
        );

        let (status, posted) = server
            .post(
                &format!("/{base}/ValueSet/{id}/$validate-code"),
                &json!({"resourceType": "Parameters", "parameter": [
                    {"name": "system", "valueUri": ANIMALS},
                    {"name": "code", "valueCode": "kitten"}
                ]}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {posted}");
        assert_eq!(
            parameter(&posted, "result").map(|p| &p["valueBoolean"]),
            Some(&json!(true)),
            "{base}: {posted}"
        );
    }

    let (status, body) = server
        .get(&format!(
            "/r4b/ValueSet/{id}/$validate-code?url={VS_ALL}&system={ANIMALS}&code=kitten"
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["issue"][0]["code"], "invalid");
}

#[tokio::test]
async fn an_instance_invocation_takes_the_instance_s_own_canonical_and_refuses_another() {
    let server = Server::start_with_resources();
    let id = server.value_set_id_of(VS_PETS);
    let version = server
        .state()
        .value_set_instances()
        .into_iter()
        .find(|(_, url, _)| url == VS_PETS)
        .and_then(|(_, _, version)| version)
        .expect("the loaded value set states a version");

    // A canonical carries its version after a `|`
    // (<https://hl7.org/fhir/R4B/references.html#canonical>), so naming the
    // instance's own canonical in full is the instance, not a contradiction.
    let (status, body) = server
        .get(&format!(
            "/r4b/ValueSet/{id}/$expand?url={VS_PETS}|{version}"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["url"], VS_PETS, "{body}");

    let (status, body) = server
        .get(&format!("/r4b/ValueSet/{id}/$expand?url={VS_PETS}|0.0"))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["issue"][0]["code"], "invalid");

    let (status, body) = server
        .get(&format!("/r4b/ValueSet/{id}/$expand?valueSetVersion=0.0"))
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a version that is not the instance's: {body}"
    );
    assert_eq!(body["issue"][0]["code"], "invalid");

    // The instance form runs on the resource the URL names
    // (<https://hl7.org/fhir/R4B/operations.html#request>), so an inline one
    // has nowhere to go.
    let (status, body) = server
        .post(
            &format!("/r4b/ValueSet/{id}/$expand"),
            &json!({"resourceType": "Parameters", "parameter": [
                {"name": "valueSet", "resource": {
                    "resourceType": "ValueSet", "status": "active",
                    "compose": {"include": [{"system": ANIMALS}]}
                }}
            ]}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["issue"][0]["code"], "invalid");

    let map = server.concept_map_id_of(CM_ANIMALS_COLOURS);
    let (status, body) = server
        .get(&format!(
            "/r4b/ConceptMap/{map}/$translate?conceptMapVersion=0.0&system={ANIMALS}&code=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["issue"][0]["code"], "invalid");
}
