// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Discovery, the grants, and keeping a token fresh across a long run.
//!
//! Every clock movement is driven by hand, so a run covering a day takes no
//! longer than the requests it makes.

use std::sync::Arc;

use terminology_syndication::source::{Authorization, Source};
use wiremock::MockServer;

use crate::support;

/// The token a source would put on its next request.
async fn bearer(source: &addon_nts::source::NtsSource) -> String {
    match source.listing_auth().await.expect("a token is issued") {
        Authorization::Bearer(token) => token,
        other => panic!("the service is challenged with a bearer token, not {other:?}"),
    }
}

#[tokio::test]
async fn the_token_endpoint_comes_from_the_discovery_document() {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("password")
        .respond_with(support::issued("access-first", 300, "refresh-first"))
        .mount(&server)
        .await;

    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::password_account(), clock);
    assert_eq!(bearer(&source).await, "access-first");

    let discovery = server
        .received_requests()
        .await
        .expect("the requests are recorded")
        .iter()
        .filter(|request| request.url.path() == "/fhir/.well-known/smart-configuration")
        .count();
    assert_eq!(discovery, 1, "the discovery document is read once");
    assert_eq!(
        support::grant_requests(&server, "password").await,
        1,
        "the documented password grant logs in"
    );
}

#[tokio::test]
async fn a_discovery_document_without_a_token_endpoint_is_refused() {
    let server = MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/fhir/.well-known/smart-configuration",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"issuer": "https://example.invalid"})),
        )
        .mount(&server)
        .await;

    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::password_account(), clock);
    let error = source
        .listing_auth()
        .await
        .expect_err("there is nowhere to ask for a token");
    assert!(
        error.to_string().contains("could not authenticate"),
        "the source reports that it could not authenticate: {error}"
    );
}

#[tokio::test]
async fn a_configured_client_secret_is_used_before_the_password_grant() {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("client_credentials")
        .respond_with(support::issued("access-service", 300, "refresh-service"))
        .mount(&server)
        .await;
    support::token_mock("password")
        .respond_with(support::issued("access-personal", 300, "refresh-personal"))
        .mount(&server)
        .await;

    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::confidential_account(), clock);
    assert_eq!(bearer(&source).await, "access-service");
    assert_eq!(
        support::grant_requests(&server, "password").await,
        0,
        "the password grant is not reached while client credentials work"
    );
}

#[tokio::test]
async fn a_refused_client_credentials_grant_falls_back_to_the_password_grant() {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("client_credentials")
        .respond_with(support::refused())
        .mount(&server)
        .await;
    support::token_mock("password")
        .respond_with(support::issued("access-personal", 300, "refresh-personal"))
        .mount(&server)
        .await;

    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::confidential_account(), clock);
    assert_eq!(bearer(&source).await, "access-personal");
    assert_eq!(
        support::grant_requests(&server, "client_credentials").await,
        1,
        "the undocumented grant is attempted once"
    );
}

#[tokio::test]
async fn a_token_with_life_left_is_used_again() {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("password")
        .respond_with(support::issued("access-first", 300, "refresh-first"))
        .mount(&server)
        .await;

    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::password_account(), Arc::clone(&clock));
    assert_eq!(bearer(&source).await, "access-first");
    clock.advance(100);
    assert_eq!(bearer(&source).await, "access-first");
    assert_eq!(
        support::grant_requests(&server, "password").await,
        1,
        "a token with life left is not replaced"
    );
}

#[tokio::test]
async fn a_token_inside_the_renewal_margin_is_refreshed() {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("password")
        .respond_with(support::issued("access-first", 300, "refresh-first"))
        .mount(&server)
        .await;
    support::token_mock("refresh_token")
        .respond_with(support::issued("access-refreshed", 300, "refresh-next"))
        .mount(&server)
        .await;

    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::password_account(), Arc::clone(&clock));
    assert_eq!(bearer(&source).await, "access-first");
    clock.advance(280);
    assert_eq!(
        bearer(&source).await,
        "access-refreshed",
        "the token is renewed inside the margin, before it expires"
    );
    assert_eq!(
        support::grant_requests(&server, "password").await,
        1,
        "a refresh costs no second login"
    );
}

#[tokio::test]
async fn a_refused_refresh_logs_in_again_from_the_stored_credentials() {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("password")
        .respond_with(support::issued("access-first", 300, "refresh-first"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    support::token_mock("password")
        .respond_with(support::issued("access-second", 300, "refresh-second"))
        .mount(&server)
        .await;
    support::token_mock("refresh_token")
        .respond_with(support::refused())
        .mount(&server)
        .await;

    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::password_account(), Arc::clone(&clock));
    assert_eq!(bearer(&source).await, "access-first");
    clock.advance_hours(25);
    assert_eq!(
        bearer(&source).await,
        "access-second",
        "a spent refresh token ends in a fresh login"
    );
    assert_eq!(
        support::grant_requests(&server, "refresh_token").await,
        1,
        "the refresh is attempted before the login"
    );
    assert_eq!(support::grant_requests(&server, "password").await, 2);
}

#[tokio::test]
async fn a_run_spanning_twenty_five_hours_needs_no_interaction() {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("password")
        .respond_with(support::issued("access-first", 300, "refresh-first"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    support::token_mock("password")
        .respond_with(support::issued("access-second", 300, "refresh-second"))
        .mount(&server)
        .await;
    // The refresh token lives 24 hours, so the 24th hourly renewal is the
    // first the server refuses.
    support::token_mock("refresh_token")
        .respond_with(support::issued("access-refreshed", 300, "refresh-next"))
        .up_to_n_times(23)
        .mount(&server)
        .await;
    support::token_mock("refresh_token")
        .respond_with(support::refused())
        .mount(&server)
        .await;

    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::password_account(), Arc::clone(&clock));
    assert_eq!(bearer(&source).await, "access-first");
    for hour in 1..=25 {
        clock.advance_hours(1);
        let token = bearer(&source).await;
        assert!(
            !token.is_empty(),
            "hour {hour} of the run answered with no token"
        );
    }
    assert_eq!(
        bearer(&source).await,
        "access-second",
        "the run ends on a token from a fresh login"
    );
    assert_eq!(
        support::grant_requests(&server, "refresh_token").await,
        25,
        "every renewal tries the refresh token first"
    );
    assert_eq!(
        support::grant_requests(&server, "password").await,
        3,
        "a login happens at the start and after each refused refresh"
    );
}

#[tokio::test]
async fn no_rendering_of_the_source_carries_a_secret() {
    let server = MockServer::start().await;
    support::mount_discovery(&server).await;
    support::token_mock("password")
        .respond_with(support::issued("access-first", 300, "refresh-first"))
        .mount(&server)
        .await;

    let clock = support::TestClock::at("2026-09-22T09:00:00Z");
    let source = support::source(&server, support::confidential_account(), clock);
    let rendered = format!("{source:?}");
    // The failure message names which credential leaked and never prints it
    // or the rendering, because either would write the secret into the output.
    for (named, secret) in [
        ("the client secret", "a-client-secret"),
        ("the password", "a-password"),
        ("the account name", "an-account"),
    ] {
        assert!(
            !rendered.contains(secret),
            "the source rendering carries {named}"
        );
    }
    let token = bearer(&source).await;
    assert!(!token.is_empty(), "a token was issued");
    assert!(
        !format!("{source:?}").contains("access-first"),
        "a held token is never rendered"
    );
}
