//! The credential of the reload call: none, HTTP Basic (RFC 7617), or an
//! OAuth 2.0 client credentials token (RFC 6749 §4.4) that `ferrohealth-smart`
//! fetches and renews once when the server answers `401`.
//!
//! No specification governs the reload call: our own design (#750).

use ferrohealth_smart::config::{CredentialsSection, OauthSection};
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
fn client(server: &MockServer, section: &CredentialsSection) -> AdminClient {
    let outbound = section
        .resolve()
        .expect("the table resolves")
        .expect("the table names a credential");
    AdminClient::new(reqwest::Client::new(), &server.uri())
        .with_credentials(outbound)
        .expect("the token client builds")
}

/// The client credentials table against the token endpoint of `server`.
fn oauth(server: &MockServer) -> CredentialsSection {
    CredentialsSection {
        oauth: Some(OauthSection {
            token_endpoint: format!("{}/token", server.uri()),
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

#[tokio::test]
async fn the_reload_carries_a_basic_credential() {
    let server = MockServer::start().await;
    // RFC 7617 §2: `sync:synthetic-password` in base64.
    Mock::given(method("POST"))
        .and(path("/reload"))
        .and(header(
            "authorization",
            "Basic c3luYzpzeW50aGV0aWMtcGFzc3dvcmQ=",
        ))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let section = CredentialsSection {
        user: String::from("sync"),
        password: Some(SecretString::from("synthetic-password")),
        ..CredentialsSection::default()
    };
    client(&server, &section)
        .reload()
        .await
        .expect("the server reloads");
}

#[test]
fn basic_and_oauth_together_or_a_secret_twice_are_refused() {
    let both = CredentialsSection {
        user: String::from("sync"),
        password: Some(SecretString::from("synthetic-password")),
        oauth: Some(OauthSection::default()),
        ..CredentialsSection::default()
    };
    assert!(both.resolve().is_err(), "the two modes exclude each other");
    let twice = CredentialsSection {
        user: String::from("sync"),
        password: Some(SecretString::from("synthetic-password")),
        password_file: Some(std::path::PathBuf::from("/run/secrets/sync")),
        ..CredentialsSection::default()
    };
    assert!(
        twice.resolve().is_err(),
        "exactly one of the secret and its file"
    );
    let parsed: Result<ferroterm_sync::config::Config, _> =
        toml::from_str("[server_admin_credentials]\ntoken = \"static\"\n");
    assert!(parsed.is_err(), "a static bearer token is no credential");
}
