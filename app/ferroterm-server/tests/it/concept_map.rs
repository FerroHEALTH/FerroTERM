// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ConceptMap` instances the deployment loaded, read and searched.
//!
//! The FHIR REST API defines the instance read and the type-level search over
//! every resource a server holds (<https://hl7.org/fhir/R4B/http.html#read>,
//! <https://hl7.org/fhir/R4B/http.html#search>), and `ConceptMap` search
//! defines `url` and `version`
//! (<https://hl7.org/fhir/R4B/conceptmap.html#search>).

use http::StatusCode;
use serde_json::Value;

use crate::fixture::{Server, base_of};
use ferroterm_testkit::fhir::{ANIMALS, CM_ANIMALS_COLOURS, CM_FALLBACK, COLOURS};

/// The instance id of the concept map at `url`, from the search that finds it.
fn found_id(body: &Value) -> String {
    body["entry"]
        .as_array()
        .and_then(|entries| entries.first())
        .and_then(|entry| entry["fullUrl"].as_str())
        .and_then(|full| full.strip_prefix(&format!("{}/ConceptMap/", base_of("r4b"))))
        .expect("the entry addresses a concept map")
        .to_owned()
}

#[tokio::test]
async fn a_map_translate_resolves_through_is_searchable_and_readable() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .get(&format!(
            "/r4b/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&system={ANIMALS}&code=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["parameter"]
            .as_array()
            .expect("parameters")
            .iter()
            .find(|p| p["name"] == "result")
            .expect("result")["valueBoolean"],
        true,
        "the map translates: {body}"
    );

    let (status, body) = server
        .get(&format!("/r4b/ConceptMap?url={CM_ANIMALS_COLOURS}"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["resourceType"], "Bundle");
    assert_eq!(body["type"], "searchset");
    assert_eq!(
        body["total"], 1,
        "the map the server translates through: {body}"
    );
    let id = found_id(&body);
    assert_eq!(body["entry"][0]["search"]["mode"], "match");
    assert_eq!(body["entry"][0]["resource"]["id"], id, "{body}");
    assert_eq!(body["entry"][0]["resource"]["url"], CM_ANIMALS_COLOURS);

    let (status, body) = server.get(&format!("/r4b/ConceptMap/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["resourceType"], "ConceptMap");
    assert_eq!(body["id"], id, "{body}");
    assert_eq!(body["url"], CM_ANIMALS_COLOURS);
    assert_eq!(body["version"], "1.0");
    assert_eq!(body["status"], "active");
    assert_eq!(body["group"][0]["source"], ANIMALS);
    assert_eq!(body["group"][0]["target"], COLOURS);
}

#[tokio::test]
async fn a_search_without_criteria_lists_every_loaded_concept_map() {
    let server = Server::start_with_resources();
    let (status, body) = server.get("/r4b/ConceptMap").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let urls: Vec<&str> = body["entry"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|entry| entry["resource"]["url"].as_str())
        .collect();
    assert!(urls.contains(&CM_ANIMALS_COLOURS), "{urls:?}");
    assert!(urls.contains(&CM_FALLBACK), "{urls:?}");
    assert_eq!(
        body["total"].as_u64(),
        Some(u64::try_from(urls.len()).expect("fits")),
        "total counts the entries: {body}"
    );
}

#[tokio::test]
async fn a_search_matches_the_version_the_map_declares() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .get(&format!(
            "/r4b/ConceptMap?url={CM_ANIMALS_COLOURS}&version=1.0"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 1);

    let (status, body) = server
        .get(&format!(
            "/r4b/ConceptMap?url={CM_ANIMALS_COLOURS}&version=0.0"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["total"], 0,
        "a version that is not served matches none: {body}"
    );
}

#[tokio::test]
async fn a_loaded_map_reads_in_each_version_s_own_vocabulary() {
    let server = Server::start_with_resources();
    let (_, found) = server
        .get(&format!("/r4b/ConceptMap?url={CM_ANIMALS_COLOURS}"))
        .await;
    let id = found_id(&found);
    for base in ["r4", "r4b"] {
        let (status, body) = server.get(&format!("/{base}/ConceptMap/{id}")).await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body["id"], id, "{base}");
        // R4 and R4B carry `source[x]`, `equivalence`, and the `provided`
        // family of `unmapped.mode`
        // (<https://hl7.org/fhir/R4B/conceptmap.html>).
        assert_eq!(body["sourceUri"], ferroterm_testkit::fhir::VS_ALL, "{base}");
        let target = &body["group"][0]["element"][0]["target"][0];
        assert_eq!(target["equivalence"], "equivalent", "{base}: {body}");
        assert!(target["relationship"].is_null(), "{base}: {body}");
    }
    for base in ["r5", "r6"] {
        let (status, body) = server.get(&format!("/{base}/ConceptMap/{id}")).await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body["id"], id, "{base}");
        // R5 and R6 carry `sourceScope[x]` and `relationship`
        // (<https://hl7.org/fhir/R5/conceptmap.html>).
        assert_eq!(
            body["sourceScopeUri"],
            ferroterm_testkit::fhir::VS_ALL,
            "{base}"
        );
        let target = &body["group"][0]["element"][0]["target"][0];
        assert_eq!(target["relationship"], "equivalent", "{base}: {body}");
        assert!(target["equivalence"].is_null(), "{base}: {body}");
    }
}

#[tokio::test]
async fn an_element_that_maps_to_nothing_reads_in_each_family_s_spelling() {
    let server = Server::start_with_resources();
    let (_, found) = server
        .get(&format!("/r4b/ConceptMap?url={CM_ANIMALS_COLOURS}"))
        .await;
    let id = found_id(&found);
    let (status, body) = server.get(&format!("/r4b/ConceptMap/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let unmatched = element(&body, "fish");
    // R4 spells an element that maps to nothing as a target with
    // `equivalence = unmatched` and no code
    // (<https://hl7.org/fhir/R4B/conceptmap.html#unmapped>).
    assert_eq!(unmatched["target"][0]["equivalence"], "unmatched", "{body}");
    assert!(unmatched["target"][0]["code"].is_null(), "{body}");

    let (status, body) = server.get(&format!("/r5/ConceptMap/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let no_map = element(&body, "fish");
    // R5 spells it `noMap`, and `cmd-4` keeps `target` empty beside it
    // (<https://hl7.org/fhir/R5/conceptmap.html#invs>).
    assert_eq!(no_map["noMap"], true, "{body}");
    assert!(no_map["target"].is_null(), "{body}");
}

#[tokio::test]
async fn the_unmapped_mode_reads_in_each_family_s_vocabulary() {
    let server = Server::start_with_resources();
    let (_, found) = server
        .get(&format!("/r4b/ConceptMap?url={CM_FALLBACK}"))
        .await;
    let id = found_id(&found);
    // The R4 family names the other-map target `url`, the R5 family `otherMap`
    // (<https://hl7.org/fhir/R4B/conceptmap.html>,
    // <https://hl7.org/fhir/R5/conceptmap.html>).
    let (status, body) = server.get(&format!("/r4b/ConceptMap/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["group"][0]["unmapped"]["mode"], "other-map");
    assert_eq!(body["group"][0]["unmapped"]["url"], CM_ANIMALS_COLOURS);

    let (status, body) = server.get(&format!("/r5/ConceptMap/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["group"][0]["unmapped"]["mode"], "other-map");
    assert_eq!(body["group"][0]["unmapped"]["otherMap"], CM_ANIMALS_COLOURS);
}

#[tokio::test]
async fn an_id_the_server_serves_no_map_under_is_a_not_found() {
    let server = Server::start_with_resources();
    let (status, body) = server.get("/r4b/ConceptMap/no-such-map").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["resourceType"], "OperationOutcome");
    assert_eq!(body["issue"][0]["code"], "not-found");
}

#[tokio::test]
async fn a_search_parameter_this_server_does_not_answer_is_refused() {
    let server = Server::start_with_resources();
    let (status, body) = server.get("/r4b/ConceptMap?name=fallback").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["issue"][0]["code"], "not-supported", "{body}");
}

#[tokio::test]
async fn a_search_returns_the_persisted_and_the_loaded_concept_maps() {
    let server = Server::start_persisting();
    let map = serde_json::json!({
        "resourceType": "ConceptMap",
        "url": "http://ferroterm.test/ConceptMap/pets", "version": "1.0",
        "status": "active",
        "group": [{
            "source": ANIMALS, "target": COLOURS,
            "element": [{"code": "cat", "target": [
                {"code": "RED", "equivalence": "equivalent"}
            ]}]
        }]
    });
    let response = server.put("/r4b/ConceptMap/pets", &map).await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let (status, body) = server.get("/r4b/ConceptMap").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let urls: Vec<&str> = body["entry"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|entry| entry["resource"]["url"].as_str())
        .collect();
    assert!(
        urls.contains(&"http://ferroterm.test/ConceptMap/pets"),
        "the persisted map is in the searchset: {urls:?}"
    );
    assert!(
        urls.contains(&CM_ANIMALS_COLOURS),
        "the loaded map is there too: {urls:?}"
    );
}

/// The `group.element` with `code`, in the first group.
fn element<'a>(body: &'a Value, code: &str) -> &'a Value {
    body["group"]
        .as_array()
        .and_then(|groups| groups.first())
        .and_then(|group| group["element"].as_array())
        .expect("elements")
        .iter()
        .find(|element| element["code"] == code)
        .unwrap_or_else(|| panic!("{code} is mapped: {body}"))
}
