// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credential of the reload call: none, or an OAuth 2.0 client
//! credentials token (RFC 6749 §4.4) that `ferrohealth-smart` fetches and
//! renews once when the server answers `401`, written under
//! `[server_admin.credentials]`. The reload is a machine's call, so HTTP
//! Basic, a person's credential, is refused at load.
//!
//! No specification governs the reload call: our own design (#750).

use std::collections::BTreeMap;

use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_smart::client::outbound::SendError;
use ferrohealth_smart::config::credentials::{CredentialsSection, OauthSection};
use ferrohealth_smart::config::endpoint::EndpointSection;
use ferroterm_sync::config::{Config, ConfigError};
use ferroterm_sync::reload::{AdminClient, ReloadError};
use secrecy::SecretString;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The token endpoint of `server` issuing `token`.
async fn issues(server: &MockServer, token: &str) {
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=client_credentials"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": token,
            "token_type": "Bearer",
            "expires_in": 300,
        })))
        .mount(server)
        .await;
}

/// The admin client of `server` under `credentials`.
fn client(server: &MockServer, credentials: CredentialsSection) -> AdminClient {
    AdminClient::new(&EndpointSection {
        url: SecretUrl::new(server.uri()),
        credentials,
        ..EndpointSection::default()
    })
    .expect("the client builds")
}

/// The client credentials table against the token endpoint of `server`.
fn oauth(server: &MockServer) -> CredentialsSection {
    CredentialsSection {
        oauth: Some(OauthSection {
            token_endpoint: SecretUrl::new(format!("{}/token", server.uri())),
            client_id: String::from("ferroterm-sync"),
            client_secret: Some(SecretString::from("synthetic-secret")),
            scope: String::from("__ferroterm.admin"),
            ..OauthSection::default()
        }),
        ..CredentialsSection::default()
    }
}

#[tokio::test]
async fn the_reload_carries_a_client_credentials_token() {
    let server = MockServer::start().await;
    issues(&server, "synthetic-token").await;
    Mock::given(method("POST"))
        .and(path("/reload"))
        .and(header("authorization", "Bearer synthetic-token"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"outcome\":\"ok\"}"))
        .expect(1)
        .mount(&server)
        .await;

    let reply = client(&server, oauth(&server))
        .reload()
        .await
        .expect("the server reloads");
    assert_eq!(reply.status, 200);
    let fetched = server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/token")
        .count();
    assert_eq!(fetched, 1, "one token for one call");
}

// RFC 6750 §3.1: `401` answers an invalid token, so the token is renewed once
// and a second refusal reaches the run as the refusal it is.
#[tokio::test]
async fn a_reload_refused_twice_is_a_refused_reload() {
    let server = MockServer::start().await;
    issues(&server, "synthetic-token").await;
    Mock::given(method("POST"))
        .and(path("/reload"))
        .respond_with(ResponseTemplate::new(401).set_body_string("refused"))
        .expect(2)
        .mount(&server)
        .await;

    let error = client(&server, oauth(&server))
        .reload()
        .await
        .expect_err("the server refuses");
    assert!(
        matches!(&error, ReloadError::Refused { status: 401, body, .. } if body == "refused"),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_token_endpoint_that_refuses_is_a_failed_reload() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(serde_json::json!({"error": "invalid_client"})),
        )
        .mount(&server)
        .await;

    let error = client(&server, oauth(&server))
        .reload()
        .await
        .expect_err("no token, no reload");
    assert!(
        matches!(&error, ReloadError::Request { source, .. } if matches!(**source, SendError::Token(_))),
        "{error:?}"
    );
    let reloads = server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/reload")
        .count();
    assert_eq!(reloads, 0, "the server is never asked without a token");
}

/// The refusal of the sync configuration `text`, rendered.
fn refused(text: &str) -> String {
    Config::from_sources(Some(text), &BTreeMap::new())
        .expect_err("the table is refused at load")
        .to_string()
}

// The reload is a machine's call, so it carries a client credentials token
// (RFC 6749 §4.4); the Basic form, a person's credential, is refused at load
// naming the one key the table takes (#1140).
#[test]
fn the_basic_form_is_refused_at_load_naming_oauth() {
    for text in [
        "[server_admin.credentials]\nuser = \"sync\"\npassword = \"synthetic-password\"\n",
        "[server_admin.credentials]\npassword_file = \"/run/secrets/sync\"\n",
    ] {
        let refusal = Config::from_sources(Some(text), &BTreeMap::new())
            .expect_err("the table is refused at load");
        assert!(
            matches!(refusal, ConfigError::BasicReload),
            "{text}: {refusal}"
        );
        assert!(refusal.to_string().contains("oauth"), "{text}: {refusal}");
    }
}

#[test]
fn a_static_bearer_token_is_no_credential() {
    let reason = refused("[server_admin.credentials]\ntoken = \"static\"\n");
    assert!(reason.contains("token"), "{reason}");
}

#[test]
fn the_client_credentials_table_reads_from_the_file() {
    let config = Config::from_sources(
        Some(
            "[server_admin.credentials.oauth]\ntoken_endpoint = \"https://as.example.org/token\"\nclient_id = \"ferroterm-sync\"\nclient_secret_file = \"/run/secrets/sync\"\n",
        ),
        &BTreeMap::new(),
    )
    .expect("the table loads");
    let oauth = config
        .server_admin
        .credentials
        .oauth
        .expect("the client credentials table");
    assert_eq!(oauth.client_id, "ferroterm-sync");
}

/// An ML-DSA-65 private key as PKCS#8 PEM in its seed form (RFC 9881 §6),
/// the file `openssl genpkey -algorithm ML-DSA-65` writes, from a fixed seed.
fn ml_dsa_65_pem() -> String {
    use base64::Engine as _;
    let mut der = vec![
        0x30, 0x34, 0x02, 0x01, 0x00, 0x30, 0x0b, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03,
        0x04, 0x03, 0x12, 0x04, 0x22, 0x80, 0x20,
    ];
    der.extend_from_slice(&[7; 32]);
    let body = base64::engine::general_purpose::STANDARD.encode(der);
    format!("-----BEGIN PRIVATE KEY-----\n{body}\n-----END PRIVATE KEY-----\n")
}

// RFC 9449 §4.2 asks only for an asymmetric algorithm, and RFC 9964 names
// ML-DSA for JOSE: an ML-DSA DPoP key signs every proof with its parameter set.
#[tokio::test]
async fn an_ml_dsa_dpop_key_signs_its_proofs_with_ml_dsa() {
    use base64::Engine as _;
    let dir = tempfile::tempdir().expect("tempdir");
    let key = dir.path().join("sync-dpop.pem");
    std::fs::write(&key, ml_dsa_65_pem()).expect("the key is written");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "synthetic-token",
            "token_type": "DPoP",
            "expires_in": 300,
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/reload"))
        .and(header("authorization", "DPoP synthetic-token"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"outcome\":\"ok\"}"))
        .expect(1)
        .mount(&server)
        .await;
    let mut credentials = oauth(&server);
    if let Some(oauth) = credentials.oauth.as_mut() {
        oauth.dpop_key_file = Some(key);
    }

    client(&server, credentials)
        .reload()
        .await
        .expect("the server reloads");
    let requests = server.received_requests().await.unwrap_or_default();
    assert_eq!(requests.len(), 2, "one token request and one reload");
    for request in &requests {
        let proof = request
            .headers
            .get("dpop")
            .and_then(|value| value.to_str().ok())
            .expect("every call carries a DPoP proof");
        let header = proof.split('.').next().expect("a compact JWS");
        let header: serde_json::Value = serde_json::from_slice(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(header)
                .expect("base64url"),
        )
        .expect("a JSON header");
        assert_eq!(header["alg"], "ML-DSA-65", "{}", request.url.path());
        assert_eq!(header["typ"], "dpop+jwt", "{}", request.url.path());
        assert_eq!(header["jwk"]["kty"], "AKP", "{}", request.url.path());
    }
}
