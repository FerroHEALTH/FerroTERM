// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The authorization code exchange at the provider's token endpoint and the
//! check of the ID Token it answers with.
//!
//! The code goes to the token endpoint with the PKCE verifier (RFC 6749
//! §4.1.3, RFC 7636 §4.5), with the client secret as HTTP Basic when the
//! console is a confidential client (RFC 6749 §2.3.1). The answer must carry
//! a bearer access token and an ID Token, whose signature verifies against
//! the provider's JWK Set and whose `iss`, `aud`, `azp`, `exp` and `nonce`
//! are those the sign-in expects (OpenID Connect Core 1.0 §3.1.3.7). No
//! token and no code is ever logged or rendered.

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use http::StatusCode;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;

use crate::config::settings::OidcSettings;
use crate::session::{PendingSignIn, SignedIn};
use url::Url;

/// The signature algorithms an ID Token may use: the asymmetric ones, so
/// only the provider's own key verifies it.
pub const ALGORITHMS: [Algorithm; 9] = [
    Algorithm::RS256,
    Algorithm::RS384,
    Algorithm::RS512,
    Algorithm::PS256,
    Algorithm::PS384,
    Algorithm::PS512,
    Algorithm::ES256,
    Algorithm::ES384,
    Algorithm::EdDSA,
];

/// How much clock skew the ID Token's times are allowed.
const LEEWAY: Duration = Duration::from_secs(60);

/// The most of a token endpoint's error answer the console reads.
const ERROR_BODY_LIMIT: usize = 16 * 1024;

/// Why a sign-in, or a refresh of its access token, could not be completed.
#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
    /// The provider could not be reached, or its answer could not be read.
    #[error("the OpenID Provider could not be reached")]
    Transport {
        /// What the HTTP client reported.
        #[source]
        source: reqwest::Error,
    },
    /// The token endpoint refused the request.
    #[error("the token endpoint answered {status}")]
    Refused {
        /// The status it answered with.
        status: StatusCode,
        /// The `error` code of its answer (RFC 6749 §5.2), when it carried one.
        error: Option<String>,
    },
    /// The token endpoint's answer is not a token response.
    #[error("the token endpoint's answer is not a token response")]
    Body {
        /// What the JSON reader refused.
        #[source]
        source: serde_json::Error,
    },
    /// The access token is not a bearer token, which the FHIR server takes.
    #[error("the access token is not a bearer token")]
    TokenType,
    /// The answer carries no ID Token.
    #[error("the token endpoint answered no ID Token")]
    NoIdToken,
    /// The provider's JWK Set could not be reached.
    #[error("the provider's JWK Set could not be reached")]
    KeySetTransport {
        /// What the HTTP client reported.
        #[source]
        source: reqwest::Error,
    },
    /// The provider's JWK Set location answered other than `200`.
    #[error("the provider's JWK Set answered {status}")]
    KeySetRefused {
        /// The status it answered with.
        status: StatusCode,
    },
    /// The provider's JWK Set is not a JWK Set (RFC 7517 §5).
    #[error("the provider's JWK Set does not read")]
    KeySetBody {
        /// What the JSON reader refused.
        #[source]
        source: serde_json::Error,
    },
    /// The ID Token is signed with an algorithm the console refuses, or by a
    /// key the JWK Set does not hold.
    #[error("the ID Token is signed with a key or an algorithm the console does not accept")]
    Key,
    /// The ID Token's signature or claims do not verify.
    #[error("the ID Token does not verify")]
    IdToken {
        /// What the verifier refused.
        #[source]
        source: jsonwebtoken::errors::Error,
    },
    /// The ID Token names an audience beyond this console's client id.
    #[error("the ID Token names an audience beyond this console")]
    Audience,
    /// The ID Token was issued to another client (`azp`).
    #[error("the ID Token was issued to another client")]
    AuthorizedParty,
    /// The ID Token's `nonce` is not the one the sign-in sent.
    #[error("the ID Token's nonce is not the sign-in's")]
    Nonce,
}

impl ExchangeError {
    /// Whether the failure is the ID Token's, which refuses the operator,
    /// rather than the provider's, which the operator can retry.
    #[must_use]
    pub const fn refuses_the_operator(&self) -> bool {
        matches!(
            self,
            Self::Key | Self::IdToken { .. } | Self::Audience | Self::AuthorizedParty | Self::Nonce
        )
    }

    /// Whether the provider answered the request and refused it, or answered
    /// something no token response is, rather than not answering at all or
    /// failing on its side (`5xx`).
    #[must_use]
    pub fn is_answered_refusal(&self) -> bool {
        match self {
            Self::Refused { status, .. } => status.is_client_error(),
            Self::Body { .. } | Self::TokenType | Self::NoIdToken => true,
            _ => self.refuses_the_operator(),
        }
    }
}

/// The token endpoint's answer (RFC 6749 §5.1, OpenID Connect Core 1.0
/// §3.1.3.3).
#[derive(Deserialize)]
pub(super) struct TokenResponse {
    /// The access token.
    pub(super) access_token: String,
    /// Its type, which must be `Bearer`.
    token_type: String,
    /// Its lifetime in seconds.
    pub(super) expires_in: Option<u64>,
    /// The refresh token, when the provider issued one.
    pub(super) refresh_token: Option<String>,
    /// The scopes granted, when they differ from those asked for or the
    /// provider states them anyway (RFC 6749 §5.1).
    pub(super) scope: Option<String>,
    /// The ID Token.
    id_token: Option<String>,
}

/// The token endpoint's error answer (RFC 6749 §5.2), of which the console
/// keeps the code alone.
#[derive(Deserialize)]
struct ErrorResponse {
    error: String,
}

/// The ID Token claims the console checks beyond those the verifier checks.
#[derive(Deserialize)]
struct IdTokenClaims {
    aud: Audience,
    nonce: Option<String>,
    azp: Option<String>,
    name: Option<String>,
    preferred_username: Option<String>,
}

impl IdTokenClaims {
    /// The operator's name the console shows: `name`, else
    /// `preferred_username` (OpenID Connect Core 1.0 §5.1).
    fn operator(self) -> Option<String> {
        [self.name, self.preferred_username]
            .into_iter()
            .flatten()
            .map(|name| name.trim().to_owned())
            .find(|name| !name.is_empty())
    }
}

/// An ID Token's `aud`: one string, or an array (OpenID Connect Core 1.0
/// §2).
#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Several(Vec<String>),
}

impl Audience {
    /// Whether the audience is `client_id` and nothing else.
    fn is_only(&self, client_id: &str) -> bool {
        match self {
            Self::One(audience) => audience == client_id,
            Self::Several(audiences) => matches!(audiences.as_slice(), [only] if only == client_id),
        }
    }
}

/// Exchanges `code` for the operator's tokens and checks the ID Token
/// against `pending`.
///
/// # Errors
/// Returns the [`ExchangeError`] of the step that failed.
pub async fn exchange(
    client: &reqwest::Client,
    oidc: &OidcSettings,
    pending: &PendingSignIn,
    code: &str,
) -> Result<SignedIn, ExchangeError> {
    let grant = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", oidc.redirect_uri.as_str()),
        ("code_verifier", pending.verifier().expose_secret()),
    ];
    let tokens = token_request(client, oidc, &grant).await?;
    let id_token = tokens.id_token.ok_or(ExchangeError::NoIdToken)?;
    let operator = verify_id_token(client, oidc, pending, &id_token).await?;
    // NOTE: RFC 6749 §5.1: an answer that names no scope granted the scope
    // requested, so the requested scopes stand for it.
    let scopes = tokens.scope.as_deref().map_or_else(
        || oidc.scopes.clone(),
        |granted| granted.split_whitespace().map(str::to_owned).collect(),
    );
    Ok(SignedIn {
        scopes,
        access_token: SecretString::from(tokens.access_token),
        expires_in: tokens.expires_in.map(Duration::from_secs),
        refresh_token: tokens.refresh_token.map(SecretString::from),
        id_token: Some(SecretString::from(id_token)),
        operator,
    })
}

/// Sends the parameters of the grant `grant` to the provider's token
/// endpoint and reads its bearer token answer (RFC 6749 §3.2, §5.1).
///
/// The console authenticates as a confidential client with HTTP Basic, and
/// a public client names itself in the form (RFC 6749 §2.3.1, §3.2.1).
///
/// # Errors
/// Returns [`ExchangeError::Transport`] when the provider cannot be reached,
/// [`ExchangeError::Refused`] for any answer but `200`,
/// [`ExchangeError::Body`] for one that is not a token response, and
/// [`ExchangeError::TokenType`] for a token that is not a bearer token.
pub(super) async fn token_request(
    client: &reqwest::Client,
    oidc: &OidcSettings,
    grant: &[(&str, &str)],
) -> Result<TokenResponse, ExchangeError> {
    let answer = authenticated(client, oidc, oidc.token_endpoint.clone())
        .body(grant_form(oidc, grant))
        .send()
        .await
        .map_err(|source| ExchangeError::Transport { source })?;
    let status = answer.status();
    let body = answer
        .bytes()
        .await
        .map_err(|source| ExchangeError::Transport { source })?;
    if status != StatusCode::OK {
        // NOTE: RFC 6749 §5.2: an error answer names its `error` code; a body
        // that names none is a refusal without one, never a different fault.
        let error = body
            .get(..ERROR_BODY_LIMIT.min(body.len()))
            .and_then(|head| serde_json::from_slice::<ErrorResponse>(head).ok())
            .map(|answer| answer.error);
        return Err(ExchangeError::Refused { status, error });
    }
    let tokens: TokenResponse =
        serde_json::from_slice(&body).map_err(|source| ExchangeError::Body { source })?;
    if !tokens.token_type.eq_ignore_ascii_case("bearer") {
        return Err(ExchangeError::TokenType);
    }
    Ok(tokens)
}

/// A form post to `endpoint` of the provider, authenticated as the console:
/// HTTP Basic for a confidential client, nothing for a public one, which
/// names itself in the form (RFC 6749 §2.3.1, §3.2.1).
fn authenticated(
    client: &reqwest::Client,
    oidc: &OidcSettings,
    endpoint: Url,
) -> reqwest::RequestBuilder {
    let request = client
        .post(endpoint)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, "application/json");
    let Some(secret) = &oidc.client_secret else {
        return request;
    };
    // NOTE: RFC 6749 §2.3.1: the id and the secret are each form-urlencoded
    // before they are joined and encoded for Basic.
    let encode =
        |text: &str| url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>();
    let credentials = SecretString::from(STANDARD.encode(format!(
        "{}:{}",
        encode(&oidc.client_id),
        encode(secret.expose_secret())
    )));
    request.header(
        AUTHORIZATION,
        format!("Basic {}", credentials.expose_secret()),
    )
}

/// Posts `form` to `endpoint` as the console, as a revocation request is
/// sent (RFC 7009 §2.1), and returns the provider's status.
///
/// # Errors
/// Returns the transport error when the provider cannot be reached.
pub(super) async fn client_post(
    client: &reqwest::Client,
    oidc: &OidcSettings,
    endpoint: Url,
    form: &[(&str, &str)],
) -> Result<StatusCode, reqwest::Error> {
    let answer = authenticated(client, oidc, endpoint)
        .body(grant_form(oidc, form))
        .send()
        .await?;
    Ok(answer.status())
}

/// The form of a token request carrying `grant`, naming the client in it
/// only for a public client, which sends no secret.
fn grant_form(oidc: &OidcSettings, grant: &[(&str, &str)]) -> String {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    form.extend_pairs(grant);
    if oidc.client_secret.is_none() {
        form.append_pair("client_id", &oidc.client_id);
    }
    form.finish()
}

/// Checks `id_token` against the provider's keys and `pending`, and returns
/// the operator's name it carries.
async fn verify_id_token(
    client: &reqwest::Client,
    oidc: &OidcSettings,
    pending: &PendingSignIn,
    id_token: &str,
) -> Result<Option<String>, ExchangeError> {
    let header = jsonwebtoken::decode_header(id_token).map_err(|_unreadable| ExchangeError::Key)?;
    if !ALGORITHMS.contains(&header.alg) {
        return Err(ExchangeError::Key);
    }
    let keys = key_set(client, oidc).await?;
    let jwk = match &header.kid {
        Some(kid) => keys.find(kid),
        None => match keys.keys.as_slice() {
            [only] => Some(only),
            _ => None,
        },
    }
    .ok_or(ExchangeError::Key)?;
    let key = DecodingKey::from_jwk(jwk).map_err(|_unusable| ExchangeError::Key)?;
    let mut validation = Validation::new(header.alg);
    validation.algorithms = vec![header.alg];
    validation.leeway = LEEWAY.as_secs();
    validation.set_issuer(&[oidc.issuer.as_str()]);
    validation.set_audience(&[oidc.client_id.as_str()]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    let claims = jsonwebtoken::decode::<IdTokenClaims>(id_token, &key, &validation)
        .map_err(|source| ExchangeError::IdToken { source })?
        .claims;
    // NOTE: OpenID Connect Core 1.0 §3.1.3.7 item 3: a token with an audience
    // beyond this client is refused, as no other audience is trusted here.
    if !claims.aud.is_only(&oidc.client_id) {
        return Err(ExchangeError::Audience);
    }
    // NOTE: OpenID Connect Core 1.0 §3.1.3.7 item 6: an `azp` present names the
    // client the token was issued to, which must be this console.
    if claims
        .azp
        .as_deref()
        .is_some_and(|azp| azp != oidc.client_id)
    {
        return Err(ExchangeError::AuthorizedParty);
    }
    // NOTE: OpenID Connect Core 1.0 §3.1.3.7 item 11: the `nonce` must be the
    // one the authorization request sent, so a replayed ID Token is refused.
    let nonce = claims.nonce.as_deref().ok_or(ExchangeError::Nonce)?;
    if aws_lc_rs::constant_time::verify_slices_are_equal(
        nonce.as_bytes(),
        pending.nonce().as_bytes(),
    )
    .is_err()
    {
        return Err(ExchangeError::Nonce);
    }
    Ok(claims.operator())
}

/// Reads the provider's JWK Set (RFC 7517 §5).
async fn key_set(client: &reqwest::Client, oidc: &OidcSettings) -> Result<JwkSet, ExchangeError> {
    let answer = client
        .get(oidc.jwks_uri.clone())
        .header(ACCEPT, "application/json")
        .send()
        .await
        .map_err(|source| ExchangeError::KeySetTransport { source })?;
    let status = answer.status();
    if status != StatusCode::OK {
        return Err(ExchangeError::KeySetRefused { status });
    }
    let body = answer
        .bytes()
        .await
        .map_err(|source| ExchangeError::KeySetTransport { source })?;
    serde_json::from_slice(&body).map_err(|source| ExchangeError::KeySetBody { source })
}
