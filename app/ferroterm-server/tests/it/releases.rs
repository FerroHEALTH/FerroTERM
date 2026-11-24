// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator-supplied releases the European exchange format binds, built
//! from the testkit's synthetic tables and answered by `$lookup`,
//! `$validate-code`, and `$expand` (<https://hl7.org/fhir/R4B/codesystem-operation-lookup.html>,
//! <https://hl7.org/fhir/R4B/codesystem-operation-validate-code.html>,
//! <https://hl7.org/fhir/R4B/valueset-operation-expand.html>).

use ferroterm_server::config::{Config, TerminologySection};
use ferroterm_testkit::tables::{self, VERSION};
use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::Server;

/// One served release: its system, version, a code with its display and one
/// property, and the codes an `is-a` filter on a root reaches (the whole
/// system for a flat list, which offers no `is-a`).
struct Case {
    system: &'static str,
    version: &'static str,
    code: &'static str,
    display: &'static str,
    property: (&'static str, &'static str),
    root: Option<&'static str>,
    under_root: &'static [&'static str],
}

const CASES: [Case; 4] = [
    Case {
        system: "http://npu-terminology.org",
        version: VERSION,
        code: tables::npu::ACTIVE,
        display: tables::npu::SHORT,
        property: ("unit", "millimole per litre"),
        root: None,
        under_root: &[tables::npu::ACTIVE, tables::npu::RETIRED],
    },
    Case {
        system: "http://terminology.hl7.org/CodeSystem/icd-o-3",
        version: VERSION,
        code: tables::icdo::SUBCATEGORY,
        display: "Upper synthetic organ",
        property: ("kind", "topography-subcategory"),
        root: Some(tables::icdo::CATEGORY),
        under_root: &[tables::icdo::CATEGORY, tables::icdo::SUBCATEGORY],
    },
    Case {
        system: "https://smt.esante.gouv.fr/terminologie-emdn",
        version: VERSION,
        code: tables::emdn::TYPE,
        display: "SYNTHETIC PROBES, STERILE",
        property: ("terminal", "YES"),
        root: Some(tables::emdn::GROUP),
        under_root: &[tables::emdn::GROUP, tables::emdn::TYPE],
    },
    Case {
        system: "urn:oid:2.16.840.1.113883.2.9.6.2.7",
        version: VERSION,
        code: tables::isco::UNIT,
        display: "Synthesis Helpers",
        property: ("definition", "Helpers assist synthesis."),
        root: Some("999"),
        under_root: &["999", tables::isco::UNIT],
    },
];

fn server() -> Server {
    let dir = tempfile::tempdir().expect("tempdir");
    tables::write_artifacts(dir.path()).expect("builds");
    let index = ["npu", "icdo", "emdn", "isco"]
        .iter()
        .map(|name| dir.path().join(name))
        .collect();
    let config = Config {
        terminology: TerminologySection {
            index,
            ..TerminologySection::default()
        },
        ..Config::default()
    };
    Server::start_with_config(dir, config)
}

fn parameter<'a>(body: &'a Value, name: &str) -> Option<&'a Value> {
    body["parameter"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[tokio::test]
async fn lookup_answers_the_display_and_properties_of_each_release() {
    let server = server();
    for case in CASES {
        let (status, body) = server
            .get(&format!(
                "/r4/CodeSystem/$lookup?system={}&version={}&code={}&property={}",
                encode(case.system),
                case.version,
                encode(case.code),
                case.property.0
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{}: {body}", case.system);
        assert_eq!(
            parameter(&body, "display").expect("display")["valueString"],
            case.display,
            "{}",
            case.system
        );
        assert_eq!(
            parameter(&body, "version").expect("version")["valueString"],
            case.version,
            "{}",
            case.system
        );
        let properties: Vec<&Value> = body["parameter"]
            .as_array()
            .expect("parameters")
            .iter()
            .filter(|p| p["name"] == "property")
            .collect();
        let found = properties.iter().any(|p| {
            let parts = p["part"].as_array().expect("parts");
            let code = parts.iter().find(|x| x["name"] == "code");
            let value = parts.iter().find(|x| x["name"] == "value");
            code.is_some_and(|c| c["valueCode"] == case.property.0)
                && value.is_some_and(|v| {
                    v.as_object()
                        .expect("value")
                        .iter()
                        .any(|(k, x)| k.starts_with("value") && x == case.property.1)
                })
        });
        assert!(found, "{}: {} in {body}", case.system, case.property.0);
    }
}

#[tokio::test]
async fn validate_code_checks_code_and_display_against_the_loaded_version() {
    let server = server();
    for case in CASES {
        let uri = |display: &str| {
            format!(
                "/r4/CodeSystem/$validate-code?url={}&version={}&code={}&display={}",
                encode(case.system),
                case.version,
                encode(case.code),
                encode(display)
            )
        };
        let (status, body) = server.get(&uri(case.display)).await;
        assert_eq!(status, StatusCode::OK, "{}: {body}", case.system);
        assert_eq!(
            parameter(&body, "result").expect("result")["valueBoolean"],
            true,
            "{}: {body}",
            case.system
        );
        let (_, body) = server.get(&uri("Not the display")).await;
        assert_eq!(
            parameter(&body, "result").expect("result")["valueBoolean"],
            false,
            "{}: a wrong display fails: {body}",
            case.system
        );
        let (_, body) = server
            .get(&format!(
                "/r4/CodeSystem/$validate-code?url={}&version={}&code=NOPE-0",
                encode(case.system),
                case.version
            ))
            .await;
        assert_eq!(
            parameter(&body, "result").expect("result")["valueBoolean"],
            false,
            "{}: an unknown code fails: {body}",
            case.system
        );
    }
}

#[tokio::test]
async fn an_unloaded_version_is_an_operation_outcome_naming_it() {
    let server = server();
    for case in CASES {
        let (status, body) = server
            .get(&format!(
                "/r4/CodeSystem/$lookup?system={}&version=1900&code={}",
                encode(case.system),
                encode(case.code)
            ))
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{}: {body}", case.system);
        assert_eq!(body["resourceType"], "OperationOutcome", "{}", case.system);
        let text = body["issue"][0]["details"]["text"]
            .as_str()
            .or_else(|| body["issue"][0]["diagnostics"].as_str())
            .unwrap_or_default();
        assert!(text.contains("1900"), "{}: {body}", case.system);
    }
}

/// Every code of a `contains` list, nested entries included: a tree system's
/// expansion nests children under their parent
/// (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.expansion.contains.contains>).
fn flatten<'a>(contains: &'a Value, out: &mut Vec<&'a str>) {
    for entry in contains.as_array().into_iter().flatten() {
        out.extend(entry["code"].as_str());
        flatten(&entry["contains"], out);
    }
}

#[tokio::test]
async fn expand_enumerates_a_release_and_filters_its_tree() {
    let server = server();
    for case in CASES {
        let mut include = json!({"system": case.system, "version": case.version});
        if let Some(root) = case.root {
            include["filter"] = json!([{"property": "concept", "op": "is-a", "value": root}]);
        }
        let (status, body) = server
            .post(
                "/r4/ValueSet/$expand",
                &json!({"resourceType": "Parameters", "parameter": [
                    {"name": "valueSet", "resource": {"resourceType": "ValueSet", "status": "active",
                        "compose": {"include": [include]}}}
                ]}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{}: {body}", case.system);
        let mut codes = Vec::new();
        flatten(&body["expansion"]["contains"], &mut codes);
        codes.sort_unstable();
        let mut expected = case.under_root.to_vec();
        expected.sort_unstable();
        assert_eq!(codes, expected, "{}", case.system);
    }
}
