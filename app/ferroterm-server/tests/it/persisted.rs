// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The persisted `CodeSystem`, `ValueSet`, and `ConceptMap` resources on the
//! wire (<https://hl7.org/fhir/R4B/http.html>).

use axum::body::Body;
use http::header::{ETAG, IF_MATCH, LAST_MODIFIED, LOCATION};
use http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::fixture::{self, Server, header};

const COLOURS: &str = "http://ferroterm.test/CodeSystem/colours";
const COLOUR_SET: &str = "http://ferroterm.test/ValueSet/colours";

fn colours(version: &str) -> Value {
    json!({
        "resourceType": "CodeSystem",
        "url": COLOURS,
        "version": version,
        "status": "active",
        "content": "complete",
        "concept": [
            {"code": "red", "display": "Red"},
            {"code": "blue", "display": "Blue"}
        ]
    })
}

fn colour_set() -> Value {
    json!({
        "resourceType": "ValueSet",
        "url": COLOUR_SET,
        "version": "1.0",
        "status": "active",
        "compose": {"include": [{"system": COLOURS}]}
    })
}

#[tokio::test]
async fn a_put_creates_then_updates_with_the_fhir_status_codes_and_headers() {
    let server = Server::start_persisting();

    let response = server.put("/r4b/CodeSystem/colours", &colours("1.0")).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(header(&response, ETAG).as_deref(), Some("W/\"1\""));
    assert_eq!(
        header(&response, LOCATION).as_deref(),
        Some("/r4b/CodeSystem/colours/_history/1")
    );
    assert!(
        header(&response, LAST_MODIFIED).is_some_and(|value| value.ends_with("GMT")),
        "the created resource carries an HTTP-date Last-Modified"
    );
    let (_, body) = fixture::json(response).await;
    assert_eq!(body["id"], "colours");
    assert_eq!(body["meta"]["versionId"], "1");
    assert!(
        body["meta"]["lastUpdated"]
            .as_str()
            .is_some_and(|value| value.contains('T')),
        "the stored resource carries meta.lastUpdated: {body}"
    );

    let response = server.put("/r4b/CodeSystem/colours", &colours("2.0")).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a second put updates the resource"
    );
    assert_eq!(header(&response, ETAG).as_deref(), Some("W/\"2\""));
    assert!(
        header(&response, LOCATION).is_none(),
        "an update names no new location"
    );
}

#[tokio::test]
async fn a_persisted_code_system_answers_every_operation_and_survives_a_restart() {
    let server = Server::start_persisting();
    assert_eq!(
        server
            .put("/r4b/CodeSystem/colours", &colours("1.0"))
            .await
            .status(),
        StatusCode::CREATED
    );

    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/$lookup?system={COLOURS}&code=red"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let display = body["parameter"]
        .as_array()
        .expect("parameters")
        .iter()
        .find(|parameter| parameter["name"] == "display")
        .and_then(|parameter| parameter["valueString"].as_str());
    assert_eq!(display, Some("Red"), "the persisted system answers $lookup");

    let restarted = server.restarted();
    let (status, body) = restarted
        .get(&format!(
            "/r5/CodeSystem/$lookup?system={COLOURS}&code=blue"
        ))
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the resource written on R4B answers on R5 after a restart: {body}"
    );
    let (status, body) = restarted.get("/r4b/CodeSystem/colours").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["version"], "1.0");
}

#[tokio::test]
async fn a_persisted_value_set_expands_over_its_persisted_code_system() {
    let server = Server::start_persisting();
    server.put("/r4b/CodeSystem/colours", &colours("1.0")).await;
    server.put("/r4b/ValueSet/colour-set", &colour_set()).await;

    let (status, body) = server
        .get(&format!("/r4b/ValueSet/$expand?url={COLOUR_SET}"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let codes: Vec<&str> = body["expansion"]["contains"]
        .as_array()
        .expect("contains")
        .iter()
        .filter_map(|entry| entry["code"].as_str())
        .collect();
    assert_eq!(codes, ["blue", "red"]);
}

#[tokio::test]
async fn a_persisted_concept_map_answers_translate() {
    let server = Server::start_persisting();
    server.put("/r4b/CodeSystem/colours", &colours("1.0")).await;
    let map = json!({
        "resourceType": "ConceptMap",
        "url": "http://ferroterm.test/ConceptMap/colours-hues",
        "version": "1.0",
        "status": "active",
        "group": [{
            "source": COLOURS,
            "target": "http://ferroterm.test/CodeSystem/hues",
            "element": [{
                "code": "red",
                "target": [{"code": "crimson", "equivalence": "equivalent"}]
            }]
        }]
    });
    let response = server.put("/r4b/ConceptMap/colours-hues", &map).await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let (status, body) = server
        .get(&format!(
            "/r4b/ConceptMap/$translate?url=http://ferroterm.test/ConceptMap/colours-hues&system={COLOURS}&code=red"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let matched = body["parameter"]
        .as_array()
        .expect("parameters")
        .iter()
        .find(|parameter| parameter["name"] == "match")
        .expect("a match");
    let concept = matched["part"]
        .as_array()
        .expect("parts")
        .iter()
        .find(|part| part["name"] == "concept")
        .expect("the concept");
    assert_eq!(concept["valueCoding"]["code"], "crimson");
}

#[tokio::test]
async fn a_version_read_answers_an_earlier_version_and_a_delete_leaves_the_history() {
    let server = Server::start_persisting();
    server.put("/r4b/CodeSystem/colours", &colours("1.0")).await;
    server.put("/r4b/CodeSystem/colours", &colours("2.0")).await;

    let (status, body) = server.get("/r4b/CodeSystem/colours/_history/1").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["version"], "1.0");
    let (status, body) = server.get("/r4b/CodeSystem/colours/_history/9").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    let request = Request::delete("/r4b/CodeSystem/colours")
        .body(Body::empty())
        .expect("request");
    let response = server.send(request).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let request = Request::delete("/r4b/CodeSystem/colours")
        .body(Body::empty())
        .expect("request");
    assert_eq!(
        server.send(request).await.status(),
        StatusCode::NO_CONTENT,
        "deleting again has no effect and is not an error"
    );

    let (status, body) = server.get("/r4b/CodeSystem/colours").await;
    assert_eq!(
        status,
        StatusCode::GONE,
        "a deleted resource reads as gone: {body}"
    );
    let (status, body) = server.get("/r4b/CodeSystem/colours/_history/2").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the history outlives the delete: {body}"
    );
    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/$lookup?system={COLOURS}&code=red"
        ))
        .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "the deleted system stops answering: {body}"
    );
}

#[tokio::test]
async fn a_search_returns_the_persisted_and_the_loaded_value_sets() {
    let server = Server::start_persisting();
    server.put("/r4b/CodeSystem/colours", &colours("1.0")).await;
    server.put("/r4b/ValueSet/colour-set", &colour_set()).await;

    let (status, body) = server.get("/r4b/ValueSet").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let urls: Vec<&str> = body["entry"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|entry| entry["resource"]["url"].as_str())
        .collect();
    assert!(
        urls.contains(&COLOUR_SET),
        "the persisted value set is in the searchset: {urls:?}"
    );
    assert!(
        urls.len() > 1,
        "the loaded value sets are there too: {urls:?}"
    );

    let (status, body) = server.get(&format!("/r4b/ValueSet?url={COLOUR_SET}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 1, "a url narrows the searchset");

    let (status, body) = server.get("/r4b/CodeSystem?url=nothing.example").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 0);
}

#[tokio::test]
async fn an_if_match_that_names_another_version_is_refused() {
    let server = Server::start_persisting();
    server.put("/r4b/CodeSystem/colours", &colours("1.0")).await;

    let request = Request::put("/r4b/CodeSystem/colours")
        .header(http::header::CONTENT_TYPE, "application/fhir+json")
        .header(IF_MATCH, "W/\"7\"")
        .body(Body::from(colours("2.0").to_string()))
        .expect("request");
    let response = server.send(request).await;
    assert_eq!(response.status(), StatusCode::PRECONDITION_FAILED);

    let request = Request::put("/r4b/CodeSystem/colours")
        .header(http::header::CONTENT_TYPE, "application/fhir+json")
        .header(IF_MATCH, "W/\"1\"")
        .body(Body::from(colours("2.0").to_string()))
        .expect("request");
    assert_eq!(server.send(request).await.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_write_is_refused_where_the_deployment_persists_nothing() {
    let server = Server::start_with_resources();
    let response = server.put("/r4b/CodeSystem/colours", &colours("1.0")).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let (_, body) = fixture::json(response).await;
    assert_eq!(body["resourceType"], "OperationOutcome");
    assert!(
        body["issue"][0]["diagnostics"]
            .as_str()
            .is_some_and(|text| text.contains("FERROTERM_RESOURCES")),
        "the refusal names the variable that turns persistence on: {body}"
    );
}

#[tokio::test]
async fn a_body_of_another_type_or_an_id_that_is_not_the_url_id_is_refused() {
    let server = Server::start_persisting();
    let response = server.put("/r4b/CodeSystem/colours", &colour_set()).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let mut wrong = colours("1.0");
    wrong["id"] = json!("other");
    let response = server.put("/r4b/CodeSystem/colours", &wrong).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let (status, body) = server.get("/r4b/CodeSystem/never-written").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn the_capability_statement_declares_the_write_interactions_only_when_they_answer() {
    let server = Server::start_persisting();
    let (status, body) = server.get("/r4b/metadata").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let codes = interactions(&body, "ValueSet");
    for wanted in ["read", "vread", "search-type", "create", "update", "delete"] {
        assert!(codes.contains(&wanted.to_owned()), "{wanted} in {codes:?}");
    }

    let plain = Server::start_with_resources();
    let (status, body) = plain.get("/r4b/metadata").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let codes = interactions(&body, "ValueSet");
    assert_eq!(
        codes,
        vec![String::from("read"), String::from("search-type")],
        "a deployment that persists nothing declares no write"
    );
}

const LIFECYCLE: &str = "http://ferroterm.test/CodeSystem/lifecycle";
const LIFECYCLE_SET: &str = "http://ferroterm.test/ValueSet/lifecycle";

/// A system carrying one status marker per concept, with the R5 property set
/// (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>). A date the
/// request is behind retires a concept and one still to come does not.
fn lifecycle() -> Value {
    let property = |code: &str, kind: &str| {
        json!({
            "code": code,
            "uri": format!("http://hl7.org/fhir/concept-properties#{code}"),
            "type": kind
        })
    };
    json!({
        "resourceType": "CodeSystem",
        "url": LIFECYCLE,
        "version": "1.0",
        "status": "active",
        "content": "complete",
        "caseSensitive": true,
        "property": [
            property("status", "code"),
            property("inactive", "boolean"),
            property("deprecated", "dateTime"),
            property("deprecationDate", "dateTime"),
            property("retirementDate", "dateTime")
        ],
        "concept": [
            {"code": "current", "display": "Current"},
            {"code": "deprecated", "display": "Deprecated",
             "property": [{"code": "status", "valueCode": "deprecated"},
                          {"code": "deprecated", "valueDateTime": "2001-06-15"},
                          {"code": "deprecationDate", "valueDateTime": "2001-06-15"}]},
            {"code": "flagged", "display": "Flagged",
             "property": [{"code": "inactive", "valueBoolean": true}]},
            {"code": "retired", "display": "Retired",
             "property": [{"code": "status", "valueCode": "retired"}]},
            {"code": "retired-date", "display": "Retired by date",
             "property": [{"code": "retirementDate", "valueDateTime": "2001-06-15"}]},
            {"code": "retiring", "display": "Retiring later",
             "property": [{"code": "retirementDate", "valueDateTime": "2999-01-01"}]},
            {"code": "contradiction", "display": "Flagged active and retired",
             "property": [{"code": "inactive", "valueBoolean": false},
                          {"code": "status", "valueCode": "retired"}]}
        ]
    })
}

fn lifecycle_set() -> Value {
    json!({
        "resourceType": "ValueSet",
        "url": LIFECYCLE_SET,
        "version": "1.0",
        "status": "active",
        "compose": {"include": [{"system": LIFECYCLE}]}
    })
}

/// The concepts of an expansion body, in the order the page carries them.
fn expansion_entries(body: &Value) -> &[Value] {
    body.get("expansion")
        .and_then(|expansion| expansion.get("contains"))
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The codes of `entries`.
fn expansion_codes(entries: &[Value]) -> Vec<&str> {
    entries
        .iter()
        .filter_map(|entry| entry.get("code").and_then(Value::as_str))
        .collect()
}

// `activeOnly` "controls whether inactive concepts are included or excluded in
// value set expansions" (<https://hl7.org/fhir/R4/valueset-operation-expand.html>,
// <https://hl7.org/fhir/R5/valueset-operation-expand.html>); the markers that
// make a concept inactive are the standard concept properties, which a
// persisted resource carries as written on every version.
#[tokio::test]
async fn the_concept_status_properties_answer_the_same_on_every_served_version() {
    let server = Server::start_persisting();
    assert_eq!(
        server
            .put("/r4/CodeSystem/lifecycle", &lifecycle())
            .await
            .status(),
        StatusCode::CREATED,
        "the R5 concept properties are data, so an R4 write takes them"
    );
    server.put("/r4/ValueSet/lifecycle", &lifecycle_set()).await;

    for version in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$expand?url={LIFECYCLE_SET}&activeOnly=true"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            expansion_codes(expansion_entries(&body)),
            ["current", "deprecated", "retiring"],
            "{version} drops every concept a marker retires"
        );

        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$expand?url={LIFECYCLE_SET}&activeOnly=false"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        let inactive: Vec<&str> = expansion_entries(&body)
            .iter()
            .filter(|entry| entry.get("inactive") == Some(&json!(true)))
            .filter_map(|entry| entry.get("code").and_then(Value::as_str))
            .collect();
        assert_eq!(
            inactive,
            ["contradiction", "flagged", "retired", "retired-date"],
            "{version} keeps them and flags them"
        );
    }
}

// "Inactive is not invalid": the code validates, and the warning and the
// `inactive` output say what it is
// (<https://hl7.org/fhir/R5/valueset-operation-validate-code.html>).
#[tokio::test]
async fn validate_code_calls_a_retired_concept_valid_and_inactive_on_every_version() {
    let server = Server::start_persisting();
    server.put("/r4/CodeSystem/lifecycle", &lifecycle()).await;

    for version in ["r4", "r4b", "r5", "r6"] {
        for code in ["flagged", "retired", "retired-date", "contradiction"] {
            let (status, body) = server
                .get(&format!(
                    "/{version}/CodeSystem/$validate-code?url={LIFECYCLE}&code={code}"
                ))
                .await;
            assert_eq!(status, StatusCode::OK, "{version} {code}: {body}");
            assert_eq!(
                fixture::parameter(&body, "result").expect("result")["valueBoolean"],
                json!(true),
                "{version} {code}: inactive is not invalid"
            );
            assert_eq!(
                fixture::parameter(&body, "inactive").expect("inactive")["valueBoolean"],
                json!(true),
                "{version} {code}: {body}"
            );
        }

        let (status, body) = server
            .get(&format!(
                "/{version}/CodeSystem/$validate-code?url={LIFECYCLE}&code=deprecated"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert!(
            fixture::parameter(&body, "inactive").is_none(),
            "{version}: a deprecated concept is still active: {body}"
        );
    }
}

// `$lookup` answers the properties the system states for the concept
// (<https://hl7.org/fhir/R4/codesystem-operation-lookup.html>), so the
// lifecycle properties come back as written.
#[tokio::test]
async fn lookup_answers_the_declared_status_properties_as_written() {
    let server = Server::start_persisting();
    server.put("/r4/CodeSystem/lifecycle", &lifecycle()).await;

    for version in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server
            .get(&format!(
                "/{version}/CodeSystem/$lookup?system={LIFECYCLE}&code=retired-date&property=*"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        let properties: Vec<(&str, &Value)> = body["parameter"]
            .as_array()
            .expect("parameters")
            .iter()
            .filter(|parameter| parameter["name"] == "property")
            .filter_map(|parameter| {
                let parts = parameter["part"].as_array()?;
                let code = parts.first()?["valueCode"].as_str()?;
                Some((code, &parts.get(1)?["valueDateTime"]))
            })
            .collect();
        assert_eq!(
            properties
                .iter()
                .map(|(code, _)| *code)
                .collect::<Vec<&str>>(),
            ["inactive", "retirementDate"],
            "{version}: {body}"
        );
        assert_eq!(
            properties[1].1,
            &json!("2001-06-15"),
            "{version}: the date is the one the resource carried"
        );
    }
}

/// The interaction codes the capability statement declares on `resource_type`.
fn interactions(body: &Value, resource_type: &str) -> Vec<String> {
    body.get("rest")
        .and_then(|rest| rest.get(0))
        .and_then(|rest| rest.get("resource"))
        .and_then(Value::as_array)
        .expect("resources")
        .iter()
        .find(|resource| resource["type"] == resource_type)
        .and_then(|resource| resource["interaction"].as_array())
        .expect("interactions")
        .iter()
        .filter_map(|interaction| interaction["code"].as_str())
        .map(str::to_owned)
        .collect()
}

/// A resource this server holds and the reading version has no representation
/// for: R5 declares `copyrightLabel` on every canonical resource and R4B does
/// not (<https://hl7.org/fhir/R5/codesystem.html>).
const R5_ONLY: (&str, &str) = ("copyrightLabel", "CC0-1.0");

/// The searchset entries whose `search.mode` is `mode`.
fn entries_of<'a>(body: &'a Value, mode: &str) -> Vec<&'a Value> {
    body.get("entry")
        .and_then(Value::as_array)
        .expect("entries")
        .iter()
        .filter(|entry| {
            entry
                .get("search")
                .and_then(|search| search.get("mode"))
                .and_then(Value::as_str)
                == Some(mode)
        })
        .collect()
}

/// The `url`s a searchset matched.
fn matched_urls(body: &Value) -> Vec<&str> {
    entries_of(body, "match")
        .into_iter()
        .filter_map(|entry| entry.get("resource")?.get("url")?.as_str())
        .collect()
}

/// The three resource types, each written with an element only R5 defines.
fn r5_only_resources() -> [(&'static str, &'static str, Value); 3] {
    let (label, value) = R5_ONLY;
    [
        (
            "CodeSystem",
            "cross-cs",
            json!({
                "resourceType": "CodeSystem", "url": "http://ferroterm.test/CodeSystem/cross",
                "version": "1.0", "status": "active", "content": "complete",
                label: value,
                "concept": [{"code": "a", "display": "A"}]
            }),
        ),
        (
            "ValueSet",
            "cross-vs",
            json!({
                "resourceType": "ValueSet", "url": "http://ferroterm.test/ValueSet/cross",
                "version": "1.0", "status": "active",
                label: value,
                "compose": {"include": [{"system": "http://ferroterm.test/CodeSystem/cross"}]}
            }),
        ),
        (
            "ConceptMap",
            "cross-cm",
            json!({
                "resourceType": "ConceptMap", "url": "http://ferroterm.test/ConceptMap/cross",
                "version": "1.0", "status": "active",
                "group": [{
                    "source": "http://ferroterm.test/CodeSystem/cross",
                    "target": COLOURS,
                    // R5 states a target's direction as `relationship`; R4B
                    // spells it `equivalence`, and the two code sets are
                    // disjoint (<https://hl7.org/fhir/R5/conceptmap.html>).
                    "element": [{"code": "a", "target": [{"code": "red", "relationship": "equivalent"}]}]
                }]
            }),
        ),
    ]
}

#[tokio::test]
async fn a_resource_written_on_one_version_leaves_another_version_s_search_answering() {
    let server = Server::start_persisting();
    for (resource_type, id, body) in r5_only_resources() {
        let response = server
            .put(&format!("/r5/{resource_type}/{id}"), &body)
            .await;
        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "{resource_type} is written as FHIR 5.0.0"
        );
    }

    for (resource_type, id, body) in r5_only_resources() {
        let url = body["url"].as_str().expect("a url");

        let (status, found) = server.get(&format!("/r5/{resource_type}")).await;
        assert_eq!(status, StatusCode::OK, "{resource_type}: {found}");
        assert!(
            matched_urls(&found).contains(&url),
            "{resource_type}: the version it was written as serves it: {found}"
        );
        assert!(
            entries_of(&found, "outcome").is_empty(),
            "{resource_type}: nothing was left out: {found}"
        );

        // A searchset may carry an `OperationOutcome` about the search beside
        // its matches, with no `fatal` or `error` issue and a `search.mode` of
        // `outcome` (<https://hl7.org/fhir/R4B/http.html#search>).
        let (status, found) = server.get(&format!("/r4b/{resource_type}")).await;
        assert_eq!(status, StatusCode::OK, "{resource_type}: {found}");
        assert!(
            !matched_urls(&found).contains(&url),
            "{resource_type}: the resource is not among the matches: {found}"
        );
        let outcome = entries_of(&found, "outcome");
        assert_eq!(
            outcome.len(),
            1,
            "{resource_type}: one outcome entry: {found}"
        );
        let issue = &outcome[0]["resource"]["issue"][0];
        assert_eq!(outcome[0]["resource"]["resourceType"], "OperationOutcome");
        assert_eq!(issue["severity"], "warning", "{resource_type}: {found}");
        assert_eq!(issue["code"], "not-supported", "{resource_type}: {found}");
        let text = issue["details"]["text"].as_str().expect("details.text");
        assert!(
            text.contains(id) && text.contains("5.0.0"),
            "{resource_type}: the text names the resource and the version it was written as: {text}"
        );
        let matched = u64::try_from(entries_of(&found, "match").len()).expect("fits");
        assert_eq!(
            found["total"].as_u64(),
            Some(matched),
            "{resource_type}: total counts the matches alone: {found}"
        );

        // `_elements` names elements of the resources a search matched, so the
        // outcome entry keeps its mandatory `issue`
        // (<https://hl7.org/fhir/R5/search.html#elements>).
        let (status, projected) = server
            .get(&format!("/r4b/{resource_type}?_elements=url"))
            .await;
        assert_eq!(status, StatusCode::OK, "{resource_type}: {projected}");
        let outcome = entries_of(&projected, "outcome");
        assert_eq!(outcome.len(), 1, "{resource_type}: {projected}");
        assert_eq!(
            outcome[0]["resource"]["issue"][0]["code"], "not-supported",
            "{resource_type}: {projected}"
        );

        // Serving one release per base is the strategy where "the same record
        // has a different identity depending on the version of FHIR in use"
        // (<https://hl7.org/fhir/R5/versioning.html>), so the read of an
        // identity this base does not hold is a `404`
        // (<https://hl7.org/fhir/R4B/http.html#read>).
        let (status, refusal) = server.get(&format!("/r4b/{resource_type}/{id}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{resource_type}: {refusal}");
        assert_eq!(refusal["resourceType"], "OperationOutcome");
        assert_eq!(refusal["issue"][0]["code"], "not-supported");
        assert_eq!(
            server.get(&format!("/r5/{resource_type}/{id}")).await.0,
            StatusCode::OK,
            "{resource_type}: the version it was written as still reads it"
        );
    }
}

#[tokio::test]
async fn a_concept_map_written_as_r4b_leaves_the_r5_search_answering() {
    let server = Server::start_persisting();
    let map = json!({
        "resourceType": "ConceptMap",
        "url": "http://ferroterm.test/ConceptMap/reverse",
        "version": "1.0", "status": "active",
        "group": [{
            "source": COLOURS, "target": COLOURS,
            "element": [{"code": "red", "target": [{"code": "blue", "equivalence": "wider"}]}]
        }]
    });
    let response = server.put("/r4b/ConceptMap/reverse", &map).await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let (status, body) = server.get("/r5/ConceptMap").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        !matched_urls(&body).contains(&"http://ferroterm.test/ConceptMap/reverse"),
        "{body}"
    );
    let outcome = entries_of(&body, "outcome");
    assert_eq!(outcome.len(), 1, "{body}");
    let text = outcome[0]["resource"]["issue"][0]["details"]["text"]
        .as_str()
        .expect("details.text");
    assert!(text.contains("reverse") && text.contains("4.3.0"), "{text}");

    let (status, body) = server.get("/r4b/ConceptMap").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        matched_urls(&body).contains(&"http://ferroterm.test/ConceptMap/reverse"),
        "the version it was written as is unaffected: {body}"
    );
    assert!(entries_of(&body, "outcome").is_empty(), "{body}");
}

#[tokio::test]
async fn the_instance_lists_are_sorted_by_id_across_loaded_and_persisted() {
    let server = Server::start_persisting();
    for (resource_type, body) in [
        (
            "ValueSet",
            json!({"resourceType": "ValueSet", "url": COLOUR_SET, "status": "active"}),
        ),
        (
            "ConceptMap",
            json!({
                "resourceType": "ConceptMap",
                "url": "http://ferroterm.test/ConceptMap/sorted",
                "status": "active"
            }),
        ),
    ] {
        // An id that sorts before every loaded one and one that sorts after.
        for id in ["000-first", "zzz-last"] {
            let mut written = body.clone();
            written["version"] = json!(id);
            let response = server
                .put(&format!("/r4b/{resource_type}/{id}"), &written)
                .await;
            assert_eq!(
                response.status(),
                StatusCode::CREATED,
                "{resource_type}/{id}"
            );
        }
    }
    let state = server.state();
    let value_sets: Vec<String> = state
        .value_set_instances()
        .into_iter()
        .map(|(id, _, _)| id)
        .collect();
    let concept_maps: Vec<String> = state
        .concept_map_instances()
        .into_iter()
        .map(|(id, _, _)| id)
        .collect();
    assert!(
        value_sets.len() > 2,
        "the loaded value sets are in the list too: {value_sets:?}"
    );
    for (resource_type, ids) in [("ValueSet", value_sets), ("ConceptMap", concept_maps)] {
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "{resource_type}: the list is sorted by id");
        assert_eq!(
            ids.first().map(String::as_str),
            Some("000-first"),
            "{resource_type}"
        );
        assert_eq!(
            ids.last().map(String::as_str),
            Some("zzz-last"),
            "{resource_type}"
        );
    }
}
