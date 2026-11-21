// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The issuer the SMART discovery document and the capability statement
//! name, chosen by `ferrohealth-smart`'s `describe`: `[auth]
//! described_issuer`, else the one issuer a user launch starts at, else, with
//! only `machines` issuers trusted, the one `machines` issuer for SMART
//! Backend Services; never by the order of the `[auth.issuer.<name>]` tables.
//! SMART App Launch 2.2.0, Conformance, §Metadata, and Backend Services,
//! §Retrieve `.well-known/smart-configuration`; which of several issuers is
//! described is our own design.

use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_smart::config::IssuerEntry;
use ferrohealth_smart::kind::IssuerKind;
use ferroterm_server::smart::Smart;
use serde_json::{Value, json};
use wiremock::MockServer;

use super::{SigningKey, capabilities, causes, configuration, issuer};
use crate::fixture::{AUDIENCE, Server, SmartSetup};

/// The `[auth.issuer.<name>]` table of `mock` as an issuer of `kind`.
fn entry(mock: &MockServer, kind: IssuerKind) -> IssuerEntry {
    IssuerEntry {
        issuer: SecretUrl::new(mock.uri()),
        kind: Some(kind),
        audience: vec![String::from(AUDIENCE)],
        ..IssuerEntry::default()
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

// SMART App Launch 2.2.0, Backend Services, Retrieve
// `.well-known/smart-configuration`: a backend client reads the token endpoint here.
#[tokio::test]
async fn a_machines_only_server_is_described_for_backend_services() {
    let key = SigningKey::generate("k1");
    let mock = issuer(&key).await;
    let base = mock.uri();
    let mut setup = SmartSetup::for_issuer(&base);
    setup.auth.scopes_supported = vec![
        String::from("system/CodeSystem.cud"),
        String::from("user/CodeSystem.cud"),
    ];
    let server = Server::start_persisting_with_smart_setup(setup).await;
    for version in ["r4", "r4b", "r5", "r6"] {
        let document = configuration(&server, version).await;
        assert_eq!(document["token_endpoint"], json!(format!("{base}/token")));
        assert_eq!(
            document["grant_types_supported"],
            json!(["client_credentials"]),
            "{version}"
        );
        assert_eq!(
            document["token_endpoint_auth_methods_supported"],
            json!(["private_key_jwt"]),
            "{version}"
        );
        assert_eq!(
            capabilities(&document),
            ["client-confidential-asymmetric", "permission-v1"],
            "{version}"
        );
        assert_eq!(
            document["scopes_supported"],
            json!(["system/CodeSystem.cud"]),
            "{version}"
        );
        for absent in [
            "authorization_endpoint",
            "issuer",
            "response_types_supported",
        ] {
            assert!(
                document[absent].is_null(),
                "{version}: {absent}: {document}"
            );
        }
    }
    let uris = oauth_uris(&server).await;
    let names: Vec<&str> = uris.iter().map(|(name, _)| name.as_str()).collect();
    assert!(
        uris.contains(&(String::from("token"), format!("{base}/token"))),
        "the token endpoint is named: {names:?}"
    );
    assert!(
        !names.contains(&"authorize"),
        "no user launch starts at a machines issuer: {names:?}"
    );
}

#[tokio::test]
async fn a_machines_issuer_written_first_is_never_described() {
    let key = SigningKey::generate("k1");
    let machines = issuer(&key).await;
    let persons = issuer(&key).await;
    let written = vec![
        entry(&machines, IssuerKind::Machines),
        entry(&persons, IssuerKind::Persons),
    ];
    let mut reversed = written.clone();
    reversed.reverse();
    for entries in [written, reversed] {
        let server = Server::start_persisting_with_smart_setup(trusting(entries, None)).await;
        let document: Value = configuration(&server, "r4b").await;
        let base = persons.uri();
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
async fn several_user_issuers_without_the_key_refuse_the_start() {
    let key = SigningKey::generate("k1");
    let persons = issuer(&key).await;
    let patients = issuer(&key).await;
    let written = vec![
        entry(&persons, IssuerKind::Persons),
        entry(&patients, IssuerKind::Patients),
    ];
    let mut reversed = written.clone();
    reversed.reverse();
    let mut sorted = [persons.uri(), patients.uri()];
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
    let persons = issuer(&key).await;
    let patients = issuer(&key).await;
    let entries = vec![
        entry(&persons, IssuerKind::Persons),
        entry(&patients, IssuerKind::Patients),
    ];
    for named in [&persons, &patients] {
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
async fn a_named_machines_issuer_beside_a_user_issuer_refuses_the_start() {
    let key = SigningKey::generate("k1");
    let machines = issuer(&key).await;
    let persons = issuer(&key).await;
    let entries = vec![
        entry(&persons, IssuerKind::Persons),
        entry(&machines, IssuerKind::Machines),
    ];
    let reason = refused(trusting(entries, Some(&machines))).await;
    assert!(reason.contains("machines issuer"), "{reason}");
    assert!(reason.contains(&machines.uri()), "{reason}");
}

#[tokio::test]
async fn a_named_unknown_issuer_refuses_the_start() {
    let key = SigningKey::generate("k1");
    let persons = issuer(&key).await;
    let elsewhere = issuer(&key).await;
    let reason = refused(trusting(
        vec![entry(&persons, IssuerKind::Persons)],
        Some(&elsewhere),
    ))
    .await;
    assert!(
        reason.contains(
            "auth.described_issuer names the issuer \"elsewhere\", and the issuers are [\"idp0\"]"
        ),
        "{reason}"
    );
}
