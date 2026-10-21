// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Listing a FHIR API as a feed, and fetching what it lists.
//!
//! No FHIR specification governs a feed built from a search (our own design);
//! the paging follows the search bundle's `next` link
//! (<https://hl7.org/fhir/R4/search.html#paging>).

use std::path::Path;

use terminology_syndication::download::Fetched;
use terminology_syndication::model::{CategoryTerm, Checksum, ContentLink, Feed};
use terminology_syndication::select::{Holdings, SkipReason, Subscription, unseen};
use terminology_syndication::source::{Authorization, BoxFuture, Source, SourceError};
use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A source whose feed is empty and whose FHIR API is the mock.
#[derive(Debug)]
struct ApiSource {
    client: reqwest::Client,
    feed_url: String,
    api_url: String,
}

impl Source for ApiSource {
    fn name(&self) -> &'static str {
        "api"
    }

    fn feed_url(&self) -> &str {
        &self.feed_url
    }

    fn fhir_api_url(&self) -> Option<&str> {
        Some(&self.api_url)
    }

    fn yields(&self) -> &[CategoryTerm] {
        &[]
    }

    fn client(&self) -> &reqwest::Client {
        &self.client
    }

    fn listing_auth(&self) -> BoxFuture<'_, Result<Authorization, SourceError>> {
        Box::pin(async { Ok(Authorization::Bearer(String::from("a-token"))) })
    }

    fn download_auth(&self) -> BoxFuture<'_, Result<Authorization, SourceError>> {
        Box::pin(async { Ok(Authorization::Bearer(String::from("a-token"))) })
    }
}

fn bundle(entries: &[serde_json::Value], next: Option<&str>) -> serde_json::Value {
    let mut links = vec![serde_json::json!({"relation": "self", "url": "self"})];
    if let Some(next) = next {
        links.push(serde_json::json!({"relation": "next", "url": next}));
    }
    serde_json::json!({
        "resourceType": "Bundle",
        "type": "searchset",
        "link": links,
        "entry": entries.iter().map(|resource| serde_json::json!({"resource": resource})).collect::<Vec<_>>()
    })
}

/// Mounts a two-page `ValueSet` search, empty `CodeSystem` and `ConceptMap`
/// searches, and the resource behind each listed value set.
async fn service() -> MockServer {
    let server = MockServer::start().await;
    let base = format!("{}/fhir", server.uri());
    let first = serde_json::json!({
        "resourceType": "ValueSet", "id": "a1", "url": "https://example.invalid/ValueSet/a",
        "version": "1", "title": "A", "meta": {"lastUpdated": "2026-01-01T00:00:00Z"}
    });
    let second = serde_json::json!({
        "resourceType": "ValueSet", "id": "b1", "url": "https://example.invalid/ValueSet/b",
        "version": "1", "name": "B", "meta": {"lastUpdated": "2026-02-01T00:00:00Z"}
    });
    Mock::given(method("GET"))
        .and(path("/fhir/ValueSet"))
        .and(query_param("_summary", "true"))
        .and(query_param_is_missing("_page"))
        .and(header("accept", "application/fhir+json"))
        .and(header("authorization", "Bearer a-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bundle(
            &[first],
            Some(&format!("{base}/ValueSet?_summary=true&_count=500&_page=2")),
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/fhir/ValueSet"))
        .and(query_param("_page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bundle(&[second], None)))
        .mount(&server)
        .await;
    for kind in ["CodeSystem", "ConceptMap"] {
        Mock::given(method("GET"))
            .and(path(format!("/fhir/{kind}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(bundle(&[], None)))
            .mount(&server)
            .await;
    }
    for id in ["a1", "b1"] {
        Mock::given(method("GET"))
            .and(path(format!("/fhir/ValueSet/{id}")))
            .and(header("accept", "application/fhir+json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(format!(r#"{{"resourceType":"ValueSet","id":"{id}"}}"#)),
            )
            .mount(&server)
            .await;
    }
    server
}

fn source(server: &MockServer) -> ApiSource {
    ApiSource {
        client: reqwest::Client::new(),
        feed_url: format!("{}/synd/syndication.xml", server.uri()),
        api_url: format!("{}/fhir/", server.uri()),
    }
}

#[tokio::test]
async fn the_api_is_listed_across_every_page_as_feed_entries() {
    let server = service().await;
    let feed = source(&server).list_api().await.expect("the API lists");
    let ids: Vec<&str> = feed.entries.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["ValueSet/a1", "ValueSet/b1"],
        "both pages are read, in order, and the empty types add nothing"
    );
    assert!(
        feed.entries
            .iter()
            .all(|entry| entry.term() == CategoryTerm::FhirValueSet),
        "each entry carries the FHIR category of its type"
    );
    let selection = terminology_syndication::select::select(
        &feed,
        &Subscription::new().with_category(CategoryTerm::FhirValueSet),
        &Holdings::new(),
    );
    assert_eq!(
        selection.taken.len(),
        2,
        "the selection reads API entries as it reads the feed"
    );
}

#[tokio::test]
async fn the_api_lists_nothing_for_a_source_without_one() {
    #[derive(Debug)]
    struct FeedOnly(reqwest::Client);
    impl Source for FeedOnly {
        fn name(&self) -> &'static str {
            "feed-only"
        }
        fn feed_url(&self) -> &'static str {
            "https://example.invalid/synd/syndication.xml"
        }
        fn yields(&self) -> &[CategoryTerm] {
            &[]
        }
        fn client(&self) -> &reqwest::Client {
            &self.0
        }
        fn listing_auth(&self) -> BoxFuture<'_, Result<Authorization, SourceError>> {
            Box::pin(async { Ok(Authorization::Open) })
        }
        fn download_auth(&self) -> BoxFuture<'_, Result<Authorization, SourceError>> {
            Box::pin(async { Ok(Authorization::Open) })
        }
    }
    let listed = FeedOnly(reqwest::Client::new()).list_api().await;
    assert!(
        matches!(listed, Err(SourceError::NoFhirApi { .. })),
        "a source without an API says so: {listed:?}"
    );
}

#[tokio::test]
async fn a_page_the_server_refuses_fails_the_listing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/CodeSystem"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    let listed = source(&server).list_api().await;
    assert!(
        matches!(listed, Err(SourceError::Status { .. })),
        "a refused page is a failed listing, never an empty one: {listed:?}"
    );
}

#[tokio::test]
async fn a_listed_resource_is_fetched_and_its_digest_computed() {
    let server = service().await;
    let dir = tempfile::tempdir().expect("a directory");
    let destination = dir.path().join("a.json");
    let link = ContentLink {
        href: format!("{}/fhir/ValueSet/a1", server.uri()),
        ..ContentLink::default()
    };
    let Fetched {
        path: written,
        bytes,
        checksum,
    } = source(&server)
        .fetch_api(&link, &destination)
        .await
        .expect("the resource is fetched");
    assert_eq!(written, destination, "the file lands where it was asked to");
    let body = std::fs::read(&written).expect("the file reads");
    assert_eq!(
        bytes,
        u64::try_from(body.len()).expect("a length"),
        "every byte is counted"
    );
    assert_eq!(
        checksum,
        Checksum::Sha256(sha256(&body)),
        "the digest of what arrived is recorded, since the API advertised none"
    );
    assert!(
        !Path::new(&format!("{}.part", destination.display())).exists(),
        "no partial file is left behind"
    );
}

#[tokio::test]
async fn a_subscribed_canonical_no_listing_carries_is_named() {
    let server = service().await;
    let api = source(&server).list_api().await.expect("the API lists");
    let subscription = Subscription::new()
        .with_system("https://example.invalid/ValueSet/a")
        .with_system("http://loinc.org")
        .with_system("http://snomed.info/sct/11000146104");
    let feed = Feed {
        entries: vec![terminology_syndication::model::Entry {
            content_item_identifier: Some(String::from("http://snomed.info/sct")),
            content_item_version: Some(String::from(
                "http://snomed.info/sct/11000146104/version/20260831",
            )),
            ..Default::default()
        }],
        ..Feed::default()
    };
    assert_eq!(
        unseen(&subscription, &[&feed, &api]),
        vec![String::from("http://loinc.org")],
        "the value set is on the API, the edition is on the feed, and LOINC is on neither"
    );
    assert!(
        unseen(&Subscription::new(), &[&feed, &api]).is_empty(),
        "a subscription to every system has nothing to miss"
    );
}

#[tokio::test]
async fn a_code_system_without_content_is_left_behind_with_its_term() {
    let server = MockServer::start().await;
    let stub = serde_json::json!({
        "resourceType": "CodeSystem", "id": "s", "url": "http://snomed.info/sct",
        "version": "http://snomed.info/sct/11000146104/version/20260831",
        "content": "not-present", "meta": {"lastUpdated": "2026-08-31T00:00:00Z"}
    });
    Mock::given(method("GET"))
        .and(path("/fhir/CodeSystem"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bundle(&[stub], None)))
        .mount(&server)
        .await;
    for kind in ["ValueSet", "ConceptMap"] {
        Mock::given(method("GET"))
            .and(path(format!("/fhir/{kind}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(bundle(&[], None)))
            .mount(&server)
            .await;
    }
    let feed = source(&server).list_api().await.expect("the API lists");
    let selection = terminology_syndication::select::select(
        &feed,
        &Subscription::new().with_category(CategoryTerm::FhirCodeSystem),
        &Holdings::new(),
    );
    assert!(selection.taken.is_empty(), "a stub is never taken");
    assert_eq!(
        selection.skipped.first().map(|skipped| &skipped.reason),
        Some(&SkipReason::CategoryNotSubscribed {
            term: CategoryTerm::Other(String::from("FHIR_CodeSystem(content=not-present)"))
        }),
        "the record says what it was: {:?}",
        selection.skipped
    );
}

fn sha256(bytes: &[u8]) -> String {
    use core::fmt::Write as _;
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut out, byte| {
            write!(out, "{byte:02x}").expect("writing into a String cannot fail");
            out
        })
}
