// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `POST [base]/$closure`
//! (<https://hl7.org/fhir/R4B/conceptmap-operation-closure.html>,
//! <https://hl7.org/fhir/R4B/terminology-service.html> "Maintaining a Closure
//! Table").

use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::Server;
use ferroterm_testkit::snomed::{ANIMAL, CAT, DOG, TOP, item, sctid};

/// The SNOMED CT code of the fixture concept at `ordinal`.
fn code(ordinal: u32) -> String {
    sctid(item(ordinal))
}

/// A `$closure` request naming `name`, registering `concepts`.
fn call(name: &str, concepts: &[String]) -> Value {
    let mut parameter = vec![json!({"name": "name", "valueString": name})];
    for code in concepts {
        parameter.push(json!({
            "name": "concept",
            "valueCoding": {"system": "http://snomed.info/sct", "code": code}
        }));
    }
    json!({"resourceType": "Parameters", "parameter": parameter})
}

/// A `$closure` resynchronisation from `version`.
fn resync(name: &str, version: &str) -> Value {
    json!({"resourceType": "Parameters", "parameter": [
        {"name": "name", "valueString": name},
        {"name": "version", "valueString": version}
    ]})
}

/// Every `source -> target` with its equivalence, sorted.
///
/// The equivalence is read from target to source, so `a subsumes b` means the
/// target `b` subsumes the source `a`.
fn edges(body: &Value) -> Vec<String> {
    let mut out: Vec<String> = body
        .get("group")
        .and_then(Value::as_array)
        .map(|groups| {
            groups
                .iter()
                .filter_map(|group| group.get("element")?.as_array())
                .flatten()
                .flat_map(|element| {
                    let source = element
                        .get("code")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    element
                        .get("target")
                        .and_then(Value::as_array)
                        .map(Vec::as_slice)
                        .unwrap_or_default()
                        .iter()
                        .map(|target| {
                            format!(
                                "{source} {} {}",
                                target
                                    .get("equivalence")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default(),
                                target
                                    .get("code")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                            )
                        })
                        .collect::<Vec<String>>()
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

#[tokio::test]
async fn naming_a_table_alone_initialises_it_at_version_zero() {
    let server = Server::start_persisting();
    let (status, body) = server.post("/r4b/$closure", &call("pets", &[])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["resourceType"], "ConceptMap");
    assert_eq!(body["id"], "pets");
    assert_eq!(body["name"], "Closure Table pets Creation");
    assert_eq!(body["status"], "active");
    assert_eq!(body["experimental"], true);
    assert_eq!(body["version"], "0");
    assert!(
        body["group"].is_null(),
        "an initialised table has no entries: {body}"
    );
}

#[tokio::test]
async fn a_closure_reports_each_new_subsumption_once_and_names_its_updates() {
    let server = Server::start_persisting();
    server.post("/r4b/$closure", &call("pets", &[])).await;

    let (status, body) = server
        .post("/r4b/$closure", &call("pets", &[code(CAT)]))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "Updates for Closure Table pets");
    assert_eq!(body["version"], "1");
    assert!(
        edges(&body).is_empty(),
        "one concept has nothing to relate to: {body}"
    );

    let (status, body) = server
        .post("/r4b/$closure", &call("pets", &[code(ANIMAL)]))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["version"], "2");
    assert_eq!(
        edges(&body),
        [format!("{} specializes {}", code(ANIMAL), code(CAT))],
        "read target to target-to-source: the cat, the target, specializes the animal"
    );

    let (status, body) = server
        .post("/r4b/$closure", &call("pets", &[code(CAT)]))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        edges(&body).is_empty(),
        "a concept already in the table brings nothing new: {body}"
    );
}

#[tokio::test]
async fn a_closure_never_relates_a_concept_to_itself_or_to_a_sibling() {
    let server = Server::start_persisting();
    server.post("/r4b/$closure", &call("zoo", &[])).await;
    let (status, body) = server
        .post(
            "/r4b/$closure",
            &call("zoo", &[code(TOP), code(ANIMAL), code(CAT), code(DOG)]),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let found = edges(&body);
    assert!(
        !found
            .iter()
            .any(|edge| edge.split(' ').next() == edge.split(' ').next_back()),
        "a code is never related to itself: {found:?}"
    );
    let cat_and_dog = format!("{} ", code(CAT));
    assert!(
        !found
            .iter()
            .any(|edge| edge.starts_with(&cat_and_dog) && edge.ends_with(&code(DOG))),
        "the cat and the dog are siblings and subsume neither way: {found:?}"
    );
    assert_eq!(
        found.len(),
        5,
        "the top over three, the animal over two, and no pair of siblings: {found:?}"
    );
}

#[tokio::test]
async fn a_resynchronisation_replays_the_entries_after_the_named_version() {
    let server = Server::start_persisting();
    server.post("/r4b/$closure", &call("pets", &[])).await;
    server
        .post("/r4b/$closure", &call("pets", &[code(CAT)]))
        .await;
    server
        .post("/r4b/$closure", &call("pets", &[code(ANIMAL)]))
        .await;

    let (status, body) = server.post("/r4b/$closure", &resync("pets", "1")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["version"], "2",
        "a replay answers the server's latest version"
    );
    assert_eq!(
        edges(&body),
        [format!("{} specializes {}", code(ANIMAL), code(CAT))],
        "everything sent after version 1, excluding version 1"
    );

    let (status, body) = server.post("/r4b/$closure", &resync("pets", "2")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        edges(&body).is_empty(),
        "nothing was sent after the latest version: {body}"
    );

    let (status, body) = server.post("/r4b/$closure", &resync("pets", "0")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        edges(&body).len(),
        1,
        "version 0 resyncs the whole table: {body}"
    );
}

#[tokio::test]
async fn a_closure_table_survives_a_restart() {
    let server = Server::start_persisting();
    server.post("/r4b/$closure", &call("pets", &[])).await;
    server
        .post("/r4b/$closure", &call("pets", &[code(CAT), code(ANIMAL)]))
        .await;

    let restarted = server.restarted();
    let (status, body) = restarted.post("/r4b/$closure", &resync("pets", "0")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        edges(&body),
        [format!("{} subsumes {}", code(CAT), code(ANIMAL))],
        "the table outlives the process; the entry is stated from the cat, which \
         was registered first in the same call"
    );
    let (status, body) = restarted
        .post("/r4b/$closure", &call("pets", &[code(TOP)]))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["version"], "2",
        "the version counts on from what was stored"
    );
}

#[tokio::test]
async fn a_table_that_was_never_initialised_is_not_found_and_is_not_created() {
    let server = Server::start_persisting();
    let (status, body) = server
        .post("/r4b/$closure", &call("nothing", &[code(CAT)]))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["issue"][0]["code"], "not-found");
    assert!(
        body["issue"][0]["diagnostics"]
            .as_str()
            .is_some_and(|text| text.contains("nothing")),
        "the refusal names the table: {body}"
    );

    let (status, body) = server.post("/r4b/$closure", &resync("nothing", "1")).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "adding did not create it: {body}"
    );
}

#[tokio::test]
async fn initialising_an_existing_table_empties_it() {
    let server = Server::start_persisting();
    server.post("/r4b/$closure", &call("pets", &[])).await;
    server
        .post("/r4b/$closure", &call("pets", &[code(CAT), code(ANIMAL)]))
        .await;

    let (status, body) = server.post("/r4b/$closure", &call("pets", &[])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["version"], "0", "the table starts again");
    let (status, body) = server.post("/r4b/$closure", &resync("pets", "0")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(edges(&body).is_empty(), "nothing is left: {body}");
}

#[tokio::test]
async fn a_missing_name_an_impossible_version_and_both_inputs_are_refused() {
    let server = Server::start_persisting();
    let (status, body) = server
        .post(
            "/r4b/$closure",
            &json!({"resourceType": "Parameters", "parameter": []}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    server.post("/r4b/$closure", &call("pets", &[])).await;
    let (status, body) = server.post("/r4b/$closure", &resync("pets", "9")).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a version later than the table's own: {body}"
    );
    let (status, body) = server.post("/r4b/$closure", &resync("pets", "x")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, body) = server
        .post(
            "/r4b/$closure",
            &json!({"resourceType": "Parameters", "parameter": [
                {"name": "name", "valueString": "pets"},
                {"name": "version", "valueString": "0"},
                {"name": "concept", "valueCoding": {
                    "system": "http://snomed.info/sct", "code": code(CAT)
                }}
            ]}),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a client passes a concept or a version, not both: {body}"
    );
}

#[tokio::test]
async fn a_concept_the_server_does_not_have_is_refused() {
    let server = Server::start_persisting();
    server.post("/r4b/$closure", &call("pets", &[])).await;
    let (status, body) = server
        .post("/r4b/$closure", &call("pets", &[String::from("404684003")]))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["resourceType"], "OperationOutcome");
}

#[tokio::test]
async fn a_deployment_that_persists_nothing_refuses_a_closure() {
    let server = Server::start_with_resources();
    let (status, body) = server.post("/r4b/$closure", &call("pets", &[])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["resourceType"], "OperationOutcome");
}

#[tokio::test]
async fn the_r6_ballot_offers_no_closure_and_says_so() {
    let server = Server::start_persisting();
    let (status, body) = server.post("/r6/$closure", &call("pets", &[])).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "the R6 ballot ships no ConceptMap-closure definition: {body}"
    );

    for (base, declared) in [("/r4", true), ("/r4b", true), ("/r5", true), ("/r6", false)] {
        let (status, body) = server.get(&format!("{base}/metadata")).await;
        assert_eq!(status, StatusCode::OK, "{base}");
        let names: Vec<&str> = body["rest"][0]["operation"]
            .as_array()
            .expect("the system operations")
            .iter()
            .filter_map(|held| held["name"].as_str())
            .collect();
        assert_eq!(
            names.contains(&"closure"),
            declared,
            "{base} declares closure: {names:?}"
        );
    }
}

#[tokio::test]
async fn r5_states_the_relationship_in_its_own_vocabulary() {
    let server = Server::start_persisting();
    server.post("/r5/$closure", &call("pets", &[])).await;
    server
        .post("/r5/$closure", &call("pets", &[code(CAT)]))
        .await;
    let (status, body) = server
        .post("/r5/$closure", &call("pets", &[code(ANIMAL)]))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let relationship = body["group"][0]["element"][0]["target"][0]["relationship"].as_str();
    assert_eq!(
        relationship,
        Some("source-is-broader-than-target"),
        "R5 has no `specializes`, and the animal is broader than the cat: {body}"
    );
}
