// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FHIR R4 under `/r4`: the 4.0.1 resources and operation shapes over the same
//! engine as `/r4b` (<https://hl7.org/fhir/R4/terminology-service.html>).

use axum::body::Body;
use ferroterm_testkit::fhir::{ANIMALS, CM_ANIMALS_COLOURS, VS_PETS};
use ferroterm_testkit::snomed::{ANIMAL, CAT, VERSION, item, sctid};
use fhir_types::codec::{Json, Path, expect_object};
use http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

use crate::fixture::{Server, json as read_json, parameter};

const SCT: &str = "http://snomed.info/sct";
const INLINE_CS: &str = "http://example.org/fhir/CodeSystem/inline-r4";
const INLINE_VS: &str = "http://example.org/fhir/ValueSet/inline-r4";

#[tokio::test]
async fn the_capability_statement_is_an_r4_resource() {
    let server = Server::start();
    let (status, body) = server.get("/r4/metadata").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["resourceType"], "CapabilityStatement");
    assert_eq!(body["fhirVersion"], "4.0.1");
    assert_eq!(
        body["url"],
        "https://ferroterm.eu/fhir/CapabilityStatement/ferroterm-r4"
    );
    assert_eq!(body["title"], "FerroTERM terminology server (R4)");
    let names: Vec<&str> = body["rest"][0]["resource"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["operation"].as_array().unwrap())
        .map(|o| o["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "lookup",
            "validate-code",
            "subsumes",
            "expand",
            "validate-code",
            "translate"
        ]
    );
    // The resource decodes through the generated 4.0.1 codec.
    let mut path = Path::root("CapabilityStatement");
    let carried = crate::fixture::document(&body);
    let object = expect_object(&carried, &path).expect("object");
    let decoded =
        fhir_types::r4::capability_statement::CapabilityStatement::from_json(object, &mut path)
            .expect("an R4 CapabilityStatement");
    assert_eq!(decoded.fhir_version.value.as_deref(), Some("4.0.1"));
    assert_eq!(
        crate::fixture::written(&decoded.to_json().expect("encodes")),
        body
    );
}

#[tokio::test]
async fn the_terminology_capabilities_are_an_r4_resource() {
    let server = Server::start();
    let (status, body) = server.get("/r4/metadata?mode=terminology").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["resourceType"], "TerminologyCapabilities");
    assert_eq!(body["title"], "FerroTERM terminology capabilities (R4)");
    assert_eq!(body["codeSystem"][0]["uri"], SCT);
    assert_eq!(body["codeSystem"][0]["version"][0]["code"], VERSION);
    let mut path = Path::root("TerminologyCapabilities");
    let carried = crate::fixture::document(&body);
    let object = expect_object(&carried, &path).expect("object");
    let decoded = fhir_types::r4::terminology_capabilities::TerminologyCapabilities::from_json(
        object, &mut path,
    )
    .expect("an R4 TerminologyCapabilities");
    assert_eq!(
        crate::fixture::written(&decoded.to_json().expect("encodes")),
        body
    );
    // The same registry, rendered for each version, differs only in the
    // version-specific texts.
    let (_, r4b) = server.get("/r4b/metadata?mode=terminology").await;
    assert_eq!(body["codeSystem"], r4b["codeSystem"]);
    assert_eq!(body["expansion"], r4b["expansion"]);
}

#[tokio::test]
async fn versions_names_r4() {
    let server = Server::start();
    let (status, body) = server.get("/r4/$versions").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(parameter(&body, "version").unwrap()["valueCode"], "4.0");
    assert_eq!(parameter(&body, "default").unwrap()["valueCode"], "4.0");
}

#[tokio::test]
async fn the_code_system_operations_answer_under_r4() {
    let server = Server::start();
    let cat = sctid(item(CAT));
    let animal = sctid(item(ANIMAL));
    let (status, body) = server
        .get(&format!(
            "/r4/CodeSystem/$lookup?system={SCT}&code={cat}&displayLanguage=nl"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "display").unwrap()["valueString"], "Kat");
    assert_eq!(parameter(&body, "version").unwrap()["valueString"], VERSION);

    let (status, body) = server
        .post(
            "/r4/CodeSystem/$validate-code",
            &json!({"resourceType": "Parameters", "parameter": [
                {"name": "url", "valueUri": SCT},
                {"name": "code", "valueCode": cat},
                {"name": "display", "valueString": "Cat"}
            ]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], true);

    let id = server.snomed_id();
    let (status, body) = server
        .get(&format!(
            "/r4/CodeSystem/{id}/$subsumes?codeA={animal}&codeB={cat}"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        parameter(&body, "outcome").unwrap()["valueCode"],
        "subsumes"
    );
}

#[tokio::test]
async fn the_value_set_and_concept_map_operations_answer_under_r4() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .get(&format!(
            "/r4/ValueSet/$expand?url={VS_PETS}&includeDesignations=true"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["resourceType"], "ValueSet");
    // The compose is one `is-a` include, so the expansion carries the hierarchy:
    // `pet` is the root and `kitten` its child.
    let codes: Vec<&str> = body["expansion"]["contains"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["code"].as_str())
        .collect();
    assert_eq!(codes, ["pet"]);
    assert!(crate::ecosystem::contained(&body["expansion"]["contains"], "kitten").is_some());
    let mut path = Path::root("ValueSet");
    let carried = crate::fixture::document(&body);
    let object = expect_object(&carried, &path).expect("object");
    fhir_types::r4::value_set::ValueSet::from_json(object, &mut path).expect("an R4 ValueSet");

    let (status, body) = server
        .get(&format!(
            "/r4/ValueSet/$validate-code?url={VS_PETS}&system={ANIMALS}&code=kitten"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], true);

    let (status, body) = server
        .get(&format!(
            "/r4/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&system={ANIMALS}&code=cat"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], true);
    assert_eq!(
        parameter(&body, "match").unwrap()["part"][0]["name"],
        "equivalence"
    );

    let (status, body) = server.get("/r4/ValueSet").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["resourceType"], "Bundle");
    let mut path = Path::root("Bundle");
    let carried = crate::fixture::document(&body);
    let object = expect_object(&carried, &path).expect("object");
    fhir_types::r4::bundle::Bundle::from_json(object, &mut path).expect("an R4 Bundle");
}

// NOTE: R4 `ValueSet/$expand` declares no `filterProperty`
// (<https://hl7.org/fhir/R4/valueset-operation-expand.html>); the R6 ballot adds it and
// the ecosystem overlay does not pre-adopt it, so R4 refuses it as undeclared.
#[tokio::test]
async fn a_later_version_parameter_is_refused_under_r4() {
    let server = Server::start_with_resources();
    let parameter_name = "filterProperty";
    let (status, body) = server
        .get(&format!(
            "/r4/ValueSet/$expand?url={VS_PETS}&{parameter_name}=x"
        ))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["resourceType"], "OperationOutcome");
    assert_eq!(body["issue"][0]["code"], "invalid");
    let diagnostics = body["issue"][0]["diagnostics"].as_str().unwrap();
    assert!(
        diagnostics.contains(&format!("does not declare a parameter `{parameter_name}`")),
        "{diagnostics}"
    );
    let (status, body) = server
        .post(
            "/r4/ValueSet/$expand",
            &json!({"resourceType": "Parameters", "parameter": [
                {"name": "url", "valueUri": VS_PETS},
                {"name": "filterProperty", "valueCode": "x"}
            ]}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["issue"][0]["code"], "invalid");
}

#[tokio::test]
async fn a_cache_started_on_one_version_serves_another() {
    let server = Server::start();
    let (status, body) = server
        .post(
            "/r4/$cache-control?mode=start",
            &json!({"resourceType": "Parameters", "parameter": [
                {"name": "tx-resource", "resource": {
                    "resourceType": "CodeSystem", "url": INLINE_CS, "status": "active",
                    "content": "complete", "concept": [{"code": "x", "display": "X"}]}},
                {"name": "tx-resource", "resource": {
                    "resourceType": "ValueSet", "url": INLINE_VS, "status": "active",
                    "compose": {"include": [{"system": INLINE_CS, "concept": [{"code": "x"}]}]}}}
            ]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = parameter(&body, "cache-id").unwrap()["valueId"]
        .as_str()
        .unwrap()
        .to_owned();
    for root in ["/r4", "/r4b"] {
        let request = Request::post(format!("{root}/ValueSet/$validate-code"))
            .header(http::header::CONTENT_TYPE, "application/fhir+json")
            .header("X-Cache-Id", &id)
            .body(Body::from(
                json!({"resourceType": "Parameters", "parameter": [
                    {"name": "url", "valueUri": INLINE_VS},
                    {"name": "code", "valueCode": "x"},
                    {"name": "system", "valueUri": INLINE_CS}
                ]})
                .to_string(),
            ))
            .expect("request");
        let (status, body) =
            read_json(server.router().oneshot(request).await.expect("response")).await;
        assert_eq!(status, StatusCode::OK, "{root}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            true,
            "{root}"
        );
    }
}

// NOTE: <https://hl7.org/fhir/uv/tx-ecosystem/requirements.html> requires `property`
// on every version, and R4 lacks `expansion.property`, so the answer is the R5
// cross-version extension (<https://hl7.org/fhir/R5/versions.html#extensions>).
#[tokio::test]
async fn expand_returns_properties_as_cross_version_extensions_under_r4() {
    let server = Server::start_with_resources();
    let (status, body) = server
        .get(&format!("/r4/ValueSet/$expand?url={VS_PETS}&property=legs"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut path = Path::root("ValueSet");
    let carried = crate::fixture::document(&body);
    let object = expect_object(&carried, &path).expect("object");
    fhir_types::r4::value_set::ValueSet::from_json(object, &mut path).expect("an R4 ValueSet");
    let property = &body["expansion"]["extension"][0];
    assert_eq!(
        property["url"],
        "http://hl7.org/fhir/5.0/StructureDefinition/extension-ValueSet.expansion.property"
    );
    assert_eq!(property["extension"][0]["url"], "code");
    assert_eq!(property["extension"][0]["valueCode"], "legs");
    assert_eq!(property["extension"][1]["url"], "uri");
    assert_eq!(
        property["extension"][1]["valueUri"],
        "http://example.org/legs"
    );
    // `VS_PETS` is one `is-a` include, so the expansion nests and `kitten` is a child.
    let kitten =
        crate::ecosystem::contained(&body["expansion"]["contains"], "kitten").expect("kitten");
    let legs = &kitten["extension"][0];
    assert_eq!(
        legs["url"],
        "http://hl7.org/fhir/5.0/StructureDefinition/extension-ValueSet.expansion.contains.property"
    );
    assert_eq!(legs["extension"][0]["valueCode"], "legs");
    assert_eq!(legs["extension"][1]["url"], "value");
    assert_eq!(legs["extension"][1]["valueInteger"], 4);
    assert!(kitten.get("property").is_none(), "{kitten}");
}

#[tokio::test]
async fn the_capability_statement_lists_only_the_expand_parameters_the_version_declares() {
    let server = Server::start_with_resources();
    for version in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server
            .get(&format!("/{version}/metadata?mode=terminology"))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        let parameters: Vec<&str> = body["expansion"]["parameter"]
            .as_array()
            .expect("parameters")
            .iter()
            .filter_map(|p| p["name"].as_str())
            .collect();
        assert!(
            parameters.contains(&"property"),
            "{version}: {parameters:?}"
        );
        assert!(
            parameters.contains(&"tx-resource"),
            "{version}: {parameters:?}"
        );
        for name in &parameters {
            if *name == "tx-resource" {
                continue;
            }
            // A value of `x` may fail the parameter's type; the refusal that must not
            // happen is the undeclared-parameter one.
            let (_, body) = server
                .get(&format!(
                    "/{version}/ValueSet/$expand?url={VS_PETS}&{name}=x"
                ))
                .await;
            let diagnostics = body["issue"][0]["diagnostics"].as_str().unwrap_or_default();
            assert!(
                !diagnostics.contains("does not declare a parameter"),
                "{version}: `{name}` is advertised and refused: {diagnostics}"
            );
        }
    }
}
