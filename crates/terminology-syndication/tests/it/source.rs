//! The `Source` seam against a mock syndication service.
//!
//! Each test stands up one of the authentication shapes the public services
//! use: a bearer challenge on the listing itself, a public listing whose
//! downloads are challenged, an open service, and affiliate credentials.

use terminology_syndication::model::{CategoryTerm, Checksum, ContentLink, LinkRel};
use terminology_syndication::source::{Authorization, Source, SourceError};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::fixtures;
use crate::reference::{NoAuthorization, ReferenceSource, UnauthenticatedSource};

/// The digest of the four bytes every content mock serves.
const BODY: &str = "unit";
const BODY_SHA256: &str = "b9a3a5ec8d5e2b3b3ffec9a2e5a0f1d1a0f1c2ed4f78b2e8b0f5b8f8bcb9f2f1";

fn feed_xml() -> String {
    fixtures::synthetic("full-feed.xml")
}

fn atom(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body, "application/atom+xml")
}

/// The digest of the served body.
fn body_sha256() -> String {
    crate::digest::sha256_of(BODY)
}

fn link(href: String, checksum: Option<Checksum>) -> ContentLink {
    ContentLink {
        href,
        rel: LinkRel::Alternate,
        media_type: Some(String::from("application/zip")),
        length: Some(4),
        checksum,
        validated: false,
    }
}

#[tokio::test]
async fn a_listing_behind_a_bearer_challenge_is_read_with_the_listing_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/synd/syndication.xml"))
        .and(header("authorization", "Bearer listing-token"))
        .respond_with(atom(feed_xml()))
        .mount(&server)
        .await;

    let source = ReferenceSource::open(
        "reference",
        &format!("{}/synd/syndication.xml", server.uri()),
    )
    .with_listing_auth(Authorization::Bearer(String::from("listing-token")));
    let feed = source.list().await.expect("the listing is served");
    assert_eq!(feed.entries.len(), 13);
}

#[tokio::test]
async fn a_public_listing_is_read_without_a_token_and_its_download_carries_one() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/synd/syndication.xml"))
        .and(NoAuthorization)
        .respond_with(atom(feed_xml()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/content/item.zip"))
        .and(header("authorization", "Bearer download-token"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BODY))
        .mount(&server)
        .await;

    let source = ReferenceSource::open(
        "reference",
        &format!("{}/synd/syndication.xml", server.uri()),
    )
    .with_download_auth(Authorization::Bearer(String::from("download-token")));
    let feed = source.list().await.expect("the listing is public");
    assert_eq!(feed.entries.len(), 13);

    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("item.zip");
    let content = link(
        format!("{}/content/item.zip", server.uri()),
        Some(Checksum::Sha256(body_sha256())),
    );
    let fetched = source
        .fetch(&content, &destination)
        .await
        .expect("the download is authorized");
    assert_eq!(fetched.bytes, 4);
    assert_eq!(
        std::fs::read_to_string(&destination).expect("the file exists"),
        BODY
    );
}

#[tokio::test]
async fn an_open_service_needs_no_credentials_at_all() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/synd/syndication.xml"))
        .and(NoAuthorization)
        .respond_with(atom(feed_xml()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/content/item.zip"))
        .and(NoAuthorization)
        .respond_with(ResponseTemplate::new(200).set_body_string(BODY))
        .mount(&server)
        .await;

    let source = ReferenceSource::open(
        "reference",
        &format!("{}/synd/syndication.xml", server.uri()),
    );
    assert_ne!(
        source.list().await.expect("the listing is open").entries,
        Vec::<terminology_syndication::model::Entry>::new()
    );
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("item.zip");
    let content = link(
        format!("{}/content/item.zip", server.uri()),
        Some(Checksum::Sha256(body_sha256())),
    );
    source
        .fetch(&content, &destination)
        .await
        .expect("the download is open");
}

#[tokio::test]
async fn affiliate_credentials_travel_as_http_basic() {
    let server = MockServer::start().await;
    // "affiliate:secret" base64-encoded, the RFC 7617 credentials form.
    Mock::given(method("GET"))
        .and(path("/api/feed"))
        .and(header("authorization", "Basic YWZmaWxpYXRlOnNlY3JldA=="))
        .respond_with(atom(feed_xml()))
        .mount(&server)
        .await;

    let source = ReferenceSource::open("reference", &format!("{}/api/feed", server.uri()))
        .with_listing_auth(Authorization::Basic {
            user: String::from("affiliate"),
            password: String::from("secret"),
        });
    assert_ne!(
        source.list().await.expect("the listing is served").entries,
        Vec::<terminology_syndication::model::Entry>::new()
    );
}

#[tokio::test]
async fn the_listing_and_the_download_authenticate_separately() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/synd/syndication.xml"))
        .and(header("authorization", "Bearer listing-token"))
        .respond_with(atom(feed_xml()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/content/item.zip"))
        .and(header("authorization", "Bearer download-token"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BODY))
        .mount(&server)
        .await;

    let source = ReferenceSource::open(
        "reference",
        &format!("{}/synd/syndication.xml", server.uri()),
    )
    .with_listing_auth(Authorization::Bearer(String::from("listing-token")))
    .with_download_auth(Authorization::Bearer(String::from("download-token")));
    source.list().await.expect("the listing token is used");
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("item.zip");
    let content = link(
        format!("{}/content/item.zip", server.uri()),
        Some(Checksum::Sha256(body_sha256())),
    );
    source
        .fetch(&content, &destination)
        .await
        .expect("the download token is used");
}

#[tokio::test]
async fn a_refused_listing_reports_the_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/synd/syndication.xml"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let source = ReferenceSource::open(
        "reference",
        &format!("{}/synd/syndication.xml", server.uri()),
    );
    let error = source.list().await.expect_err("the listing is refused");
    match error {
        SourceError::Status { status, .. } => {
            assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
        }
        other => panic!("expected Status, got {other:?}"),
    }
}

#[tokio::test]
async fn a_listing_that_is_not_a_feed_reports_where_it_came_from() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/synd/syndication.xml"))
        .respond_with(atom(fixtures::synthetic("not-a-feed.xml")))
        .mount(&server)
        .await;

    let source = ReferenceSource::open(
        "reference",
        &format!("{}/synd/syndication.xml", server.uri()),
    );
    let error = source
        .list()
        .await
        .expect_err("an RSS document is not a feed");
    match error {
        SourceError::Parse { url, .. } => assert!(url.ends_with("/synd/syndication.xml")),
        other => panic!("expected Parse, got {other:?}"),
    }
}

#[tokio::test]
async fn a_source_that_cannot_authenticate_reports_its_own_cause() {
    let source = UnauthenticatedSource::new("https://example.invalid/synd/syndication.xml");
    let error = source
        .list()
        .await
        .expect_err("the credential store is empty");
    match &error {
        SourceError::Authentication { source_name, .. } => assert_eq!(source_name, "reference"),
        other => panic!("expected Authentication, got {other:?}"),
    }
    let cause = core::error::Error::source(&error).expect("the error carries its cause");
    assert_eq!(cause.to_string(), "the credential store is empty");
}

#[tokio::test]
async fn an_entry_with_no_checksum_is_never_fetched() {
    let server = MockServer::start().await;
    let source = ReferenceSource::open(
        "reference",
        &format!("{}/synd/syndication.xml", server.uri()),
    );
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("item.zip");
    let content = link(format!("{}/content/item.zip", server.uri()), None);
    let error = source
        .fetch(&content, &destination)
        .await
        .expect_err("unverifiable bytes are not fetched");
    assert!(
        matches!(error, SourceError::NoChecksum { .. }),
        "expected NoChecksum, got {error:?}"
    );
    assert!(!destination.exists(), "nothing is written");
}

#[tokio::test]
async fn a_fetch_whose_checksum_does_not_match_leaves_nothing_behind() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/content/item.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BODY))
        .mount(&server)
        .await;

    let source = ReferenceSource::open(
        "reference",
        &format!("{}/synd/syndication.xml", server.uri()),
    );
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("item.zip");
    let content = link(
        format!("{}/content/item.zip", server.uri()),
        Some(Checksum::Sha256(String::from(BODY_SHA256))),
    );
    let error = source
        .fetch(&content, &destination)
        .await
        .expect_err("the digest does not match");
    assert!(
        matches!(error, SourceError::Download(_)),
        "expected Download, got {error:?}"
    );
    assert!(!destination.exists(), "the destination is not created");
    assert!(
        !directory.path().join("item.zip.part").exists(),
        "the partial file is removed"
    );
}

#[test]
fn a_caller_holds_its_configured_sources_as_trait_objects() {
    let sources: Vec<Box<dyn Source>> = vec![
        Box::new(ReferenceSource::open(
            "first",
            "https://example.invalid/synd/syndication.xml",
        )),
        Box::new(UnauthenticatedSource::new(
            "https://example.invalid/api/feed",
        )),
    ];
    assert_eq!(
        sources
            .iter()
            .map(|source| source.name())
            .collect::<Vec<_>>(),
        ["first", "reference"]
    );
    assert!(sources[0].yields().contains(&CategoryTerm::FhirCodeSystem));
}

#[test]
fn an_authorization_never_renders_its_secret() {
    let bearer = Authorization::Bearer(String::from("super-secret-token"));
    assert_eq!(format!("{bearer:?}"), "Authorization(bearer)");
    let basic = Authorization::Basic {
        user: String::from("affiliate"),
        password: String::from("super-secret-password"),
    };
    assert_eq!(format!("{basic:?}"), "Authorization(basic)");
    assert_eq!(format!("{:?}", Authorization::Open), "Authorization(open)");
}
