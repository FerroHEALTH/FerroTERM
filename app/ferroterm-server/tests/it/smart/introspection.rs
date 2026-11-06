// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Token introspection for an issuer that publishes an endpoint.

use ferrohealth_smart::config::{AuthMode, IntrospectionSection, OauthSection};
use ferroterm_server::config::{AdminSection, Config, TerminologySection};
use ferroterm_server::smart::{Smart, StartError};
use http::StatusCode;
use secrecy::SecretString;
use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{
    PROXIED, SigningKey, capabilities, causes, challenge, claims, configuration, discovery, issuer,
    issuer_publishing, put_colours, sparse_discovery,
};
use crate::fixture::{self, Server, SmartSetup};

/// A `wiremock` issuer publishing the discovery document, whose
/// `introspection_endpoint` is `/introspect`, with nothing mounted there yet.
async fn introspecting_issuer() -> MockServer {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(discovery(&base)))
        .mount(&server)
        .await;
    server
}

/// The `[auth]` of a server introspecting the tokens of `issuer` under a
/// client secret.
fn introspecting(issuer: &str) -> SmartSetup {
    let mut setup = SmartSetup::for_issuer(issuer);
    setup.issuer_mut().mode = AuthMode::Introspection;
    setup.issuer_mut().introspection = Some(IntrospectionSection {
        client: Some(OauthSection {
            client_id: String::from("ferroterm"),
            client_secret: Some(SecretString::from("a-synthetic-secret")),
            ..OauthSection::default()
        }),
        ..IntrospectionSection::default()
    });
    setup
}

/// The introspection endpoint of `issuer` answers `body` with `status` for
/// `token`.
async fn introspects(issuer: &MockServer, token: &str, status: u16, body: Value) {
    Mock::given(method("POST"))
        .and(path("/introspect"))
        .and(body_string_contains(format!("token={token}")))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(issuer)
        .await;
}

/// An active introspection response (RFC 7662 §2.2) for `issuer` carrying
/// `scope`, with the members SMART App Launch 2.2.0, Token Introspection,
/// requires.
fn active(issuer: &str, scope: &str) -> Value {
    let mut response = claims(issuer, scope, 300);
    if let Value::Object(members) = &mut response {
        members.insert(String::from("active"), json!(true));
    }
    response
}

// RFC 7662 §2.2: an active response admits the token with the scope it names,
// so a write scope writes and a read scope is refused `403`.
#[tokio::test]
async fn an_active_introspected_token_is_admitted_with_its_scopes() {
    let mock = introspecting_issuer().await;
    let base = mock.uri();
    introspects(
        &mock,
        "writer-token",
        200,
        active(&base, "system/CodeSystem.cud"),
    )
    .await;
    introspects(
        &mock,
        "reader-token",
        200,
        active(&base, "system/CodeSystem.rs"),
    )
    .await;
    let server = Server::start_persisting_with_smart_setup(introspecting(&base)).await;

    let response = put_colours(&server, Some("reader-token")).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let (status, body) = fixture::json(put_colours(&server, Some("writer-token")).await).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

// RFC 7662 §2.2: a response whose `active` is false refuses the token, which
// RFC 6750 §3.1 answers `401` with `invalid_token`.
#[tokio::test]
async fn an_inactive_introspected_token_is_refused() {
    let mock = introspecting_issuer().await;
    introspects(&mock, "revoked-token", 200, json!({"active": false})).await;
    let server = Server::start_persisting_with_smart_setup(introspecting(&mock.uri())).await;

    let response = put_colours(&server, Some("revoked-token")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let sent = challenge(&response);
    assert!(sent.contains("error=\"invalid_token\""), "{sent}");
}

// An endpoint that cannot answer is an upstream failure the write is answered
// `503` for, never a refusal of the token: our own design over the crate's
// `Rejection::Upstream`.
#[tokio::test]
async fn a_failing_introspection_endpoint_answers_503() {
    let mock = introspecting_issuer().await;
    introspects(&mock, "any-token", 500, json!({})).await;
    let server = Server::start_persisting_with_smart_setup(introspecting(&mock.uri())).await;

    let response = put_colours(&server, Some("any-token")).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let (_, body) = fixture::json(response).await;
    assert_eq!(body["issue"][0]["code"], "transient");
}

// SMART App Launch, Conformance: the discovery document comes from the
// introspecting issuer's metadata as it does from a JWT issuer's.
#[tokio::test]
async fn the_discovery_document_names_the_introspecting_issuer() {
    let mock = introspecting_issuer().await;
    let base = mock.uri();
    let server = Server::start_persisting_with_smart_setup(introspecting(&base)).await;
    let document = configuration(&server, "r4b").await;
    assert_eq!(document["token_endpoint"], format!("{base}/token"));
    assert_eq!(
        document["introspection_endpoint"],
        format!("{base}/introspect")
    );
}

// The introspecting issuer is read at the start, so one whose metadata names
// no introspection endpoint refuses the start by name.
#[tokio::test]
async fn an_issuer_without_an_introspection_endpoint_refuses_the_start() {
    let key = SigningKey::generate("k1");
    let mock = issuer_publishing(&key, sparse_discovery).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let config = Server::smart_config(dir.path(), introspecting(&mock.uri()));
    let error = Smart::start(&config)
        .await
        .expect_err("the start is refused");
    assert!(
        matches!(error, StartError::Introspector { .. }),
        "the refusal names the introspection endpoint: {error:?}"
    );
}

#[tokio::test]
async fn a_read_only_server_or_auth_off_starts_without_an_issuer() {
    let read_only = Smart::start(&Config::default())
        .await
        .expect("a read-only server needs no issuer");
    assert!(read_only.is_none());
    let mut off = Config {
        terminology: TerminologySection {
            resources: Some(std::path::PathBuf::from("resources.redb")),
            ..TerminologySection::default()
        },
        admin: AdminSection {
            listen: Some(String::from("127.0.0.1:0")),
        },
        ..Config::default()
    };
    off.auth.enabled = false;
    let started = Smart::start(&off).await.expect("auth off starts");
    assert!(
        started.is_none(),
        "the guarded surfaces authenticate nobody"
    );
}

// RFC 9068 §4: the resource server validates that `aud` names it, so an issuer
// without an audience refuses the start rather than accepting a token minted
// for any resource server of that issuer (RFC 8725 §3.9).
#[tokio::test]
async fn an_issuer_without_an_audience_refuses_the_start() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let mut setup = SmartSetup::for_issuer(&mock.uri());
    setup.issuer_mut().audience = Vec::new();
    setup.base_url = Some(String::from(PROXIED));
    let config = Server::smart_config(dir.path(), setup);
    let error = Smart::start(&config)
        .await
        .expect_err("the start is refused");
    let chain = causes(&error);
    assert!(chain.contains("audience"), "{chain}");
}

// SMART App Launch: the document lives at `[base]/.well-known/smart-configuration`
// and `token_endpoint`, `grant_types_supported`, and `capabilities` are required.
#[tokio::test]
async fn the_smart_configuration_is_served_from_the_issuer_document() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let base = mock.uri();
    let mut setup = SmartSetup::for_issuer(&base);
    setup.auth.scopes_supported = vec![
        String::from("system/CodeSystem.cud"),
        String::from("system/ValueSet.cud"),
    ];
    let server = Server::start_persisting_with_smart_setup(setup).await;

    for version in ["r4", "r4b", "r5", "r6"] {
        let document = configuration(&server, version).await;
        assert_eq!(document["token_endpoint"], format!("{base}/token"));
        assert_eq!(
            document["authorization_endpoint"],
            format!("{base}/authorize")
        );
        assert_eq!(document["jwks_uri"], format!("{base}/jwks"));
        assert_eq!(document["scopes_supported"][0], "system/CodeSystem.cud");
        // `issuer` is conditional on the `sso-openid-connect` capability, which
        // this server does not claim, so the document omits it.
        assert!(document["issuer"].is_null(), "{version}: {document}");
        // The options each member admits are the ones SMART names; the extras
        // the issuer publishes are dropped, and `plain` is a SHALL NOT.
        assert_eq!(
            document["grant_types_supported"],
            json!(["authorization_code", "client_credentials"]),
            "{version}"
        );
        assert_eq!(
            document["code_challenge_methods_supported"],
            json!(["S256"]),
            "{version}"
        );
        assert_eq!(
            document["token_endpoint_auth_methods_supported"],
            json!(["private_key_jwt"]),
            "{version}"
        );
        let claimed = capabilities(&document);
        assert!(
            claimed
                .iter()
                .any(|value| value == "client-confidential-asymmetric"),
            "the issuer advertises private_key_jwt: {claimed:?}"
        );
        assert!(
            !claimed.iter().any(|value| value == "permission-v2"),
            "the granular scope syntax is refused, so it is not claimed: {claimed:?}"
        );
    }

    // The specification fixes the media type whatever the request asks for.
    let (status, content_type, _) = server
        .get_text(
            "/r4b/.well-known/smart-configuration",
            Some("application/fhir+xml"),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, "application/json");
}
