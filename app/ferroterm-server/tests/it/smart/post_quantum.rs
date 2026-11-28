// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Access tokens signed with ML-DSA-65 (RFC 9964 §5) under the issuer's
//! default algorithms and under `[auth] post_quantum_only`, which narrows
//! them to the ML-DSA parameter sets and refuses a classical list.

use ferrohealth_smart::config::ConfigError;
use ferrohealth_smart::jws::algorithm::Algorithm;
use ferrohealth_smart::jws::compact::{Header, encode};
use ferrohealth_smart::jws::key::{PrivateKey, SigningKey as JwsKey};
use ferroterm_server::config::Config;
use ferroterm_server::smart::{Smart, StartError};
use http::StatusCode;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{SigningKey, claims, discovery, put_colours};
use crate::fixture::{Server, SmartSetup};

/// The `kid` of the ML-DSA-65 key.
const ML_DSA_KID: &str = "ml-dsa-1";

/// The ML-DSA-65 key, expanded from a fixed seed.
fn ml_dsa_key() -> JwsKey {
    let private =
        PrivateKey::from_ml_dsa_seed(Algorithm::MlDsa65, &[5; 32]).expect("a 32-octet seed");
    JwsKey::new(private, Algorithm::MlDsa65).expect("an ML-DSA-65 key signs ML-DSA-65")
}

/// `claims` signed with the ML-DSA-65 key as an access token.
fn ml_dsa_token(claims: &Value) -> String {
    let mut header = Header::new(Algorithm::MlDsa65);
    header.kid = Some(ML_DSA_KID.to_owned());
    header.typ = Some(String::from("at+jwt"));
    encode(&header, claims, &ml_dsa_key()).expect("the token signs")
}

/// An issuer publishing the ES256 key `es256` and the ML-DSA-65 key.
async fn issuer(es256: &SigningKey) -> MockServer {
    let server = MockServer::start().await;
    let ml_dsa = ml_dsa_key()
        .public_jwk()
        .clone()
        .with_kid(ML_DSA_KID)
        .with_signature_use()
        .to_value();
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(discovery(&server.uri())))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "keys": [es256.jwk(), ml_dsa] })),
        )
        .mount(&server)
        .await;
    server
}

/// The server trusting `issuer`, `[auth] post_quantum_only` set to `on`.
async fn server(issuer: &str, on: bool) -> Server {
    let mut setup = SmartSetup::for_issuer(issuer);
    setup.auth.post_quantum_only = on;
    Server::start_persisting_with_smart_setup(setup).await
}

#[tokio::test]
async fn post_quantum_only_admits_an_ml_dsa_token_and_refuses_an_es256_one() {
    let es256 = SigningKey::generate("k1");
    let mock = issuer(&es256).await;
    let server = server(&mock.uri(), true).await;
    let granted = claims(&mock.uri(), "system/CodeSystem.cud", 300);
    let refused = put_colours(&server, Some(&es256.sign(&granted))).await;
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED, "ES256");
    let admitted = put_colours(&server, Some(&ml_dsa_token(&granted))).await;
    assert_eq!(admitted.status(), StatusCode::CREATED, "ML-DSA-65");
}

#[tokio::test]
async fn the_default_algorithms_admit_an_es256_token_and_no_ml_dsa_one() {
    let es256 = SigningKey::generate("k1");
    let mock = issuer(&es256).await;
    let server = server(&mock.uri(), false).await;
    let granted = claims(&mock.uri(), "system/CodeSystem.cud", 300);
    let refused = put_colours(&server, Some(&ml_dsa_token(&granted))).await;
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED, "ML-DSA-65");
    let admitted = put_colours(&server, Some(&es256.sign(&granted))).await;
    assert_eq!(admitted.status(), StatusCode::CREATED, "ES256");
}

#[tokio::test]
async fn post_quantum_only_refuses_an_issuer_listing_a_classical_algorithm() {
    let es256 = SigningKey::generate("k1");
    let mock = issuer(&es256).await;
    let mut setup = SmartSetup::for_issuer(&mock.uri());
    setup.issuer_mut().algorithms = vec![Algorithm::MlDsa65, Algorithm::Es256];
    setup.auth.post_quantum_only = true;
    let dir = tempfile::tempdir().expect("tempdir");
    let config: Config = Server::smart_config(dir.path(), setup);
    let refused = Smart::start(&config).await;
    assert!(
        matches!(&refused, Err(StartError::Auth { source })
            if matches!(source.as_ref(), ConfigError::ClassicalAlgorithm { key, algorithm }
                if key == "auth.issuer.idp.algorithms" && *algorithm == Algorithm::Es256)),
        "{refused:?}"
    );
}
