// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ConceptMap/$translate` on the wire
//! (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>).

use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::Server;
use ferroterm_testkit::fhir::{ANIMALS, CM_ANIMALS_COLOURS, COLOURS};

fn param<'a>(body: &'a Value, name: &str) -> Option<&'a Value> {
    body["parameter"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
}

fn part<'a>(m: &'a Value, name: &str) -> Option<&'a Value> {
    m["part"].as_array()?.iter().find(|p| p["name"] == name)
}

#[tokio::test]
async fn translate_by_get_and_post_answers_matches() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .get(&format!(
            "/r4b/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&system={ANIMALS}&code=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        param(&body, "result").expect("result")["valueBoolean"],
        true
    );
    let m = param(&body, "match").expect("match");
    assert_eq!(
        part(m, "equivalence").expect("equivalence")["valueCode"],
        "equivalent"
    );
    assert_eq!(
        part(m, "concept").expect("concept")["valueCoding"]["code"],
        "RED"
    );
    assert_eq!(
        part(m, "concept").expect("concept")["valueCoding"]["system"],
        COLOURS
    );
    // The ecosystem answers `source` as a canonical (`url|version`).
    assert_eq!(
        part(m, "source").expect("source")["valueCanonical"],
        format!("{CM_ANIMALS_COLOURS}|1.0")
    );
    let (status, body) = server
        .post(
            "/r4b/ConceptMap/$translate",
            &json!({"resourceType": "Parameters", "parameter": [
                {"name": "coding", "valueCoding": {"system": ANIMALS, "code": "fish"}},
                {"name": "targetsystem", "valueUri": COLOURS}
            ]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // An explicit `noMap` is a match that translates nothing: `result` false
    // (R4B `$translate`, "can only be true if at least one returned match has
    // an equivalence which is not unmatched or disjoint"), the overlay's
    // `noMap` part, no `equivalence`, and no message.
    assert_eq!(
        param(&body, "result").expect("result")["valueBoolean"],
        false
    );
    let m = param(&body, "match").expect("match");
    assert!(part(m, "equivalence").is_none(), "{body}");
    assert_eq!(part(m, "noMap").expect("noMap")["valueBoolean"], true);
    assert_eq!(
        part(m, "sourceComment").expect("sourceComment")["valueString"],
        "fish have no colour"
    );
    assert_eq!(
        part(m, "sourceConcept").expect("sourceConcept")["valueCoding"]["code"],
        "fish"
    );
    assert_eq!(
        part(m, "originMap").expect("originMap")["valueCanonical"],
        format!("{CM_ANIMALS_COLOURS}|1.0"),
        "originMap is pre-adopted from R6 as a canonical"
    );
    assert!(param(&body, "message").is_none(), "{body}");
}

#[tokio::test]
async fn translate_on_r4b_accepts_the_pre_adopted_r6_names() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .get(&format!(
            "/r4b/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&sourceSystem={ANIMALS}&sourceCode=cat&targetSystem={COLOURS}"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        param(&body, "result").expect("result")["valueBoolean"],
        true
    );
    let m = param(&body, "match").expect("match");
    assert_eq!(
        part(m, "concept").expect("concept")["valueCoding"]["code"],
        "RED"
    );
    // A `targetCode` reads the map in reverse, as R5 defines it; the match is
    // reported source to target.
    let (status, body) = server
        .get(&format!(
            "/r4b/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&system={COLOURS}&targetCode=RED"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let m = param(&body, "match").expect("match");
    assert_eq!(
        part(m, "concept").expect("concept")["valueCoding"]["code"],
        "RED"
    );
    assert_eq!(
        part(m, "sourceConcept").expect("sourceConcept")["valueCoding"]["code"],
        "cat"
    );
    // An R4 `reverse` reports the match as the map states it, as `targetCode`
    // does, and a reverse read that finds nothing names both systems.
    for version in ["r4", "r4b"] {
        let (status, body) = server
            .get(&format!(
                "/{version}/ConceptMap/$translate?system={COLOURS}&code=RED&reverse=true&targetsystem={ANIMALS}"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        let m = param(&body, "match").expect("match");
        assert_eq!(
            part(m, "concept").expect("concept")["valueCoding"]["code"],
            "RED",
            "{version}: {body}"
        );
        assert_eq!(
            part(m, "sourceConcept").expect("sourceConcept")["valueCoding"]["code"],
            "cat",
            "{version}: {body}"
        );
        let (status, body) = server
            .get(&format!(
                "/{version}/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&system={ANIMALS}&code=cat&reverse=true&targetsystem={COLOURS}"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        let message = param(&body, "message").expect("message")["valueString"]
            .as_str()
            .expect("a message");
        assert!(
            message.contains(ANIMALS) && message.contains(COLOURS),
            "{version}: {message}"
        );
    }
    // R5 declares no `reverse`; the refusal names the target inputs.
    let (status, body) = server
        .get(&format!(
            "/r5/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&system={ANIMALS}&sourceCode=cat&reverse=true"
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["issue"][0]["code"], "not-supported");
}

#[tokio::test]
async fn translate_refuses_what_it_cannot_answer() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .get(&format!(
            "/r4b/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&code=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["issue"][0]["code"], "invalid");
    let (status, body) = server
        .get(&format!("/r4b/ConceptMap/$translate?url=http://example.org/fhir/ConceptMap/nowhere&system={ANIMALS}&code=cat"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, body) = server.get("/r4b/metadata").await;
    assert_eq!(status, StatusCode::OK);
    let concept_map = body["rest"][0]["resource"]
        .as_array()
        .expect("resources")
        .iter()
        .find(|r| r["type"] == "ConceptMap")
        .expect("ConceptMap");
    assert_eq!(concept_map["operation"][0]["name"], "translate");
}

// The ecosystem IG 1.11.0, `$translate`, The result: an invalid target keeps its
// match and adds one `error`, and an unknown target system adds a `note` part
// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
#[tokio::test]
async fn translate_checks_every_target_code_on_every_version() {
    let server = Server::start_with_resources();
    let nowhere = "http://example.org/fhir/CodeSystem/nowhere";
    let map = "http://example.org/fhir/ConceptMap/checked";
    for (base, relation) in [
        ("r4", "equivalence"),
        ("r4b", "equivalence"),
        ("r5", "relationship"),
        ("r6", "relationship"),
    ] {
        let group = |target: &str, code: &str, to: &str| {
            json!({"source": ANIMALS, "target": target, "element": [
                {"code": code, "target": [{"code": to, (relation): "equivalent"}]}]})
        };
        let concept_map = json!({"resourceType": "ConceptMap", "url": map,
            "version": "1", "status": "draft", "group": [
                group(COLOURS, "dog", "MAUVE"), group(nowhere, "fish", "x")]});
        let ask = |code: &str| {
            json!({"resourceType": "Parameters", "parameter": [
                {"name": "url", "valueUri": map},
                {"name": "system", "valueUri": ANIMALS},
                {"name": "code", "valueCode": code},
                {"name": "tx-resource", "resource": concept_map.clone()}]})
        };
        let path = format!("/{base}/ConceptMap/$translate");
        let (status, body) = server.post(&path, &ask("dog")).await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        let m = param(&body, "match").expect("the match stays");
        assert_eq!(
            part(m, "concept").expect("concept")["valueCoding"]["code"],
            "MAUVE",
            "{base}"
        );
        assert_eq!(
            param(&body, "error").expect("error")["valueString"],
            format!("The code 'MAUVE' is not valid in the code system '{COLOURS}'"),
            "{base}: {body}"
        );
        assert!(part(m, "note").is_none(), "{base}: {body}");
        let (status, body) = server.post(&path, &ask("fish")).await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert!(param(&body, "error").is_none(), "{base}: {body}");
        let m = param(&body, "match").expect("match");
        assert_eq!(
            part(m, "note").expect("note")["valueString"],
            format!("The code system '{nowhere}' is not known, so the code 'x' has not been validated"),
            "{base}: {body}"
        );
    }
}
