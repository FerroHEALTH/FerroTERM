// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The code exchange at the token endpoint (RFC 6749 §4.1.3, RFC 7636 §4.5)
//! and the check of the ID Token it answers with (OpenID Connect Core 1.0
//! §3.1.3.7), and the refresh grant (RFC 6749 §6).

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ferroterm_console::config::Oidc;
use ferroterm_console::config::settings::OidcSettings;
use ferroterm_console::oidc::exchange::{ExchangeError, exchange};
use ferroterm_console::oidc::refresh::refresh;
use ferroterm_console::session::{PendingSignIn, SignedIn};
use http::StatusCode;
use http::header::AUTHORIZATION;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::provider::{
    CLIENT_ID, JWKS_PATH, KID, Provider, REDIRECT_URI, SigningKey, TOKEN_PATH, console, es256,
    form, token_answer,
};

/// The resolved settings of `oidc`.
fn settings(oidc: Oidc) -> OidcSettings {
    console(Some(oidc), "")
        .settings()
        .oidc
        .clone()
        .expect("sign-in is configured")
}

/// A pending sign-in, as `GET /auth/login` mints one.
fn pending() -> PendingSignIn {
    PendingSignIn::mint().expect("the random source works")
}

/// Exchanges a code at `provider` after it answers `status` with `body`.
async fn exchanged(
    provider: &Provider,
    pending: &PendingSignIn,
    status: u16,
    body: &Value,
) -> Result<SignedIn, ExchangeError> {
    provider.answer_tokens(status, body).await;
    exchange(
        &reqwest::Client::new(),
        &settings(provider.oidc()),
        pending,
        "c1",
    )
    .await
}

/// Exchanges a code at `provider` for a token answer carrying the ID Token
/// `claims` sign under `header` with `key`.
async fn with_id_token(
    provider: &Provider,
    pending: &PendingSignIn,
    key: &EncodingKey,
    header: &Header,
    claims: &Value,
) -> Result<SignedIn, ExchangeError> {
    let id_token = jsonwebtoken::encode(header, claims, key).expect("signed");
    exchanged(provider, pending, 200, &token_answer(&id_token)).await
}

/// Exchanges a code for an ID Token of `claims` signed with the published
/// key.
async fn with_claims(
    provider: &Provider,
    pending: &PendingSignIn,
    claims: &Value,
) -> Result<SignedIn, ExchangeError> {
    with_id_token(
        provider,
        pending,
        &provider.key.encoding,
        &es256(Some(KID)),
        claims,
    )
    .await
}

/// The claims of a valid ID Token for `pending`, with `change` applied.
fn claims_with(
    provider: &Provider,
    pending: &PendingSignIn,
    change: impl FnOnce(&mut Value),
) -> Value {
    let mut claims = provider.claims(pending.nonce());
    change(&mut claims);
    claims
}

#[tokio::test]
async fn a_valid_answer_signs_the_operator_in_with_its_tokens() {
    let provider = Provider::start().await;
    let pending = pending();
    let signed_in = with_claims(&provider, &pending, &provider.claims(pending.nonce()))
        .await
        .expect("the exchange succeeds");
    assert_eq!(
        "synthetic-access-token",
        signed_in.access_token.expose_secret()
    );
    assert_eq!(
        Some("synthetic-refresh-token"),
        signed_in
            .refresh_token
            .as_ref()
            .map(ExposeSecret::expose_secret)
    );
    assert_eq!(
        Some(std::time::Duration::from_secs(3600)),
        signed_in.expires_in
    );
    assert_eq!(Some(String::from("Synthetic Operator")), signed_in.operator);
    assert!(signed_in.id_token.is_some(), "kept as the sign-out hint");
}

#[tokio::test]
async fn the_scopes_an_answer_names_replace_those_requested() {
    let provider = Provider::start().await;
    let pending = pending();
    let id_token = provider.id_token(&provider.claims(pending.nonce()));
    let mut answer = token_answer(&id_token);
    answer["scope"] = json!("openid  user/CodeSystem.r");
    let signed_in = exchanged(&provider, &pending, 200, &answer)
        .await
        .expect("the exchange succeeds");
    assert_eq!(vec!["openid", "user/CodeSystem.r"], signed_in.scopes);
}

#[tokio::test]
async fn a_lowercase_bearer_token_type_is_accepted() {
    let provider = Provider::start().await;
    let pending = pending();
    let id_token = provider.id_token(&provider.claims(pending.nonce()));
    let mut answer = token_answer(&id_token);
    answer["token_type"] = json!("bearer");
    assert!(exchanged(&provider, &pending, 200, &answer).await.is_ok());
}

#[tokio::test]
async fn the_operator_is_named_by_preferred_username_when_name_is_blank() {
    let provider = Provider::start().await;
    let pending = pending();
    let claims = claims_with(&provider, &pending, |claims| {
        claims["name"] = json!("   ");
        claims["preferred_username"] = json!(" operator.one ");
    });
    let signed_in = with_claims(&provider, &pending, &claims)
        .await
        .expect("valid");
    assert_eq!(Some(String::from("operator.one")), signed_in.operator);
}

#[tokio::test]
async fn a_confidential_client_authenticates_with_basic_and_names_no_client_id_in_the_form() {
    let provider = Provider::start().await;
    let pending = pending();
    let oidc = Oidc {
        client_secret: Some(SecretString::from("s3cr:t&+")),
        ..provider.oidc()
    };
    let id_token = provider.id_token(&provider.claims(pending.nonce()));
    provider.answer_tokens(200, &token_answer(&id_token)).await;
    exchange(&reqwest::Client::new(), &settings(oidc), &pending, "c1")
        .await
        .expect("the exchange succeeds");
    let requests = provider.requests_at(TOKEN_PATH).await;
    let [request] = requests.as_slice() else {
        panic!("one token request");
    };
    // NOTE: RFC 6749 §2.3.1: the id and the secret are each form-urlencoded,
    // then joined by a colon and base64-encoded.
    let expected = format!(
        "Basic {}",
        STANDARD.encode(format!("{CLIENT_ID}:s3cr%3At%26%2B"))
    );
    assert_eq!(
        Some(expected.as_str()),
        request
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
    );
    let fields = form(request);
    assert!(!fields.contains_key("client_id"), "{fields:?}");
    assert_eq!("authorization_code", fields["grant_type"]);
    assert_eq!("c1", fields["code"]);
    assert_eq!(REDIRECT_URI, fields["redirect_uri"]);
    assert_eq!(pending.verifier().expose_secret(), fields["code_verifier"]);
}

#[tokio::test]
async fn a_public_client_sends_no_authorization_header() {
    let provider = Provider::start().await;
    let pending = pending();
    let id_token = provider.id_token(&provider.claims(pending.nonce()));
    exchanged(&provider, &pending, 200, &token_answer(&id_token))
        .await
        .expect("the exchange succeeds");
    let requests = provider.requests_at(TOKEN_PATH).await;
    let [request] = requests.as_slice() else {
        panic!("one token request");
    };
    assert!(request.headers.get(AUTHORIZATION).is_none());
    assert_eq!(CLIENT_ID, form(request)["client_id"]);
}

#[tokio::test]
async fn a_refusal_keeps_its_status_and_rfc_6749_error_code() {
    let provider = Provider::start().await;
    let error = exchanged(
        &provider,
        &pending(),
        400,
        &json!({"error": "invalid_grant"}),
    )
    .await
    .expect_err("refused");
    let ExchangeError::Refused {
        status,
        error: code,
    } = &error
    else {
        panic!("{error:?}");
    };
    assert_eq!(StatusCode::BAD_REQUEST, *status);
    assert_eq!(Some("invalid_grant"), code.as_deref());
    assert!(error.is_answered_refusal());
    assert!(!error.refuses_the_operator());
}

#[tokio::test]
async fn a_refusal_whose_body_names_no_error_code_is_still_a_refusal() {
    let provider = Provider::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(ResponseTemplate::new(401).set_body_string("<html>denied</html>"))
        .mount(&provider.server)
        .await;
    let error = exchange(
        &reqwest::Client::new(),
        &settings(provider.oidc()),
        &pending(),
        "c1",
    )
    .await
    .expect_err("refused");
    assert!(
        matches!(
            error,
            ExchangeError::Refused {
                status: StatusCode::UNAUTHORIZED,
                error: None
            }
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_server_error_is_a_refusal_the_operator_can_retry() {
    let provider = Provider::start().await;
    let error = exchanged(&provider, &pending(), 503, &json!({}))
        .await
        .expect_err("refused");
    assert!(matches!(error, ExchangeError::Refused { .. }), "{error:?}");
    assert!(!error.is_answered_refusal());
}

#[tokio::test]
async fn an_answer_that_is_no_token_response_is_refused() {
    let provider = Provider::start().await;
    let error = exchanged(&provider, &pending(), 200, &json!({"token": "x"}))
        .await
        .expect_err("refused");
    assert!(matches!(error, ExchangeError::Body { .. }), "{error:?}");
    assert!(error.is_answered_refusal());
}

#[tokio::test]
async fn an_access_token_that_is_not_a_bearer_token_is_refused() {
    let provider = Provider::start().await;
    let error = exchanged(
        &provider,
        &pending(),
        200,
        &json!({"access_token": "a", "token_type": "mac", "id_token": "x"}),
    )
    .await
    .expect_err("refused");
    assert!(matches!(error, ExchangeError::TokenType), "{error:?}");
}

#[tokio::test]
async fn an_answer_without_an_id_token_is_refused() {
    let provider = Provider::start().await;
    let error = exchanged(
        &provider,
        &pending(),
        200,
        &json!({"access_token": "a", "token_type": "Bearer"}),
    )
    .await
    .expect_err("refused");
    assert!(matches!(error, ExchangeError::NoIdToken), "{error:?}");
    assert!(error.is_answered_refusal());
    assert!(!error.refuses_the_operator());
}

#[tokio::test]
async fn an_unreachable_token_endpoint_is_a_transport_failure() {
    let provider = Provider::start().await;
    let oidc = Oidc {
        token_endpoint: String::from("http://127.0.0.1:9/token"),
        ..provider.oidc()
    };
    let error = exchange(&reqwest::Client::new(), &settings(oidc), &pending(), "c1")
        .await
        .expect_err("unreachable");
    assert!(
        matches!(error, ExchangeError::Transport { .. }),
        "{error:?}"
    );
    assert!(!error.is_answered_refusal());
}

#[tokio::test]
async fn a_key_set_that_answers_other_than_200_is_refused() {
    let provider = Provider::bare().await;
    Mock::given(method("GET"))
        .and(path(JWKS_PATH))
        .respond_with(ResponseTemplate::new(404))
        .mount(&provider.server)
        .await;
    let pending = pending();
    let error = with_claims(&provider, &pending, &provider.claims(pending.nonce()))
        .await
        .expect_err("refused");
    assert!(
        matches!(
            error,
            ExchangeError::KeySetRefused {
                status: StatusCode::NOT_FOUND
            }
        ),
        "{error:?}"
    );
    assert!(!error.is_answered_refusal(), "the operator can retry");
}

#[tokio::test]
async fn a_key_set_that_does_not_read_is_refused() {
    let provider = Provider::bare().await;
    Mock::given(method("GET"))
        .and(path(JWKS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string("not a key set"))
        .mount(&provider.server)
        .await;
    let pending = pending();
    let error = with_claims(&provider, &pending, &provider.claims(pending.nonce()))
        .await
        .expect_err("refused");
    assert!(
        matches!(error, ExchangeError::KeySetBody { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn an_unreachable_key_set_is_a_transport_failure() {
    let provider = Provider::bare().await;
    let pending = pending();
    let oidc = Oidc {
        jwks_uri: String::from("http://127.0.0.1:9/jwks.json"),
        ..provider.oidc()
    };
    let id_token = provider.id_token(&provider.claims(pending.nonce()));
    provider.answer_tokens(200, &token_answer(&id_token)).await;
    let error = exchange(&reqwest::Client::new(), &settings(oidc), &pending, "c1")
        .await
        .expect_err("unreachable");
    assert!(
        matches!(error, ExchangeError::KeySetTransport { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn an_id_token_signed_by_another_key_under_the_published_kid_does_not_verify() {
    let provider = Provider::start().await;
    let pending = pending();
    let impostor = SigningKey::generate(KID);
    let error = with_id_token(
        &provider,
        &pending,
        &impostor.encoding,
        &es256(Some(KID)),
        &provider.claims(pending.nonce()),
    )
    .await
    .expect_err("refused");
    assert!(matches!(error, ExchangeError::IdToken { .. }), "{error:?}");
    assert!(error.refuses_the_operator());
}

#[tokio::test]
async fn an_id_token_naming_a_kid_the_key_set_does_not_hold_is_refused() {
    let provider = Provider::start().await;
    let pending = pending();
    let error = with_id_token(
        &provider,
        &pending,
        &provider.key.encoding,
        &es256(Some("unknown")),
        &provider.claims(pending.nonce()),
    )
    .await
    .expect_err("refused");
    assert!(matches!(error, ExchangeError::Key), "{error:?}");
    assert!(error.refuses_the_operator());
}

#[tokio::test]
async fn an_id_token_without_a_kid_verifies_against_a_key_set_of_one_key() {
    let provider = Provider::start().await;
    let pending = pending();
    let signed_in = with_id_token(
        &provider,
        &pending,
        &provider.key.encoding,
        &es256(None),
        &provider.claims(pending.nonce()),
    )
    .await;
    assert!(signed_in.is_ok(), "{:?}", signed_in.err());
}

#[tokio::test]
async fn an_id_token_without_a_kid_is_refused_when_the_key_set_holds_several() {
    let provider = Provider::bare().await;
    let other = SigningKey::generate("k2");
    provider
        .publish(&[provider.key.jwk.clone(), other.jwk.clone()])
        .await;
    let pending = pending();
    let error = with_id_token(
        &provider,
        &pending,
        &provider.key.encoding,
        &es256(None),
        &provider.claims(pending.nonce()),
    )
    .await
    .expect_err("refused");
    assert!(matches!(error, ExchangeError::Key), "{error:?}");
}

#[tokio::test]
async fn an_id_token_signed_with_a_shared_secret_is_refused() {
    let provider = Provider::start().await;
    let pending = pending();
    let mut header = Header::new(Algorithm::HS256);
    header.kid = Some(String::from(KID));
    let error = with_id_token(
        &provider,
        &pending,
        &EncodingKey::from_secret(b"a-secret-anyone-could-know"),
        &header,
        &provider.claims(pending.nonce()),
    )
    .await
    .expect_err("refused");
    assert!(matches!(error, ExchangeError::Key), "{error:?}");
}

#[tokio::test]
async fn an_id_token_that_is_no_jwt_is_refused() {
    let provider = Provider::start().await;
    let error = exchanged(&provider, &pending(), 200, &token_answer("not-a-jwt"))
        .await
        .expect_err("refused");
    assert!(matches!(error, ExchangeError::Key), "{error:?}");
    assert!(error.refuses_the_operator());
}

/// The error of an exchange whose ID Token carries `claims` changed by
/// `change`.
async fn refused_for(change: impl FnOnce(&mut Value)) -> ExchangeError {
    let provider = Provider::start().await;
    let pending = pending();
    let claims = claims_with(&provider, &pending, change);
    with_claims(&provider, &pending, &claims)
        .await
        .expect_err("refused")
}

#[tokio::test]
async fn an_id_token_from_another_issuer_does_not_verify() {
    let error = refused_for(|claims| claims["iss"] = json!("https://another.example")).await;
    assert!(matches!(error, ExchangeError::IdToken { .. }), "{error:?}");
}

#[tokio::test]
async fn an_id_token_for_another_audience_does_not_verify() {
    let error = refused_for(|claims| claims["aud"] = json!("another-client")).await;
    assert!(matches!(error, ExchangeError::IdToken { .. }), "{error:?}");
}

#[tokio::test]
async fn an_id_token_naming_an_audience_beyond_this_console_is_refused() {
    let error = refused_for(|claims| claims["aud"] = json!([CLIENT_ID, "another-client"])).await;
    assert!(matches!(error, ExchangeError::Audience), "{error:?}");
    assert!(error.refuses_the_operator());
}

#[tokio::test]
async fn an_id_token_whose_audience_is_this_console_alone_in_an_array_verifies() {
    let provider = Provider::start().await;
    let pending = pending();
    let claims = claims_with(&provider, &pending, |claims| {
        claims["aud"] = json!([CLIENT_ID]);
    });
    assert!(with_claims(&provider, &pending, &claims).await.is_ok());
}

#[tokio::test]
async fn an_id_token_issued_to_another_client_is_refused() {
    let error = refused_for(|claims| claims["azp"] = json!("another-client")).await;
    assert!(matches!(error, ExchangeError::AuthorizedParty), "{error:?}");
    assert!(error.refuses_the_operator());
}

#[tokio::test]
async fn an_id_token_issued_to_this_console_by_azp_verifies() {
    let provider = Provider::start().await;
    let pending = pending();
    let claims = claims_with(&provider, &pending, |claims| {
        claims["azp"] = json!(CLIENT_ID);
    });
    assert!(with_claims(&provider, &pending, &claims).await.is_ok());
}

#[tokio::test]
async fn an_expired_id_token_does_not_verify() {
    let error = refused_for(|claims| {
        let now = jiff::Timestamp::now().as_second();
        claims["exp"] = json!(now - 3600);
    })
    .await;
    assert!(matches!(error, ExchangeError::IdToken { .. }), "{error:?}");
}

#[tokio::test]
async fn an_id_token_expired_within_the_leeway_still_verifies() {
    let provider = Provider::start().await;
    let pending = pending();
    let claims = claims_with(&provider, &pending, |claims| {
        claims["exp"] = json!(jiff::Timestamp::now().as_second() - 10);
    });
    assert!(with_claims(&provider, &pending, &claims).await.is_ok());
}

#[tokio::test]
async fn an_id_token_without_a_subject_does_not_verify() {
    let error = refused_for(|claims| {
        claims.as_object_mut().expect("an object").remove("sub");
    })
    .await;
    assert!(matches!(error, ExchangeError::IdToken { .. }), "{error:?}");
}

#[tokio::test]
async fn an_id_token_without_a_nonce_is_refused() {
    let error = refused_for(|claims| {
        claims.as_object_mut().expect("an object").remove("nonce");
    })
    .await;
    assert!(matches!(error, ExchangeError::Nonce), "{error:?}");
    assert!(error.refuses_the_operator());
}

#[tokio::test]
async fn an_id_token_with_another_sign_ins_nonce_is_refused() {
    let other = pending();
    let error = refused_for(|claims| claims["nonce"] = json!(other.nonce())).await;
    assert!(matches!(error, ExchangeError::Nonce), "{error:?}");
}

#[tokio::test]
async fn a_refresh_sends_the_refresh_grant_and_reads_the_rotated_tokens() {
    let provider = Provider::start().await;
    provider
        .answer_tokens(
            200,
            &json!({
                "access_token": "synthetic-access-2",
                "token_type": "Bearer",
                "expires_in": 600,
                "refresh_token": "synthetic-refresh-2",
                "scope": "openid user/CodeSystem.r",
                "id_token": "not-read",
            }),
        )
        .await;
    let refreshed = refresh(
        &reqwest::Client::new(),
        &settings(provider.oidc()),
        &SecretString::from("synthetic-refresh-1"),
    )
    .await
    .expect("the refresh succeeds");
    assert_eq!("synthetic-access-2", refreshed.access_token.expose_secret());
    assert_eq!(
        Some("synthetic-refresh-2"),
        refreshed
            .refresh_token
            .as_ref()
            .map(ExposeSecret::expose_secret)
    );
    assert_eq!(
        Some(std::time::Duration::from_secs(600)),
        refreshed.expires_in
    );
    assert_eq!(
        Some(vec![
            String::from("openid"),
            String::from("user/CodeSystem.r")
        ]),
        refreshed.scopes
    );
    let requests = provider.requests_at(TOKEN_PATH).await;
    let [request] = requests.as_slice() else {
        panic!("one token request");
    };
    let fields = form(request);
    assert_eq!("refresh_token", fields["grant_type"]);
    assert_eq!("synthetic-refresh-1", fields["refresh_token"]);
    assert_eq!(CLIENT_ID, fields["client_id"]);
}

#[tokio::test]
async fn a_refresh_answer_that_names_no_scope_leaves_the_scopes_unchanged() {
    let provider = Provider::start().await;
    provider
        .answer_tokens(200, &json!({"access_token": "a2", "token_type": "Bearer"}))
        .await;
    let refreshed = refresh(
        &reqwest::Client::new(),
        &settings(provider.oidc()),
        &SecretString::from("r1"),
    )
    .await
    .expect("the refresh succeeds");
    assert_eq!(None, refreshed.scopes);
    assert!(refreshed.refresh_token.is_none());
    assert_eq!(None, refreshed.expires_in);
}

#[tokio::test]
async fn a_refused_refresh_is_an_answered_refusal() {
    let provider = Provider::start().await;
    provider
        .answer_tokens(400, &json!({"error": "invalid_grant"}))
        .await;
    let error = refresh(
        &reqwest::Client::new(),
        &settings(provider.oidc()),
        &SecretString::from("r1"),
    )
    .await
    .expect_err("refused");
    assert!(error.is_answered_refusal(), "{error:?}");
}
