// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `NamingSystem` read and search: a client resolves the namespace an OID, a
//! URI or a local name stands for (<https://hl7.org/fhir/R4/namingsystem.html#search>).

use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::Server;
use ferroterm_server::config::{Config, TerminologySection};
use ferroterm_testkit::mvc::{NATIONAL, NATIONAL_OID, PATIENT_ID_NAME, PATIENT_ID_SYSTEM, imported};

/// The id the 10.1.0 release gives its `NamingSystem` of kind `identifier`.
const PATIENT: &str = "national-patient-10-1-0";

/// The id the 10.1.0 release gives its `NamingSystem` of kind `codesystem`.
const ALLERGY: &str = "national-allergy-10-1-0";

/// A server over the 10.1.0 release of the synthetic package, with a resource
/// database for written resources.
fn one_release() -> Server {
    let dir = tempfile::tempdir().expect("tempdir");
    let release = imported(dir.path(), "10.1.0").expect("imports");
    let config = Config {
        terminology: TerminologySection {
            code_systems: vec![release],
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

/// The ids of the resources of a searchset, sorted.
fn ids(bundle: &Value) -> Vec<String> {
    let mut out: Vec<String> = bundle
        .get("entry")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.pointer("/resource/id").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    out.sort();
    out
}

/// The ids a search with `query` finds on `base`, after checking its status.
async fn found(server: &Server, base: &str, query: &str) -> Vec<String> {
    let (status, body) = server.get(&format!("/{base}/NamingSystem?{query}")).await;
    assert_eq!(status, StatusCode::OK, "{base} {query}: {body}");
    assert_eq!(body.get("type"), Some(&json!("searchset")), "{body}");
    ids(&body)
}

#[tokio::test]
async fn an_identifier_namespace_is_found_by_its_local_name_on_every_base() {
    let server = one_release();
    for base in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server
            .get(&format!(
                "/{base}/NamingSystem?kind=identifier&value={PATIENT_ID_NAME}"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body.get("total"), Some(&json!(1)), "{base}: {body}");
        assert_eq!(ids(&body), [PATIENT], "{base}");
        assert_eq!(
            body.pointer("/entry/0/resource/uniqueId/0/value"),
            Some(&json!(PATIENT_ID_SYSTEM)),
            "{base}: the preferred URI comes back with the match"
        );
    }
}

#[tokio::test]
async fn each_search_parameter_matches_as_its_type_defines() {
    let server = one_release();
    let both = vec![ALLERGY.to_owned(), PATIENT.to_owned()];
    let oid = format!("value={NATIONAL_OID}");
    let uri = format!("value:exact={NATIONAL}");
    let cases: [(&str, Vec<String>); 14] = [
        ("", both.clone()),
        ("kind=codesystem", vec![ALLERGY.to_owned()]),
        ("kind=identifier,codesystem", both.clone()),
        (
            "kind=http://hl7.org/fhir/namingsystem-type%7Cidentifier",
            vec![PATIENT.to_owned()],
        ),
        ("kind=http://example.org/other%7Cidentifier", Vec::new()),
        ("kind=%7Cidentifier", Vec::new()),
        ("status=active", vec![PATIENT.to_owned()]),
        ("id-type=oid", vec![ALLERGY.to_owned()]),
        (
            "type=http://terminology.hl7.org/CodeSystem/v2-0203%7CNI",
            vec![PATIENT.to_owned()],
        ),
        (&oid, vec![ALLERGY.to_owned()]),
        (&uri, vec![ALLERGY.to_owned()]),
        ("value=natp", vec![PATIENT.to_owned()]),
        ("value:exact=natp", Vec::new()),
        ("value:contains=PATIENT", vec![PATIENT.to_owned()]),
    ];
    for (query, expected) in &cases {
        assert_eq!(&found(&server, "r4", query).await, expected, "{query}");
    }
    assert_eq!(
        found(&server, "r4", "kind=identifier&status=draft").await,
        Vec::<String>::new(),
        "every parameter must match"
    );
}

#[tokio::test]
async fn a_value_no_namespace_carries_finds_nothing() {
    let server = one_release();
    let (status, body) = server.get("/r4/NamingSystem?value=2.999.404").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.get("total"), Some(&json!(0)), "{body}");
    assert!(ids(&body).is_empty(), "{body}");
}

#[tokio::test]
async fn a_parameter_or_modifier_the_search_does_not_answer_is_refused() {
    let server = one_release();
    for query in ["url=x", "kind:not=identifier", "value:text=x", "kind=a%7Cb%7Cc"] {
        let (status, body) = server.get(&format!("/r4/NamingSystem?{query}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {body}");
        assert_eq!(
            body.get("resourceType"),
            Some(&json!("OperationOutcome")),
            "{query}: {body}"
        );
    }
}

#[tokio::test]
async fn a_loaded_naming_system_reads_at_its_id() {
    let server = one_release();
    let (status, body) = server.get(&format!("/r4/NamingSystem/{ALLERGY}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.get("kind"), Some(&json!("codesystem")), "{body}");
    assert_eq!(
        body.pointer("/uniqueId/1/value"),
        Some(&json!(NATIONAL_OID)),
        "{body}"
    );
    let (status, missing) = server.get("/r4/NamingSystem/no-such-id").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
}

#[tokio::test]
async fn a_written_naming_system_is_found_beside_the_loaded_ones() {
    let server = one_release();
    let written = json!({"resourceType": "NamingSystem", "name": "Written",
        "status": "active", "kind": "identifier", "date": "2026-11-26",
        "uniqueId": [{"type": "uri", "value": "http://example.org/fhir/sid/written"},
            {"type": "other", "value": "NATWRITTEN"}]});
    let (status, created) = server.post("/r4/NamingSystem", &written).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let (status, body) = server
        .get("/r4/NamingSystem?kind=identifier&value=NAT")
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.get("total"), Some(&json!(2)), "{body}");
    let (status, body) = server.get("/r4/NamingSystem?value=natwritten").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.get("total"), Some(&json!(1)), "{body}");
}

#[tokio::test]
async fn the_capability_statement_declares_naming_system_read_and_search() {
    let server = one_release();
    for (base, status_definition) in [
        ("r4", "http://hl7.org/fhir/SearchParameter/conformance-status"),
        ("r5", "http://hl7.org/fhir/SearchParameter/CanonicalResource-status"),
    ] {
        let (_, statement) = server.get(&format!("/{base}/metadata")).await;
        let declared = statement
            .pointer("/rest/0/resource")
            .and_then(Value::as_array)
            .and_then(|resources| {
                resources
                    .iter()
                    .find(|resource| resource.get("type") == Some(&json!("NamingSystem")))
            })
            .expect("NamingSystem is declared");
        let interactions: Vec<&Value> = declared
            .get("interaction")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|interaction| interaction.get("code"))
            .collect();
        assert!(interactions.contains(&&json!("read")), "{declared}");
        assert!(interactions.contains(&&json!("search-type")), "{declared}");
        let params: Vec<(&Value, &Value)> = declared
            .get("searchParam")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|param| Some((param.get("name")?, param.get("definition")?)))
            .collect();
        for name in ["kind", "value", "type", "id-type"] {
            assert!(
                params.contains(&(
                    &json!(name),
                    &json!(format!("http://hl7.org/fhir/SearchParameter/NamingSystem-{name}"))
                )),
                "{base} {name}: {declared}"
            );
        }
        assert!(
            params.contains(&(&json!("status"), &json!(status_definition))),
            "{base}: {declared}"
        );
    }
}
