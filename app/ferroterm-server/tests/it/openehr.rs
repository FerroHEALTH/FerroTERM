// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An openEHR archetype's local terminology, served through the ordinary
//! operations.
//!
//! An archetype constrains most of its coded fields with an archetype-local
//! `at`-code list, and nothing publishes those as FHIR resources, so no
//! terminology server can address them. A producer derives ordinary resources
//! from the archetype, and this server holds them the way it holds any other
//! supplied resource: nothing here is openEHR-specific, on the wire or in the
//! loader.
//!
//! No openEHR specification defines a canonical URI for an archetype's local
//! terminology, so the URL is minted by whoever produced the resource, from the
//! archetype id that the Archetype Object Model 2 specification §3.2 makes
//! globally unique. This server reads it as an opaque canonical.

use http::StatusCode;
use serde_json::Value;

use crate::fixture::Server;
use ferroterm_testkit::openehr::{
    AC_VALUE_SET, ARCHETYPE, AT_CODES, AT_CODES_ELSEWHERE, BOUND_SYSTEM, TERM_BINDINGS,
};

/// `url` percent-encoded for a query value.
fn encoded(url: &str) -> String {
    url.replace(':', "%3A").replace('/', "%2F")
}

#[tokio::test]
async fn an_archetypes_local_codes_are_looked_up_like_any_other_code_system() {
    let server = Server::start_with_archetype_terminology();
    let (status, body) = server
        .get(&format!(
            "/r5/CodeSystem/$lookup?system={}&code=at0005",
            encoded(AT_CODES)
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let parameter = |name: &str| -> Option<String> {
        body["parameter"]
            .as_array()?
            .iter()
            .find(|p| p["name"] == name)?["valueString"]
            .as_str()
            .map(str::to_owned)
    };
    assert_eq!(
        parameter("display").as_deref(),
        Some("Steady"),
        "the rubric of the archetype node is the display: {body}"
    );
}

#[tokio::test]
async fn an_ac_code_expands_to_the_at_codes_it_constrains() {
    let server = Server::start_with_archetype_terminology();
    let (status, body) = server
        .get(&format!(
            "/r5/ValueSet/$expand?url={}",
            encoded(AC_VALUE_SET)
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let codes: Vec<&str> = body["expansion"]["contains"]
        .as_array()
        .map(|contains| {
            contains
                .iter()
                .filter_map(|entry| entry["code"].as_str())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        codes,
        ["at0005", "at0006"],
        "the value set an `ac`-code became expands to the nodes it constrains: {body}"
    );
}

#[tokio::test]
async fn a_code_outside_the_ac_code_is_not_in_its_value_set() {
    let server = Server::start_with_archetype_terminology();
    let ask = async |code: &str| {
        let url = format!(
            "/r5/ValueSet/$validate-code?url={}&system={}&code={code}",
            encoded(AC_VALUE_SET),
            encoded(AT_CODES)
        );
        server.get(&url).await
    };
    let result = |body: &Value| -> Option<bool> {
        body["parameter"]
            .as_array()?
            .iter()
            .find(|p| p["name"] == "result")?["valueBoolean"]
            .as_bool()
    };
    let (status, inside) = ask("at0005").await;
    assert_eq!(status, StatusCode::OK, "{inside}");
    assert_eq!(result(&inside), Some(true), "{inside}");

    // `at0004` is a node of the same archetype that the `ac`-code does not
    // constrain, so it is a valid code outside this value set.
    let (status, outside) = ask("at0004").await;
    assert_eq!(status, StatusCode::OK, "{outside}");
    assert_eq!(
        result(&outside),
        Some(false),
        "a node the `ac`-code leaves out is not in it: {outside}"
    );
}

#[tokio::test]
async fn a_term_binding_translates_onto_the_system_it_binds_to() {
    let server = Server::start_with_archetype_terminology();
    let (status, body) = server
        .get(&format!(
            "/r5/ConceptMap/$translate?url={}&sourceSystem={}&sourceCode=at0005&targetSystem={}",
            encoded(TERM_BINDINGS),
            encoded(AT_CODES),
            encoded(BOUND_SYSTEM)
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let parameters = body["parameter"].as_array().cloned().unwrap_or_default();
    let matched = parameters.iter().any(|p| p["name"] == "result")
        && parameters
            .iter()
            .find(|p| p["name"] == "result")
            .and_then(|p| p["valueBoolean"].as_bool())
            == Some(true);
    assert!(
        matched,
        "the archetype's term binding is a map the ordinary operation answers: {body}"
    );
}

#[tokio::test]
async fn the_minted_canonical_is_what_a_client_finds_the_archetype_by() {
    let server = Server::start_with_archetype_terminology();
    // No openEHR specification defines a canonical URI for an archetype's local
    // terminology, so the producer mints one. That minted URL is the only name
    // a client has for it, and it is an ordinary canonical to this server.
    let (status, body) = server
        .get(&format!("/r5/CodeSystem?url={}", encoded(AT_CODES)))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["total"], 1, "{body}");
    let found = &body["entry"][0]["resource"];
    assert_eq!(found["url"].as_str(), Some(AT_CODES), "{body}");
    assert!(
        found["title"]
            .as_str()
            .unwrap_or_default()
            .contains(ARCHETYPE),
        "the resource says which archetype it came from: {body}"
    );
}

#[tokio::test]
async fn the_archetype_terminology_is_declared_like_every_other_code_system() {
    let server = Server::start_with_archetype_terminology();
    let (status, body) = server.get("/r5/metadata?mode=terminology").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let declared = body["codeSystem"].as_array().is_some_and(|systems| {
        systems
            .iter()
            .any(|system| system["uri"].as_str() == Some(AT_CODES))
    });
    assert!(
        declared,
        "a client discovers the archetype's codes the way it discovers any system: {body}"
    );
}

// NOTE: a canonical identifies a code system, so two producers of one
// archetype mint two and both are served
// (<https://hl7.org/fhir/R5/codesystem.html#invs>, `CodeSystem.url`).
#[tokio::test]
async fn two_deployers_minting_the_same_archetype_do_not_collide() {
    let server = Server::start_with_archetype_terminology();
    for (url, display) in [(AT_CODES, "Steady"), (AT_CODES_ELSEWHERE, "Another")] {
        let (status, body) = server
            .get(&format!(
                "/r5/CodeSystem/$lookup?system={}&code=at0005",
                encoded(url)
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{url} does not answer: {body}");
        let answered = body["parameter"]
            .as_array()
            .and_then(|parameters| {
                parameters
                    .iter()
                    .find(|p| p["name"] == "display")?
                    .get("valueString")?
                    .as_str()
            })
            .unwrap_or_default()
            .to_owned();
        assert!(
            answered.starts_with(display),
            "each minted canonical keeps its own producer's rubric for `at0005`, and \
             `{url}` answered `{answered}`"
        );
    }
}

// NOTE: a code system is identified by its canonical and version, so two
// directories naming one is a deployment mistake the load reports rather than
// resolving (`fhir_terminology::registry::RegisterError::Duplicate`).
#[tokio::test]
async fn two_directories_minting_one_canonical_refuse_to_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (first, second) = (dir.path().join("first"), dir.path().join("second"));
    for path in [&first, &second] {
        std::fs::create_dir_all(path).expect("creates");
        ferroterm_testkit::openehr::write_archetype_terminology(path)
            .expect("writes the archetype's terminology");
    }
    let config = ferroterm_server::config::Config {
        code_systems: vec![first, second],
        ..ferroterm_server::config::Config::default()
    };
    let Err(error) = ferroterm_server::state::AppState::load(&config) else {
        panic!("one canonical at one version cannot be served twice");
    };
    assert!(
        error.to_string().to_lowercase().contains("duplicate")
            || format!("{error:?}").contains("Duplicate"),
        "the load names the collision rather than picking a winner: {error}"
    );
}
