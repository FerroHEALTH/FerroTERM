// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Basic users and the format of a refusal.

use argon2::Argon2;
use argon2::password_hash::PasswordHasher;
use axum::body::Body;
use base64::Engine;
use ferrohealth_smart::config::{BasicSection, BasicUser};
use http::header::AUTHORIZATION;
use http::{Request, StatusCode};
use ring::rand::SystemRandom;
use secrecy::SecretString;
use tower::ServiceExt;

use super::{SigningKey, challenge, colours, issuer, put_colours, reload};
use crate::fixture::{self, Server, SmartSetup};

/// Sixteen random bytes generated for this test run, so no credential or salt
/// is committed.
fn random_bytes() -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    ring::rand::SecureRandom::fill(&SystemRandom::new(), &mut bytes).expect("random bytes");
    bytes
}

/// A password generated for this test run.
pub(super) fn password() -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes())
}

/// The Argon2id PHC string of `password` under a salt generated for the run.
pub(super) fn phc(password: &str) -> SecretString {
    let hash = Argon2::default()
        .hash_password_with_salt(password.as_bytes(), &random_bytes())
        .expect("hashes");
    SecretString::from(hash.to_string())
}

/// `Basic` credentials of `user` and `password` (RFC 7617 §2).
fn basic(user: &str, password: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"));
    format!("Basic {encoded}")
}

// A person authenticates with HTTP Basic (RFC 7617) as a `[[auth.basic.users]]`
// entry beside the bearer tokens; the user's `user/` scopes and the product
// scope decide what the person reaches, as a token's would. A machine takes a
// client credentials token instead (RFC 6749 §4.4).
#[tokio::test]
async fn a_basic_user_writes_and_reloads_with_its_configured_scopes() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let (editor, operator, wrong) = (password(), password(), password());
    let mut setup = SmartSetup::for_issuer(&mock.uri());
    setup.auth.basic = Some(BasicSection {
        users: vec![
            BasicUser {
                user: String::from("editor"),
                password_hash: Some(phc(&editor)),
                scopes: vec![String::from("user/CodeSystem.cud")],
                ..BasicUser::default()
            },
            BasicUser {
                user: String::from("operator"),
                password_hash: Some(phc(&operator)),
                scopes: vec![String::from("__ferroterm.admin")],
                ..BasicUser::default()
            },
        ],
        ..BasicSection::default()
    });
    let server = Server::start_persisting_with_smart_setup(setup).await;
    let admin = server.admin_router();

    let response = put_colours(&server, None).await;
    let sent = challenge(&response);
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(
        sent.contains("Bearer") && sent.contains("Basic realm="),
        "{sent}"
    );

    let write = |authorization: String| {
        Request::put("/r4b/CodeSystem/colours")
            .header(http::header::CONTENT_TYPE, "application/fhir+json")
            .header(AUTHORIZATION, authorization)
            .body(Body::from(colours().to_string()))
            .expect("request")
    };
    let response = server.send(write(basic("editor", &wrong))).await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "a wrong password"
    );
    let response = server.send(write(basic("operator", &operator))).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "no write scope");
    let response = server.send(write(basic("editor", &editor))).await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = admin
        .clone()
        .oneshot(reload(Some(basic("editor", &editor))))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "no admin scope");
    let response = admin
        .oneshot(reload(Some(basic("operator", &operator))))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

// A refusal is an `OperationOutcome` in the format the request asked for
// (<https://hl7.org/fhir/R4B/http.html#mime-type>); the gate's renderer reads
// the refused request's `Accept`.
#[tokio::test]
async fn a_refusal_answers_in_the_format_the_request_asked_for() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let request = Request::put("/r4b/CodeSystem/colours")
        .header(http::header::CONTENT_TYPE, "application/fhir+json")
        .header(http::header::ACCEPT, "application/fhir+xml")
        .body(Body::from(colours().to_string()))
        .expect("request");
    let response = server.send(request).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(challenge(&response).starts_with("Bearer realm="));
    let (_, content_type, body) = fixture::text(response).await;
    assert!(
        content_type.starts_with("application/fhir+xml"),
        "{content_type}"
    );
    assert!(body.contains("<OperationOutcome"), "{body}");
}
