// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The find: the answer route the panel under the top bar's search asks as
//! you type, and the find page that lists every match in the page, both over
//! a `wiremock` terminology server.

use ferrohealth_ui::view::search::SearchAnswer;
use http::StatusCode;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::provider::{console, get, send, text};

/// A terminology server that declares `$lookup`, serves one synthetic code
/// system and no concept map, and refuses its value sets.
async fn terminology() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/[^/]+/metadata$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "CapabilityStatement",
            "fhirVersion": "4.0.1",
            "rest": [{
                "resource": [{
                    "type": "CodeSystem",
                    "operation": [{ "name": "lookup", "definition": "http://hl7.org/fhir/OperationDefinition/CodeSystem-lookup" }]
                }]
            }]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/[^/]+/CodeSystem$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "Bundle",
            "type": "searchset",
            "entry": [{ "resource": {
                "resourceType": "CodeSystem",
                "id": "respiratory",
                "url": "https://terminology.example/CodeSystem/respiratory",
                "title": "Synthetic respiratory findings",
                "status": "active"
            } }]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/[^/]+/ValueSet$"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/[^/]+/ConceptMap$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "Bundle",
            "type": "searchset"
        })))
        .mount(&server)
        .await;
    server
}

/// The console's configuration reaching `server`.
fn reaching(server: &MockServer) -> String {
    format!("[ferroterm]\nbase_url = \"{}/\"\n", server.uri())
}

#[tokio::test]
async fn the_answer_route_answers_the_matches_and_names_a_read_that_failed() {
    let server = terminology().await;
    let state = console(None, &reaching(&server));
    let response = send(&state, get("/find/answer?q=respir", &[])).await;
    assert_eq!(StatusCode::OK, response.status());
    let body = text(response).await;
    let answer: SearchAnswer = serde_json::from_str(&body).expect("a SearchAnswer");
    assert_eq!("respir", answer.query);
    let names: Vec<&str> = answer
        .groups
        .iter()
        .flat_map(|group| &group.hits)
        .map(|hit| hit.name.as_str())
        .collect();
    assert!(names.contains(&"Synthetic respiratory findings"), "{body}");
    assert!(
        answer
            .unfinished
            .as_deref()
            .is_some_and(|why| why.contains("ValueSet")),
        "a refused read is said, never an empty answer: {body}"
    );
}

#[tokio::test]
async fn the_find_page_lists_every_match_in_the_page() {
    let server = terminology().await;
    let state = console(None, &reaching(&server));
    let response = send(&state, get("/find?q=respir", &[])).await;
    assert_eq!(StatusCode::OK, response.status());
    let page = text(response).await;
    let start = page.find("<main").expect("a main");
    let end = page.find("</main>").expect("a main's end");
    let main = page.get(start..end).expect("the main");
    assert!(
        main.contains("CodeSystem%2Frespiratory"),
        "the matching code system is a link in the page: {main}"
    );
    assert!(
        main.contains("fui-find-list"),
        "the matches are in the page"
    );
    assert!(
        !main.contains("fui-search-results"),
        "the find page lists the matches in its flow, never in a panel: {main}"
    );
}

#[tokio::test]
async fn the_top_bar_search_asks_the_answer_route() {
    let server = terminology().await;
    let state = console(None, &reaching(&server));
    let page = text(send(&state, get("/find?q=respir", &[])).await).await;
    assert!(
        page.contains("/find/answer"),
        "the search names its answer route: {page}"
    );
}
