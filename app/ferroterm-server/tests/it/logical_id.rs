// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The logical id a loaded `CodeSystem`, `ValueSet`, or `ConceptMap` answers
//! on.
//!
//! The read interaction addresses a resource by its logical id
//! (<https://hl7.org/fhir/R4B/http.html#read>), and `Resource.id` is that id on
//! the server that holds the resource
//! (<https://hl7.org/fhir/R4B/resource.html#id>). A resource loaded from disk
//! keeps the id it was authored with, so a client that knows the id reads it
//! there; a resource that carries none keeps the id the server mints.

use ferroterm_server::config::Config;
use ferroterm_server::state::{AppState, LoadError, instance_id};
use http::StatusCode;
use serde_json::Value;

use crate::fixture::Server;

/// The `id` of the loaded `CodeSystem`.
const CODE_SYSTEM: &str = "e2e-taxonomy";
/// The `id` of the loaded `ValueSet`.
const VALUE_SET: &str = "e2e-taxonomy-all";
/// The `id` of the loaded `ConceptMap`.
const CONCEPT_MAP: &str = "e2e-taxonomy-map";
/// The canonical of the loaded `CodeSystem`.
const SYSTEM_URL: &str = "https://ferroterm.eu/fhir/CodeSystem/e2e-taxonomy";
/// The canonical of the loaded `ValueSet`.
const VALUE_SET_URL: &str = "https://ferroterm.eu/fhir/ValueSet/e2e-taxonomy-all";
/// The canonical of the loaded `ConceptMap`.
const CONCEPT_MAP_URL: &str = "https://ferroterm.eu/fhir/ConceptMap/e2e-taxonomy-map";

#[tokio::test]
async fn a_loaded_code_system_answers_at_the_id_it_was_authored_with() {
    let server = Server::start_with_authored_ids();
    let (status, body) = server.get(&format!("/r4b/CodeSystem/{CODE_SYSTEM}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], CODE_SYSTEM);
    assert_eq!(body["url"], SYSTEM_URL);

    let (status, body) = server
        .get(&format!("/r4b/CodeSystem?url={SYSTEM_URL}"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 1);
    assert!(
        full_url(&body).ends_with(&format!("CodeSystem/{CODE_SYSTEM}")),
        "a search names the resource by the id a read answers on: {body}"
    );
    assert_eq!(body["entry"][0]["resource"]["id"], CODE_SYSTEM);

    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/{CODE_SYSTEM}/$validate-code?code=aa-root"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);

    // R5 declares `$lookup` at the instance level, R4B does not
    // (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>).
    let (status, body) = server
        .get(&format!(
            "/r5/CodeSystem/{CODE_SYSTEM}/$lookup?code=aa-root"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        parameter(&body, "display")["valueString"],
        "Root of the shaped taxonomy"
    );
}

#[tokio::test]
async fn a_loaded_value_set_answers_at_the_id_it_was_authored_with() {
    let server = Server::start_with_authored_ids();
    let (status, body) = server.get(&format!("/r4b/ValueSet/{VALUE_SET}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], VALUE_SET);
    assert_eq!(body["url"], VALUE_SET_URL);

    let (status, body) = server
        .get(&format!("/r4b/ValueSet?url={VALUE_SET_URL}"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 1);
    assert!(
        full_url(&body).ends_with(&format!("ValueSet/{VALUE_SET}")),
        "a search names the resource by the id a read answers on: {body}"
    );
    assert_eq!(body["entry"][0]["resource"]["id"], VALUE_SET);

    for base in ["r4", "r5", "r6"] {
        let (status, body) = server.get(&format!("/{base}/ValueSet/{VALUE_SET}")).await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body["id"], VALUE_SET, "{base}");
    }

    // The resource answers on its own id alone: a read of an id this server
    // does not hold is a 404 (<https://hl7.org/fhir/R4B/http.html#read>).
    let minted = instance_id(VALUE_SET_URL, "1");
    let (status, body) = server.get(&format!("/r4b/ValueSet/{minted}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn a_loaded_concept_map_answers_at_the_id_it_was_authored_with() {
    let server = Server::start_with_authored_ids();
    let (status, body) = server.get(&format!("/r4b/ConceptMap/{CONCEPT_MAP}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], CONCEPT_MAP);
    assert_eq!(body["url"], CONCEPT_MAP_URL);

    let (status, body) = server
        .get(&format!("/r4b/ConceptMap?url={CONCEPT_MAP_URL}"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 1);
    assert!(
        full_url(&body).ends_with(&format!("ConceptMap/{CONCEPT_MAP}")),
        "a search names the resource by the id a read answers on: {body}"
    );
    assert_eq!(body["entry"][0]["resource"]["id"], CONCEPT_MAP);

    for base in ["r4", "r5", "r6"] {
        let (status, body) = server
            .get(&format!("/{base}/ConceptMap/{CONCEPT_MAP}"))
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body["id"], CONCEPT_MAP, "{base}");
    }
}

#[test]
fn a_resource_without_an_id_keeps_the_one_the_server_mints() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_value_set(dir.path(), "anonymous.json", None, "https://a.example/vs");
    let state = AppState::load(&Config {
        code_systems: vec![dir.path().to_path_buf()],
        ..Config::default()
    })
    .expect("loads");
    let ids: Vec<String> = state
        .value_set_instances()
        .into_iter()
        .map(|(id, _, _)| id)
        .collect();
    assert_eq!(ids, [instance_id("https://a.example/vs", "1")]);
}

#[test]
fn two_loaded_resources_with_one_id_refuse_the_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_value_set(
        dir.path(),
        "first.json",
        Some("shared"),
        "https://a.example/first",
    );
    write_value_set(
        dir.path(),
        "second.json",
        Some("shared"),
        "https://a.example/second",
    );
    let loaded = AppState::load(&Config {
        code_systems: vec![dir.path().to_path_buf()],
        ..Config::default()
    });
    let Err(error) = loaded else {
        panic!("two resources carrying one id refuse the load");
    };
    assert!(
        matches!(&error, LoadError::DuplicateId { id, .. } if id == "shared"),
        "{error}"
    );
    let text = error.to_string();
    assert!(text.contains("https://a.example/first|1"), "{text}");
    assert!(text.contains("https://a.example/second|1"), "{text}");
}

#[test]
fn an_id_outside_the_fhir_id_alphabet_refuses_the_load() {
    // An `id` is 1 to 64 characters of `A-Z`, `a-z`, `0-9`, `-`, and `.`
    // (<https://hl7.org/fhir/R4B/datatypes.html#id>).
    for refused in ["not an id", "under_score", "slash/ed", &"a".repeat(65)] {
        let dir = tempfile::tempdir().expect("tempdir");
        write_value_set(
            dir.path(),
            "outside.json",
            Some(refused),
            "https://a.example/vs",
        );
        assert!(
            AppState::load(&Config {
                code_systems: vec![dir.path().to_path_buf()],
                ..Config::default()
            })
            .is_err(),
            "`{refused}` is no FHIR id, so the load is refused"
        );
    }
}

#[tokio::test]
async fn the_longest_id_the_alphabet_admits_is_served() {
    let dir = tempfile::tempdir().expect("tempdir");
    let resources = dir.path().to_path_buf();
    let longest = "a".repeat(64);
    write_value_set(
        &resources,
        "longest.json",
        Some(&longest),
        "https://a.example/vs",
    );
    let server = Server::start_with_config(dir, loading(&resources));
    let (status, body) = server.get(&format!("/r4b/ValueSet/{longest}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["id"], longest);
}

#[tokio::test]
async fn one_id_names_one_resource_of_each_type() {
    let dir = tempfile::tempdir().expect("tempdir");
    let resources = dir.path().to_path_buf();
    write_value_set(
        &resources,
        "shared-vs.json",
        Some("shared"),
        "https://a.example/vs",
    );
    std::fs::write(
        resources.join("shared-cs.json"),
        serde_json::json!({
            "resourceType": "CodeSystem",
            "id": "shared",
            "url": "https://a.example/cs",
            "version": "1",
            "status": "active",
            "content": "complete",
            "concept": [{"code": "a", "display": "A"}],
        })
        .to_string(),
    )
    .expect("writes");
    // A logical id is unique within a resource type, so the two answer on their
    // own endpoints (<https://hl7.org/fhir/R4B/resource.html#id>).
    let server = Server::start_with_config(dir, loading(&resources));
    let (status, body) = server.get("/r4b/ValueSet/shared").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["url"], "https://a.example/vs");
    let (status, body) = server.get("/r4b/CodeSystem/shared").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["url"], "https://a.example/cs");
}

#[tokio::test]
async fn an_authored_id_keeps_the_address_a_minted_id_would_have_taken() {
    let dir = tempfile::tempdir().expect("tempdir");
    let edition = dir.path().join("snomed");
    let resources = dir.path().join("fhir");
    for path in [&edition, &resources] {
        std::fs::create_dir_all(path).expect("creates");
    }
    ferroterm_testkit::snomed::write(&edition).expect("writes the edition");
    // The id the server mints for the loaded edition, authored on another
    // resource: the resource that carries the id is the one read at it
    // (<https://hl7.org/fhir/R4B/resource.html#id>).
    let taken = instance_id("http://snomed.info/sct", ferroterm_testkit::snomed::VERSION);
    std::fs::write(
        resources.join("claimant.json"),
        serde_json::json!({
            "resourceType": "CodeSystem",
            "id": taken,
            "url": "https://a.example/claimant",
            "version": "1",
            "status": "active",
            "content": "complete",
            "concept": [{"code": "a", "display": "A"}],
        })
        .to_string(),
    )
    .expect("writes");
    let server = Server::start_with_config(
        dir,
        Config {
            index: vec![edition],
            code_systems: vec![resources],
            ..Config::default()
        },
    );
    let (status, body) = server.get(&format!("/r4b/CodeSystem/{taken}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["url"], "https://a.example/claimant");
    let snomed: Vec<String> = server
        .state()
        .instances()
        .filter(|(_, url, _)| *url == "http://snomed.info/sct")
        .map(|(id, _, _)| id.to_owned())
        .collect();
    assert_eq!(
        snomed,
        [format!("{}-2", taken.chars().take(60).collect::<String>())],
        "the edition keeps a minted id, and the minted one yields"
    );
}

#[tokio::test]
async fn a_write_onto_a_loaded_id_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let resources = dir.path().to_path_buf();
    write_value_set(
        &resources,
        "authored.json",
        Some("held"),
        "https://a.example/vs",
    );
    let server = Server::start_with_config(
        dir,
        Config {
            code_systems: vec![resources.clone()],
            resources: Some(resources.join("resources.redb")),
            ..Config::default()
        },
    );
    let written = serde_json::json!({
        "resourceType": "ValueSet",
        "id": "held",
        "url": "https://b.example/vs",
        "version": "1",
        "status": "active",
    });
    let response = server.put("/r4b/ValueSet/held", &written).await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "the id is the one a loaded resource is read at"
    );
    let (status, body) = server.get("/r4b/ValueSet/held").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["url"], "https://a.example/vs",
        "the loaded resource still answers"
    );
}

#[test]
fn a_loaded_id_a_persisted_record_holds_refuses_the_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_value_set(
        dir.path(),
        "authored.json",
        Some("held"),
        "https://a.example/vs",
    );
    let config = Config {
        code_systems: vec![dir.path().to_path_buf()],
        resources: Some(dir.path().join("resources.redb")),
        ..Config::default()
    };
    let state = AppState::load(&config).expect("loads");
    let written = serde_json::json!({
        "resourceType": "ValueSet",
        "id": "held",
        "url": "https://b.example/vs",
        "version": "1",
        "status": "active",
    });
    let object = match crate::fixture::document(&written) {
        fhir_types::codec::Value::Object(object) => object,
        other => panic!("the body is a resource: {other:?}"),
    };
    state
        .put_persisted(
            ferroterm_server::persistence::ResourceType::ValueSet,
            "held",
            "4.3.0",
            ferroterm_server::persistence::Method::Put,
            object,
        )
        .expect("writes");
    // `redb` holds the database file for as long as its handle lives
    // (<https://docs.rs/redb/latest/redb/struct.Database.html>).
    drop(state);

    let loaded = AppState::load(&config);
    let Err(error) = loaded else {
        panic!("a loaded id a record holds refuses the load");
    };
    assert!(
        matches!(&error, LoadError::PersistedId { id, .. } if id == "held"),
        "{error}"
    );
    assert!(
        error.to_string().contains("https://a.example/vs|1"),
        "{error}"
    );
}

/// A configuration that loads the resources in `dir` and nothing else.
fn loading(dir: &std::path::Path) -> Config {
    Config {
        code_systems: vec![dir.to_path_buf()],
        ..Config::default()
    }
}

/// The `fullUrl` of the first entry of a `searchset`.
fn full_url(body: &Value) -> String {
    body.get("entry")
        .and_then(|entries| entries.get(0))
        .and_then(|entry| entry.get("fullUrl"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The `Parameters.parameter` of `name`, or `Value::Null`.
fn parameter(body: &Value, name: &str) -> Value {
    body.get("parameter")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|part| part.get("name").and_then(Value::as_str) == Some(name))
        .cloned()
        .unwrap_or(Value::Null)
}

/// Writes a minimal `ValueSet` resource, with `id` when one is given.
fn write_value_set(dir: &std::path::Path, name: &str, id: Option<&str>, url: &str) {
    let mut resource = serde_json::json!({
        "resourceType": "ValueSet",
        "url": url,
        "version": "1",
        "status": "active",
    });
    if let Some(id) = id
        && let Value::Object(object) = &mut resource
    {
        object.insert(String::from("id"), Value::String(id.to_owned()));
    }
    std::fs::write(
        dir.join(name),
        serde_json::to_string_pretty(&resource).expect("writes"),
    )
    .expect("writes");
}
