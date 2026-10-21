// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology ecosystem overlay on the wire: every served version accepts
//! the pre-adopted and ecosystem-defined parameters
//! (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>), refuses
//! the ones whose semantics are not implemented yet with `not-supported`, and
//! documents the overlay in its `CapabilityStatement`.

use ferroterm_testkit::fhir::{
    ANIMALS, ANIMALS_NL, CM_ANIMALS_COLOURS, COLOURS, VS_ALL, VS_ENUMERATED, VS_PETS, VS_PETS_REF,
};
use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::{Server, parameter};

const VERSIONS: [&str; 4] = ["r4", "r4b", "r5", "r6"];

/// The number of concepts an expansion carries, at every depth.
pub(crate) fn counted(contains: &Value) -> usize {
    contains.as_array().map_or(0, |entries| {
        entries
            .iter()
            .map(|entry| 1 + counted(&entry["contains"]))
            .sum()
    })
}

/// The entry for `code` anywhere in an expansion, which nests when the compose
/// is one hierarchical include
/// (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.expansion.contains>).
pub(crate) fn contained(contains: &Value, code: &str) -> Option<Value> {
    for entry in contains.as_array()? {
        if entry["code"] == code {
            return Some(entry.clone());
        }
        if let Some(found) = contained(&entry["contains"], code) {
            return Some(found);
        }
    }
    None
}

#[tokio::test]
async fn the_version_negotiation_parameters_are_accepted_on_every_version() {
    let server = Server::start_with_resources();
    for version in VERSIONS {
        // A default for a system the value set does not pin, and a matching check.
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$validate-code?url={VS_PETS}&system={ANIMALS}&code=kitten&system-version={ANIMALS}|2.0&check-system-version={ANIMALS}|2.0"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            true,
            "{version}"
        );
        assert_eq!(
            parameter(&body, "version").unwrap()["valueString"],
            "2.0",
            "{version}"
        );
        // A forced version the server does not serve is an unknown version, named.
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$validate-code?url={VS_PETS}&system={ANIMALS}&code=kitten&force-system-version={ANIMALS}|9.9"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            false,
            "{version}"
        );
        assert_eq!(
            parameter(&body, "x-caused-by-unknown-system").unwrap()["valueCanonical"],
            format!("{ANIMALS}|9.9"),
            "{version}"
        );
        // A check that disagrees with the reference is refused.
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$validate-code?url={VS_ALL}&valueSetVersion=2.0&system={ANIMALS}&code=cat&check-valueset-version={VS_ALL}|1.0"
            ))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{version}: {body}");
        assert_eq!(body["issue"][0]["code"], "invalid", "{version}: {body}");
        // $expand pins an imported value set by default-valueset-version and says so.
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$expand?url={VS_PETS_REF}&default-valueset-version={VS_PETS}|1.0"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        let names: Vec<(String, String)> = body["expansion"]["parameter"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p["name"].as_str().unwrap().to_owned(),
                    p["valueUri"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        let used = (String::from("used-valueset"), format!("{VS_PETS}|1.0"));
        let echoed = (
            String::from("default-valueset-version"),
            format!("{VS_PETS}|1.0"),
        );
        assert!(names.contains(&used), "{version}: {names:?}");
        assert!(names.contains(&echoed), "{version}: {names:?}");
    }
}

#[tokio::test]
async fn infer_system_and_lenient_display_validation_apply_on_every_version() {
    let server = Server::start_with_resources();
    for version in VERSIONS {
        // A bare code finds its system among the two the enumerated value set draws on.
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$validate-code?url={VS_ENUMERATED}&code=RED&inferSystem=true"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            true,
            "{version}"
        );
        assert_eq!(
            parameter(&body, "system").unwrap()["valueUri"],
            COLOURS,
            "{version}"
        );
        // A bare code in neither system cannot be inferred: not in the value set, and the
        // system undetermined.
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$validate-code?url={VS_ENUMERATED}&code=dodo&inferSystem=true"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            false,
            "{version}"
        );
        let kinds: Vec<&str> = parameter(&body, "issues").unwrap()["resource"]["issue"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["details"]["coding"][0]["code"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["not-in-vs", "cannot-infer"], "{version}");
        // A wrong display is a warning under lenient-display-validation.
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$validate-code?url={VS_PETS}&system={ANIMALS}&code=kitten&display=Puppy&lenient-display-validation=true"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            true,
            "{version}"
        );
        let issue = &parameter(&body, "issues").unwrap()["resource"]["issue"][0];
        assert_eq!(issue["severity"], "warning", "{version}");
        assert_eq!(
            issue["details"]["coding"][0]["code"], "invalid-display",
            "{version}"
        );
        // Without it the same display fails the validation.
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$validate-code?url={VS_PETS}&system={ANIMALS}&code=kitten&display=Puppy"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            false,
            "{version}"
        );
    }
}

#[tokio::test]
async fn translate_accepts_the_r6_source_system_and_source_version_names() {
    let server = Server::start_with_resources();
    for version in VERSIONS {
        let code = if matches!(version, "r5" | "r6") {
            "sourceCode"
        } else {
            "code"
        };
        let (status, body) = server
            .get(&format!(
                "/{version}/ConceptMap/$translate?url={CM_ANIMALS_COLOURS}&sourceSystem={ANIMALS}&{code}=cat"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            true,
            "{version}: {body}"
        );
    }
}

#[tokio::test]
async fn the_capability_statement_documents_the_overlay_per_operation() {
    let server = Server::start();
    for version in VERSIONS {
        let (status, body) = server.get(&format!("/{version}/metadata")).await;
        assert_eq!(status, StatusCode::OK);
        let operations: Vec<&Value> = body["rest"][0]["resource"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| r["operation"].as_array().unwrap())
            .collect();
        let documentation = |name: &str| -> String {
            operations
                .iter()
                .find(|o| o["name"] == name)
                .and_then(|o| o["documentation"].as_str())
                .unwrap_or_default()
                .to_owned()
        };
        let validate = documentation("validate-code");
        assert!(
            validate.contains("`x-caused-by-unknown-system`"),
            "{version}: {validate}"
        );
        assert!(
            validate.contains("https://hl7.org/fhir/uv/tx-ecosystem/requirements.html"),
            "{version}: {validate}"
        );
        let lookup = documentation("lookup");
        assert!(lookup.contains("`abstract`"), "{version}: {lookup}");
        assert!(
            documentation("subsumes").is_empty(),
            "{version}: the overlay adds nothing to $subsumes"
        );
        let value_set_validate = operations
            .iter()
            .filter(|o| o["name"] == "validate-code")
            .nth(1)
            .and_then(|o| o["documentation"].as_str())
            .unwrap_or_default();
        if matches!(version, "r5" | "r6") {
            assert!(
                !value_set_validate.contains("`issues`"),
                "{version} declares issues itself: {value_set_validate}"
            );
        } else {
            assert!(
                value_set_validate.contains("`issues`"),
                "{version}: {value_set_validate}"
            );
        }
        // The R6 ballot declares the version trio itself; the earlier versions
        // pre-adopt it.
        if version == "r6" {
            assert!(
                !value_set_validate.contains("`check-system-version`"),
                "{version}: {value_set_validate}"
            );
        } else {
            assert!(
                value_set_validate.contains("`check-system-version`"),
                "{version}: {value_set_validate}"
            );
        }
    }
}

#[tokio::test]
async fn every_version_answers_the_validated_code_system_version_and_issues() {
    let server = Server::start_with_resources();
    for version in VERSIONS {
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$validate-code?url={VS_PETS}&system={ANIMALS}&code=kitten"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], true);
        assert_eq!(
            parameter(&body, "code").unwrap()["valueCode"],
            "kitten",
            "{version}"
        );
        assert_eq!(
            parameter(&body, "system").unwrap()["valueUri"],
            ANIMALS,
            "{version}"
        );
        assert_eq!(
            parameter(&body, "version").unwrap()["valueString"],
            "2.0",
            "{version}"
        );
        assert!(
            parameter(&body, "issues").is_none(),
            "{version}: no issues on a clean pass"
        );
        let (status, body) = server
            .get(&format!(
                "/{version}/CodeSystem/$validate-code?url={ANIMALS}&code=unicorn"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(parameter(&body, "result").unwrap()["valueBoolean"], false);
        assert_eq!(
            parameter(&body, "code").unwrap()["valueCode"],
            "unicorn",
            "{version}"
        );
        let issues = parameter(&body, "issues").expect("issues");
        assert_eq!(issues["resource"]["resourceType"], "OperationOutcome");
        assert_eq!(
            issues["resource"]["issue"][0]["details"]["coding"][0]["code"], "invalid-code",
            "{version}"
        );
    }
}

#[tokio::test]
async fn an_unknown_system_is_a_false_result_naming_the_system_on_every_version() {
    let server = Server::start_with_resources();
    for version in VERSIONS {
        for path in [
            format!("/{version}/CodeSystem/$validate-code?url=http://example.org/none&code=x"),
            format!(
                "/{version}/ValueSet/$validate-code?url={VS_PETS}&system=http://example.org/none&code=x"
            ),
        ] {
            let (status, body) = server.get(&path).await;
            assert_eq!(status, StatusCode::OK, "{path}: {body}");
            assert_eq!(
                parameter(&body, "result").unwrap()["valueBoolean"],
                false,
                "{path}"
            );
            // NOTE: a value set that never named the system reports the input's own
            // unknown system; a code system named directly is the cause (#189).
            let named_as = if path.contains("/ValueSet/") {
                "x-unknown-system"
            } else {
                "x-caused-by-unknown-system"
            };
            assert_eq!(
                parameter(&body, named_as).unwrap()["valueCanonical"],
                "http://example.org/none",
                "{path}: {body}"
            );
            assert_eq!(
                parameter(&body, "code").unwrap()["valueCode"],
                "x",
                "{path}"
            );
            let index = usize::from(path.contains("/ValueSet/"));
            let issue = &parameter(&body, "issues").expect("issues")["resource"]["issue"][index];
            assert_eq!(issue["code"], "not-found", "{path}");
            assert_eq!(issue["details"]["coding"][0]["code"], "not-found", "{path}");
        }
        let (status, body) = server
            .get(&format!(
                "/{version}/CodeSystem/$validate-code?url={ANIMALS}&version=9.9&code=cat"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "x-caused-by-unknown-system").unwrap()["valueCanonical"],
            format!("{ANIMALS}|9.9"),
            "{version}: an unknown version names url|version"
        );
    }
}

// NOTE: the server SHALL return `x-caused-by-unknown-system` for each code
// system it did not support
// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
#[tokio::test]
async fn a_value_set_that_selects_an_unheld_system_names_it_as_caused_on_every_version() {
    let server = Server::start_with_resources();
    let missing = "http://example.org/fhir/CodeSystem/absent";
    for version in VERSIONS {
        let request = json!({"resourceType": "Parameters", "parameter": [
            {"name": "valueSet", "resource": {
                "resourceType": "ValueSet",
                "url": "http://example.org/fhir/ValueSet/selects-absent",
                "status": "active",
                "compose": {"include": [{"system": missing}]}
            }},
            {"name": "system", "valueUri": missing},
            {"name": "code", "valueCode": "x"}
        ]});
        let (status, body) = server
            .post(&format!("/{version}/ValueSet/$validate-code"), &request)
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            false,
            "{version}: {body}"
        );
        assert_eq!(
            parameter(&body, "x-caused-by-unknown-system").unwrap()["valueCanonical"],
            missing,
            "{version}: {body}"
        );
        assert!(
            parameter(&body, "x-unknown-system").is_none(),
            "{version}: the value set is the cause, not the input: {body}"
        );
        let issue = &parameter(&body, "issues").expect("issues")["resource"]["issue"][0];
        assert_eq!(issue["code"], "not-found", "{version}: {body}");
        assert_eq!(
            issue["details"]["coding"][0]["code"], "not-found",
            "{version}: {body}"
        );
    }
}

#[tokio::test]
async fn lookup_answers_code_system_and_abstract_on_every_version() {
    let server = Server::start_with_resources();
    for version in VERSIONS {
        let (status, body) = server
            .get(&format!(
                "/{version}/CodeSystem/$lookup?system={ANIMALS}&code=living"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "code").unwrap()["valueCode"],
            "living",
            "{version}"
        );
        assert_eq!(
            parameter(&body, "system").unwrap()["valueUri"],
            ANIMALS,
            "{version}"
        );
        assert_eq!(
            parameter(&body, "abstract").unwrap()["valueBoolean"],
            true,
            "{version}"
        );
        let (status, body) = server
            .get(&format!(
                "/{version}/CodeSystem/$lookup?system={ANIMALS}&code=cat"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert!(
            parameter(&body, "abstract").is_none(),
            "a selectable concept answers no `abstract`: {version}: {body}"
        );
    }
}

#[tokio::test]
async fn a_wildcard_system_version_names_the_greatest_matching_version() {
    let server = Server::start_with_resources();
    for version in VERSIONS {
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$validate-code?url={VS_PETS}&system={ANIMALS}&code=kitten&system-version={ANIMALS}|2.x"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            true,
            "{version}"
        );
        assert_eq!(
            parameter(&body, "version").unwrap()["valueString"],
            "2.0",
            "{version}"
        );
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$validate-code?url={VS_PETS}&system={ANIMALS}&code=kitten&force-system-version={ANIMALS}|3.x"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            false,
            "{version}"
        );
        assert_eq!(
            parameter(&body, "x-caused-by-unknown-system").unwrap()["valueCanonical"],
            format!("{ANIMALS}|3.x"),
            "{version}"
        );
    }
}

#[tokio::test]
async fn a_codeable_concept_is_echoed_and_its_unknown_systems_named_on_every_version() {
    let server = Server::start_with_resources();
    for version in VERSIONS {
        let request = json!({"resourceType": "Parameters", "parameter": [
            {"name": "url", "valueUri": VS_ENUMERATED},
            {"name": "codeableConcept", "valueCodeableConcept": {"coding": [
                {"system": COLOURS, "code": "blue"},
                {"system": ANIMALS, "code": "cat"}
            ], "text": "some plain text"}}
        ]});
        let (status, body) = server
            .post(&format!("/{version}/ValueSet/$validate-code"), &request)
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            true,
            "{version}: {body}"
        );
        assert_eq!(
            parameter(&body, "code").unwrap()["valueCode"],
            "cat",
            "{version}"
        );
        let echoed = &parameter(&body, "codeableConcept").expect("echo")["valueCodeableConcept"];
        assert_eq!(echoed["coding"][1]["code"], "cat", "{version}: {body}");
        // The echo is "a codeableConcept containing codings for all the validated
        // codes", so the text the request sent stays on it
        // (<https://hl7.org/fhir/R5/valueset-operation-validate-code.html>).
        assert_eq!(echoed["text"], "some plain text", "{version}: {body}");
        let issue = &parameter(&body, "issues").unwrap()["resource"]["issue"][0];
        assert_eq!(issue["severity"], "information", "{version}");
        assert_eq!(
            issue["expression"][0], "CodeableConcept.coding[0].code",
            "{version}"
        );
        let request = json!({"resourceType": "Parameters", "parameter": [
            {"name": "url", "valueUri": VS_ENUMERATED},
            {"name": "codeableConcept", "valueCodeableConcept": {"coding": [
                {"system": "http://example.org/none", "code": "x"}
            ]}}
        ]});
        let (status, body) = server
            .post(&format!("/{version}/ValueSet/$validate-code"), &request)
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            false,
            "{version}"
        );
        assert!(
            parameter(&body, "code").is_none(),
            "{version}: no coding answers the code"
        );
        assert_eq!(
            parameter(&body, "x-unknown-system").unwrap()["valueCanonical"],
            "http://example.org/none",
            "{version}: {body}"
        );
        let issue = &parameter(&body, "issues").unwrap()["resource"]["issue"][1];
        assert_eq!(
            issue["expression"][0], "CodeableConcept.coding[0].system",
            "{version}"
        );
    }
}

#[tokio::test]
async fn a_loaded_supplement_applies_only_when_use_supplement_names_it() {
    let server = Server::start_with_resources();
    for version in VERSIONS {
        let base = format!(
            "/{version}/ValueSet/$validate-code?url={VS_ALL}&system={ANIMALS}&code=cat&display=Kat"
        );
        let (status, body) = server.get(&base).await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            false,
            "{version}: dormant"
        );
        let (status, body) = server
            .get(&format!("{base}&useSupplement={ANIMALS_NL}"))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            true,
            "{version}: applied"
        );
        let (status, body) = server
            .get(&format!(
                "{base}&useSupplement=http://example.org/fhir/CodeSystem/none"
            ))
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{version}: {body}");
        assert_eq!(body["issue"][0]["code"], "not-found", "{version}");
        assert_eq!(
            body["issue"][0]["details"]["text"],
            "Required supplement not found: http://example.org/fhir/CodeSystem/none",
            "{version}"
        );
    }
}

#[tokio::test]
async fn an_inactive_concept_answers_inactive_and_status_with_a_warning_on_every_version() {
    let server = Server::start_with_resources();
    for version in VERSIONS {
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$validate-code?url={VS_ALL}&system={ANIMALS}&code=fish"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            true,
            "{version}"
        );
        assert_eq!(
            parameter(&body, "inactive").unwrap()["valueBoolean"],
            true,
            "{version}: {body}"
        );
        assert_eq!(
            parameter(&body, "status").unwrap()["valueCode"],
            "retired",
            "{version}"
        );
        let issue = &parameter(&body, "issues").unwrap()["resource"]["issue"][0];
        assert_eq!(issue["severity"], "warning", "{version}");
        assert_eq!(
            issue["details"]["coding"][0]["code"], "code-comment",
            "{version}"
        );
        let (status, body) = server
            .get(&format!(
                "/{version}/CodeSystem/$validate-code?url={ANIMALS}&code=fish"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "inactive").unwrap()["valueBoolean"],
            true,
            "{version}: {body}"
        );
        assert_eq!(
            parameter(&body, "status").unwrap()["valueCode"],
            "retired",
            "{version}"
        );
    }
}

#[tokio::test]
async fn every_issue_and_outcome_carries_the_ecosystems_message_id() {
    let server = Server::start_with_resources();
    for version in VERSIONS {
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$validate-code?url={VS_PETS}&system={ANIMALS}&code=unicorn"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        let issue = &parameter(&body, "issues").unwrap()["resource"]["issue"][0];
        assert_eq!(
            issue["extension"][0]["url"],
            "http://hl7.org/fhir/StructureDefinition/operationoutcome-message-id",
            "{version}: {body}"
        );
        assert_eq!(
            issue["extension"][0]["valueString"],
            "None_of_the_provided_codes_are_in_the_value_set_one",
            "{version}"
        );
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$expand?url=http://example.org/fhir/ValueSet/none"
            ))
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{version}: {body}");
        // The outcome of a refused request carries no message id; the ids live on
        // the itemised `issues` above.
        assert!(
            body["issue"][0].get("extension").is_none(),
            "{version}: {body}"
        );
        assert_eq!(
            body["issue"][0]["details"]["coding"][0]["code"], "not-found",
            "{version}: {body}"
        );
        // A versionless system answers no version on $lookup.
        let (status, body) = server
            .get(&format!(
                "/{version}/CodeSystem/$lookup?system=urn:ietf:bcp:13&code=text/plain"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert!(parameter(&body, "version").is_none(), "{version}: {body}");
    }
}

#[tokio::test]
async fn expand_flags_inactive_concepts_with_their_status_on_every_version() {
    let server = Server::start_with_resources();
    for version in VERSIONS {
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$expand?url={VS_ALL}&valueSetVersion=1.0"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        let echoed: Vec<&str> = body["expansion"]["parameter"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert!(
            !echoed.contains(&"displayLanguage"),
            "{version}: {echoed:?}"
        );
        let fish = contained(&body["expansion"]["contains"], "fish").expect("fish");
        assert_eq!(fish["inactive"], true, "{version}: {fish}");
        if matches!(version, "r5" | "r6") {
            assert_eq!(fish["property"][0]["code"], "status", "{version}: {fish}");
            assert_eq!(
                fish["property"][0]["valueCode"], "retired",
                "{version}: {fish}"
            );
        } else {
            // R4 and R4B carry `contains.property` as the R5 cross-version extension
            // (<https://hl7.org/fhir/R5/versions.html#extensions>).
            let property = &fish["extension"][0];
            assert_eq!(
                property["url"],
                "http://hl7.org/fhir/5.0/StructureDefinition/extension-ValueSet.expansion.contains.property",
                "{version}: {fish}"
            );
            assert_eq!(property["extension"][0]["url"], "code", "{version}: {fish}");
            assert_eq!(
                property["extension"][0]["valueCode"], "status",
                "{version}: {fish}"
            );
            assert_eq!(
                property["extension"][1]["url"], "value",
                "{version}: {fish}"
            );
            assert_eq!(
                property["extension"][1]["valueCode"], "retired",
                "{version}: {fish}"
            );
        }
    }
}

// NOTE: `ValueSet/$validate-code` declares `version` only as an output
// (<https://hl7.org/fhir/R5/valueset-operation-validate-code.html>), and no
// OperationDefinition declares `limit` or `allowMaximumSizeExpansion` (#349).
#[tokio::test]
async fn the_undeclared_suite_parameters_are_refused_on_every_version() {
    let server = Server::start_with_resources();
    for version in VERSIONS {
        for (operation, query, name) in [
            (
                "ValueSet/$validate-code",
                format!("url={VS_PETS}&system={ANIMALS}&code=kitten&version=1.0"),
                "version",
            ),
            (
                "ValueSet/$expand",
                format!("url={VS_PETS}&limit=1000"),
                "limit",
            ),
            (
                "ValueSet/$expand",
                format!("url={VS_PETS}&count=3000&allowMaximumSizeExpansion=true"),
                "allowMaximumSizeExpansion",
            ),
        ] {
            let (status, body) = server.get(&format!("/{version}/{operation}?{query}")).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{version} {name}: {body}");
            assert_eq!(
                body["issue"][0]["code"], "invalid",
                "{version} {name}: {body}"
            );
            let diagnostics = body["issue"][0]["diagnostics"].as_str().unwrap();
            assert!(
                diagnostics.contains(&format!("does not declare a parameter `{name}`")),
                "{version} {name}: {diagnostics}"
            );
        }

        // The spellings the operations do declare carry the same intent.
        let (status, body) = server
            .get(&format!(
                "/{version}/ValueSet/$validate-code?url={VS_PETS}&system={ANIMALS}&code=kitten&systemVersion=2.0"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            true,
            "{version}: {body}"
        );

        // `version` is an input of the CodeSystem operation, which is where the suite's
        // shape comes from, so the same name is accepted one resource over.
        let (status, body) = server
            .get(&format!(
                "/{version}/CodeSystem/$validate-code?url={ANIMALS}&version=2.0&code=kitten"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            parameter(&body, "result").unwrap()["valueBoolean"],
            true,
            "{version}: {body}"
        );
    }
}
