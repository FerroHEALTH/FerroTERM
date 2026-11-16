// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admin listener behind `ferrohealth-smart`: every route but
//! `GET /health` needs a credential carrying the product scope, and an
//! `[auth]` that admits no credential refuses the start.
//!
//! No specification governs the admin surface: our own design.

use argon2::Argon2;
use argon2::password_hash::PasswordHasher as _;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use ferrohealth_smart::basic::StoreError;
use ferrohealth_smart::config::credentials::OauthSection;
use ferrohealth_smart::config::{
    AuthMode, AuthSection, BasicGrant, BasicGrants, BasicSection, BasicUser, IntrospectionSection,
    IssuerEntry,
};
use ferrohealth_smart::kind::IssuerKind;
use ferroterm_sync::auth::{AdminAuth, AuthError, SYNC_SCOPE};
use secrecy::SecretString;
use tower::ServiceExt as _;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{Harness, TestClock};

/// The password of every test user, synthetic.
const PASSWORD: &str = "a-synthetic-password";

/// An Argon2id PHC string of [`PASSWORD`], salted with a fixed test salt.
fn phc() -> SecretString {
    let hash = Argon2::default()
        .hash_password_with_salt(PASSWORD.as_bytes(), b"ferroterm-sync-t")
        .expect("hashes");
    SecretString::from(hash.to_string())
}

/// `Basic` credentials of `user` (RFC 7617 §2).
fn basic(user: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{PASSWORD}"));
    format!("Basic {encoded}")
}

/// An `[auth]` admitting the Basic users `users`.
fn admitting(users: &[&str]) -> AuthSection {
    AuthSection {
        basic: Some(BasicSection {
            users: users
                .iter()
                .map(|user| BasicUser {
                    user: String::from(*user),
                    password_hash: Some(phc()),
                    ..BasicUser::default()
                })
                .collect(),
            ..BasicSection::default()
        }),
        ..AuthSection::default()
    }
}

/// An `[auth]` admitting two Basic users, `operator` and `reader`.
fn section() -> AuthSection {
    admitting(&["operator", "reader"])
}

/// The `[basic_grants]` of `grants`, one scope per user.
fn granting(grants: &[(&str, &str)]) -> BasicGrants {
    BasicGrants {
        users: grants
            .iter()
            .map(|(user, scope)| {
                let grant = BasicGrant {
                    scopes: vec![String::from(*scope)],
                    ..BasicGrant::default()
                };
                (String::from(*user), grant)
            })
            .collect(),
    }
}

/// The grants of [`section`]: `operator` the product scope and `reader` an
/// unrelated one.
fn grants() -> BasicGrants {
    granting(&[("operator", SYNC_SCOPE), ("reader", "user/CodeSystem.rs")])
}

/// No `[basic_grants]` table.
fn none() -> BasicGrants {
    BasicGrants::default()
}

/// The guarded admin router over a service that has run nothing.
async fn guarded() -> Router {
    let harness = Harness::new().await;
    let service = harness.service(Vec::new(), TestClock::new("2026-10-01T03:00:00Z", 0));
    let auth = AdminAuth::start(&section(), &grants())
        .await
        .expect("the section starts")
        .expect("authentication is on");
    auth.guard(ferroterm_sync::admin::router(service))
}

/// The status `method` on `path` answers with `authorization`.
async fn status(
    router: &Router,
    method: &str,
    path: &str,
    authorization: Option<&str>,
) -> (StatusCode, Option<String>) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(value) = authorization {
        request = request.header("authorization", value);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::empty()).expect("a request"))
        .await
        .expect("the listener answers");
    let challenge = response
        .headers()
        .get("www-authenticate")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    (response.status(), challenge)
}

#[tokio::test]
async fn the_health_probe_needs_no_credential() {
    let router = guarded().await;
    assert_eq!(
        status(&router, "GET", "/health", None).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn the_liveness_and_readiness_probes_need_no_credential() {
    let router = guarded().await;
    for path in ["/health/liveness", "/health/readiness"] {
        assert_eq!(
            status(&router, "GET", path, None).await.0,
            StatusCode::OK,
            "{path}"
        );
    }
}

#[tokio::test]
async fn every_other_route_refuses_a_request_without_a_credential() {
    let router = guarded().await;
    for (method, path) in [
        ("POST", "/run"),
        ("POST", "/activate"),
        ("GET", "/runs"),
        ("GET", "/runs/2026-10-01T03-00-00Z"),
        ("GET", "/metrics"),
        ("GET", "/elsewhere"),
    ] {
        let (answered, challenge) = status(&router, method, path, None).await;
        assert_eq!(answered, StatusCode::UNAUTHORIZED, "{method} {path}");
        assert!(
            challenge.is_some_and(|sent| sent.contains("Basic realm=\"ferroterm-sync\"")),
            "{method} {path} names the realm in its challenge"
        );
    }
}

#[tokio::test]
async fn a_credential_without_the_product_scope_is_forbidden() {
    let router = guarded().await;
    let reader = basic("reader");
    for (method, path) in [("GET", "/runs"), ("GET", "/metrics"), ("POST", "/activate")] {
        let (answered, _) = status(&router, method, path, Some(&reader)).await;
        assert_eq!(answered, StatusCode::FORBIDDEN, "{method} {path}");
    }
}

#[tokio::test]
async fn a_wrong_password_is_refused() {
    let router = guarded().await;
    let encoded = base64::engine::general_purpose::STANDARD.encode("operator:not-the-password");
    let (answered, _) = status(&router, "GET", "/runs", Some(&format!("Basic {encoded}"))).await;
    assert_eq!(answered, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_product_scope_reaches_the_records_and_the_metrics() {
    let router = guarded().await;
    let operator = basic("operator");
    assert_eq!(
        status(&router, "GET", "/runs", Some(&operator)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        status(&router, "GET", "/metrics", Some(&operator)).await.0,
        StatusCode::OK
    );
    assert_eq!(
        status(&router, "GET", "/runs/no-such-run", Some(&operator))
            .await
            .0,
        StatusCode::NOT_FOUND,
        "past the gate, the route answers as it would unguarded"
    );
}

#[tokio::test]
async fn a_section_admitting_no_credential_refuses_the_start() {
    let refused = AdminAuth::start(&AuthSection::default(), &none()).await;
    assert!(
        matches!(refused, Err(AuthError::Config { .. })),
        "an enabled [auth] with no issuer and no Basic user fails closed: {refused:?}"
    );
}

/// The audience the introspection tests name.
const AUDIENCE: &str = "ferroterm-sync";

/// A `wiremock` issuer whose metadata names an introspection endpoint
/// (RFC 8414 §2, `introspection_endpoint`), and nothing mounted there yet.
async fn introspecting_issuer() -> MockServer {
    let server = MockServer::start().await;
    let base = server.uri();
    let metadata = format!(
        r#"{{"issuer":"{base}","token_endpoint":"{base}/token","introspection_endpoint":"{base}/introspect","jwks_uri":"{base}/jwks"}}"#
    );
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(metadata, "application/json"))
        .mount(&server)
        .await;
    server
}

/// An `[auth]` introspecting the tokens of `issuer` under a client secret.
fn introspecting(issuer: &str) -> AuthSection {
    AuthSection {
        issuer: vec![IssuerEntry {
            issuer: ferrohealth_secret::secret_url::SecretUrl::new(issuer),
            kind: Some(IssuerKind::Machines),
            audience: vec![String::from(AUDIENCE)],
            mode: AuthMode::Introspection,
            introspection: Some(IntrospectionSection {
                client: Some(OauthSection {
                    client_id: String::from("ferroterm-sync"),
                    client_secret: Some(SecretString::from("a-synthetic-secret")),
                    ..OauthSection::default()
                }),
                ..IntrospectionSection::default()
            }),
            ..IssuerEntry::default()
        }],
        ..AuthSection::default()
    }
}

/// The endpoint answers `body` with `status` for the token `token`.
async fn answers(issuer: &MockServer, token: &str, status: u16, body: String) {
    Mock::given(method("POST"))
        .and(path("/introspect"))
        .and(body_string_contains(format!("token={token}")))
        .respond_with(ResponseTemplate::new(status).set_body_raw(body, "application/json"))
        .mount(issuer)
        .await;
}

/// An active introspection response (RFC 7662 §2.2) for `issuer` carrying
/// `scope`, with the members SMART App Launch 2.2.0, Token Introspection,
/// requires.
fn active(issuer: &str, scope: &str) -> String {
    let exp = jiff::Timestamp::now().as_second() + 300;
    format!(
        r#"{{"active":true,"iss":"{issuer}","aud":"{AUDIENCE}","sub":"operator","client_id":"operator","scope":"{scope}","exp":{exp}}}"#
    )
}

/// The guarded admin router over an introspecting `[auth]`.
async fn introspected(issuer: &MockServer) -> Router {
    let harness = Harness::new().await;
    let service = harness.service(Vec::new(), TestClock::new("2026-10-01T03:00:00Z", 0));
    let auth = AdminAuth::start(&introspecting(&issuer.uri()), &none())
        .await
        .expect("the introspecting section starts")
        .expect("authentication is on");
    auth.guard(ferroterm_sync::admin::router(service))
}

// RFC 7662 §2.2: an active response admits the token with the scope it
// names, so the product scope opens the records and another scope does not.
#[tokio::test]
async fn an_active_introspected_token_is_admitted_with_its_scopes() {
    let issuer = introspecting_issuer().await;
    answers(
        &issuer,
        "operator-token",
        200,
        active(&issuer.uri(), SYNC_SCOPE),
    )
    .await;
    answers(
        &issuer,
        "reader-token",
        200,
        active(&issuer.uri(), "system/CodeSystem.rs"),
    )
    .await;
    let router = introspected(&issuer).await;
    assert_eq!(
        status(&router, "GET", "/runs", Some("Bearer operator-token"))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        status(&router, "GET", "/runs", Some("Bearer reader-token"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}

// RFC 7662 §2.2: a response whose `active` is false refuses the token, and
// RFC 6750 §3.1 answers it `invalid_token`.
#[tokio::test]
async fn an_inactive_introspected_token_is_refused() {
    let issuer = introspecting_issuer().await;
    answers(
        &issuer,
        "revoked-token",
        200,
        String::from(r#"{"active":false}"#),
    )
    .await;
    let router = introspected(&issuer).await;
    let (answered, challenge) = status(&router, "GET", "/runs", Some("Bearer revoked-token")).await;
    assert_eq!(answered, StatusCode::UNAUTHORIZED);
    assert!(
        challenge.is_some_and(|sent| sent.contains("error=\"invalid_token\"")),
        "the challenge names invalid_token"
    );
}

// An endpoint that cannot answer is an upstream failure, never a refusal of
// the token: our own design over the crate's `503`.
#[tokio::test]
async fn a_failing_introspection_endpoint_answers_503() {
    let issuer = introspecting_issuer().await;
    answers(&issuer, "any-token", 500, String::from("{}")).await;
    let router = introspected(&issuer).await;
    assert_eq!(
        status(&router, "GET", "/runs", Some("Bearer any-token"))
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

// The introspecting issuer is read at the start, so one whose metadata names
// no introspection endpoint refuses the start by name.
#[tokio::test]
async fn an_issuer_without_an_introspection_endpoint_refuses_the_start() {
    let issuer = MockServer::start().await;
    let base = issuer.uri();
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(
                r#"{{"issuer":"{base}","token_endpoint":"{base}/token","jwks_uri":"{base}/jwks"}}"#
            ),
            "application/json",
        ))
        .mount(&issuer)
        .await;
    let refused = AdminAuth::start(&introspecting(&base), &none()).await;
    assert!(
        matches!(refused, Err(AuthError::Introspector { .. })),
        "the refusal names the introspection endpoint: {refused:?}"
    );
}

#[tokio::test]
async fn enabled_false_turns_the_guard_off() {
    let off = AuthSection {
        enabled: false,
        ..AuthSection::default()
    };
    assert!(
        AdminAuth::start(&off, &none())
            .await
            .expect("starts")
            .is_none()
    );
}

#[tokio::test]
async fn a_deployment_names_its_own_product_scope() {
    let named = AuthSection {
        product_scope: String::from("__example.sync"),
        ..section()
    };
    let auth = AdminAuth::start(&named, &grants())
        .await
        .expect("starts")
        .expect("on");
    assert_eq!(auth.scope(), "__example.sync");
}

/// What `method` on `path` answers with `authorization`: the status, the
/// content type and the body read as JSON.
async fn answered(
    router: &Router,
    method: &str,
    path: &str,
    authorization: Option<&str>,
) -> (StatusCode, Option<String>, serde_json::Value) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(value) = authorization {
        request = request.header("authorization", value);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::empty()).expect("a request"))
        .await
        .expect("the listener answers");
    let status = response.status();
    let kind = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("a body");
    (status, kind, serde_json::from_slice(&bytes).expect("JSON"))
}

#[tokio::test]
async fn every_refusal_and_error_is_a_problem_that_quotes_nothing_it_was_sent() {
    let router = guarded().await;
    let mut bodies = String::new();
    let encoded =
        base64::engine::general_purpose::STANDARD.encode("operator:a-sentinel-secret-9137");
    let wrong = format!("Basic {encoded}");
    let (status, kind, body) = answered(&router, "GET", "/runs", Some(&wrong)).await;
    assert_eq!(StatusCode::UNAUTHORIZED, status);
    assert_eq!(Some("application/problem+json"), kind.as_deref());
    assert_eq!(
        "https://ferroterm.eu/docs/operate/problems.html#unauthenticated",
        body["type"]
    );
    bodies.push_str(&body.to_string());
    let (status, _, body) = answered(&router, "GET", "/runs", Some(&basic("reader"))).await;
    assert_eq!(StatusCode::FORBIDDEN, status);
    assert_eq!(
        "https://ferroterm.eu/docs/operate/problems.html#not-permitted",
        body["type"]
    );
    bodies.push_str(&body.to_string());
    let operator = basic("operator");
    let (status, _, body) =
        answered(&router, "GET", "/runs/a-sentinel-run-4471", Some(&operator)).await;
    assert_eq!(StatusCode::NOT_FOUND, status);
    assert_eq!(
        "https://ferroterm.eu/docs/operate/problems.html#run-not-found",
        body["type"]
    );
    bodies.push_str(&body.to_string());
    let (status, _, body) = answered(&router, "GET", "/a-sentinel-path", Some(&operator)).await;
    assert_eq!(StatusCode::NOT_FOUND, status);
    assert_eq!("about:blank", body["type"]);
    bodies.push_str(&body.to_string());
    for sent in [
        "a-sentinel-secret-9137",
        "a-sentinel-run-4471",
        "a-sentinel-path",
    ] {
        assert!(!bodies.contains(sent), "{bodies}");
    }
}

/// The reason the listener refuses to start under `section` and `grants`,
/// with its causes.
async fn refusal(section: &AuthSection, grants: &BasicGrants) -> String {
    let error = AdminAuth::start(section, grants)
        .await
        .expect_err("the start is refused");
    let mut reason = error.to_string();
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        reason.push_str(": ");
        reason.push_str(&cause.to_string());
        source = cause.source();
    }
    reason
}

// ferrohealth-smart 0.15 deleted the named refusal of the flat `issuer`, so
// the strict load refuses it as a value of the wrong shape.
#[test]
fn the_flat_issuer_of_the_old_shape_is_refused_by_the_strict_load() {
    let refused = ferroterm_sync::config::Config::from_sources(
        Some("[auth]\nissuer = \"https://as.example.org\"\n"),
        &std::collections::BTreeMap::new(),
    );
    assert!(refused.is_err(), "{refused:?}");
}

#[tokio::test]
async fn an_issuer_without_a_kind_refuses_the_start_naming_the_key() {
    let issuer = MockServer::start().await;
    let mut section = introspecting(&issuer.uri());
    for entry in &mut section.issuer {
        entry.kind = None;
    }
    let reason = refusal(&section, &none()).await;
    assert!(reason.contains("auth.issuer.kind"), "{reason}");
}

// A Basic credential is a person's, and a `system/` scope is a backend
// service's grant (SMART App Launch 2.2.0, Scopes and Launch Context), so
// `ferrohealth-smart` refuses a Basic user granted one (GHSA-jhg2-jwjp-hq74),
// and the start carries that refusal.
#[tokio::test]
async fn a_basic_user_with_a_system_scope_refuses_the_start_naming_the_user() {
    let machine = admitting(&["batch-loader"]);
    let grant = BasicGrant {
        scopes: vec![
            String::from("system/CodeSystem.rs"),
            String::from(SYNC_SCOPE),
        ],
        ..BasicGrant::default()
    };
    let grants = BasicGrants {
        users: std::iter::once((String::from("batch-loader"), grant)).collect(),
    };
    let error = AdminAuth::start(&machine, &grants)
        .await
        .expect_err("the start is refused");
    let AuthError::BasicGrants {
        source: StoreError::SystemScope { username, scope },
    } = &error
    else {
        panic!("the grants refuse the Basic user's system scope: {error:?}");
    };
    assert_eq!(username, "batch-loader");
    assert_eq!(scope, "system/CodeSystem.rs");
    let reason = refusal(&machine, &grants).await;
    assert!(reason.contains("batch-loader"), "{reason}");
    assert!(reason.contains("system/"), "{reason}");
}

// A grant whose user-id `[[auth.basic.users]]` does not list would grant no
// one, so a misspelt user-id refuses the start (our own design).
#[tokio::test]
async fn a_grant_naming_no_listed_user_refuses_the_start_naming_the_user() {
    let grants = granting(&[("opertor", SYNC_SCOPE)]);
    let issuer = introspecting_issuer().await;
    for section in [admitting(&["operator"]), introspecting(&issuer.uri())] {
        let error = AdminAuth::start(&section, &grants)
            .await
            .expect_err("the start is refused");
        let AuthError::BasicGrants {
            source: StoreError::UnknownUser { username },
        } = &error
        else {
            panic!("the grants refuse the unknown user: {error:?}");
        };
        assert_eq!(username, "opertor");
    }
}

// A Basic user's scopes and roles are `[basic_grants]`'s since
// ferrohealth-smart 0.23, so the strict load refuses either under
// `[[auth.basic.users]]` as a key it does not know.
#[test]
fn a_scope_or_a_role_under_the_basic_user_is_refused_by_the_strict_load() {
    let load = |text: &str| {
        ferroterm_sync::config::Config::from_sources(Some(text), &std::collections::BTreeMap::new())
    };
    let user = "[[auth.basic.users]]\nuser = \"operator\"\npassword_hash_file = \"/run/secrets/operator\"\n";
    assert!(load(user).is_ok(), "the credential alone loads");
    for key in ["scopes = [\"__ferroterm.sync\"]", "roles = [\"auditor\"]"] {
        let text = format!("{user}{key}\n");
        assert!(load(&text).is_err(), "{text}");
    }
}
