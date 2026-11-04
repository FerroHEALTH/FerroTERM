// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The audience a token names, against every served version base.

use axum::body::Body;
use http::StatusCode;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::{
    PROXIED, SigningKey, challenge, claims, colours, issuer, put_colours, put_with, reload,
};
use crate::fixture::{self, Server, SmartSetup};

/// A server behind [`PROXIED`] whose `[auth] audience` is `audience`, or
/// [`fixture::AUDIENCE`] when the test names none.
async fn proxied(issuer: &str, audience: Option<&str>) -> Server {
    let mut setup = SmartSetup::for_issuer(issuer);
    if let Some(audience) = audience {
        setup.auth.audience = vec![audience.to_owned()];
    }
    setup.base_url = Some(String::from(PROXIED));
    Server::start_persisting_with_smart_setup(setup).await
}

/// The claims of a write token for `issuer` carrying `audience` as `aud`.
#[expect(
    clippy::indexing_slicing,
    reason = "serde_json assigns the member on the object `claims` returns"
)]
fn claims_for(issuer: &str, audience: Value) -> Value {
    let mut payload = claims(issuer, "system/CodeSystem.cud", 300);
    payload["aud"] = audience;
    payload
}

/// A `PUT` of the colours code system on `version` carrying `token`.
async fn put_colours_on(server: &Server, version: &str, token: &str) -> http::Response<Body> {
    let uri = format!("/{version}/CodeSystem/colours");
    put_with(server, &uri, &colours(), Some(token)).await
}

// SMART has a client send the FHIR base as `aud`
// (<https://hl7.org/fhir/smart-app-launch/app-launch.html>), and this server
// serves one base per version, so a token minted on one version writes on
// every other.
#[tokio::test]
async fn a_token_for_one_version_base_writes_on_every_served_version() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = proxied(&mock.uri(), None).await;

    let signed_in_on_r5 = key.sign(&claims_for(&mock.uri(), json!(format!("{PROXIED}/r5"))));
    let response = put_colours_on(&server, "r5", &signed_in_on_r5).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let response = put_colours_on(&server, "r4b", &signed_in_on_r5).await;
    assert!(
        response.status().is_success(),
        "the same reader writes on another version: {}",
        response.status()
    );

    // A client that kept the trailing slash of the base names the same server.
    let with_slash = key.sign(&claims_for(&mock.uri(), json!(format!("{PROXIED}/r4b/"))));
    let response = put_colours_on(&server, "r4b", &with_slash).await;
    assert!(response.status().is_success(), "{}", response.status());
}

// RFC 7519 §4.1.3: `aud` may be an array, and the recipient identifies itself
// with a value in it.
#[tokio::test]
async fn an_array_audience_with_one_matching_member_passes() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = proxied(&mock.uri(), None).await;

    let several = json!(["https://elsewhere.example.org", format!("{PROXIED}/r4b")]);
    let token = key.sign(&claims_for(&mock.uri(), several));
    let response = put_colours(&server, Some(&token)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

// RFC 8725 §3.9: a token another resource server is the audience of is not
// this server's to spend.
#[tokio::test]
async fn an_audience_naming_another_server_is_refused() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = proxied(&mock.uri(), Some("ferroterm")).await;

    for foreign in [
        json!("https://elsewhere.example.org/r4b"),
        json!(["https://elsewhere.example.org/r4b", "another-server"]),
    ] {
        let token = key.sign(&claims_for(&mock.uri(), foreign.clone()));
        let response = put_colours(&server, Some(&token)).await;
        let sent = challenge(&response);
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{foreign}");
        assert!(sent.contains("error=\"invalid_token\""), "{sent}");
    }

    // The audience the deployment named outright stays accepted beside the
    // version bases.
    let configured = key.sign(&claims_for(&mock.uri(), json!("ferroterm")));
    let response = put_colours(&server, Some(&configured)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

// RFC 8725 §3.9: a claim checked only when present is no check, so a token
// with no `aud` is refused wherever a base URL names the accepted set. The
// root is no FHIR base of this server, so it is refused with it.
#[tokio::test]
async fn a_token_with_no_audience_or_the_root_as_one_is_refused() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = proxied(&mock.uri(), None).await;

    let mut without = claims(&mock.uri(), "system/CodeSystem.cud", 300);
    without.as_object_mut().expect("an object").remove("aud");
    let response = put_colours(&server, Some(&key.sign(&without))).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "no `aud`");

    let root = key.sign(&claims_for(&mock.uri(), json!(PROXIED)));
    let response = put_colours(&server, Some(&root)).await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "the root serves no FHIR base"
    );
}

// The admin listener serves no FHIR interaction, so a version base is not an
// audience for it: only the configured audiences open it (no specification
// governs the admin surface: our own design).
#[tokio::test]
async fn the_admin_listener_does_not_take_a_version_base_as_its_audience() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = proxied(&mock.uri(), None).await;
    let admin = server.admin_router();

    let mut for_a_base = claims_for(&mock.uri(), json!(format!("{PROXIED}/r4b")));
    for_a_base["scope"] = json!("__ferroterm.admin");
    let response = admin
        .clone()
        .oneshot(reload(Some(format!("Bearer {}", key.sign(&for_a_base)))))
        .await
        .expect("the admin answers");
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "a version base is no admin audience"
    );

    let mut configured = claims_for(&mock.uri(), json!(fixture::AUDIENCE));
    configured["scope"] = json!("__ferroterm.admin");
    let response = admin
        .oneshot(reload(Some(format!("Bearer {}", key.sign(&configured)))))
        .await
        .expect("the admin answers");
    assert_eq!(response.status(), StatusCode::OK);
}

// RFC 6750 §3: `error_description` is optional, and every invalid token gets
// one generic `401` that names no claim, no `kid` and no issuer message, so a
// refusal teaches the holder of a forged or stolen token nothing.
#[tokio::test]
async fn every_invalid_token_gets_the_same_refusal() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let expired = key.sign(&claims(&mock.uri(), "system/CodeSystem.cud", -600));
    let stranger = SigningKey::generate("unknown-kid");
    let unknown = stranger.sign(&claims(&mock.uri(), "system/CodeSystem.cud", 300));
    let mut answers = Vec::new();
    for token in [expired.as_str(), unknown.as_str(), "not-a-token"] {
        let response = put_colours(&server, Some(token)).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let sent = challenge(&response);
        assert!(!sent.contains("error_description"), "{sent}");
        assert!(!sent.contains("unknown-kid"), "{sent}");
        let (_, body) = fixture::json(response).await;
        assert!(!body.to_string().contains("unknown-kid"), "{body}");
        answers.push((sent, body));
    }
    let (first, rest) = answers.split_first().expect("three answers");
    for other in rest {
        assert_eq!(first, other, "two failures answer alike");
    }
}
