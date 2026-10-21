// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Fetching a FHIR resource file and correcting it for this service.

use addon_nts::fixup::FixupKind;
use terminology_syndication::model::{Checksum, ContentLink, LinkRel};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support;

/// A map published with `experimental` as a string, the way this service does.
const MAP_WITH_STRING: &str = r#"{"resourceType":"ConceptMap","id":"a-map","experimental":"true"}"#;

/// A code system that needs nothing corrected.
const CLEAN_CODE_SYSTEM: &str =
    r#"{"resourceType":"CodeSystem","id":"a-system","experimental":false}"#;

/// A service serving `body` at `/content/resource.json` behind the challenge.
async fn service(body: &'static str) -> MockServer {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("password")
        .respond_with(support::issued("access-first", 300, "refresh-first"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/content/resource.json"))
        .and(header("authorization", "Bearer access-first"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;
    server
}

/// The entry link pointing at the served resource.
fn link(server: &MockServer, body: &str) -> ContentLink {
    ContentLink {
        href: format!("{}/content/resource.json", server.uri()),
        rel: LinkRel::Alternate,
        media_type: Some(String::from("application/fhir+json")),
        length: None,
        checksum: Some(Checksum::Sha256(support::sha256_of(body))),
        validated: false,
    }
}

#[tokio::test]
async fn a_string_experimental_is_corrected_and_recorded() {
    let server = service(MAP_WITH_STRING).await;
    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::password_account(), clock);
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("ConceptMap-a-map.json");

    let taken = source
        .fetch_resource(&link(&server, MAP_WITH_STRING), &destination)
        .await
        .expect("the resource is fetched and corrected");

    assert_eq!(
        taken.fixups.len(),
        1,
        "the run record carries the one correction: {:?}",
        taken.fixups
    );
    let fixup = taken.fixups.first().expect("the correction");
    assert_eq!(fixup.pointer, "/experimental");
    assert_eq!(
        fixup.kind,
        FixupKind::ExperimentalString {
            was: String::from("true"),
            now: true,
        }
    );
    let written = std::fs::read_to_string(&destination).expect("the file was written");
    let document: serde_json::Value =
        serde_json::from_str(&written).expect("the corrected resource reads");
    assert_eq!(
        document.get("experimental"),
        Some(&serde_json::Value::Bool(true)),
        "the file that lands carries the boolean FHIR declares"
    );
    assert_eq!(
        taken.fetched.checksum,
        Checksum::Sha256(support::sha256_of(MAP_WITH_STRING)),
        "the digest that was verified is the one the feed advertised"
    );
}

#[tokio::test]
async fn a_resource_that_needs_nothing_lands_byte_identical() {
    let server = service(CLEAN_CODE_SYSTEM).await;
    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::password_account(), clock);
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("CodeSystem-a-system.json");

    let taken = source
        .fetch_resource(&link(&server, CLEAN_CODE_SYSTEM), &destination)
        .await
        .expect("the resource is fetched");

    assert!(
        taken.fixups.is_empty(),
        "nothing needed correcting: {:?}",
        taken.fixups
    );
    assert_eq!(
        std::fs::read(&destination).expect("the file was written"),
        CLEAN_CODE_SYSTEM.as_bytes(),
        "an untouched resource keeps the bytes the service served"
    );
}

#[tokio::test]
async fn a_resource_whose_digest_does_not_match_never_lands() {
    let server = service(MAP_WITH_STRING).await;
    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::password_account(), clock);
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("ConceptMap-a-map.json");

    let mut wrong = link(&server, MAP_WITH_STRING);
    wrong.checksum = Some(Checksum::Sha256(support::sha256_of("other bytes")));
    let error = source
        .fetch_resource(&wrong, &destination)
        .await
        .expect_err("the bytes do not verify");

    assert!(
        error.to_string().contains("syndication request failed"),
        "the failure names the fetch: {error}"
    );
    assert!(
        !destination.exists(),
        "an unverified file is never handed on"
    );
}
