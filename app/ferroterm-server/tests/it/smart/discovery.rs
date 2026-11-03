// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SMART configuration and the capability statement the server publishes from the issuer.

use http::StatusCode;
use serde_json::json;

use super::{
    SigningKey, capabilities, configuration, issuer, issuer_publishing, public_discovery,
    sparse_discovery,
};
use crate::fixture::Server;

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

// The console is a confidential client of its own (app/ferroterm-console) and
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
            document["ferroterm_console_client_id"].is_null(),
            "{version}: the server publishes no console client: {document}"
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
