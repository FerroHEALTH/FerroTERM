// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Shared helpers: a driven clock, the fixture feed, and a mock service.
//!
//! No test reaches the network. The mock service answers the three requests
//! the add-on makes: the issuer's metadata, the token endpoint, and the
//! listing.

use core::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use std::time::{Duration, Instant};

use addon_nts::config::NtsConfig;
use addon_nts::source::NtsSource;
use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_smart::client::cache::Clock;
use ferrohealth_smart::config::endpoint::EndpointSection;
use sha2::Digest as _;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A clock a test moves by hand.
#[derive(Debug)]
pub(crate) struct TestClock {
    start: Instant,
    elapsed: Mutex<Duration>,
}

impl TestClock {
    pub(crate) fn start() -> Arc<Self> {
        Arc::new(Self {
            start: Instant::now(),
            elapsed: Mutex::new(Duration::ZERO),
        })
    }

    pub(crate) fn advance(&self, seconds: u64) {
        let mut elapsed = self.elapsed.lock().expect("the clock is not poisoned");
        *elapsed += Duration::from_secs(seconds);
    }

    pub(crate) fn advance_hours(&self, hours: u64) {
        self.advance(hours * 3600);
    }
}

impl Clock for TestClock {
    fn now(&self) -> Instant {
        self.start + *self.elapsed.lock().expect("the clock is not poisoned")
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

/// The confidential client the service issued.
#[derive(Clone, Copy)]
pub(crate) struct Account {
    client_id: &'static str,
    client_secret: &'static str,
}

/// The client of an account that has the client credentials grant.
pub(crate) fn client_account() -> Account {
    Account {
        client_id: "a-client",
        client_secret: "a-client-secret",
    }
}

/// The `[nts]` endpoint table pointing at `server`, with `account` as its
/// client and every other key the add-on's default.
pub(crate) fn endpoint(server: &MockServer, account: &Account) -> EndpointSection {
    let mut endpoint = addon_nts::config::endpoint();
    endpoint.url = SecretUrl::new(server.uri());
    let oauth = endpoint
        .credentials
        .oauth
        .as_mut()
        .expect("the default table names the grant");
    oauth.issuer = format!("{}/realms/synthetic", server.uri());
    account.client_id.clone_into(&mut oauth.client_id);
    oauth.client_secret = Some(account.client_secret.to_owned().into());
    endpoint
}

/// A source pointed at `server`, driven by `clock`.
pub(crate) fn source(server: &MockServer, account: Account, clock: Arc<TestClock>) -> NtsSource {
    NtsSource::with_clock(&NtsConfig::default(), &endpoint(server, &account), clock)
        .expect("the source builds")
}

/// Mounts the issuer's metadata naming the mock token endpoint, where a
/// Keycloak realm serves it (OpenID Connect Discovery 1.0 §4).
/// Where the mock issuer serves its metadata.
pub(crate) const DISCOVERY: &str = "/realms/synthetic/.well-known/openid-configuration";

pub(crate) async fn mount_discovery(server: &MockServer) {
    let document = serde_json::json!({
        "issuer": format!("{}/realms/synthetic", server.uri()),
        "authorization_endpoint": format!("{}/authorize", server.uri()),
        "token_endpoint": format!("{}/token", server.uri()),
        "jwks_uri": format!("{}/certs", server.uri()),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "client_credentials", "password"],
    });
    Mock::given(method("GET"))
        .and(path(DISCOVERY))
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
