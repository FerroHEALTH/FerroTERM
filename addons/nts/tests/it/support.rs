// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Shared helpers: a driven clock, the fixture feed, and a mock service.
//!
//! No test reaches the network. The mock service answers the three requests
//! the add-on makes: the SMART configuration document, the token endpoint, and
//! the listing.

use core::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use addon_nts::auth::Clock;
use addon_nts::config::NtsConfig;
use addon_nts::credentials::Credentials;
use addon_nts::source::NtsSource;
use ferrohealth_secret::secret::Secret;
use sha2::Digest as _;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A clock a test moves by hand.
#[derive(Debug)]
pub(crate) struct TestClock {
    now: Mutex<jiff::Timestamp>,
}

impl TestClock {
    /// A clock standing at `instant`, an RFC 3339 timestamp.
    pub(crate) fn at(instant: &str) -> Arc<Self> {
        Arc::new(Self {
            now: Mutex::new(instant.parse().expect("the instant reads")),
        })
    }

    /// Moves the clock forward by `seconds`.
    pub(crate) fn advance(&self, seconds: i64) {
        let mut now = self.now.lock().expect("the clock is not poisoned");
        *now = now
            .checked_add(jiff::Span::new().try_seconds(seconds).expect("a span"))
            .expect("an instant");
    }

    /// Moves the clock forward by `hours`.
    pub(crate) fn advance_hours(&self, hours: i64) {
        self.advance(hours * 3600);
    }
}

impl Clock for TestClock {
    fn now(&self) -> jiff::Timestamp {
        *self.now.lock().expect("the clock is not poisoned")
    }
}

/// The synthetic feed the service is mocked to serve.
pub(crate) fn feed_xml() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/service-feed.xml");
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The lowercase hexadecimal SHA-256 of `body`, computed outside the crate.
pub(crate) fn sha256_of(body: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(body.as_bytes());
    hasher
        .finalize()
        .iter()
        .fold(String::new(), |mut out, byte| {
            write!(out, "{byte:02x}").expect("writing into a String cannot fail");
            out
        })
}

/// The credentials of an account that has only the password grant.
pub(crate) fn password_account() -> Credentials {
    Credentials {
        client_id: String::from("cli_client"),
        client_secret: None,
        username: Some(String::from("an-account")),
        password: Some(Secret::new("a-password")),
    }
}

/// The credentials of a deployment that also holds a client secret.
pub(crate) fn confidential_account() -> Credentials {
    Credentials {
        client_secret: Some(Secret::new("a-client-secret")),
        ..password_account()
    }
}

/// The configuration pointing at `server`.
pub(crate) fn config(server: &MockServer) -> NtsConfig {
    NtsConfig {
        base_url: server.uri(),
        ..NtsConfig::default()
    }
}

/// A source pointed at `server`, driven by `clock`.
pub(crate) fn source(
    server: &MockServer,
    credentials: Credentials,
    clock: Arc<TestClock>,
) -> NtsSource {
    NtsSource::with_parts(&config(server), credentials, reqwest::Client::new(), clock)
}

/// Mounts the SMART configuration document naming the mock token endpoint.
pub(crate) async fn mount_discovery(server: &MockServer) {
    let document = serde_json::json!({
        "issuer": format!("{}/realms/synthetic", server.uri()),
        "authorization_endpoint": format!("{}/authorize", server.uri()),
        "token_endpoint": format!("{}/token", server.uri()),
        "grant_types_supported": ["authorization_code", "client_credentials", "password"],
        "capabilities": ["launch-standalone"],
    });
    Mock::given(method("GET"))
        .and(path("/fhir/.well-known/smart-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(document))
        .mount(server)
        .await;
}

/// A token response body.
pub(crate) fn token_body(
    access_token: &str,
    expires_in: i64,
    refresh_token: &str,
) -> serde_json::Value {
    serde_json::json!({
        "access_token": access_token,
        "token_type": "Bearer",
        "expires_in": expires_in,
        "refresh_token": refresh_token,
        "refresh_expires_in": 86_400,
    })
}

/// A token-endpoint mock matching one grant type.
pub(crate) fn token_mock(grant: &str) -> wiremock::MockBuilder {
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains(format!("grant_type={grant}")))
}

/// The response a token endpoint gives to an accepted exchange.
pub(crate) fn issued(access_token: &str, expires_in: i64, refresh_token: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(token_body(access_token, expires_in, refresh_token))
}

/// The response a Keycloak realm gives to a spent or wrong credential
/// (RFC 6749 §5.2 names `invalid_grant` for both).
pub(crate) fn refused() -> ResponseTemplate {
    ResponseTemplate::new(400).set_body_json(serde_json::json!({
        "error": "invalid_grant",
        "error_description": "Invalid refresh token",
    }))
}

/// How many token requests carried `grant_type=<grant>`.
pub(crate) async fn grant_requests(server: &MockServer, grant: &str) -> usize {
    let needle = format!("grant_type={grant}");
    server
        .received_requests()
        .await
        .expect("the mock server records its requests")
        .iter()
        .filter(|request| {
            request.url.path() == "/token"
                && String::from_utf8_lossy(&request.body).contains(needle.as_str())
        })
        .count()
}
