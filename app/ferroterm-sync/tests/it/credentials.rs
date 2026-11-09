// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credential of the reload call: none, or an OAuth 2.0 client
//! credentials token (RFC 6749 §4.4) that `ferrohealth-smart` fetches and
//! renews once when the server answers `401`. The reload is a machine's call,
//! so HTTP Basic, a person's credential, is refused at load.
//!
//! No specification governs the reload call: our own design (#750).

use std::collections::BTreeMap;

use ferrohealth_smart::config::credentials::OauthSection;
use ferroterm_sync::config::{AdminCredentials, Config};
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

/// The admin client of `server` under `section`.
fn client(server: &MockServer, section: &AdminCredentials) -> AdminClient {
    let grant = section
        .resolve()
        .expect("the table resolves")
        .expect("the table names a credential");
    AdminClient::new(reqwest::Client::new(), &server.uri())
        .with_credentials(grant)
        .expect("the token client builds")
}

/// The client credentials table against the token endpoint of `server`.
fn oauth(server: &MockServer) -> AdminCredentials {
    AdminCredentials {
        oauth: Some(OauthSection {
            token_endpoint: format!("{}/token", server.uri()),
            client_id: String::from("ferroterm-sync"),
            client_secret: Some(SecretString::from("synthetic-secret")),
            scope: String::from("__ferroterm.admin"),
            ..OauthSection::default()
        }),
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

    let reply = client(&server, &oauth(&server))
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

    let error = client(&server, &oauth(&server))
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

    let error = client(&server, &oauth(&server))
        .reload()
        .await
        .expect_err("no token, no reload");
    assert!(matches!(error, ReloadError::Token { .. }), "{error:?}");
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
// (RFC 6749 §4.4); the Basic form of earlier releases, a person's credential,
// is refused at load naming the one key the table takes (#1140).
#[test]
fn the_basic_form_is_refused_at_load_naming_oauth() {
    for text in [
        "[server_admin_credentials]\nuser = \"sync\"\npassword = \"synthetic-password\"\n",
        "[server_admin_credentials]\npassword_file = \"/run/secrets/sync\"\n",
    ] {
        let reason = refused(text);
        assert!(reason.contains("oauth"), "{text}: {reason}");
    }
}

#[test]
fn a_static_bearer_token_is_no_credential() {
    let reason = refused("[server_admin_credentials]\ntoken = \"static\"\n");
    assert!(reason.contains("oauth"), "{reason}");
}

#[test]
fn the_client_credentials_table_reads_from_the_file() {
    let config = Config::from_sources(
        Some(
            "[server_admin_credentials.oauth]\ntoken_endpoint = \"https://as.example.org/token\"\nclient_id = \"ferroterm-sync\"\nclient_secret_file = \"/run/secrets/sync\"\n",
        ),
        &BTreeMap::new(),
    )
    .expect("the table loads");
    let oauth = config
        .server_admin_credentials
        .oauth
        .expect("the client credentials table");
    assert_eq!(oauth.client_id, "ferroterm-sync");
}
