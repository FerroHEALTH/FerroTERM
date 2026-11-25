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

const CASES: [Case; 11] = [
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
    Case {
        system: "https://www.orpha.net",
        version: "2099-01-31",
        code: tables::orpha::DISEASE,
        display: "Synthetic syndrome",
        property: ("type", "Disease"),
        root: None,
        under_root: &[tables::orpha::DISEASE, tables::orpha::OBSOLETE],
    },
    Case {
        system: "http://dicom.nema.org/resources/ontology/DCM",
        version: tables::dicom::EDITION,
        code: tables::dicom::CURRENT,
        display: "Synthetic acquisition",
        property: ("definition", "An invented acquisition."),
        root: None,
        under_root: &[tables::dicom::CURRENT, tables::dicom::RETIRED],
    },
    Case {
        system: "http://standardterms.edqm.eu",
        version: tables::edqm::MODIFIED,
        code: tables::edqm::DOSE_FORM,
        display: "Synthetic tablet",
        property: ("link-bdf", "99992000"),
        root: None,
        under_root: &[tables::edqm::DOSE_FORM, tables::edqm::ROUTE],
    },
    Case {
        system: "https://spor.ema.europa.eu/v1/lists/999999000001",
        version: "2099-03-01",
        code: tables::spor::TERM,
        display: "Synthetic use",
        property: ("status", "CURRENT"),
        root: None,
        under_root: &[tables::spor::TERM, tables::spor::OLD_TERM],
    },
    Case {
        system: "https://spor.ema.europa.eu/v1/organisations",
        version: "2099-02-01",
        code: tables::spor::ORGANISATION,
        display: "Synthetic Medicines Agency",
        property: ("status", "ACTIVE"),
        root: None,
        under_root: &[tables::spor::ORGANISATION],
    },
    Case {
        system: "https://spor.ema.europa.eu/v2/SubstanceDefinition",
        version: "2099-04-01",
        code: tables::spor::SUBSTANCE,
        display: "SYNTHETIC ACID",
        property: ("kind", "substance"),
        root: None,
        under_root: &[tables::spor::SUBSTANCE],
    },
    Case {
        system: "http://ema.europa.eu/fhir/MPID",
        version: "2099-05-01",
        code: tables::spor::PRODUCT,
        display: "Synthomab 10 mg tablets",
        property: ("kind", "medicinal-product"),
        root: None,
        under_root: &[tables::spor::PRODUCT],
    },
];

fn server() -> Server {
    let dir = tempfile::tempdir().expect("tempdir");
    tables::write_artifacts(dir.path()).expect("builds");
    let index = tables::ARTIFACTS
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

#[tokio::test]
async fn a_dicom_context_group_expands_and_validates_over_the_loaded_dcm() {
    let dir = tempfile::tempdir().expect("tempdir");
    tables::write_artifacts(dir.path()).expect("builds");
    let groups = dir.path().join("context-groups");
    std::fs::create_dir_all(&groups).expect("creates");
    tables::dicom::write_context_group(&groups).expect("writes");
    let config = Config {
        terminology: TerminologySection {
            index: vec![dir.path().join("dicom")],
            code_systems: vec![groups],
            ..TerminologySection::default()
        },
        ..Config::default()
    };
    let server = Server::start_with_config(dir, config);
    let url = encode(tables::dicom::CONTEXT_GROUP);
    let (status, body) = server.get(&format!("/r4/ValueSet/$expand?url={url}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut codes = Vec::new();
    flatten(&body["expansion"]["contains"], &mut codes);
    assert_eq!(codes, [tables::dicom::CURRENT]);
    let (_, body) = server
        .get(&format!(
            "/r4/ValueSet/$validate-code?url={url}&system={}&code={}",
            encode("http://dicom.nema.org/resources/ontology/DCM"),
            tables::dicom::CURRENT
        ))
        .await;
    assert_eq!(
        parameter(&body, "result").expect("result")["valueBoolean"],
        true,
        "{body}"
    );
}

/// The EHDS identifier types of the exchange format act (Regulation (EU) 2025/327,
/// Annex X, `EHDSPatient.identifier.type`): the HL7 identifier type extended
/// with NI, HI, HIN, SS, REGPID, and REGION, expanding as one value set over
/// the HL7 Terminology the deployment loads (two of its codes here, CC0).
#[tokio::test]
async fn the_ehds_identifier_types_expand_as_one_value_set() {
    let dir = tempfile::tempdir().expect("tempdir");
    let tho = dir.path().join("tho");
    std::fs::create_dir_all(&tho).expect("creates");
    let v2_0203 = json!({"resourceType": "CodeSystem", "url": "http://terminology.hl7.org/CodeSystem/v2-0203",
    "version": "5.0.0", "status": "active", "content": "complete", "caseSensitive": true,
    "concept": [
        {"code": "MR", "display": "Medical record number"},
        {"code": "NI", "display": "National unique individual identifier"},
        {"code": "PPN", "display": "Passport number"},
        {"code": "SS", "display": "Social Security number"}
    ]});
    let identifier_type = json!({"resourceType": "ValueSet", "url": "http://hl7.org/fhir/ValueSet/identifier-type",
    "version": "4.0.1", "status": "active", "compose": {"include": [
        {"system": "http://terminology.hl7.org/CodeSystem/v2-0203", "concept": [{"code": "MR"}, {"code": "PPN"}]}
    ]}});
    std::fs::write(tho.join("CodeSystem-v2-0203.json"), v2_0203.to_string()).expect("writes");
    std::fs::write(
        tho.join("ValueSet-identifier-type.json"),
        identifier_type.to_string(),
    )
    .expect("writes");
    let config = Config {
        terminology: TerminologySection {
            code_systems: vec![tho],
            ..TerminologySection::default()
        },
        ..Config::default()
    };
    let server = Server::start_with_config(dir, config);
    let url = encode("https://ferroterm.eu/fhir/ValueSet/ehds-identifier-type");
    for version in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server
            .get(&format!("/{version}/ValueSet/$expand?url={url}"))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        let mut codes = Vec::new();
        flatten(&body["expansion"]["contains"], &mut codes);
        codes.sort_unstable();
        assert_eq!(
            codes,
            ["HI", "HIN", "MR", "NI", "PPN", "REGION", "REGPID", "SS"],
            "{version}"
        );
    }
    let (_, body) = server
        .get(&format!(
            "/r4/ValueSet/$validate-code?url={url}&system={}&code=REGPID",
            encode("https://ferroterm.eu/fhir/CodeSystem/ehds-identifier-type")
        ))
        .await;
    assert_eq!(
        parameter(&body, "result").expect("result")["valueBoolean"],
        true,
        "{body}"
    );
    assert_eq!(
        parameter(&body, "display").expect("display")["valueString"],
        "Regional person identifier"
    );
}

/// The SNOMED CT Global Patient Set freeset becomes a value set over the
/// loaded edition: its active concepts expand and validate there.
#[tokio::test]
async fn the_gps_freeset_expands_over_the_loaded_edition() {
    let dir = tempfile::tempdir().expect("tempdir");
    let release = dir.path().join("release");
    let snomed = dir.path().join("snomed");
    let out = dir.path().join("out");
    for path in [&release, &snomed, &out] {
        std::fs::create_dir_all(path).expect("creates");
    }
    tables::write_releases(&release).expect("writes");
    ferroterm_testkit::snomed::write(&snomed).expect("writes the edition");
    tables::gps::write_value_set(&release.join(tables::gps::FILE), &out).expect("builds");
    let config = Config {
        terminology: TerminologySection {
            index: vec![snomed],
            code_systems: vec![out.join("gps")],
            ..TerminologySection::default()
        },
        ..Config::default()
    };
    let server = Server::start_with_config(dir, config);
    let url = encode("https://ferroterm.eu/fhir/ValueSet/snomed-gps");
    let (status, body) = server.get(&format!("/r4/ValueSet/$expand?url={url}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["version"], tables::gps::RELEASE, "{body}");
    let mut codes = Vec::new();
    flatten(&body["expansion"]["contains"], &mut codes);
    codes.sort_unstable();
    let mut expected = tables::gps::active().to_vec();
    expected.sort_unstable();
    assert_eq!(codes, expected);
    let [cat, _] = tables::gps::active();
    let (_, body) = server
        .get(&format!(
            "/r4/ValueSet/$validate-code?url={url}&system={}&code={cat}",
            encode("http://snomed.info/sct")
        ))
        .await;
    assert_eq!(
        parameter(&body, "result").expect("result")["valueBoolean"],
        true,
        "{body}"
    );
}
