// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ID Token of the sign-in signed with ML-DSA-65 (RFC 9964 §5) or ES256,
//! checked by the relying party the console builds, with `[jose]` off and
//! with `post_quantum_only`, which accepts ML-DSA alone.

use ferrohealth_smart::jws::algorithm::Algorithm;
use ferrohealth_smart::jws::compact::{Header, encode};
use ferrohealth_smart::jws::key::{PrivateKey, SigningKey};
use ferrohealth_smart::oidc::{Callback, OidcError, PendingSignIn, SignedIn};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::provider::{JWKS_PATH, KID, Provider, console, token_answer};

/// The `kid` of the ML-DSA-65 key.
const ML_DSA_KID: &str = "ml-dsa-1";

/// The ML-DSA-65 key, expanded from a fixed seed.
fn ml_dsa_key() -> SigningKey {
    let private =
        PrivateKey::from_ml_dsa_seed(Algorithm::MlDsa65, &[11; 32]).expect("a 32-octet seed");
    SigningKey::new(private, Algorithm::MlDsa65).expect("an ML-DSA-65 key signs ML-DSA-65")
}

/// A provider publishing its ES256 key and the ML-DSA-65 key.
async fn provider() -> Provider {
    let provider = Provider::bare().await;
    let ml_dsa = ml_dsa_key()
        .public_jwk()
        .clone()
        .with_kid(ML_DSA_KID)
        .with_signature_use()
        .to_value();
    let es256 = serde_json::to_value(&provider.key.jwk).expect("the JWK serializes");
    Mock::given(method("GET"))
        .and(path(JWKS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "keys": [es256, ml_dsa] })))
        .mount(&provider.server)
        .await;
    provider
}

/// Exchanges a code at `provider` for an ID Token signed with ML-DSA-65
/// when `ml_dsa`, else with the ES256 key, at the console whose
/// configuration is `toml`.
async fn exchange(provider: &Provider, toml: &str, ml_dsa: bool) -> Result<SignedIn, OidcError> {
    let pending = PendingSignIn::mint().expect("the random source works");
    let claims = provider.claims(pending.nonce());
    let id_token = if ml_dsa {
        let mut header = Header::new(Algorithm::MlDsa65);
        header.kid = Some(ML_DSA_KID.to_owned());
        encode(&header, &claims, &ml_dsa_key()).expect("the ID Token signs")
    } else {
        provider.id_token(&claims)
    };
    provider.answer_tokens(200, &token_answer(&id_token)).await;
    let state = console(Some(provider.oidc()), toml).await;
    let party = state
        .sign_in()
        .party()
        .cloned()
        .expect("sign-in is configured");
    party
        .exchange(&pending, &Callback::granted("c1", pending.state()))
        .await
}

#[tokio::test]
async fn an_ml_dsa_id_token_signs_in_with_or_without_post_quantum_only() {
    for toml in ["", "[jose]\npost_quantum_only = true\n"] {
        let provider = provider().await;
        let signed_in = exchange(&provider, toml, true).await;
        assert!(signed_in.is_ok(), "{toml:?}: {signed_in:?}");
    }
}

#[tokio::test]
async fn an_es256_id_token_signs_in_by_default_and_is_refused_under_post_quantum_only() {
    let provider = provider().await;
    assert!(
        exchange(&provider, "", false).await.is_ok(),
        "{KID} by default"
    );
    let provider = self::provider().await;
    let refused = exchange(&provider, "[jose]\npost_quantum_only = true\n", false).await;
    assert!(refused.is_err(), "post_quantum_only refuses ES256");
}

#[test]
fn post_quantum_only_names_the_three_ml_dsa_parameter_sets_alone() {
    let on = ferroterm_console::config::Jose {
        post_quantum_only: true,
    };
    assert_eq!(
        on.id_token_algorithms(),
        [Algorithm::MlDsa44, Algorithm::MlDsa65, Algorithm::MlDsa87]
    );
    let off = ferroterm_console::config::Jose::default().id_token_algorithms();
    assert!(off.contains(&Algorithm::Es256) && off.contains(&Algorithm::MlDsa65));
}
