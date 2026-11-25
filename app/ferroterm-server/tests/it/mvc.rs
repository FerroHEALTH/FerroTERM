// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Operator-supplied MyHealth@EU-shaped packages, imported with
//! `ferroterm-build --fhir-package` and served side by side: each value set
//! keeps its canonical and version (the draft `EEHRxF` act, Art 6(5)).

use http::StatusCode;
use serde_json::Value;

use crate::fixture::Server;
use ferroterm_server::config::{Config, TerminologySection};
use ferroterm_testkit::mvc::{
    UNSERVED_SYSTEM, UNSERVED_VERSION, VS_ALLERGY, VS_UNSERVED, imported,
};

/// A server over the 10.0.0 and 10.1.0 releases of the synthetic package.
fn two_releases() -> Server {
    let dir = tempfile::tempdir().expect("tempdir");
    let older = imported(dir.path(), "10.0.0").expect("imports");
    let newer = imported(dir.path(), "10.1.0").expect("imports");
    let config = Config {
        terminology: TerminologySection {
            code_systems: vec![older, newer],
            ..TerminologySection::default()
        },
        ..Config::default()
    };
    Server::start_with_config(dir, config)
}

fn codes(body: &Value) -> Vec<&str> {
    body.pointer("/expansion/contains")
        .and_then(Value::as_array)
        .map(|contains| {
            contains
                .iter()
                .filter_map(|c| c.get("code").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn each_release_of_a_value_set_answers_by_its_version() {
    let server = two_releases();
    for version in ["r4", "r4b", "r5"] {
        for (release, expected) in [
            ("10.0.0", vec!["a1", "a2"]),
            ("10.1.0", vec!["a1", "a2", "a3"]),
        ] {
            let (status, body) = server
                .get(&format!(
                    "/{version}/ValueSet/$expand?url={VS_ALLERGY}&valueSetVersion={release}"
                ))
                .await;
            assert_eq!(status, StatusCode::OK, "{version} {release}: {body}");
            assert_eq!(body["version"], release, "{version}");
            assert_eq!(codes(&body), expected, "{version} {release}");
        }
        let (_, latest) = server
            .get(&format!("/{version}/ValueSet/$expand?url={VS_ALLERGY}"))
            .await;
        assert_eq!(
            latest["version"], "10.1.0",
            "{version}: the greatest answers"
        );

        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$validate-code?url={VS_ALLERGY}&valueSetVersion=10.0.0&system=http://example.org/fhir/CodeSystem/mvc-synthetic&code=a3"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        let result = body["parameter"]
            .as_array()
            .and_then(|parameters| parameters.iter().find(|p| p["name"] == "result"))
            .map(|p| p["valueBoolean"].clone());
        assert_eq!(
            result,
            Some(Value::Bool(false)),
            "{version}: a3 is not in 10.0.0"
        );
    }
}

#[tokio::test]
async fn a_value_set_over_an_unloaded_code_system_names_it_and_its_version() {
    let server = two_releases();
    let (status, body) = server
        .get(&format!("/r4b/ValueSet/$expand?url={VS_UNSERVED}"))
        .await;
    assert!(status.is_client_error(), "{status}: {body}");
    assert_eq!(
        body["resourceType"], "OperationOutcome",
        "never an empty expansion"
    );
    let text = body["issue"][0]["details"]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(text.contains(UNSERVED_SYSTEM), "{text}");
    assert!(text.contains(UNSERVED_VERSION), "{text}");
}

// The MVC badge counts conformance/ehds/mvc-synthetic.tsv, so the list names
// every value set of the fixture with the answer the server gives over it.
#[tokio::test]
async fn the_badge_list_names_every_value_set_and_its_answer() {
    let server = two_releases();
    let list = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../conformance/ehds/mvc-synthetic.tsv"
    ))
    .expect("the list reads");
    let rows: Vec<(&str, &str)> = list
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| line.split_once('\t').expect("two columns"))
        .collect();
    let mut listed: Vec<&str> = rows.iter().map(|(url, _)| *url).collect();
    listed.sort_unstable();
    let mut fixture = ferroterm_testkit::mvc::VALUE_SETS.to_vec();
    fixture.sort_unstable();
    assert_eq!(listed, fixture, "every value set of the fixture, once");
    for (url, answer) in rows {
        let (status, body) = server
            .get(&format!(
                "/r4b/ValueSet/$expand?url={url}&valueSetVersion=10.1.0"
            ))
            .await;
        match answer {
            "expands" => assert_eq!(status, StatusCode::OK, "{url}: {body}"),
            "names-missing" => {
                assert!(status.is_client_error(), "{url}: {body}");
                assert_eq!(body["resourceType"], "OperationOutcome", "{url}");
            }
            other => panic!("{url}: unknown answer {other}"),
        }
    }
}
