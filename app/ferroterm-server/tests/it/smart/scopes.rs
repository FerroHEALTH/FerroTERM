// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SMART scopes each write route needs, by version, compartment and interaction.

use axum::body::Body;
use http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use http::{Request, StatusCode};
use serde_json::json;

use super::{COLOURS, SigningKey, challenge, claims, colours, issuer, put_colours, put_with};
use crate::fixture::{self, Server};

// A deployment that turned `[auth]` off is served the way `Serving::new`
// serves: no credential is asked for anywhere.
#[tokio::test]
async fn a_server_with_auth_off_leaves_every_route_open() {
    let server = Server::start_persisting();
    let response = put_colours(&server, None).await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "a write answers without a token when authentication is off"
    );
    assert_eq!(
        fixture::header(&response, WWW_AUTHENTICATE),
        None,
        "no challenge is ever sent"
    );
    let (status, _) = server.get("/r4b/CodeSystem/colours").await;
    assert_eq!(status, StatusCode::OK);
}

// RFC 6750 §3.1: a request that carried no authentication is challenged
// without an error code.
#[tokio::test]
async fn a_write_without_a_token_is_challenged() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let response = put_colours(&server, None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let sent = challenge(&response);
    assert!(sent.starts_with("Bearer realm="), "{sent}");
    assert!(
        !sent.contains("error="),
        "a request with no credential names no error: {sent}"
    );
    let (_, body) = fixture::json(response).await;
    assert_eq!(body["resourceType"], "OperationOutcome");
    assert_eq!(body["issue"][0]["code"], "login");
}

#[tokio::test]
async fn the_read_surface_answers_without_a_token() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let cat = ferroterm_testkit::snomed::sctid(ferroterm_testkit::snomed::item(
        ferroterm_testkit::snomed::CAT,
    ));
    for uri in [
        String::from("/r4b/metadata"),
        String::from("/r4b/CodeSystem"),
        format!("/r4b/CodeSystem/$lookup?system=http://snomed.info/sct&code={cat}"),
        String::from("/r4b/ValueSet/$expand?url=http://snomed.info/sct?fhir_vs"),
    ] {
        let (status, body) = server.get(&uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }
}

// The version 2 scope for a create is `system/CodeSystem.c`
// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
#[tokio::test]
async fn a_token_with_the_scope_writes_and_one_without_it_is_refused() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let read_only = key.sign(&claims(&mock.uri(), "system/CodeSystem.rs", 300));
    let response = put_colours(&server, Some(&read_only)).await;
    let sent = challenge(&response);
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "{sent}");
    assert!(sent.contains("error=\"insufficient_scope\""), "{sent}");
    // RFC 6750 §3 makes the `scope` attribute optional, and the refusal names
    // no scope anywhere, so it teaches a caller nothing about its grants.
    assert!(!sent.contains("scope="), "{sent}");
    let (_, body) = fixture::json(response).await;
    assert_eq!(body["issue"][0]["code"], "forbidden");
    let diagnostics = body["issue"][0]["diagnostics"].as_str().unwrap_or_default();
    assert!(!diagnostics.contains("CodeSystem."), "{diagnostics}");

    let writer = key.sign(&claims(&mock.uri(), "system/CodeSystem.cud", 300));
    let (status, body) = fixture::json(put_colours(&server, Some(&writer)).await).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

// `$closure` keeps a stored closure table
// (<https://hl7.org/fhir/R4B/conceptmap-operation-closure.html>), so it is
// gated with the writes; SMART names no scope for it.
#[tokio::test]
async fn closure_is_gated_as_a_concept_map_update() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let call = json!({
        "resourceType": "Parameters",
        "parameter": [{"name": "name", "valueString": "pets"}]
    });

    let (status, _) = server.post("/r4b/$closure", &call).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "no token, no closure table"
    );

    let read_only = key.sign(&claims(&mock.uri(), "system/ConceptMap.rs", 300));
    let (status, _) = server
        .post_with_header(
            "/r4b/$closure",
            &call,
            AUTHORIZATION.as_str(),
            &format!("Bearer {read_only}"),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let writer = key.sign(&claims(&mock.uri(), "system/ConceptMap.cud", 300));
    let (status, body) = server
        .post_with_header(
            "/r4b/$closure",
            &call,
            AUTHORIZATION.as_str(),
            &format!("Bearer {writer}"),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

// The version 1 `.write` scope maps to `.cud`, which the specification states
// in the same section.
#[tokio::test]
async fn the_version_one_write_scope_is_accepted() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let writer = key.sign(&claims(&mock.uri(), "system/CodeSystem.write", 300));
    let response = put_colours(&server, Some(&writer)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

// The user compartment is "data that a user can access", which is what a
// person signed in to an editor carries
// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
#[tokio::test]
async fn a_user_update_scope_writes_a_put_and_a_user_create_scope_does_not() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let colour_set = json!({
        "resourceType": "ValueSet",
        "url": "http://ferroterm.test/ValueSet/colour-set",
        "version": "1.0",
        "status": "active",
        "compose": {"include": [{"system": COLOURS}]}
    });

    let creator = key.sign(&claims(&mock.uri(), "user/ValueSet.c", 300));
    let response = put_with(
        &server,
        "/r4b/ValueSet/colour-set",
        &colour_set,
        Some(&creator),
    )
    .await;
    let sent = challenge(&response);
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "a `PUT` needs the update letter: {sent}"
    );
    assert!(sent.contains("error=\"insufficient_scope\""), "{sent}");
    let (_, body) = fixture::json(response).await;
    assert_eq!(body["issue"][0]["code"], "forbidden");
    let diagnostics = body["issue"][0]["diagnostics"].as_str().unwrap_or_default();
    assert!(!diagnostics.contains("ValueSet."), "{diagnostics}");

    let updater = key.sign(&claims(&mock.uri(), "user/ValueSet.u", 300));
    let response = put_with(
        &server,
        "/r4b/ValueSet/colour-set",
        &colour_set,
        Some(&updater),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

// The version 1 `.write` maps to `.cud` in the user compartment as it does in
// the system one
// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
#[tokio::test]
async fn the_version_one_user_write_scope_creates_a_concept_map() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
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

    let (code, _) = server.post("/r4b/ConceptMap", &map).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED, "a create needs a token");

    let writer = key.sign(&claims(&mock.uri(), "user/ConceptMap.write", 300));
    let (code, body) = server
        .post_with_header(
            "/r4b/ConceptMap",
            &map,
            AUTHORIZATION.as_str(),
            &format!("Bearer {writer}"),
        )
        .await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
}

// A terminology server holds no patient record, so a patient-compartment scope
// selects nothing on it
// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
#[tokio::test]
async fn a_patient_scope_is_refused_on_every_write_route() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let token = key.sign(&claims(
        &mock.uri(),
        "patient/CodeSystem.cud patient/ValueSet.cruds patient/*.*",
        300,
    ));

    let response = put_colours(&server, Some(&token)).await;
    let sent = challenge(&response);
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "{sent}");
    assert!(sent.contains("error=\"insufficient_scope\""), "{sent}");
    let (_, body) = fixture::json(response).await;
    assert_eq!(body["issue"][0]["code"], "forbidden");

    let (code, _) = server
        .post_with_header(
            "/r4b/CodeSystem",
            &colours(),
            AUTHORIZATION.as_str(),
            &format!("Bearer {token}"),
        )
        .await;
    assert_eq!(code, StatusCode::FORBIDDEN, "a create is refused too");

    // With a launch context the grant reaches that patient's compartment
    // (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>),
    // which holds no terminology resource, so the write is refused all the same.
    let mut launched = claims(&mock.uri(), "patient/CodeSystem.cud patient/*.*", 300);
    launched["patient"] = json!("p-1");
    let launched = key.sign(&launched);
    let response = put_colours(&server, Some(&launched)).await;
    let sent = challenge(&response);
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "{sent}");
    assert!(sent.contains("error=\"insufficient_scope\""), "{sent}");
    let (_, body) = fixture::json(response).await;
    assert_eq!(body["issue"][0]["code"], "forbidden");
    let (code, _) = server
        .post_with_header(
            "/r4b/CodeSystem",
            &colours(),
            AUTHORIZATION.as_str(),
            &format!("Bearer {launched}"),
        )
        .await;
    assert_eq!(code, StatusCode::FORBIDDEN, "a create is refused too");
}

// Every write interaction is gated, not only the `PUT` the other cases use
// (<https://hl7.org/fhir/R4B/http.html>).
#[tokio::test]
async fn create_and_delete_are_gated_like_update() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let writer = key.sign(&claims(&mock.uri(), "system/CodeSystem.cud", 300));

    let (status, _) = server.post("/r4b/CodeSystem", &colours()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "a create needs a token");
    let (status, body) = server
        .post_with_header(
            "/r4b/CodeSystem",
            &colours(),
            AUTHORIZATION.as_str(),
            &format!("Bearer {writer}"),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let created = body["id"].as_str().expect("an id").to_owned();

    let deletion = |token: Option<&str>| {
        let mut request = Request::delete(format!("/r4b/CodeSystem/{created}"));
        if let Some(token) = token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        request.body(Body::empty()).expect("request")
    };
    let response = server.send(deletion(None)).await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "a delete needs a token"
    );
    let response = server.send(deletion(Some(&writer))).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}
