//! The resource server over the write routes and the admin listener:
//! `ferrohealth-smart`, configured by `[auth]`.
//!
//! The issuer is a `wiremock` server that publishes an OpenID Connect discovery
//! document and a JWKS; the RFC 9068 access tokens are minted in the test with
//! a key generated for the run, so no key material is committed. The requirements asserted are
//! SMART App Launch conformance and discovery
//! (<https://hl7.org/fhir/smart-app-launch/conformance.html>), the version 2
//! scope syntax
//! (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>),
//! and the bearer challenge of RFC 6750 §3.

use argon2::Argon2;
use argon2::password_hash::PasswordHasher;
use axum::body::Body;
use base64::Engine;
use ferrohealth_smart::config::{BasicSection, BasicUser};
use ferroterm_server::config::Config;
use ferroterm_server::smart::Smart;
use http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use http::{Request, StatusCode};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};
use secrecy::SecretString;
use serde_json::{Value, json};
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::fixture::{self, Server, SmartSetup};

const COLOURS: &str = "http://ferroterm.test/CodeSystem/colours";

fn colours() -> Value {
    json!({
        "resourceType": "CodeSystem",
        "url": COLOURS,
        "version": "1.0",
        "status": "active",
        "content": "complete",
        "concept": [{"code": "red", "display": "Red"}]
    })
}

/// One ECDSA P-256 signing key, generated for this test run.
struct SigningKey {
    /// The PKCS#8 private key `jsonwebtoken` signs with.
    pkcs8: Vec<u8>,
    /// The uncompressed public point, `0x04 || X || Y` (RFC 5480 §2.2).
    public: Vec<u8>,
    /// The `kid` the JWK and the token header carry (RFC 7517 §4.5).
    kid: String,
}

impl SigningKey {
    fn generate(kid: &str) -> Self {
        let rng = SystemRandom::new();
        let document = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng)
            .expect("generates a key");
        let pair =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, document.as_ref(), &rng)
                .expect("reads the key back");
        Self {
            pkcs8: document.as_ref().to_vec(),
            public: pair.public_key().as_ref().to_vec(),
            kid: kid.to_owned(),
        }
    }

    /// The public key as a JWK (RFC 7517 §4, RFC 7518 §6.2.1).
    fn jwk(&self) -> Value {
        let url = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let x = self.public.get(1..33).expect("the point carries X");
        let y = self.public.get(33..65).expect("the point carries Y");
        json!({
            "kty": "EC",
            "crv": "P-256",
            "alg": "ES256",
            "use": "sig",
            "kid": self.kid,
            "x": url.encode(x),
            "y": url.encode(y),
        })
    }

    /// `claims` signed as an ES256 token naming this key.
    fn sign(&self, claims: &Value) -> String {
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(self.kid.clone());
        // RFC 9068 §2.1: an access token is typed `at+jwt`.
        header.typ = Some(String::from("at+jwt"));
        self.sign_with(&header, claims)
    }

    /// `claims` signed with this key under a header the caller chose.
    fn sign_with(&self, header: &Header, claims: &Value) -> String {
        encode(header, claims, &EncodingKey::from_ec_der(&self.pkcs8)).expect("signs")
    }
}

/// The claims of an RFC 9068 access token minted for `issuer`.
///
/// RFC 9068 §2.2 requires `iss`, `exp`, `aud`, `sub`, `client_id`, `iat` and
/// `jti`.
fn claims(issuer: &str, scope: &str, expires_in: i64) -> Value {
    let now = jiff::Timestamp::now().as_second();
    json!({
        "iss": issuer,
        "sub": "ferroterm-test-client",
        "client_id": "ferroterm-test-client",
        "jti": format!("jti-{now}"),
        "aud": fixture::AUDIENCE,
        "iat": now,
        "nbf": now - 5,
        "exp": now + expires_in,
        "scope": scope,
    })
}

/// The discovery document of the test issuer.
fn discovery(base: &str) -> Value {
    json!({
        "issuer": base,
        "jwks_uri": format!("{base}/jwks"),
        "authorization_endpoint": format!("{base}/authorize"),
        "token_endpoint": format!("{base}/token"),
        "introspection_endpoint": format!("{base}/introspect"),
        // The extra values are what a real issuer publishes and SMART does not
        // define for these members; the served document drops them.
        "grant_types_supported": ["client_credentials", "authorization_code", "refresh_token"],
        "scopes_supported": ["system/CodeSystem.cud", "system/ValueSet.cud"],
        "response_types_supported": ["code"],
        "token_endpoint_auth_methods_supported": ["private_key_jwt", "client_secret_jwt"],
        "code_challenge_methods_supported": ["plain", "S256"],
    })
}

/// The discovery document of an issuer a public client signs in to: no client
/// authentication and the OpenID Connect scope.
fn public_discovery(base: &str) -> Value {
    json!({
        "issuer": base,
        "jwks_uri": format!("{base}/jwks"),
        "authorization_endpoint": format!("{base}/authorize"),
        "token_endpoint": format!("{base}/token"),
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "scopes_supported": ["openid", "fhirUser", "user/CodeSystem.cud"],
        "response_types_supported": ["code"],
        "token_endpoint_auth_methods_supported": ["none", "client_secret_basic"],
        "code_challenge_methods_supported": ["plain", "S256"],
    })
}

/// The discovery document of an issuer that omits every optional member and
/// advertises a patient scope this server refuses.
fn sparse_discovery(base: &str) -> Value {
    json!({
        "issuer": base,
        "jwks_uri": format!("{base}/jwks"),
        "authorization_endpoint": format!("{base}/authorize"),
        "token_endpoint": format!("{base}/token"),
        "scopes_supported": ["openid", "patient/CodeSystem.cud", "launch/patient", "user/ValueSet.cud"],
    })
}

/// A `wiremock` issuer publishing the key set and the document `build` writes
/// for its own base URL.
async fn issuer_publishing(key: &SigningKey, build: fn(&str) -> Value) -> MockServer {
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(build(&base)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"keys": [key.jwk()]})))
        .mount(&server)
        .await;
    server
}

/// A `wiremock` issuer publishing the document and the key set.
async fn issuer(key: &SigningKey) -> MockServer {
    issuer_publishing(key, discovery).await
}

/// A `PUT` of `body` at `uri` carrying `token`, when one is given.
async fn put_with(
    server: &Server,
    uri: &str,
    body: &Value,
    token: Option<&str>,
) -> http::Response<Body> {
    let mut request = Request::put(uri).header(http::header::CONTENT_TYPE, "application/fhir+json");
    if let Some(token) = token {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = request.body(Body::from(body.to_string())).expect("request");
    server.send(request).await
}

/// A `PUT` of the colours code system carrying `token`, when one is given.
async fn put_colours(server: &Server, token: Option<&str>) -> http::Response<Body> {
    put_with(server, "/r4b/CodeSystem/colours", &colours(), token).await
}

/// The capability strings of a served SMART configuration document.
fn capabilities(document: &Value) -> Vec<String> {
    document["capabilities"]
        .as_array()
        .expect("capabilities are required")
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// The served SMART configuration document of `version`.
async fn configuration(server: &Server, version: &str) -> Value {
    let (code, content_type, body) = server
        .get_text(
            &format!("/{version}/.well-known/smart-configuration"),
            Some("application/json"),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{version}: {body}");
    assert_eq!(content_type, "application/json", "{version}");
    serde_json::from_str(&body).expect("json")
}

/// The `WWW-Authenticate` value of a response.
fn challenge(response: &http::Response<Body>) -> String {
    fixture::header(response, WWW_AUTHENTICATE).unwrap_or_default()
}

// A deployment that turned `[auth]` off is served the way `Serving::new`
// serves: no credential is asked for anywhere.
#[tokio::test]
async fn a_server_with_auth_off_leaves_every_route_open() {
    let server = Server::start_persisting();
    let response = put_colours(&server, None).await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "a write answers without a token when authentication is off"
    );
    assert_eq!(
        fixture::header(&response, WWW_AUTHENTICATE),
        None,
        "no challenge is ever sent"
    );
    let (status, _) = server.get("/r4b/CodeSystem/colours").await;
    assert_eq!(status, StatusCode::OK);
}

// RFC 6750 §3.1: a request that carried no authentication is challenged
// without an error code.
#[tokio::test]
async fn a_write_without_a_token_is_challenged() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let response = put_colours(&server, None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let sent = challenge(&response);
    assert!(sent.starts_with("Bearer realm="), "{sent}");
    assert!(
        !sent.contains("error="),
        "a request with no credential names no error: {sent}"
    );
    let (_, body) = fixture::json(response).await;
    assert_eq!(body["resourceType"], "OperationOutcome");
    assert_eq!(body["issue"][0]["code"], "login");
}

#[tokio::test]
async fn the_read_surface_answers_without_a_token() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let cat = ferroterm_testkit::snomed::sctid(ferroterm_testkit::snomed::item(
        ferroterm_testkit::snomed::CAT,
    ));
    for uri in [
        String::from("/r4b/metadata"),
        String::from("/r4b/CodeSystem"),
        format!("/r4b/CodeSystem/$lookup?system=http://snomed.info/sct&code={cat}"),
        String::from("/r4b/ValueSet/$expand?url=http://snomed.info/sct?fhir_vs"),
    ] {
        let (status, body) = server.get(&uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }
}

// The version 2 scope for a create is `system/CodeSystem.c`
// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
#[tokio::test]
async fn a_token_with_the_scope_writes_and_one_without_it_is_refused() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let read_only = key.sign(&claims(&mock.uri(), "system/CodeSystem.rs", 300));
    let response = put_colours(&server, Some(&read_only)).await;
    let sent = challenge(&response);
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "{sent}");
    assert!(sent.contains("error=\"insufficient_scope\""), "{sent}");
    // RFC 6750 §3 makes the `scope` attribute optional, and the refusal names
    // no scope anywhere, so it teaches a caller nothing about its grants.
    assert!(!sent.contains("scope="), "{sent}");
    let (_, body) = fixture::json(response).await;
    assert_eq!(body["issue"][0]["code"], "forbidden");
    let diagnostics = body["issue"][0]["diagnostics"].as_str().unwrap_or_default();
    assert!(!diagnostics.contains("CodeSystem."), "{diagnostics}");

    let writer = key.sign(&claims(&mock.uri(), "system/CodeSystem.cud", 300));
    let (status, body) = fixture::json(put_colours(&server, Some(&writer)).await).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

// `$closure` keeps a stored closure table
// (<https://hl7.org/fhir/R4B/conceptmap-operation-closure.html>), so it is
// gated with the writes; SMART names no scope for it.
#[tokio::test]
async fn closure_is_gated_as_a_concept_map_update() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let call = json!({
        "resourceType": "Parameters",
        "parameter": [{"name": "name", "valueString": "pets"}]
    });

    let (status, _) = server.post("/r4b/$closure", &call).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "no token, no closure table"
    );

    let read_only = key.sign(&claims(&mock.uri(), "system/ConceptMap.rs", 300));
    let (status, _) = server
        .post_with_header(
            "/r4b/$closure",
            &call,
            AUTHORIZATION.as_str(),
            &format!("Bearer {read_only}"),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let writer = key.sign(&claims(&mock.uri(), "system/ConceptMap.cud", 300));
    let (status, body) = server
        .post_with_header(
            "/r4b/$closure",
            &call,
            AUTHORIZATION.as_str(),
            &format!("Bearer {writer}"),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

// The version 1 `.write` scope maps to `.cud`, which the specification states
// in the same section.
#[tokio::test]
async fn the_version_one_write_scope_is_accepted() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let writer = key.sign(&claims(&mock.uri(), "system/CodeSystem.write", 300));
    let response = put_colours(&server, Some(&writer)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

// The user compartment is "data that a user can access", which is what a
// person signed in to an editor carries
// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
#[tokio::test]
async fn a_user_update_scope_writes_a_put_and_a_user_create_scope_does_not() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let colour_set = json!({
        "resourceType": "ValueSet",
        "url": "http://ferroterm.test/ValueSet/colour-set",
        "version": "1.0",
        "status": "active",
        "compose": {"include": [{"system": COLOURS}]}
    });

    let creator = key.sign(&claims(&mock.uri(), "user/ValueSet.c", 300));
    let response = put_with(
        &server,
        "/r4b/ValueSet/colour-set",
        &colour_set,
        Some(&creator),
    )
    .await;
    let sent = challenge(&response);
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "a `PUT` needs the update letter: {sent}"
    );
    assert!(sent.contains("error=\"insufficient_scope\""), "{sent}");
    let (_, body) = fixture::json(response).await;
    assert_eq!(body["issue"][0]["code"], "forbidden");
    let diagnostics = body["issue"][0]["diagnostics"].as_str().unwrap_or_default();
    assert!(!diagnostics.contains("ValueSet."), "{diagnostics}");

    let updater = key.sign(&claims(&mock.uri(), "user/ValueSet.u", 300));
    let response = put_with(
        &server,
        "/r4b/ValueSet/colour-set",
        &colour_set,
        Some(&updater),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

// The version 1 `.write` maps to `.cud` in the user compartment as it does in
// the system one
// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
#[tokio::test]
async fn the_version_one_user_write_scope_creates_a_concept_map() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let map = json!({
        "resourceType": "ConceptMap",
        "url": "http://ferroterm.test/ConceptMap/colours-hues",
        "version": "1.0",
        "status": "active",
        "group": [{
            "source": COLOURS,
            "target": "http://ferroterm.test/CodeSystem/hues",
            "element": [{
                "code": "red",
                "target": [{"code": "crimson", "equivalence": "equivalent"}]
            }]
        }]
    });

    let (code, _) = server.post("/r4b/ConceptMap", &map).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED, "a create needs a token");

    let writer = key.sign(&claims(&mock.uri(), "user/ConceptMap.write", 300));
    let (code, body) = server
        .post_with_header(
            "/r4b/ConceptMap",
            &map,
            AUTHORIZATION.as_str(),
            &format!("Bearer {writer}"),
        )
        .await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
}

// A terminology server holds no patient record, so a patient-compartment scope
// selects nothing on it
// (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
#[tokio::test]
async fn a_patient_scope_is_refused_on_every_write_route() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let token = key.sign(&claims(
        &mock.uri(),
        "patient/CodeSystem.cud patient/ValueSet.cruds patient/*.*",
        300,
    ));

    let response = put_colours(&server, Some(&token)).await;
    let sent = challenge(&response);
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "{sent}");
    assert!(sent.contains("error=\"insufficient_scope\""), "{sent}");
    let (_, body) = fixture::json(response).await;
    assert_eq!(body["issue"][0]["code"], "forbidden");

    let (code, _) = server
        .post_with_header(
            "/r4b/CodeSystem",
            &colours(),
            AUTHORIZATION.as_str(),
            &format!("Bearer {token}"),
        )
        .await;
    assert_eq!(code, StatusCode::FORBIDDEN, "a create is refused too");

    // With a launch context the grant reaches that patient's compartment
    // (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>),
    // which holds no terminology resource, so the write is refused all the same.
    let mut launched = claims(&mock.uri(), "patient/CodeSystem.cud patient/*.*", 300);
    launched["patient"] = json!("p-1");
    let launched = key.sign(&launched);
    let response = put_colours(&server, Some(&launched)).await;
    let sent = challenge(&response);
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "{sent}");
    assert!(sent.contains("error=\"insufficient_scope\""), "{sent}");
    let (_, body) = fixture::json(response).await;
    assert_eq!(body["issue"][0]["code"], "forbidden");
    let (code, _) = server
        .post_with_header(
            "/r4b/CodeSystem",
            &colours(),
            AUTHORIZATION.as_str(),
            &format!("Bearer {launched}"),
        )
        .await;
    assert_eq!(code, StatusCode::FORBIDDEN, "a create is refused too");
}

// The public client profile needs the authorization endpoint, `S256`, and the
// capabilities that name the flow
// (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
#[tokio::test]
async fn the_configuration_publishes_what_a_public_client_needs() {
    let key = SigningKey::generate("k1");
    let mock = issuer_publishing(&key, public_discovery).await;
    let base = mock.uri();
    let server = Server::start_persisting_with_smart(&base, None, None).await;

    for version in ["r4", "r4b", "r5", "r6"] {
        let document = configuration(&server, version).await;
        assert_eq!(
            document["authorization_endpoint"],
            format!("{base}/authorize"),
            "{version}"
        );
        // RFC 7636 §4.2 defines `S256`, and `plain` is a SHALL NOT here even
        // though this issuer advertises it.
        assert_eq!(
            document["code_challenge_methods_supported"],
            json!(["S256"]),
            "{version}"
        );
        // `issuer` is required where `sso-openid-connect` is claimed.
        assert_eq!(document["issuer"], base, "{version}");
        let claimed = capabilities(&document);
        for capability in [
            "launch-standalone",
            "client-public",
            "permission-user",
            "permission-v1",
            "sso-openid-connect",
        ] {
            assert!(
                claimed.iter().any(|value| value == capability),
                "{version}: {capability} is missing from {claimed:?}"
            );
        }
        for refused in ["permission-v2", "permission-patient"] {
            assert!(
                !claimed.iter().any(|value| value == refused),
                "{version}: {refused} is not supported and not claimed: {claimed:?}"
            );
        }
    }
}

// The console is a confidential client of its own (app/ferroterm-viewer) and
// is configured with its client there, so the discovery document carries the
// members SMART defines and no client of this server's.
#[tokio::test]
async fn the_configuration_names_no_client_of_its_own() {
    let key = SigningKey::generate("k1");
    let mock = issuer_publishing(&key, public_discovery).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    for version in ["r4", "r4b", "r5", "r6"] {
        let document = configuration(&server, version).await;
        assert!(
            document["ferroterm_viewer_client_id"].is_null(),
            "{version}: the server publishes no viewer client: {document}"
        );
    }
}

// An omitted member means its documented default, so an issuer that publishes
// neither still gets a conformant document
// (<https://openid.net/specs/openid-connect-discovery-1_0.html>, §3).
#[tokio::test]
async fn an_issuer_that_omits_the_optional_members_gets_their_documented_defaults() {
    let key = SigningKey::generate("k1");
    let mock = issuer_publishing(&key, sparse_discovery).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let document = configuration(&server, "r4b").await;
    // The default is `["authorization_code", "implicit"]`, of which SMART
    // names only the first as an option for this member.
    assert_eq!(
        document["grant_types_supported"],
        json!(["authorization_code"])
    );
    // The default is `client_secret_basic`.
    assert_eq!(
        document["token_endpoint_auth_methods_supported"],
        json!(["client_secret_basic"])
    );
    let claimed = capabilities(&document);
    for capability in ["launch-standalone", "client-confidential-symmetric"] {
        assert!(
            claimed.iter().any(|value| value == capability),
            "{capability} is missing from {claimed:?}"
        );
    }
    // The server SHALL support every scope it lists, so the issuer's own list,
    // with the patient scope this server refuses, is never republished; the
    // member lists `[auth] scopes_supported`, empty here
    // (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
    assert!(document["scopes_supported"].is_null(), "{document}");
    assert!(
        !document.to_string().contains("patient/CodeSystem.cud"),
        "{document}"
    );
}

// The PKCE member is required and `S256` is a SHALL in it, so it is published
// whatever the issuer advertises
// (<https://hl7.org/fhir/smart-app-launch/app-launch.html>).
#[tokio::test]
async fn the_pkce_member_is_published_even_when_the_issuer_advertises_none() {
    let key = SigningKey::generate("k1");
    let mock = issuer_publishing(&key, sparse_discovery).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let document = configuration(&server, "r4b").await;
    assert_eq!(
        document["code_challenge_methods_supported"],
        json!(["S256"])
    );
}

// `sso-openid-connect` rests on the issuer's OpenID Connect profile, so an
// issuer that lists no `openid` scope does not earn it
// (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
#[tokio::test]
async fn sso_openid_connect_is_claimed_only_where_the_issuer_lists_openid() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let document = configuration(&server, "r4b").await;
    let claimed = capabilities(&document);
    assert!(
        !claimed.iter().any(|value| value == "sso-openid-connect"),
        "{claimed:?}"
    );
    assert!(
        !claimed.iter().any(|value| value == "client-public"),
        "this issuer authenticates every client: {claimed:?}"
    );
    // `issuer` is omitted where `sso-openid-connect` is not claimed.
    assert!(document["issuer"].is_null(), "{document}");
}

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

// Every write interaction is gated, not only the `PUT` the other cases use
// (<https://hl7.org/fhir/R4B/http.html>).
#[tokio::test]
async fn create_and_delete_are_gated_like_update() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let writer = key.sign(&claims(&mock.uri(), "system/CodeSystem.cud", 300));

    let (status, _) = server.post("/r4b/CodeSystem", &colours()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "a create needs a token");
    let (status, body) = server
        .post_with_header(
            "/r4b/CodeSystem",
            &colours(),
            AUTHORIZATION.as_str(),
            &format!("Bearer {writer}"),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let created = body["id"].as_str().expect("an id").to_owned();

    let deletion = |token: Option<&str>| {
        let mut request = Request::delete(format!("/r4b/CodeSystem/{created}"));
        if let Some(token) = token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        request.body(Body::empty()).expect("request")
    };
    let response = server.send(deletion(None)).await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "a delete needs a token"
    );
    let response = server.send(deletion(Some(&writer))).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

/// A `POST /reload` carrying `authorization` as its credential, when given.
fn reload(authorization: Option<String>) -> Request<Body> {
    let mut request = Request::post("/reload");
    if let Some(value) = authorization {
        request = request.header(AUTHORIZATION, value);
    }
    request.body(Body::empty()).expect("request")
}

#[tokio::test]
async fn the_admin_listener_requires_the_configured_scope() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server =
        Server::start_persisting_with_smart(&mock.uri(), None, Some("__ferroterm.reload")).await;
    let admin = server.admin_router();

    let response = admin.clone().oneshot(reload(None)).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(challenge(&response).starts_with("Bearer realm="));

    let wrong = key.sign(&claims(&mock.uri(), "system/CodeSystem.cud", 300));
    let response = admin
        .clone()
        .oneshot(reload(Some(format!("Bearer {wrong}"))))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    // RFC 6750 §3 makes the `scope` attribute optional, and the refusal names
    // no scope anywhere.
    let sent = challenge(&response);
    assert!(sent.contains("error=\"insufficient_scope\""), "{sent}");
    assert!(!sent.contains("scope="), "{sent}");

    let right = key.sign(&claims(&mock.uri(), "__ferroterm.reload", 300));
    let response = admin
        .clone()
        .oneshot(reload(Some(format!("Bearer {right}"))))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

// The admin scope is a product scope, `__ferroterm.admin` when `[auth]` names
// none (SMART App Launch 2.2.0, Scopes and Launch Context, §Extensibility).
#[tokio::test]
async fn the_default_admin_scope_is_the_extension_scope_of_ferroterm() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;
    let admin = server.admin_router();

    let old = key.sign(&claims(&mock.uri(), "ferroterm/admin", 300));
    let response = admin
        .clone()
        .oneshot(reload(Some(format!("Bearer {old}"))))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let token = key.sign(&claims(&mock.uri(), "__ferroterm.admin", 300));
    let response = admin
        .oneshot(reload(Some(format!("Bearer {token}"))))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

// SMART App Launch 2.2.0, Scopes and Launch Context, §Extensibility: a scope
// of a server's own is a full URI or a string starting with `__`.
#[tokio::test]
async fn an_admin_scope_outside_the_extension_forms_refuses_the_start() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let mut setup = SmartSetup::for_issuer(&mock.uri());
    setup.auth.product_scope = String::from("ferroterm/reload");
    let config = Server::smart_config(dir.path(), setup);
    let error = Smart::start(&config)
        .await
        .expect_err("the start is refused");
    assert!(
        causes(&error).contains("ferroterm/reload"),
        "{}",
        causes(&error)
    );
}

/// The issuer of `mock` publishing `first` once, then `first` and `second`.
async fn rotating_issuer(mock: &MockServer, first: &SigningKey, second: &SigningKey) {
    let base = mock.uri();
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(discovery(&base)))
        .mount(mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"keys": [first.jwk()]})))
        .up_to_n_times(1)
        .mount(mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"keys": [first.jwk(), second.jwk()]})),
        )
        .mount(mock)
        .await;
}

/// A server over `mock` that reads its key set again at most once per
/// `interval_seconds`.
async fn with_refresh_interval(mock: &MockServer, interval_seconds: u64) -> Server {
    let mut setup = SmartSetup::for_issuer(&mock.uri());
    setup.auth.jwks_refresh_interval_seconds = interval_seconds;
    Server::start_persisting_with_smart_setup(setup).await
}

// RFC 7517 §4.5: a rotated key arrives under a new `kid`, so a token naming one
// the server has not read is the signal to read the set again.
#[tokio::test]
async fn an_unknown_key_id_refreshes_the_key_set() {
    let first = SigningKey::generate("k1");
    let second = SigningKey::generate("k2");
    let mock = MockServer::start().await;
    rotating_issuer(&mock, &first, &second).await;
    let server = with_refresh_interval(&mock, 0).await;
    assert_eq!(jwks_reads(&mock).await, 1, "the start reads the set once");

    let rotated = second.sign(&claims(&mock.uri(), "system/CodeSystem.cud", 300));
    let response = put_colours(&server, Some(&rotated)).await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "the refreshed set carries the new key"
    );
    assert_eq!(jwks_reads(&mock).await, 2, "the unknown kid read it again");
}

// A stream of forged `kid`s must not turn the server into a load on the
// issuer, so a second unknown `kid` inside the refresh interval reads nothing
// (no specification governs the interval: `ferrohealth-smart`'s own design,
// measured from the last read, the one at start included).
#[tokio::test]
async fn an_unknown_key_id_inside_the_interval_reads_nothing_and_is_refused() {
    let first = SigningKey::generate("k1");
    let second = SigningKey::generate("k2");
    let mock = MockServer::start().await;
    rotating_issuer(&mock, &first, &second).await;
    let server = with_refresh_interval(&mock, 3600).await;

    let third = SigningKey::generate("k3");
    let unknown = third.sign(&claims(&mock.uri(), "system/CodeSystem.cud", 300));
    let response = put_colours(&server, Some(&unknown)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        jwks_reads(&mock).await,
        1,
        "the interval holds the next refresh off"
    );
}

/// How many times the issuer's key set was read.
async fn jwks_reads(mock: &MockServer) -> usize {
    mock.received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/jwks")
        .count()
}

/// `error` and every cause under it, on one line.
fn causes(error: &dyn std::error::Error) -> String {
    let mut out = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        out.push_str(": ");
        out.push_str(&cause.to_string());
        source = cause.source();
    }
    out
}

/// The configuration of a server over the edition in `dir` whose `[auth]`
/// trusts `issuer`.
fn trusting(dir: &std::path::Path, issuer: &str) -> Config {
    Server::smart_config(dir, SmartSetup::for_issuer(issuer))
}

#[tokio::test]
async fn a_discovery_failure_refuses_the_start() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let error = Smart::start(&trusting(dir.path(), &mock.uri()))
        .await
        .expect_err("the start is refused");
    let reason = format!("{error}");
    assert!(
        reason.contains(&mock.uri()),
        "the reason names the issuer that did not answer: {reason}"
    );
    assert!(
        std::error::Error::source(&error).is_some(),
        "the refusal carries its cause"
    );
}

// RFC 8414 §2 and OpenID Connect Discovery 1.0 §3: the issuer is an https URL,
// because these bytes decide which signatures the server trusts.
#[tokio::test]
async fn a_cleartext_issuer_outside_the_loopback_refuses_the_start() {
    let dir = tempfile::tempdir().expect("tempdir");
    let error = Smart::start(&trusting(dir.path(), "http://auth.example.org/realms/tx"))
        .await
        .expect_err("the start is refused");
    let chain = causes(&error);
    assert!(chain.contains("not an https URL"), "{chain}");
}

// The family's fail-closed rule (no specification governs it: our own design):
// a server that serves writes or the admin listener starts only once `[auth]`
// names an issuer, or says it is off.
#[tokio::test]
async fn a_guarded_surface_without_an_issuer_refuses_the_start() {
    for (resources, admin, named) in [
        (true, false, "FERROTERM_RESOURCES"),
        (false, true, "FERROTERM_ADMIN_LISTEN"),
    ] {
        let config = Config {
            resources: resources.then(|| std::path::PathBuf::from("resources.redb")),
            admin_listen: admin.then(|| String::from("127.0.0.1:0")),
            ..Config::default()
        };
        let error = Smart::start(&config)
            .await
            .expect_err("the start is refused");
        let reason = error.to_string();
        assert!(reason.contains(named), "{reason}");
        assert!(reason.contains("FERROTERM_AUTH__ISSUER"), "{reason}");
    }
}

#[tokio::test]
async fn a_read_only_server_or_auth_off_starts_without_an_issuer() {
    let read_only = Smart::start(&Config::default())
        .await
        .expect("a read-only server needs no issuer");
    assert!(read_only.is_none());
    let mut off = Config {
        resources: Some(std::path::PathBuf::from("resources.redb")),
        admin_listen: Some(String::from("127.0.0.1:0")),
        ..Config::default()
    };
    off.auth.enabled = false;
    let started = Smart::start(&off).await.expect("auth off starts");
    assert!(
        started.is_none(),
        "the guarded surfaces authenticate nobody"
    );
}

// RFC 9068 §4: the resource server validates that `aud` names it, so an issuer
// without an audience refuses the start rather than accepting a token minted
// for any resource server of that issuer (RFC 8725 §3.9).
#[tokio::test]
async fn an_issuer_without_an_audience_refuses_the_start() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let dir = tempfile::tempdir().expect("tempdir");
    let mut setup = SmartSetup::for_issuer(&mock.uri());
    setup.auth.audience = Vec::new();
    setup.base_url = Some(String::from(PROXIED));
    let config = Server::smart_config(dir.path(), setup);
    let error = Smart::start(&config)
        .await
        .expect_err("the start is refused");
    let chain = causes(&error);
    assert!(chain.contains("audience"), "{chain}");
}

// SMART App Launch: the document lives at `[base]/.well-known/smart-configuration`
// and `token_endpoint`, `grant_types_supported`, and `capabilities` are required.
#[tokio::test]
async fn the_smart_configuration_is_served_from_the_issuer_document() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let base = mock.uri();
    let mut setup = SmartSetup::for_issuer(&base);
    setup.auth.scopes_supported = vec![
        String::from("system/CodeSystem.cud"),
        String::from("system/ValueSet.cud"),
    ];
    let server = Server::start_persisting_with_smart_setup(setup).await;

    for version in ["r4", "r4b", "r5", "r6"] {
        let document = configuration(&server, version).await;
        assert_eq!(document["token_endpoint"], format!("{base}/token"));
        assert_eq!(
            document["authorization_endpoint"],
            format!("{base}/authorize")
        );
        assert_eq!(document["jwks_uri"], format!("{base}/jwks"));
        assert_eq!(document["scopes_supported"][0], "system/CodeSystem.cud");
        // `issuer` is conditional on the `sso-openid-connect` capability, which
        // this server does not claim, so the document omits it.
        assert!(document["issuer"].is_null(), "{version}: {document}");
        // The options each member admits are the ones SMART names; the extras
        // the issuer publishes are dropped, and `plain` is a SHALL NOT.
        assert_eq!(
            document["grant_types_supported"],
            json!(["authorization_code", "client_credentials"]),
            "{version}"
        );
        assert_eq!(
            document["code_challenge_methods_supported"],
            json!(["S256"]),
            "{version}"
        );
        assert_eq!(
            document["token_endpoint_auth_methods_supported"],
            json!(["private_key_jwt"]),
            "{version}"
        );
        let claimed = capabilities(&document);
        assert!(
            claimed
                .iter()
                .any(|value| value == "client-confidential-asymmetric"),
            "the issuer advertises private_key_jwt: {claimed:?}"
        );
        assert!(
            !claimed.iter().any(|value| value == "permission-v2"),
            "the granular scope syntax is refused, so it is not claimed: {claimed:?}"
        );
    }

    // The specification fixes the media type whatever the request asks for.
    let (status, content_type, _) = server
        .get_text(
            "/r4b/.well-known/smart-configuration",
            Some("application/fhir+xml"),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, "application/json");
}

#[tokio::test]
async fn an_unconfigured_issuer_serves_no_smart_configuration() {
    let server = Server::start_persisting();
    let (status, body) = server.get("/r4b/.well-known/smart-configuration").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["issue"][0]["code"], "not-found");
}

// SMART App Launch declares `SMART-on-FHIR` in `rest.security.service` and the
// `oauth-uris` extension with `token` and `authorize`
// (<https://hl7.org/fhir/smart-app-launch/1.0.0/conformance/index.html>).
#[tokio::test]
async fn the_capability_statement_declares_the_service_and_the_oauth_uris() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let base = mock.uri();
    let server = Server::start_persisting_with_smart(&base, None, None).await;

    for version in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server.get(&format!("/{version}/metadata")).await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        let security = &body["rest"][0]["security"];
        let codes: Vec<&str> = security["service"][0]["coding"]
            .as_array()
            .expect("a coding")
            .iter()
            .filter_map(|coding| coding["code"].as_str())
            .collect();
        assert!(codes.contains(&"SMART-on-FHIR"), "{version}: {codes:?}");
        let uris = security["extension"]
            .as_array()
            .and_then(|list| {
                list.iter().find(|extension| {
                    extension["url"]
                        == "http://fhir-registry.smarthealthit.org/StructureDefinition/oauth-uris"
                })
            })
            .expect("the oauth-uris extension");
        let parts = uris["extension"].as_array().expect("sub-extensions");
        let named = |name: &str| {
            parts
                .iter()
                .find(|part| part["url"] == name)
                .map(|part| part["valueUri"].clone())
        };
        assert_eq!(named("token"), Some(json!(format!("{base}/token"))));
        assert_eq!(named("authorize"), Some(json!(format!("{base}/authorize"))));
        assert_eq!(
            named("introspect"),
            Some(json!(format!("{base}/introspect")))
        );
    }
}

/// The address a proxy reaches the server at in the audience tests.
const PROXIED: &str = "https://tx.example.org";

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

/// Sixteen random bytes generated for this test run, so no credential or salt
/// is committed.
fn random_bytes() -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    ring::rand::SecureRandom::fill(&SystemRandom::new(), &mut bytes).expect("random bytes");
    bytes
}

/// A password generated for this test run.
fn password() -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes())
}

/// The Argon2id PHC string of `password` under a salt generated for the run.
fn phc(password: &str) -> SecretString {
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

// Machine-to-machine callers authenticate with HTTP Basic (RFC 7617) as a
// `[[auth.basic.users]]` entry beside the bearer tokens; the user's scopes
// decide what it reaches, as a token's would.
#[tokio::test]
async fn a_basic_user_writes_and_reloads_with_its_configured_scopes() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let (editor, sync, wrong) = (password(), password(), password());
    let mut setup = SmartSetup::for_issuer(&mock.uri());
    setup.auth.basic = Some(BasicSection {
        users: vec![
            BasicUser {
                user: String::from("editor"),
                password_hash: Some(phc(&editor)),
                scopes: vec![String::from("system/CodeSystem.cud")],
                ..BasicUser::default()
            },
            BasicUser {
                user: String::from("sync"),
                password_hash: Some(phc(&sync)),
                scopes: vec![String::from("__ferroterm.admin")],
                ..BasicUser::default()
            },
        ],
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
    let response = server.send(write(basic("sync", &sync))).await;
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
        .oneshot(reload(Some(basic("sync", &sync))))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

// A refusal is an `OperationOutcome` in the format the request asked for
// (<https://hl7.org/fhir/R4B/http.html#mime-type>), whatever the gate's
// renderer can see.
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
