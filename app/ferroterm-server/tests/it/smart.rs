//! The optional SMART App Launch bearer gate over the write routes and the
//! admin listener.
//!
//! The issuer is a `wiremock` server that publishes an OpenID Connect discovery
//! document and a JWKS; the tokens are minted in the test with a key generated
//! for the run, so no key material is committed. The requirements asserted are
//! SMART App Launch conformance and discovery
//! (<https://hl7.org/fhir/smart-app-launch/conformance.html>), the version 2
//! scope syntax
//! (<https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>),
//! and the bearer challenge of RFC 6750 §3.

use axum::body::Body;
use base64::Engine;
use ferroterm_server::config::Config;
use ferroterm_server::smart::Smart;
use http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use http::{Request, StatusCode};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};
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
        self.sign_with(&header, claims)
    }

    /// `claims` signed with this key under a header the caller chose.
    fn sign_with(&self, header: &Header, claims: &Value) -> String {
        encode(header, claims, &EncodingKey::from_ec_der(&self.pkcs8)).expect("signs")
    }
}

/// The claims of a token minted for `issuer`.
fn claims(issuer: &str, scope: &str, expires_in: i64) -> Value {
    let now = jiff::Timestamp::now().as_second();
    json!({
        "iss": issuer,
        "sub": "ferroterm-test-client",
        "aud": "ferroterm",
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

#[tokio::test]
async fn an_unconfigured_issuer_leaves_every_route_open() {
    let server = Server::start_persisting();
    let response = put_colours(&server, None).await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "a write answers without a token when no issuer is configured"
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
    // RFC 6750 §3 reads `scope` as the scope a token must carry, a conjunction
    // (RFC 6749 §3.3), so two alternatives are named in the outcome instead.
    assert!(!sent.contains("scope="), "{sent}");
    let (_, body) = fixture::json(response).await;
    assert_eq!(body["issue"][0]["code"], "forbidden");
    let diagnostics = body["issue"][0]["diagnostics"].as_str().unwrap_or_default();
    assert!(
        diagnostics.contains("`system/CodeSystem.u` or `user/CodeSystem.u`"),
        "{diagnostics}"
    );

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
    let diagnostics = body["issue"][0]["diagnostics"].as_str().unwrap_or_default();
    assert!(
        diagnostics.contains("`system/ValueSet.u` or `user/ValueSet.u`"),
        "{diagnostics}"
    );

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
    // The server SHALL support every scope it republishes, so the refused ones
    // are dropped (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
    assert_eq!(
        document["scopes_supported"],
        json!(["openid", "user/ValueSet.cud"])
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

    let anonymous = first.sign_with(
        &Header::new(Algorithm::ES256),
        &claims(&base, "system/CodeSystem.cud", 300),
    );
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

#[tokio::test]
async fn the_admin_listener_requires_the_configured_scope() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server =
        Server::start_persisting_with_smart(&mock.uri(), None, Some("ferroterm/reload")).await;
    let admin = server.admin_router();

    let reload = |token: Option<String>| {
        let mut request = Request::post("/reload");
        if let Some(token) = token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        request.body(Body::empty()).expect("request")
    };

    let response = admin.clone().oneshot(reload(None)).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(challenge(&response).starts_with("Bearer realm="));

    let wrong = key.sign(&claims(&mock.uri(), "system/CodeSystem.cud", 300));
    let response = admin
        .clone()
        .oneshot(reload(Some(wrong)))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    // RFC 6750 §3: one scope opens this route, so the challenge names it.
    assert!(
        challenge(&response).contains("scope=\"ferroterm/reload\""),
        "{}",
        challenge(&response)
    );

    let right = key.sign(&claims(&mock.uri(), "ferroterm/reload", 300));
    let response = admin
        .clone()
        .oneshot(reload(Some(right)))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

// RFC 7517 §4.5: a rotated key arrives under a new `kid`, so a token naming one
// the server has not read is the signal to read the set again.
#[tokio::test]
async fn an_unknown_key_id_refreshes_the_key_set_once() {
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
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"keys": [first.jwk()]})))
        .up_to_n_times(1)
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
    assert_eq!(jwks_reads(&mock).await, 1, "the start reads the set once");

    let rotated = second.sign(&claims(&base, "system/CodeSystem.cud", 300));
    let response = put_colours(&server, Some(&rotated)).await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "the refreshed set carries the new key"
    );
    assert_eq!(jwks_reads(&mock).await, 2, "the unknown kid read it again");

    let third = SigningKey::generate("k3");
    let unknown = third.sign(&claims(&base, "system/CodeSystem.cud", 300));
    let response = put_colours(&server, Some(&unknown)).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        jwks_reads(&mock).await,
        2,
        "the cooldown holds the next refresh off"
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

#[tokio::test]
async fn a_discovery_failure_refuses_the_start() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock)
        .await;
    let config = Config {
        oidc_issuer: Some(mock.uri()),
        ..Config::default()
    };
    let error = Smart::start(&config)
        .await
        .expect_err("the start is refused");
    let reason = format!("{error}");
    assert!(
        reason.contains("discovery document"),
        "the reason names what did not answer: {reason}"
    );
    assert!(
        std::error::Error::source(&error).is_some(),
        "the refusal carries its cause"
    );
}

// OpenID Connect Discovery 1.0 §3 and §7.1: the issuer's endpoints are https,
// because these bytes decide which signatures the server trusts.
#[tokio::test]
async fn a_cleartext_issuer_outside_the_loopback_refuses_the_start() {
    let config = Config {
        oidc_issuer: Some(String::from("http://auth.example.org/realms/tx")),
        ..Config::default()
    };
    let error = Smart::start(&config)
        .await
        .expect_err("the start is refused");
    let reason = format!("{error}");
    assert!(reason.contains("discovery document"), "{reason}");
    let cause = std::error::Error::source(&error)
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(cause.contains("not an https URL"), "{cause}");
}

// SMART App Launch: the document lives at `[base]/.well-known/smart-configuration`
// and `token_endpoint`, `grant_types_supported`, and `capabilities` are required.
#[tokio::test]
async fn the_smart_configuration_is_served_from_the_issuer_document() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let base = mock.uri();
    let server = Server::start_persisting_with_smart(&base, None, None).await;

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

/// A server behind [`PROXIED`] whose gate takes `audience` from the
/// environment.
async fn proxied(issuer: &str, audience: Option<&str>) -> Server {
    Server::start_persisting_with_smart_setup(SmartSetup {
        audience: audience.map(str::to_owned),
        base_url: Some(String::from(PROXIED)),
        ..SmartSetup::for_issuer(issuer)
    })
    .await
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

// The admin listener serves no FHIR interaction, so a base URL is not an
// audience for it: naming one leaves that surface as it was.
#[tokio::test]
async fn the_admin_listener_does_not_take_a_version_base_as_its_audience() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = proxied(&mock.uri(), None).await;
    let admin = server.admin_router();

    let mut payload = claims_for(&mock.uri(), json!("an-audience-of-its-own"));
    payload["scope"] = json!("ferroterm/admin");
    let request = Request::post("/reload")
        .header(AUTHORIZATION, format!("Bearer {}", key.sign(&payload)))
        .body(Body::empty())
        .expect("request");
    let response = admin.oneshot(request).await.expect("the admin answers");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a base URL alone does not start checking the admin audience"
    );
}

// RFC 7519 §4.1.3 leaves `aud` optional: a deployment that names neither a
// base nor an audience checks none, and the issuer bounds the token.
#[tokio::test]
async fn a_deployment_naming_no_base_and_no_audience_checks_no_audience() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let server = Server::start_persisting_with_smart(&mock.uri(), None, None).await;

    let elsewhere = key.sign(&claims_for(
        &mock.uri(),
        json!("https://elsewhere.example.org/r4b"),
    ));
    let response = put_colours(&server, Some(&elsewhere)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
}
