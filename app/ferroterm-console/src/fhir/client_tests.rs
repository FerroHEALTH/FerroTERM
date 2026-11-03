// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The client's tests: the requests it sends and what it reads of each answer.

use ferrohealth_secret::secret::Secret;
use ferrohealth_session::record::SignedIn;

use super::*;

/// The root readers reach the server at, which every address is built on.
const ROOT: &str = "https://tx.example.org";

/// The console's state, whose upstream client reaches the FHIR server at
/// `base`.
async fn console_at(base: &str) -> crate::server::ConsoleState {
    let mut config = crate::config::Config::default();
    config.ferroterm.base_url = base.to_owned();
    crate::server::ConsoleState::new(config.resolve().expect("the settings resolve"))
        .await
        .expect("the clients build")
}

/// Runs `run` with a client reaching the server through `state`, as the
/// request carrying `cookie` does.
async fn served<T, F: Future<Output = T>>(
    state: crate::server::ConsoleState,
    cookie: Option<&str>,
    run: impl FnOnce(FhirClient) -> F,
) -> T {
    let mut headers = http::HeaderMap::new();
    if let Some(cookie) = cookie {
        headers.insert(
            http::header::COOKIE,
            http::HeaderValue::from_str(cookie).expect("a cookie header"),
        );
    }
    let held = state.sessions().held(&headers).expect("usable").value;
    let upstream = crate::upstream::Upstream::of(&state, &held);
    run(FhirClient::new(ROOT.to_owned()).with_upstream(upstream)).await
}

/// A FHIR server answering `GET path` with `status` and `body`.
async fn answering(path: &str, status: u16, body: &str) -> wiremock::MockServer {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(path))
        .respond_with(
            wiremock::ResponseTemplate::new(status)
                .set_body_raw(body.to_owned(), "application/fhir+json"),
        )
        .mount(&server)
        .await;
    server
}

/// The one request `server` received.
async fn the_request(server: &wiremock::MockServer) -> wiremock::Request {
    let mut received = server.received_requests().await.expect("recording");
    assert_eq!(1, received.len(), "one request");
    received.remove(0)
}

/// An address no server answers at.
const UNREACHABLE: &str = "http://127.0.0.1:9/";

#[tokio::test]
async fn a_healthy_server_answers_its_status_and_the_probe_reads_it() {
    let server = answering("/health", 200, "").await;
    let state = console_at(&server.uri()).await;
    let (health, probe) = served(state, None, |client| async move {
        (client.health().await, client.probe().await)
    })
    .await;
    assert_eq!(StatusCode::OK, health.expect("healthy"));
    assert_eq!(Some(StatusCode::OK), probe);
}

#[tokio::test]
async fn a_failing_server_s_status_is_a_failure_the_probe_still_reports() {
    let server = answering("/health", 503, "down").await;
    let (health, probe) = served(console_at(&server.uri()).await, None, |client| async move {
        (client.health().await, client.probe().await)
    })
    .await;
    let error = health.expect_err("a failure");
    assert!(
        matches!(&error, FhirError::Status { status, body, .. }
            if *status == StatusCode::SERVICE_UNAVAILABLE && body == "down"),
        "{error:?}"
    );
    assert_eq!(Some(StatusCode::SERVICE_UNAVAILABLE), probe);
}

#[tokio::test]
async fn a_server_that_does_not_answer_probes_as_none() {
    let (health, probe) = served(console_at(UNREACHABLE).await, None, |client| async move {
        (client.health().await, client.probe().await)
    })
    .await;
    assert!(
        matches!(health, Err(FhirError::Transport { .. })),
        "{health:?}"
    );
    assert_eq!(None, probe);
}

#[tokio::test]
async fn a_root_whose_smart_discovery_is_absent_serves_without_sign_in() {
    let server = answering("/r4/.well-known/smart-configuration", 404, "").await;
    assert!(
        served(console_at(&server.uri()).await, None, |client| async move {
            client.serves_without_sign_in(FhirVersion::R4).await
        })
        .await
    );
}

#[tokio::test]
async fn a_root_that_publishes_smart_discovery_asks_for_a_sign_in() {
    let server = answering("/r5/.well-known/smart-configuration", 200, "{}").await;
    assert!(
        !served(console_at(&server.uri()).await, None, |client| async move {
            client.serves_without_sign_in(FhirVersion::R5).await
        })
        .await
    );
    assert!(
        !served(console_at(UNREACHABLE).await, None, |client| async move {
            client.serves_without_sign_in(FhirVersion::R5).await
        })
        .await,
        "a root that does not answer is not known to serve without one"
    );
}

#[tokio::test]
async fn a_code_system_summary_asks_without_concepts_and_keeps_the_document() {
    let body = r#"{"resourceType":"Bundle","type":"searchset","total":1,"entry":[{"resource":{"resourceType":"CodeSystem","url":"https://terminology.example/x?e=1","name":"X"}}]}"#;
    let server = answering("/r4b/CodeSystem", 200, body).await;
    let document = served(console_at(&server.uri()).await, None, |client| async move {
        client
            .code_system_summary(FhirVersion::R4B, "https://terminology.example/x?e=1")
            .await
    })
    .await
    .expect("an answer");
    let request = the_request(&server).await;
    assert_eq!(
        Some("url=https%3A%2F%2Fterminology.example%2Fx%3Fe%3D1&_summary=true"),
        request.url.query()
    );
    assert_eq!(
        Some("application/fhir+json"),
        request
            .headers
            .get(http::header::ACCEPT)
            .and_then(|value| value.to_str().ok())
    );
    assert_eq!(1, document.value.published().len());
    assert!(
        document.text.contains("\n  \"resourceType\": \"Bundle\""),
        "the document is kept indented for a reader: {}",
        document.text
    );
}

#[tokio::test]
async fn the_published_names_are_read_narrowed_to_what_a_picker_shows() {
    let body = r#"{"resourceType":"Bundle","entry":[{"resource":{"resourceType":"ValueSet","url":"https://terminology.example/vs","title":"Colours"}}]}"#;
    let server = answering("/r5/ValueSet", 200, body).await;
    let names = served(console_at(&server.uri()).await, None, |client| async move {
        client.published_names(FhirVersion::R5, "ValueSet").await
    })
    .await
    .expect("an answer");
    assert_eq!(
        Some("_elements=url%2Cname%2Ctitle%2Cversion%2Cstatus%2Cmeta"),
        the_request(&server).await.url.query()
    );
    assert_eq!(
        Some(&String::from("Colours")),
        names.names().get("https://terminology.example/vs")
    );
}

#[tokio::test]
async fn the_capability_document_is_handed_back_as_the_server_sent_it() {
    let body = r#"{"resourceType":"CapabilityStatement","fhirVersion":"4.0.1","x-extra":[1,2]}"#;
    let server = answering("/r4/metadata", 200, body).await;
    let document = served(console_at(&server.uri()).await, None, |client| async move {
        client.capability_document(FhirVersion::R4).await
    })
    .await
    .expect("an answer");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(body).expect("JSON"),
        document
    );
}

#[tokio::test]
async fn a_lookup_reads_the_concept_the_server_answers() {
    let body = r#"{"resourceType":"Parameters","parameter":[{"name":"name","valueString":"Colours"},{"name":"display","valueString":"Red"}]}"#;
    let server = answering("/r4/CodeSystem/$lookup", 200, body).await;
    let request = LookupRequest {
        system: "https://terminology.example/colours".to_owned(),
        code: "red".to_owned(),
        ..LookupRequest::default()
    };
    let document = served(console_at(&server.uri()).await, None, |client| async move {
        client.lookup(FhirVersion::R4, &request).await
    })
    .await
    .expect("an answer");
    assert_eq!(
        Some("system=https%3A%2F%2Fterminology.example%2Fcolours&code=red"),
        the_request(&server).await.url.query()
    );
    assert_eq!(Some(String::from("Red")), document.value.display);
}

#[tokio::test]
async fn a_refusal_with_an_operation_outcome_carries_it_to_the_reader() {
    let body = r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"not-found","diagnostics":"no such code"}]}"#;
    let server = answering("/r4/CodeSystem/$lookup", 404, body).await;
    let request = LookupRequest {
        system: "https://terminology.example/colours".to_owned(),
        code: "mauve".to_owned(),
        ..LookupRequest::default()
    };
    let error = served(console_at(&server.uri()).await, None, |client| async move {
        client.lookup(FhirVersion::R4, &request).await
    })
    .await
    .expect_err("refused");
    assert_eq!(Some(StatusCode::NOT_FOUND), error.status());
    assert_eq!(
        1,
        error.outcome().map_or(0, |outcome| outcome.issues().len())
    );
}

#[tokio::test]
async fn a_success_whose_body_is_no_json_is_a_decode_failure() {
    let server = answering("/r4/CodeSystem/$lookup", 200, "<html/>").await;
    let request = LookupRequest {
        system: "s".to_owned(),
        code: "c".to_owned(),
        ..LookupRequest::default()
    };
    let error = served(console_at(&server.uri()).await, None, |client| async move {
        client.lookup(FhirVersion::R4, &request).await
    })
    .await
    .expect_err("refused");
    assert!(matches!(error, FhirError::Decode { .. }), "{error:?}");
}

#[tokio::test]
async fn a_validation_reads_the_result_the_server_answers() {
    let body = r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":false},{"name":"message","valueString":"Unknown code"}]}"#;
    let server = answering("/r4b/ValueSet/$validate-code", 200, body).await;
    let request = ValidateRequest {
        on: validation::ValidateOn::ValueSet,
        url: "https://terminology.example/vs".to_owned(),
        system: "https://terminology.example/colours".to_owned(),
        code: "mauve".to_owned(),
        ..ValidateRequest::default()
    };
    let document = served(console_at(&server.uri()).await, None, |client| async move {
        client.validate_code(FhirVersion::R4B, &request).await
    })
    .await
    .expect("an answer");
    let validation = document
        .value
        .validation(&ferrohealth_ui::i18n::Locale::default());
    assert_eq!(
        Some(false),
        validation.result,
        "an unknown code is no refusal"
    );
    assert_eq!(Some(String::from("Unknown code")), validation.message);
}

#[tokio::test]
async fn a_subsumption_reads_the_outcome_the_server_answers() {
    let body =
        r#"{"resourceType":"Parameters","parameter":[{"name":"outcome","valueCode":"subsumes"}]}"#;
    let server = answering("/r5/CodeSystem/colours/$subsumes", 200, body).await;
    let request = SubsumesRequest {
        id: "colours".to_owned(),
        code_a: "red".to_owned(),
        code_b: "crimson".to_owned(),
        ..SubsumesRequest::default()
    };
    let document = served(console_at(&server.uri()).await, None, |client| async move {
        client.subsumes(FhirVersion::R5, &request).await
    })
    .await
    .expect("an answer");
    assert_eq!(Some(String::from("subsumes")), document.value.subsumption());
}

#[tokio::test]
async fn a_translation_reads_the_result_the_server_answers() {
    let body =
        r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":true}]}"#;
    let server = answering("/r4/ConceptMap/$translate", 200, body).await;
    let request = TranslateRequest {
        system: "https://terminology.example/colours".to_owned(),
        code: "red".to_owned(),
        ..TranslateRequest::default()
    };
    let document = served(console_at(&server.uri()).await, None, |client| async move {
        client.translate(FhirVersion::R4, &request).await
    })
    .await
    .expect("an answer");
    assert_eq!(Some(true), document.value.result());
}

#[tokio::test]
async fn a_basic_sign_in_s_request_carries_its_credential_and_a_visitor_s_none() {
    let server = answering("/health", 200, "").await;
    let state = console_at(&server.uri()).await;
    let signed_in = SignedIn::basic("operator", Secret::new("synthetic-password"));
    let set = state
        .sessions()
        .establish(&http::HeaderMap::new(), signed_in);
    // NOTE: RFC 6265 §5.4: the attributes after the pair read as no cookie of the console's.
    let cookie = set.expect("room").to_str().expect("ASCII").to_owned();
    served(state.clone(), Some(&cookie), |client| async move {
        client.health().await
    })
    .await
    .expect("healthy");
    served(state, None, |client| async move { client.health().await })
        .await
        .expect("healthy");
    let received = server.received_requests().await.expect("recording");
    let authorization: Vec<Option<&str>> = received
        .iter()
        .map(|request| {
            request
                .headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
        })
        .collect();
    // NOTE: RFC 7617 §2: the user id and the password joined by a colon, in base64.
    assert_eq!(
        vec![Some("Basic b3BlcmF0b3I6c3ludGhldGljLXBhc3N3b3Jk"), None],
        authorization
    );
}
