// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The issuer's metadata, the client credentials grant, and keeping a token
//! fresh across a long run.
//!
//! Every clock movement is driven by hand, so a run covering a day takes no
//! longer than the requests it makes.

use std::sync::Arc;

use terminology_syndication::source::{Authorization, Source};
use wiremock::MockServer;

use crate::support;

/// The `Authorization` field value a source would put on its next request.
async fn authorization(source: &addon_nts::source::NtsSource) -> String {
    match source.listing_auth().await.expect("a token is issued") {
        Authorization::Field(value) => value.expose().to_str().expect("a visible value").to_owned(),
        other => panic!("the service is challenged with a bearer token, not {other:?}"),
    }
}

#[tokio::test]
async fn the_token_endpoint_comes_from_the_issuer_metadata() {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("client_credentials")
        .respond_with(support::issued("access-first", 300, "refresh-first"))
        .mount(&server)
        .await;

    let source = support::source(
        &server,
        support::client_account(),
        support::TestClock::start(),
    );
    assert_eq!(authorization(&source).await, "Bearer access-first");

    let requests = server
        .received_requests()
        .await
        .expect("the requests are recorded");
    let discovery = requests
        .iter()
        .filter(|request| request.url.path() == support::DISCOVERY)
        .count();
    assert_eq!(discovery, 1, "the issuer's metadata is read once");
    let token = requests
        .iter()
        .find(|request| request.url.path() == "/token")
        .expect("one token request");
    let body = String::from_utf8_lossy(&token.body);
    for part in [
        "grant_type=client_credentials",
        "client_id=a-client",
        "client_secret=a-client-secret",
    ] {
        assert!(
            body.contains(part),
            "the client secret travels in the body (client_secret_post), as the service's examples send it; missing {part}"
        );
    }
    assert!(!body.contains("password"), "no password grant is sent");
}

#[tokio::test]
async fn metadata_naming_another_issuer_is_refused() {
    let server = MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(support::DISCOVERY))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer": "https://another.example.invalid",
                "token_endpoint": format!("{}/token", server.uri()),
            })),
        )
        .mount(&server)
        .await;

    let source = support::source(
        &server,
        support::client_account(),
        support::TestClock::start(),
    );
    let error = source
        .listing_auth()
        .await
        .expect_err("metadata of another issuer names no token endpoint to trust");
    assert!(
        error.to_string().contains("could not authenticate"),
        "the source reports that it could not authenticate: {error}"
    );
    assert_eq!(
        support::grant_requests(&server, "client_credentials").await,
        0
    );
}

#[tokio::test]
async fn a_refused_grant_is_an_authentication_failure() {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("client_credentials")
        .respond_with(support::refused())
        .mount(&server)
        .await;

    let source = support::source(
        &server,
        support::client_account(),
        support::TestClock::start(),
    );
    let error = source
        .listing_auth()
        .await
        .expect_err("the issuer refused the client");
    assert!(
        error.to_string().contains("could not authenticate"),
        "the source reports that it could not authenticate: {error}"
    );
}

#[tokio::test]
async fn a_token_with_life_left_is_used_again() {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("client_credentials")
        .respond_with(support::issued("access-first", 300, "refresh-first"))
        .mount(&server)
        .await;

    let clock = support::TestClock::start();
    let source = support::source(&server, support::client_account(), Arc::clone(&clock));
    assert_eq!(authorization(&source).await, "Bearer access-first");
    clock.advance(120);
    assert_eq!(authorization(&source).await, "Bearer access-first");
    assert_eq!(
        support::grant_requests(&server, "client_credentials").await,
        1,
        "a token with four minutes left is not renewed"
    );
}

#[tokio::test]
async fn a_token_inside_the_renewal_margin_is_renewed() {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("client_credentials")
        .respond_with(support::issued("access-first", 300, "refresh-first"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    support::token_mock("client_credentials")
        .respond_with(support::issued("access-second", 300, "refresh-second"))
        .mount(&server)
        .await;

    let clock = support::TestClock::start();
    let source = support::source(&server, support::client_account(), Arc::clone(&clock));
    assert_eq!(authorization(&source).await, "Bearer access-first");
    clock.advance(250);
    assert_eq!(
        authorization(&source).await,
        "Bearer access-second",
        "a token within the sixty-second margin is replaced before it expires"
    );
    assert_eq!(
        support::grant_requests(&server, "refresh_token").await,
        0,
        "the client credentials grant is asked again; no refresh token is used"
    );
}

#[tokio::test]
async fn a_run_spanning_twenty_five_hours_needs_no_interaction() {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("client_credentials")
        .respond_with(support::issued("access-first", 300, "refresh-first"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    support::token_mock("client_credentials")
        .respond_with(support::issued("access-later", 300, "refresh-later"))
        .mount(&server)
        .await;

    let clock = support::TestClock::start();
    let source = support::source(&server, support::client_account(), Arc::clone(&clock));
    assert_eq!(authorization(&source).await, "Bearer access-first");
    clock.advance_hours(25);
    assert_eq!(
        authorization(&source).await,
        "Bearer access-later",
        "a run a day later is issued a token from the stored client"
    );
    assert_eq!(
        support::grant_requests(&server, "client_credentials").await,
        2
    );
}

#[tokio::test]
async fn no_rendering_of_the_source_carries_a_secret() {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("client_credentials")
        .respond_with(support::issued("access-first", 300, "refresh-first"))
        .mount(&server)
        .await;

    let source = support::source(
        &server,
        support::client_account(),
        support::TestClock::start(),
    );
    assert!(
        !format!("{source:?}").contains("a-client-secret"),
        "the source rendering carries the client secret"
    );
    let token = authorization(&source).await;
    assert!(!token.is_empty(), "a token was issued");
    assert!(
        !format!("{source:?}").contains("access-first"),
        "a held token is never rendered"
    );
}

#[tokio::test]
async fn a_table_naming_no_client_or_a_block_with_a_retired_key_is_refused() {
    let server = MockServer::start().await;
    let mut anonymous = support::endpoint(&server, &support::client_account());
    if let Some(oauth) = anonymous.credentials.oauth.as_mut() {
        oauth.client_id.clear();
    }
    let refused =
        addon_nts::source::NtsSource::new(&addon_nts::config::NtsConfig::default(), &anonymous)
            .expect_err("the service challenges every request, so a client is required");
    assert!(
        matches!(refused, addon_nts::source::NtsError::NoUsableClient),
        "{refused:?}"
    );

    let retired: addon_nts::config::NtsConfig =
        serde_json::from_str(r#"{"base_url":"https://example.invalid"}"#).expect("the block reads");
    let refused = addon_nts::source::NtsSource::new(
        &retired,
        &support::endpoint(&server, &support::client_account()),
    )
    .expect_err("a retired key of the block is refused");
    assert!(refused.to_string().contains("nts.url"), "{refused}");
}
