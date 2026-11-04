// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Access token validation: expiry, issuer, audience, algorithm, type, key and the refusal shape.

use axum::body::Body;
use http::header::AUTHORIZATION;
use http::{Request, StatusCode};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{SigningKey, challenge, claims, colours, discovery, issuer, put_colours};
use crate::fixture::Server;

// RFC 6750 §3.1: a credential that does not verify is `invalid_token`.
#[tokio::test]
async fn an_expired_or_malformed_token_is_an_invalid_token() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let expired = key.sign(&claims(&mock.uri(), "system/CodeSystem.cud", -600));
    let response = put_colours(&server, Some(&expired)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(
        challenge(&response).contains("error=\"invalid_token\""),
        "{}",
        challenge(&response)
    );

    let response = put_colours(&server, Some("not-a-token")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(
        challenge(&response).contains("error=\"invalid_token\""),
        "{}",
        challenge(&response)
    );
}

// RFC 7519 §4.1.1 and §4.1.3: a token of another issuer or another audience is
// not this server's to accept.
#[tokio::test]
async fn a_token_of_another_issuer_or_audience_is_refused() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), Some("ferroterm"), None).await;

    let elsewhere = key.sign(&claims(
        "https://other.example.org",
        "system/CodeSystem.cud",
        300,
    ));
    let response = put_colours(&server, Some(&elsewhere)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let mut other_audience = claims(&mock.uri(), "system/CodeSystem.cud", 300);
    other_audience["aud"] = json!("another-server");
    let response = put_colours(&server, Some(&key.sign(&other_audience))).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let mine = key.sign(&claims(&mock.uri(), "system/CodeSystem.cud", 300));
    let response = put_colours(&server, Some(&mine)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

// RFC 8725 §3.1: a verifier accepts only the algorithms it decided on, so a
// token signed with the published public key as an HMAC secret is refused.
#[tokio::test]
async fn a_symmetric_algorithm_is_never_accepted() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let mut header = Header::new(Algorithm::HS256);
    header.kid = Some(String::from("k1"));
    header.typ = Some(String::from("at+jwt"));
    let forged = encode(
        &header,
        &claims(&mock.uri(), "system/CodeSystem.cud", 300),
        &EncodingKey::from_secret(&key.public),
    )
    .expect("signs");
    let response = put_colours(&server, Some(&forged)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(
        challenge(&response).contains("error=\"invalid_token\""),
        "{}",
        challenge(&response)
    );
}

// RFC 8725 §3.8 and §3.9: a claim checked only when present is no check, so a
// token missing `iss`, or missing `aud` where one is configured, is refused.
#[tokio::test]
async fn a_token_missing_the_issuer_or_the_audience_is_refused() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), Some("ferroterm"), None).await;

    let mut without_issuer = claims(&mock.uri(), "system/CodeSystem.cud", 300);
    without_issuer
        .as_object_mut()
        .expect("an object")
        .remove("iss");
    let response = put_colours(&server, Some(&key.sign(&without_issuer))).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "no `iss`");

    let mut without_audience = claims(&mock.uri(), "system/CodeSystem.cud", 300);
    without_audience
        .as_object_mut()
        .expect("an object")
        .remove("aud");
    let response = put_colours(&server, Some(&key.sign(&without_audience))).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "no `aud`");
}

// RFC 8725 §3.12: the kinds of JWT one issuer mints are told apart, so a token
// typed as something other than an access token is refused.
#[tokio::test]
async fn a_token_typed_as_another_kind_is_refused() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let payload = claims(&mock.uri(), "system/CodeSystem.cud", 300);

    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(String::from("k1"));
    header.typ = Some(String::from("id_token+jwt"));
    let response = put_colours(&server, Some(&key.sign_with(&header, &payload))).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // RFC 9068 §4: the resource server rejects a token whose `typ` is neither
    // `at+jwt` nor `application/at+jwt`, an untyped one and a plain `JWT` too.
    for typ in [None, Some("JWT")] {
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(String::from("k1"));
        header.typ = typ.map(str::to_owned);
        let response = put_colours(&server, Some(&key.sign_with(&header, &payload))).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{typ:?}");
    }

    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(String::from("k1"));
    header.typ = Some(String::from("at+jwt"));
    let response = put_colours(&server, Some(&key.sign_with(&header, &payload))).await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "RFC 9068 §2.1 types an access token"
    );
}

// RFC 7515 §4.1.4: without a `kid` the server cannot tell which of two usable
// keys signed the token, and it refuses rather than guessing.
#[tokio::test]
async fn a_token_naming_no_key_against_two_usable_keys_is_refused() {
    let first = SigningKey::generate("k1");
    let second = SigningKey::generate("k2");
    let mock = MockServer::start().await;
    let base = mock.uri();
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(discovery(&base)))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"keys": [first.jwk(), second.jwk()]})),
        )
        .mount(&mock)
        .await;
    let server = Server::start_persisting_with_smart(&base, None, None).await;

    let mut header = Header::new(Algorithm::ES256);
    header.typ = Some(String::from("at+jwt"));
    let anonymous = first.sign_with(&header, &claims(&base, "system/CodeSystem.cud", 300));
    let response = put_colours(&server, Some(&anonymous)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// RFC 9068 §2.2 requires `sub` and RFC 7519 §4.1.2 has it identify a
// principal; RFC 6750 §2.1 fixes the `b64token` syntax, and §3.1 answers a
// token that is "malformed" with `invalid_token`.
#[tokio::test]
async fn a_blank_sub_or_a_value_outside_b64token_is_an_invalid_token() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let mut blank = claims(&mock.uri(), "system/CodeSystem.cud", 300);
    blank["sub"] = json!("  ");
    let blank = key.sign(&blank);
    for token in [blank.as_str(), "a,b", "a=b"] {
        let response = put_colours(&server, Some(token)).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{token}");
        let sent = challenge(&response);
        assert!(sent.contains("error=\"invalid_token\""), "{sent}");
        assert!(!sent.contains("error_description"), "{sent}");
    }
}

// RFC 6750 §3.1: a request that carries two credentials is `invalid_request`.
#[tokio::test]
async fn two_authorization_headers_are_an_invalid_request() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let writer = key.sign(&claims(&mock.uri(), "system/CodeSystem.cud", 300));

    let request = Request::put("/r4b/CodeSystem/colours")
        .header(http::header::CONTENT_TYPE, "application/fhir+json")
        .header(AUTHORIZATION, format!("Bearer {writer}"))
        .header(AUTHORIZATION, format!("Bearer {writer}"))
        .body(Body::from(colours().to_string()))
        .expect("request");
    let response = server.send(request).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let sent = challenge(&response);
    assert!(sent.contains("error=\"invalid_request\""), "{sent}");
}
