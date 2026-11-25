// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The version and provenance of a mapping: `$translate` names the versioned
//! `ConceptMap` each match came from, and the map's `Provenance` reads by that
//! canonical. The draft `EEHRxF` implementing act Art 6 admits a national code
//! only with a validated mapping whose versions and provenance are recorded.

use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::Server;
use ferroterm_server::config::{Config, TerminologySection};
use ferroterm_testkit::mvc::{MAP, NATIONAL, SYSTEM, VALIDATOR, imported};

/// A server over the 10.0.0 and 10.1.0 releases of the synthetic package,
/// with a resource database for written `Provenance` resources.
fn two_releases() -> Server {
    let dir = tempfile::tempdir().expect("tempdir");
    let older = imported(dir.path(), "10.0.0").expect("imports");
    let newer = imported(dir.path(), "10.1.0").expect("imports");
    let config = Config {
        terminology: TerminologySection {
            code_systems: vec![older, newer],
            resources: Some(dir.path().join("resources.redb")),
            ..TerminologySection::default()
        },
        auth: ferrohealth_smart::config::AuthSection {
            enabled: false,
            ..Default::default()
        },
        ..Config::default()
    };
    Server::start_with_config(dir, config)
}

/// The entries of a searchset, by resource.
fn resources(bundle: &Value) -> Vec<&Value> {
    bundle
        .get("entry")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("resource"))
        .collect()
}

/// The `match` parts of a `$translate` answer.
fn matches(body: &Value) -> Vec<&Value> {
    body.get("parameter")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|parameter| parameter.get("name") == Some(&json!("match")))
        .collect()
}

/// The value of the part `name` of `parameter`.
fn part<'a>(parameter: &'a Value, name: &str) -> Option<&'a Value> {
    parameter
        .get("part")
        .and_then(Value::as_array)?
        .iter()
        .find(|part| part.get("name") == Some(&json!(name)))
}

// R4 `ConceptMap/$translate`: `match.source` is "the canonical reference to the
// concept map from which this mapping comes from"
// (<https://hl7.org/fhir/R4/conceptmap-operation-translate.html>).
#[tokio::test]
async fn each_match_names_the_versioned_map_it_came_from() {
    let server = two_releases();
    for release in ["10.0.0", "10.1.0"] {
        let (status, body) = server
            .get(&format!(
                "/r4/ConceptMap/$translate?url={MAP}&conceptMapVersion={release}&system={NATIONAL}&code=n1&targetsystem={SYSTEM}"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{release}: {body}");
        let found = matches(&body);
        assert!(!found.is_empty(), "{release}: {body}");
        for one in found {
            assert_eq!(
                part(one, "source").and_then(|source| source.get("valueCanonical")),
                Some(&json!(format!("{MAP}|{release}"))),
                "{release}: {one}"
            );
        }
    }
}

#[tokio::test]
async fn a_maps_provenance_reads_by_its_canonical_on_every_r4_family_base() {
    let server = two_releases();
    for base in ["r4", "r4b"] {
        let (status, body) = server
            .get(&format!("/{base}/Provenance?target={MAP}%7C10.1.0"))
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body.get("total"), Some(&json!(1)), "{base}: {body}");
        let found = resources(&body);
        let [provenance] = found.as_slice() else {
            panic!("{base}: one Provenance: {body}");
        };
        assert_eq!(
            provenance.pointer("/agent/0/who/display"),
            Some(&json!(VALIDATOR)),
            "{base}"
        );
        assert_eq!(
            provenance.get("recorded"),
            Some(&json!("2026-11-01T10:00:00Z")),
            "{base}"
        );
        let (status, read) = server
            .get(&format!("/{base}/Provenance/national-to-mvc-10-1-0"))
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {read}");
        assert_eq!(read.get("target"), provenance.get("target"), "{base}");
    }
    let (_, every) = server.get("/r4/Provenance").await;
    assert_eq!(every.get("total"), Some(&json!(2)), "{every}");
    let (status, refused) = server.get("/r4/Provenance?url=x").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
}

#[tokio::test]
async fn a_written_provenance_is_kept_and_found_by_its_target() {
    let server = two_releases();
    let written = json!({"resourceType": "Provenance",
        "target": [{"reference": format!("{MAP}|10.0.0")}],
        "recorded": "2026-11-20T09:30:00Z",
        "agent": [{"who": {"display": "A second reviewer"}}]});
    let (status, created) = server.post("/r4/Provenance", &written).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let (status, body) = server
        .get(&format!("/r4/Provenance?target={MAP}%7C10.0.0"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let reviewers: Vec<&Value> = resources(&body)
        .into_iter()
        .filter_map(|provenance| provenance.pointer("/agent/0/who/display"))
        .collect();
    assert_eq!(reviewers.len(), 2, "the loaded and the written one: {body}");
    assert!(reviewers.contains(&&json!("A second reviewer")), "{body}");
}

#[tokio::test]
async fn the_capability_statement_declares_provenance_search_by_target() {
    let server = two_releases();
    let (_, statement) = server.get("/r4/metadata").await;
    let provenance = statement
        .pointer("/rest/0/resource")
        .and_then(Value::as_array)
        .and_then(|resources| {
            resources
                .iter()
                .find(|resource| resource.get("type") == Some(&json!("Provenance")))
        })
        .expect("Provenance is declared");
    let params: Vec<&Value> = provenance
        .get("searchParam")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|param| param.get("name"))
        .collect();
    assert!(params.contains(&&json!("target")), "{provenance}");
    assert!(!params.contains(&&json!("url")), "{provenance}");
}
