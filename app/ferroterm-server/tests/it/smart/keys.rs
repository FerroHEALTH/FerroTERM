// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The issuer key set and the checks that refuse a start.

use ferroterm_server::config::{AdminSection, Config, TerminologySection};
use ferroterm_server::smart::Smart;
use http::StatusCode;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{SigningKey, causes, claims, discovery, put_colours};
use crate::fixture::{Server, SmartSetup};

/// The issuer of `mock` publishing `first` once, then `first` and `second`.
async fn rotating_issuer(mock: &MockServer, first: &SigningKey, second: &SigningKey) {
    let base = mock.uri();
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(discovery(&base)))
        .mount(mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"keys": [first.jwk()]})))
        .up_to_n_times(1)
        .mount(mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"keys": [first.jwk(), second.jwk()]})),
        )
        .mount(mock)
        .await;
}

/// A server over `mock` that reads its key set again at most once per
/// `interval_seconds`.
async fn with_refresh_interval(mock: &MockServer, interval_seconds: u64) -> Server {
    let mut setup = SmartSetup::for_issuer(&mock.uri());
    setup.issuer_mut().jwks_refresh_interval =
        ferrohealth_config::units::Duration::from_secs(interval_seconds);
    Server::start_persisting_with_smart_setup(setup).await
}

// RFC 7517 §4.5: a rotated key arrives under a new `kid`, so a token naming one
// the server has not read is the signal to read the set again.
#[tokio::test]
async fn an_unknown_key_id_refreshes_the_key_set() {
    let first = SigningKey::generate("k1");
    let second = SigningKey::generate("k2");
    let mock = MockServer::start().await;
    rotating_issuer(&mock, &first, &second).await;
    let server = with_refresh_interval(&mock, 0).await;
    assert_eq!(jwks_reads(&mock).await, 1, "the start reads the set once");

    let rotated = second.sign(&claims(&mock.uri(), "system/CodeSystem.cud", 300));
    let response = put_colours(&server, Some(&rotated)).await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "the refreshed set carries the new key"
    );
    assert_eq!(jwks_reads(&mock).await, 2, "the unknown kid read it again");
}

// A stream of forged `kid`s must not turn the server into a load on the
// issuer, so a second unknown `kid` inside the refresh interval reads nothing
// (no specification governs the interval: `ferrohealth-smart`'s own design,
// measured from the last read, the one at start included).
#[tokio::test]
async fn an_unknown_key_id_inside_the_interval_reads_nothing_and_is_refused() {
    let first = SigningKey::generate("k1");
    let second = SigningKey::generate("k2");
    let mock = MockServer::start().await;
    rotating_issuer(&mock, &first, &second).await;
    let server = with_refresh_interval(&mock, 3600).await;

    let third = SigningKey::generate("k3");
    let unknown = third.sign(&claims(&mock.uri(), "system/CodeSystem.cud", 300));
    let response = put_colours(&server, Some(&unknown)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        jwks_reads(&mock).await,
        1,
        "the interval holds the next refresh off"
    );
}

/// How many times the issuer's key set was read.
async fn jwks_reads(mock: &MockServer) -> usize {
    mock.received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/jwks")
        .count()
}

/// The configuration of a server over the edition in `dir` whose `[auth]`
/// trusts `issuer`.
fn trusting(dir: &std::path::Path, issuer: &str) -> Config {
    Server::smart_config(dir, SmartSetup::for_issuer(issuer))
}

#[tokio::test]
async fn a_discovery_failure_refuses_the_start() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let error = Smart::start(&trusting(dir.path(), &mock.uri()))
        .await
        .expect_err("the start is refused");
    let reason = format!("{error}");
    assert!(
        reason.contains(&mock.uri()),
        "the reason names the issuer that did not answer: {reason}"
    );
    assert!(
        std::error::Error::source(&error).is_some(),
        "the refusal carries its cause"
    );
}

// RFC 8414 §2 and OpenID Connect Discovery 1.0 §3: the issuer is an https URL,
// because these bytes decide which signatures the server trusts.
#[tokio::test]
async fn a_cleartext_issuer_outside_the_loopback_refuses_the_start() {
    let dir = tempfile::tempdir().expect("tempdir");
    let error = Smart::start(&trusting(dir.path(), "http://auth.example.org/realms/tx"))
        .await
        .expect_err("the start is refused");
    let chain = causes(&error);
    assert!(chain.contains("not an https URL"), "{chain}");
}

// The family's fail-closed rule (no specification governs it: our own design):
// a server that serves writes or the admin listener starts only once `[auth]`
// names an issuer, or says it is off.
#[tokio::test]
async fn a_guarded_surface_without_an_issuer_refuses_the_start() {
    for (resources, admin, named) in [
        (true, false, "terminology.resources"),
        (false, true, "admin.listen"),
    ] {
        let config = Config {
            terminology: TerminologySection {
                resources: resources.then(|| std::path::PathBuf::from("resources.redb")),
                ..TerminologySection::default()
            },
            admin: AdminSection {
                listen: admin.then(|| String::from("127.0.0.1:0")),
            },
            ..Config::default()
        };
        let error = Smart::start(&config)
            .await
            .expect_err("the start is refused");
        let reason = error.to_string();
        assert!(reason.contains(named), "{reason}");
        assert!(reason.contains("[[auth.issuer]]"), "{reason}");
    }
}
