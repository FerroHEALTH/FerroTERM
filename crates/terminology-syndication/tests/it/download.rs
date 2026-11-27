// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Streaming a content item to disk with its digest verified.

use crate::digest::{md5_of, sha256_of};
use terminology_syndication::download::{self, DownloadError};
use terminology_syndication::model::Checksum;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BODY: &str = "a small content item";

/// A download limit no test item reaches.
const LIMIT: u64 = 1 << 20;

async fn serving(body: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/content/item.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn a_matching_sha256_writes_the_destination() {
    let server = serving(BODY).await;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("item.zip");
    let request = reqwest::Client::new().get(format!("{}/content/item.zip", server.uri()));
    let expected = Checksum::Sha256(sha256_of(BODY));

    let fetched = download::to_file(request, &[], &destination, Some(&expected), LIMIT)
        .await
        .expect("the digest matches");
    assert_eq!(fetched.path, destination);
    assert_eq!(
        fetched.bytes,
        u64::try_from(BODY.len()).expect("a small body")
    );
    assert_eq!(fetched.checksum, expected);
    assert_eq!(
        std::fs::read_to_string(&destination).expect("the file exists"),
        BODY
    );
    assert!(
        !directory.path().join("item.zip.part").exists(),
        "the partial file is renamed away"
    );
}

#[tokio::test]
async fn a_matching_md5_writes_the_destination() {
    let server = serving(BODY).await;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("item.zip");
    let request = reqwest::Client::new().get(format!("{}/content/item.zip", server.uri()));
    let expected = Checksum::Md5(md5_of(BODY));

    let fetched = download::to_file(request, &[], &destination, Some(&expected), LIMIT)
        .await
        .expect("the digest matches");
    assert_eq!(fetched.checksum.algorithm(), "md5");
    assert_eq!(
        std::fs::read_to_string(&destination).expect("the file exists"),
        BODY
    );
}

#[tokio::test]
async fn an_uppercase_digest_in_the_feed_still_matches() {
    let server = serving(BODY).await;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("item.zip");
    let request = reqwest::Client::new().get(format!("{}/content/item.zip", server.uri()));
    let expected = Checksum::Sha256(sha256_of(BODY).to_ascii_uppercase());

    download::to_file(request, &[], &destination, Some(&expected), LIMIT)
        .await
        .expect("a digest compares case-insensitively");
}

#[tokio::test]
async fn a_digest_that_does_not_match_removes_the_partial_file() {
    let server = serving(BODY).await;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("item.zip");
    let request = reqwest::Client::new().get(format!("{}/content/item.zip", server.uri()));
    let expected = Checksum::Sha256(sha256_of("a different content item"));

    let error = download::to_file(request, &[], &destination, Some(&expected), LIMIT)
        .await
        .expect_err("the digest does not match");
    match error {
        DownloadError::ChecksumMismatch {
            expected: advertised,
            computed,
            ..
        } => {
            assert_eq!(advertised, expected);
            assert_eq!(computed, sha256_of(BODY));
        }
        other => panic!("expected ChecksumMismatch, got {other:?}"),
    }
    assert!(!destination.exists(), "the destination is never created");
    assert!(
        !directory.path().join("item.zip.part").exists(),
        "the partial file is removed"
    );
}

#[tokio::test]
async fn an_md5_that_does_not_match_removes_the_partial_file() {
    let server = serving(BODY).await;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("item.zip");
    let request = reqwest::Client::new().get(format!("{}/content/item.zip", server.uri()));
    let expected = Checksum::Md5(md5_of("a different content item"));

    let error = download::to_file(request, &[], &destination, Some(&expected), LIMIT)
        .await
        .expect_err("the digest does not match");
    assert!(
        matches!(error, DownloadError::ChecksumMismatch { .. }),
        "expected ChecksumMismatch, got {error:?}"
    );
    assert!(!destination.exists());
    assert!(!directory.path().join("item.zip.part").exists());
}

#[tokio::test]
async fn a_refused_download_writes_nothing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/content/item.zip"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("item.zip");
    let request = reqwest::Client::new().get(format!("{}/content/item.zip", server.uri()));
    let expected = Checksum::Sha256(sha256_of(BODY));

    let error = download::to_file(request, &[], &destination, Some(&expected), LIMIT)
        .await
        .expect_err("the server refuses the download");
    match error {
        DownloadError::Status { status, .. } => {
            assert_eq!(status, reqwest::StatusCode::FORBIDDEN);
        }
        other => panic!("expected Status, got {other:?}"),
    }
    assert!(!destination.exists());
    assert!(!directory.path().join("item.zip.part").exists());
}

#[tokio::test]
async fn a_5xx_is_tried_again_after_each_delay_and_a_4xx_is_not() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/content/item.zip"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/content/item.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BODY))
        .with_priority(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/content/gone.zip"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let delays = [std::time::Duration::from_millis(1)];
    let expected = Checksum::Sha256(sha256_of(BODY));

    let request = reqwest::Client::new().get(format!("{}/content/item.zip", server.uri()));
    let fetched = download::to_file(
        request,
        &delays,
        &directory.path().join("item.zip"),
        Some(&expected),
        LIMIT,
    )
    .await
    .expect("the second try answers");
    assert_eq!(fetched.checksum, expected);

    let request = reqwest::Client::new().get(format!("{}/content/gone.zip", server.uri()));
    let error = download::to_file(
        request,
        &delays,
        &directory.path().join("gone.zip"),
        Some(&expected),
        LIMIT,
    )
    .await
    .expect_err("a 404 is the answer");
    assert!(matches!(error, DownloadError::Status { .. }), "{error:?}");
    let requests = server.received_requests().await.expect("recorded");
    let gone = requests
        .iter()
        .filter(|request| request.url.path() == "/content/gone.zip")
        .count();
    assert_eq!(gone, 1, "a 4xx is not tried again");
}

#[tokio::test]
async fn an_item_past_the_limit_is_refused_and_leaves_no_file() {
    let body = "x".repeat(4096);
    let server = serving(&body).await;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("item.zip");
    let expected = Checksum::Sha256(sha256_of(&body));
    let request = reqwest::Client::new().get(format!("{}/content/item.zip", server.uri()));
    let error = download::to_file(request, &[], &destination, Some(&expected), 1024)
        .await
        .expect_err("4096 bytes pass a limit of 1024");
    assert!(
        matches!(error, DownloadError::TooLarge { limit: 1024, .. }),
        "{error:?}"
    );
    assert!(!destination.exists(), "no destination is written");
    assert!(
        std::fs::read_dir(directory.path())
            .expect("the directory reads")
            .next()
            .is_none(),
        "no partial file is left behind"
    );
}

#[tokio::test]
async fn a_body_with_no_length_is_cut_off_at_the_limit_while_it_streams() {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    // A chunked answer announces no length, so only the streamed count can
    // stop it: four chunks of 512 bytes against a limit of 1024.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a listener");
    let address = listener.local_addr().expect("its address");
    let serve = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("one connection");
        let mut request = [0_u8; 1024];
        let _read = socket.read(&mut request).await.expect("the request");
        let mut answer = String::from(
            "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n",
        );
        let chunk = ["200\r\n", &"z".repeat(512), "\r\n"].concat();
        answer.push_str(&chunk.repeat(4));
        answer.push_str("0\r\n\r\n");
        socket
            .write_all(answer.as_bytes())
            .await
            .expect("the answer");
    });
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("item.zip");
    let expected = Checksum::Sha256(sha256_of(&"z".repeat(2048)));
    let request = reqwest::Client::new().get(format!("http://{address}/content/item.zip"));
    let error = download::to_file(request, &[], &destination, Some(&expected), 1024)
        .await
        .expect_err("the stream passes the limit");
    assert!(
        matches!(error, DownloadError::TooLarge { limit: 1024, .. }),
        "{error:?}"
    );
    assert!(!destination.exists(), "no destination is written");
    assert!(
        std::fs::read_dir(directory.path())
            .expect("the directory reads")
            .next()
            .is_none(),
        "no partial file is left behind"
    );
    serve.abort();
}

#[tokio::test]
async fn an_item_at_the_limit_is_taken() {
    let server = serving(BODY).await;
    let directory = tempfile::tempdir().expect("a temporary directory");
    let destination = directory.path().join("item.zip");
    let expected = Checksum::Sha256(sha256_of(BODY));
    let request = reqwest::Client::new().get(format!("{}/content/item.zip", server.uri()));
    let limit = u64::try_from(BODY.len()).expect("a small length");
    let fetched = download::to_file(request, &[], &destination, Some(&expected), limit)
        .await
        .expect("an item exactly at the limit is taken");
    assert_eq!(fetched.bytes, limit);
}
