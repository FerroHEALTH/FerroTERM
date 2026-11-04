// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The extensions `$expand` echoes on `expansion.contains`
//! (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>, Extensions), each
//! placed where its context admits it (FHIR Extensions Pack 5.4.0).

use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::{Server, parameters};

const SYSTEM: &str = "http://example.org/fhir/CodeSystem/echoes";
const SD: &str = "http://hl7.org/fhir/StructureDefinition/";

fn code_system() -> Value {
    json!({"resourceType": "CodeSystem", "url": SYSTEM, "status": "active", "content": "complete", "concept": [
        {"code": "a", "display": "A", "definition": "The first",
         "_display": {"extension": [{"url": format!("{SD}rendering-xhtml"), "valueString": "<b>A</b>"}]},
         "extension": [
            {"url": format!("{SD}codesystem-label"), "valueString": "1."},
            {"url": format!("{SD}codesystem-conceptOrder"), "valueInteger": 2},
            {"url": format!("{SD}rendering-style"), "valueString": "font-weight: bold"},
            {"url": format!("{SD}itemWeight"), "valueDecimal": 1.5}
         ],
         "designation": [
            {"language": "de", "value": "Erste", "extension": [{"url": format!("{SD}structuredefinition-standards-status"), "valueCode": "withdrawn"}]},
            {"language": "nl", "value": "Eerste", "use": {"system": "http://snomed.info/sct", "code": "900000000000013009"},
             "extension": [{"url": format!("{SD}structuredefinition-standards-status"), "valueCode": "deprecated"}]},
            {"language": "fr", "value": "Premier"}
         ]},
        {"code": "b", "display": "B", "definition": "The second"}
    ]})
}

fn value_set() -> Value {
    json!({"resourceType": "ValueSet", "url": "http://example.org/fhir/ValueSet/echoes", "status": "active",
    "compose": {"include": [{"system": SYSTEM, "concept": [
        {"code": "a", "extension": [{"url": format!("{SD}valueset-label"), "valueString": "i."}]},
        {"code": "b", "extension": [
            {"url": format!("{SD}valueset-conceptOrder"), "valueInteger": 7},
            {"url": format!("{SD}valueset-concept-definition"), "valueString": "The value set's second"}
        ]}
    ]}]}})
}

async fn expand(root: &str, extra: &[(&str, Value)]) -> Value {
    let server = Server::start();
    let mut pairs = vec![
        ("valueSet", json!({"resource": value_set()})),
        ("tx-resource", json!({"resource": code_system()})),
        ("includeDesignations", json!({"valueBoolean": true})),
    ];
    pairs.extend_from_slice(extra);
    let (status, body) = server
        .post(&format!("{root}/ValueSet/$expand"), &parameters(&pairs))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

fn entry<'a>(body: &'a Value, code: &str) -> &'a Value {
    body.get("expansion")
        .and_then(|e| e.get("contains"))
        .and_then(Value::as_array)
        .expect("contains")
        .iter()
        .find(|c| c.get("code").and_then(Value::as_str) == Some(code))
        .expect("the entry")
}

/// The echoed extensions of an entry as (url suffix, value) pairs, leaving out
/// the cross-version property extensions R4 and R4B carry.
fn echoed(entry: &Value) -> Vec<(String, Value)> {
    entry
        .get("extension")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|x| {
            let url = x.get("url")?.as_str()?.strip_prefix(SD)?.to_owned();
            let value = x
                .as_object()?
                .iter()
                .find(|(k, _)| k.starts_with("value"))?
                .1
                .clone();
            Some((url, value))
        })
        .collect()
}

#[tokio::test]
async fn the_code_system_and_the_value_set_extensions_are_echoed_on_each_entry() {
    for root in ["/r4b", "/r5"] {
        let body = expand(root, &[]).await;
        let a = entry(&body, "a");
        // The value set's label wins over the code system's; the order and the
        // style come from the code system. `itemWeight` admits only `Coding`.
        assert_eq!(
            echoed(a),
            [
                (String::from("rendering-style"), json!("font-weight: bold")),
                (String::from("valueset-label"), json!("i.")),
                (String::from("valueset-conceptOrder"), json!(2)),
            ],
            "{root}: {body}"
        );
        assert_eq!(
            a["_display"]["extension"][0],
            json!({"url": format!("{SD}rendering-xhtml"), "valueString": "<b>A</b>"}),
            "{root}: {body}"
        );
        // Without a request for definitions, the value set's definition stays out.
        assert_eq!(
            echoed(entry(&body, "b")),
            [(String::from("valueset-conceptOrder"), json!(7))],
            "{root}: {body}"
        );
    }
}

#[tokio::test]
async fn the_value_set_definition_is_echoed_when_definitions_are_asked() {
    let body = expand("/r5", &[("property", json!({"valueString": "definition"}))]).await;
    assert_eq!(
        echoed(entry(&body, "b")),
        [
            (String::from("valueset-conceptOrder"), json!(7)),
            (
                String::from("valueset-concept-definition"),
                json!("The value set's second")
            ),
        ],
        "{body}"
    );
}

#[tokio::test]
async fn an_inactive_designation_carries_the_inactive_use_code() {
    let body = expand("/r4b", &[]).await;
    let designations = entry(&body, "a")["designation"]
        .as_array()
        .expect("designations")
        .clone();
    // The withdrawn designation takes the use code; the deprecated one already
    // states a use and is left out; the active one is unchanged.
    assert_eq!(
        designations,
        [
            json!({"language": "de", "value": "Erste", "use": {
                "system": "http://snomed.info/sct", "code": "900000000000546006", "display": "Inactive value"}}),
            json!({"language": "fr", "value": "Premier"}),
        ],
        "{body}"
    );
}
