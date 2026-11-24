// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The issuer the SMART discovery document and the capability statement
//! name, chosen by `ferrohealth-smart`'s `describe`: `[auth]
//! described_issuer`, else the one SMART issuer; never a `federation` issuer
//! unnamed, and never by the order of the `[auth.issuer.<name>]` tables.
//! SMART App Launch 2.2.0, Conformance, §Metadata; which of several issuers
//! is described is our own design.

use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_smart::config::IssuerEntry;
use ferrohealth_smart::federation::FederationSection;
use ferrohealth_smart::kind::IssuerKind;
use ferroterm_server::smart::Smart;
use serde_json::{Value, json};
use wiremock::MockServer;

use super::{SigningKey, causes, configuration, issuer};
use crate::fixture::{AUDIENCE, Server, SmartSetup};

/// The `[auth.issuer.<name>]` table of `mock` as a SMART issuer.
fn entry(mock: &MockServer) -> IssuerEntry {
    IssuerEntry {
        issuer: SecretUrl::new(mock.uri()),
        audience: vec![String::from(AUDIENCE)],
        ..IssuerEntry::default()
    }
}

/// The `[auth.issuer.<name>]` table of `mock` as another party's
/// `federation` issuer, mapping the persons of `https://as.example.org`.
fn federation(mock: &MockServer) -> IssuerEntry {
    let section: FederationSection = serde_json::from_value(json!({
        "contact_point": false,
        "mapping": "iua",
        "countries": ["NL"],
        "issuing_authorities": ["CIBG"],
        "provider_identifier_prefixes": ["urn:oid:2.16.528.1.1007.3.3."],
        "exchanges_from": ["https://as.example.org"],
        "person_mapping": "hp_identifier",
        "fields": {
            "professional.country_code": [{"constant": "NL"}],
            "professional.issuing_authority_name": [{"constant": "CIBG"}],
        },
    }))
    .expect("the federation table reads");
    IssuerEntry {
        kind: Some(IssuerKind::Federation),
        federation: Some(section),
        ..entry(mock)
    }
}

/// The setup trusting `entries`, named `idp0`, `idp1` and on in this order,
/// describing `named` when set, by the name `elsewhere` when it is none of
/// them.
fn trusting(entries: Vec<IssuerEntry>, named: Option<&MockServer>) -> SmartSetup {
    let first = entries
        .first()
        .expect("an issuer")
        .issuer
        .expose()
        .to_owned();
    let mut setup = SmartSetup::for_issuer(&first);
    let tables: Vec<(String, IssuerEntry)> = entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| (format!("idp{index}"), entry))
        .collect();
    setup.auth.described_issuer = named.map(|mock| {
        tables
            .iter()
            .find(|(_, entry)| entry.issuer.expose() == mock.uri())
            .map_or_else(|| String::from("elsewhere"), |(name, _)| name.clone())
    });
    setup.auth.issuer = tables.into_iter().collect();
    setup
}

/// The `oauth-uris` members of the r4b capability statement, by name.
async fn oauth_uris(server: &Server) -> Vec<(String, String)> {
    let (_, body) = server.get("/r4b/metadata").await;
    body.pointer("/rest/0/security/extension")
        .and_then(Value::as_array)
        .and_then(|list| {
            list.iter().find(|extension| {
                extension["url"]
                    == "http://fhir-registry.smarthealthit.org/StructureDefinition/oauth-uris"
            })
        })
        .and_then(|uris| uris["extension"].as_array())
        .expect("the oauth-uris members")
        .iter()
        .filter_map(|part| {
            Some((
                part["url"].as_str()?.to_owned(),
                part["valueUri"].as_str()?.to_owned(),
            ))
        })
        .collect()
}

/// The reason the server refuses to start under `setup`.
async fn refused(setup: SmartSetup) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = Server::smart_config(dir.path(), setup);
    let error = Smart::start(&config)
        .await
        .expect_err("the start is refused");
    causes(&error)
}

#[tokio::test]
async fn a_federation_issuer_written_first_is_never_described() {
    let key = SigningKey::generate("k1");
    let other = issuer(&key).await;
    let smart = issuer(&key).await;
    let written = vec![federation(&other), entry(&smart)];
    let mut reversed = written.clone();
    reversed.reverse();
    for entries in [written, reversed] {
        let server = Server::start_persisting_with_smart_setup(trusting(entries, None)).await;
        let document: Value = configuration(&server, "r4b").await;
        let base = smart.uri();
        assert_eq!(
            document["authorization_endpoint"],
            json!(format!("{base}/authorize"))
        );
        assert_eq!(document["token_endpoint"], json!(format!("{base}/token")));
        assert!(
            oauth_uris(&server)
                .await
                .contains(&(String::from("authorize"), format!("{base}/authorize")))
        );
    }
}

#[tokio::test]
async fn several_smart_issuers_without_the_key_refuse_the_start() {
    let key = SigningKey::generate("k1");
    let first = issuer(&key).await;
    let second = issuer(&key).await;
    let written = vec![entry(&first), entry(&second)];
    let mut reversed = written.clone();
    reversed.reverse();
    let mut sorted = [first.uri(), second.uri()];
    sorted.sort();
    for entries in [written, reversed] {
        let reason = refused(trusting(entries, None)).await;
        assert!(reason.contains("described_issuer"), "{reason}");
        assert!(
            reason.contains(&format!("{}, {}", sorted[0], sorted[1])),
            "named sorted, never in table order: {reason}"
        );
    }
}

#[tokio::test]
async fn the_named_issuer_is_described_among_several() {
    let key = SigningKey::generate("k1");
    let first = issuer(&key).await;
    let second = issuer(&key).await;
    let entries = vec![entry(&first), entry(&second)];
    for named in [&first, &second] {
        let server =
            Server::start_persisting_with_smart_setup(trusting(entries.clone(), Some(named))).await;
        let document = configuration(&server, "r4b").await;
        assert_eq!(
            document["token_endpoint"],
            json!(format!("{}/token", named.uri()))
        );
    }
}

#[tokio::test]
async fn a_named_unknown_issuer_refuses_the_start() {
    let key = SigningKey::generate("k1");
    let smart = issuer(&key).await;
    let elsewhere = issuer(&key).await;
    let reason = refused(trusting(vec![entry(&smart)], Some(&elsewhere))).await;
    assert!(
        reason.contains(
            "auth.described_issuer names the issuer \"elsewhere\", and the issuers are [\"idp0\"]"
        ),
        "{reason}"
    );
}
