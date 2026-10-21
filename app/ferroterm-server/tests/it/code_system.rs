// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `CodeSystem` instances the deployment loaded, read and searched.
//!
//! The FHIR REST API defines the instance read and the type-level search
//! over every resource a server holds
//! (<https://hl7.org/fhir/R4B/http.html#read>,
//! <https://hl7.org/fhir/R4B/http.html#search>), and `CodeSystem` search
//! defines `url` and `version`
//! (<https://hl7.org/fhir/R4B/codesystem.html#search>).

use http::StatusCode;
use serde_json::Value;

use crate::fixture::{Server, base_of};

/// The systems the loaders and the registries put behind an index, none of
/// which carries its concepts in the resource.
const SERVED_FROM_AN_INDEX: [&str; 4] = [
    "http://snomed.info/sct",
    "http://loinc.org",
    "http://www.nlm.nih.gov/research/umls/rxnorm",
    "http://unitsofmeasure.org",
];

#[tokio::test]
async fn every_loaded_code_system_reads_at_the_id_the_server_names_it_by() {
    let server = Server::start_with_every_loader();
    for url in SERVED_FROM_AN_INDEX {
        let id = server.instance_id_of(url);
        let (status, body) = server.get(&format!("/r4b/CodeSystem/{id}")).await;
        assert_eq!(status, StatusCode::OK, "{url}: {body}");
        assert_eq!(body["resourceType"], "CodeSystem", "{url}");
        assert_eq!(body["id"], id.as_str(), "{url}");
        assert_eq!(body["url"], url, "{url}");
        assert_eq!(body["status"], "active", "{url}");
        // [F-RES-1]: the server holds the content, so none of it is in the
        // resource (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>).
        assert_eq!(body["content"], "not-present", "{url}");
        assert!(
            body["concept"].is_null(),
            "{url} carries no concepts: {body}"
        );
    }
}

#[tokio::test]
async fn a_loaded_code_system_reads_on_every_served_version() {
    let server = Server::start_with_every_loader();
    let id = server.instance_id_of("http://snomed.info/sct");
    for base in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server.get(&format!("/{base}/CodeSystem/{id}")).await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body["resourceType"], "CodeSystem", "{base}");
        assert_eq!(body["content"], "not-present", "{base}");
        assert_eq!(
            body["version"],
            ferroterm_testkit::snomed::VERSION,
            "{base}"
        );
    }
}

#[tokio::test]
async fn a_code_system_loaded_as_a_resource_reads_with_the_concepts_it_declares() {
    let server = Server::start_with_every_loader();
    let id = server.instance_id_of(ferroterm_testkit::fhir::ANIMALS);
    let (status, body) = server.get(&format!("/r4b/CodeSystem/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // `content = complete` asserts every concept is in the resource, so the
    // read carries them (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>).
    assert_eq!(body["content"], "complete");
    assert_eq!(body["caseSensitive"], true);
    assert_eq!(body["hierarchyMeaning"], "is-a");
    let cat = concept(&body, "cat");
    assert_eq!(cat["display"], "Cat");
    assert_eq!(
        parents(cat),
        ["animal"],
        "the nesting comes back as a `parent` property"
    );
    let kitten = concept(&body, "kitten");
    assert_eq!(
        parents(kitten),
        ["cat", "pet"],
        "`subsumedBy` comes back as `parent` too"
    );
    let designations: Vec<&str> = cat["designation"]
        .as_array()
        .expect("designations")
        .iter()
        .filter_map(|d| d["value"].as_str())
        .collect();
    assert_eq!(designations, ["Domestic cat", "Katze"]);
    let declared: Vec<&str> = body["filter"]
        .as_array()
        .expect("filters")
        .iter()
        .filter_map(|f| f["code"].as_str())
        .collect();
    assert_eq!(declared, ["legs"]);
}

#[tokio::test]
async fn a_search_by_url_and_version_finds_a_loaded_code_system() {
    let server = Server::start_with_every_loader();
    let id = server.instance_id_of("http://loinc.org");
    let (status, body) = server.get("/r4b/CodeSystem?url=http://loinc.org").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 1);
    // `fullUrl` is the absolute URL of the resource
    // (<https://hl7.org/fhir/R4B/bundle.html#bundle-unique>), built from the
    // base the request reached and the version's own root.
    assert_eq!(
        body["entry"][0]["fullUrl"],
        format!("{}/CodeSystem/{id}", base_of("r4b"))
    );
    assert_eq!(body["entry"][0]["search"]["mode"], "match");
    assert_eq!(body["entry"][0]["resource"]["id"], id.as_str());
    assert_eq!(body["entry"][0]["resource"]["content"], "not-present");

    let version = ferroterm_testkit::loinc::VERSION;
    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem?url=http://loinc.org&version={version}"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 1, "the version matches");

    let (status, body) = server
        .get("/r4b/CodeSystem?url=http://loinc.org&version=0.0")
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["total"], 0,
        "a version that is not served matches none"
    );
}

#[tokio::test]
async fn a_search_without_criteria_lists_every_loaded_code_system() {
    let server = Server::start_with_every_loader();
    let (status, body) = server.get("/r4b/CodeSystem").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let state = server.state();
    let loaded = u64::try_from(state.instances().count() + state.supplement_instances().len())
        .expect("fits");
    assert_eq!(
        body["total"].as_u64(),
        Some(loaded),
        "one entry per served code system version and supplement: {body}"
    );
    let urls: Vec<&str> = body["entry"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|entry| entry["resource"]["url"].as_str())
        .collect();
    for url in SERVED_FROM_AN_INDEX {
        assert!(urls.contains(&url), "{url} in {urls:?}");
    }
    assert!(
        urls.contains(&ferroterm_testkit::fhir::ANIMALS_NL),
        "a supplement is a CodeSystem the server holds: {urls:?}"
    );
}

#[tokio::test]
async fn a_loaded_supplement_reads_at_its_id_and_is_listed_by_search() {
    let server = Server::start_with_every_loader();
    // The supplement carries no `id`, so it is read at the id the server mints
    // from its canonical (<https://hl7.org/fhir/R4B/resource.html#id>).
    let id = ferroterm_server::state::instance_id(ferroterm_testkit::fhir::ANIMALS_NL, "1");
    for base in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server.get(&format!("/{base}/CodeSystem/{id}")).await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body["resourceType"], "CodeSystem", "{base}");
        assert_eq!(body["id"], id.as_str(), "{base}");
        // A supplement "defines extra properties and designations" of the
        // system it names (<https://hl7.org/fhir/R4B/codesystem.html#supplements>).
        assert_eq!(body["content"], "supplement", "{base}");
        assert_eq!(
            body["supplements"],
            ferroterm_testkit::fhir::ANIMALS,
            "{base}"
        );
    }

    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem?url={}",
            ferroterm_testkit::fhir::ANIMALS_NL
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 1, "{body}");
    assert_eq!(
        body["entry"][0]["fullUrl"],
        format!("{}/CodeSystem/{id}", base_of("r4b"))
    );
    assert_eq!(body["entry"][0]["resource"]["content"], "supplement");
    assert_eq!(body["entry"][0]["search"]["mode"], "match");

    // Reading the supplement leaves it applying to the system it names.
    let (status, body) = server
        .get(&format!(
            "/r4b/CodeSystem/$lookup?system={}&code=cat&displayLanguage=nl&useSupplement={}",
            ferroterm_testkit::fhir::ANIMALS,
            ferroterm_testkit::fhir::ANIMALS_NL
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        crate::fixture::parameter(&body, "display").map(|p| &p["valueString"]),
        Some(&serde_json::json!("Kat")),
        "{body}"
    );
}

#[tokio::test]
async fn a_search_returns_the_persisted_and_the_loaded_code_systems() {
    let server = Server::start_persisting();
    let colours = serde_json::json!({
        "resourceType": "CodeSystem",
        "url": "http://ferroterm.test/CodeSystem/hues",
        "version": "1.0", "status": "active", "content": "complete",
        "concept": [{"code": "red", "display": "Red"}]
    });
    let response = server.put("/r4b/CodeSystem/hues", &colours).await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let (status, body) = server.get("/r4b/CodeSystem").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let urls: Vec<&str> = body["entry"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|entry| entry["resource"]["url"].as_str())
        .collect();
    assert!(
        urls.contains(&"http://ferroterm.test/CodeSystem/hues"),
        "the persisted code system is in the searchset: {urls:?}"
    );
    assert!(
        urls.contains(&"http://snomed.info/sct"),
        "the loaded edition is there too: {urls:?}"
    );
}

#[tokio::test]
async fn an_id_the_server_serves_nothing_under_is_a_not_found() {
    let server = Server::start_with_every_loader();
    let (status, body) = server.get("/r4b/CodeSystem/no-such-system").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["resourceType"], "OperationOutcome");
    assert_eq!(body["issue"][0]["code"], "not-found");
}

#[tokio::test]
async fn instance_lookup_answers_exactly_where_the_version_declares_it() {
    let server = Server::start_with_every_loader();
    let id = server.instance_id_of("http://snomed.info/sct");
    let code = ferroterm_testkit::snomed::sctid(ferroterm_testkit::snomed::item(
        ferroterm_testkit::snomed::CAT,
    ));
    // R4 and R4B declare `$lookup` at the type level only; R5 and the R6
    // ballot add the instance level
    // (<https://hl7.org/fhir/R4B/codesystem-operation-lookup.html>,
    // <https://hl7.org/fhir/R5/codesystem-operation-lookup.html>).
    for base in ["r4", "r4b"] {
        let (status, body) = server
            .get(&format!("/{base}/CodeSystem/{id}/$lookup?code={code}"))
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{base}: {body}");
    }
    for base in ["r5", "r6"] {
        let (status, body) = server
            .get(&format!("/{base}/CodeSystem/{id}/$lookup?code={code}"))
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body["resourceType"], "Parameters", "{base}");
    }
}

/// The concept `code` of a rendered `CodeSystem`.
fn concept<'a>(body: &'a Value, code: &str) -> &'a Value {
    body["concept"]
        .as_array()
        .expect("concepts")
        .iter()
        .find(|concept| concept["code"] == code)
        .unwrap_or_else(|| panic!("{code} is defined"))
}

/// The `parent` property values of a rendered concept.
fn parents(concept: &Value) -> Vec<&str> {
    concept["property"]
        .as_array()
        .map(|properties| {
            properties
                .iter()
                .filter(|property| property["code"] == "parent")
                .filter_map(|property| property["valueCode"].as_str())
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn a_declared_base_url_is_the_one_a_searchset_addresses_resources_at() {
    // A deployment behind a proxy declares the address clients reach it at, and
    // that is the base an absolute `fullUrl` is built from
    // (<https://hl7.org/fhir/R4B/bundle.html#bundle-unique>).
    let server = Server::start_with_base_url("https://tx.example.org/fhir");
    let id = server.snomed_id();
    let (status, body) = server
        .get("/r4b/CodeSystem?url=http://snomed.info/sct")
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["entry"][0]["fullUrl"],
        format!("https://tx.example.org/fhir/r4b/CodeSystem/{id}"),
        "the declared base wins over the authority the request names: {body}"
    );
}
