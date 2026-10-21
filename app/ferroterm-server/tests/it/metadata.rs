// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET /r4b/metadata` and `?mode=terminology`.

use http::StatusCode;

use crate::fixture::Server;

#[tokio::test]
async fn the_capability_statement_names_the_operations() {
    let server = Server::start();
    let (status, body) = server.get("/r4b/metadata").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["resourceType"], "CapabilityStatement");
    assert_eq!(body["kind"], "instance");
    assert_eq!(body["fhirVersion"], "4.3.0");
    assert_eq!(body["status"], "active");
    assert!(body["date"].as_str().is_some_and(|d| d.contains('T')));
    assert_eq!(body["software"]["name"], "FerroTERM");
    let operations = &body["rest"][0]["resource"][0]["operation"];
    let names: Vec<&str> = operations
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["lookup", "validate-code", "subsumes"]);
    assert_eq!(
        operations[1]["definition"],
        "http://hl7.org/fhir/OperationDefinition/CodeSystem-validate-code"
    );
    assert_eq!(body["rest"][0]["resource"][0]["type"], "CodeSystem");
    let (normative, _) = server.get("/r4b/metadata?mode=normative").await;
    assert_eq!(normative, StatusCode::OK);
}

#[tokio::test]
async fn terminology_capabilities_list_the_loaded_edition() {
    let server = Server::start();
    let (status, body) = server.get("/r4b/metadata?mode=terminology").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["resourceType"], "TerminologyCapabilities");
    assert_eq!(body["kind"], "instance");
    assert_eq!(body["codeSystem"][0]["uri"], "http://snomed.info/sct");
    assert_eq!(
        body["codeSystem"][0]["version"][0]["code"],
        ferroterm_testkit::snomed::VERSION
    );
    assert_eq!(body["codeSystem"][0]["version"][0]["isDefault"], true);
    assert_eq!(body["codeSystem"][0]["subsumption"], true);
    assert_eq!(body["software"]["name"], "FerroTERM");
    let (bad, outcome) = server.get("/r4b/metadata?mode=xml").await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);
    assert_eq!(outcome["issue"][0]["code"], "value");
}

// NOTE: <https://hl7.org/fhir/uv/tx-ecosystem/requirements.html> (Metadata) requires
// a server to populate `rest[mode = server].security.service`; the binding is extensible,
// so a deployment that authenticates nobody says so in text.
#[tokio::test]
async fn the_capability_statement_names_its_security_service_on_every_version() {
    let server = Server::start();
    for version in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server.get(&format!("/{version}/metadata")).await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        let service = &body["rest"][0]["security"]["service"][0];
        assert!(
            service["coding"].as_array().is_none_or(Vec::is_empty),
            "{version}: no deployment declared one: {service}"
        );
        assert!(
            service["text"]
                .as_str()
                .is_some_and(|text| text.contains("no authentication of its own")),
            "{version}: {service}"
        );
    }
}

/// A deployment behind a proxy states the URL clients reach it at.
///
/// The server answers on an address it never sees when a proxy terminates TLS
/// in front of it, so `implementation.url` is configured rather than guessed,
/// and it carries the version prefix the surface is served under
/// (<https://hl7.org/fhir/R4B/capabilitystatement-definitions.html#CapabilityStatement.implementation.url>,
/// #123).
#[tokio::test]
async fn the_capability_statements_state_the_configured_base_url() {
    let server = Server::start();
    let (_, body) = server.get("/r4b/metadata").await;
    assert!(
        body["implementation"]["url"].is_null(),
        "a deployment that names no base states none: {body}"
    );

    let server = Server::start_with_base_url("https://tx.example.org/fhir");
    let (_, body) = server.get("/r4b/metadata").await;
    assert_eq!(
        body["implementation"]["url"], "https://tx.example.org/fhir/r4b",
        "the version prefix is part of the endpoint: {body}"
    );
    let (_, body) = server.get("/r5/metadata").await;
    assert_eq!(
        body["implementation"]["url"],
        "https://tx.example.org/fhir/r5"
    );
    let (_, body) = server.get("/r4b/metadata?mode=terminology").await;
    assert_eq!(
        body["implementation"]["url"], "https://tx.example.org/fhir/r4b",
        "the terminology capabilities say the same: {body}"
    );
}

/// Every segment the derivation names is served, and states that base.
///
/// The SMART gate accepts these same bases as a token audience
/// (<https://hl7.org/fhir/smart-app-launch/app-launch.html>), so a segment in
/// the list that no router serves would be an audience for nothing.
#[tokio::test]
async fn the_endpoint_of_every_served_segment_is_the_one_the_router_answers() {
    let base = "https://tx.example.org/fhir";
    let server = Server::start_with_base_url(base);
    for segment in ferroterm_server::version::SEGMENTS {
        let (status, body) = server.get(&format!("/{segment}/metadata")).await;
        assert_eq!(status, StatusCode::OK, "{segment}: {body}");
        assert_eq!(
            body["implementation"]["url"],
            ferroterm_server::version::endpoint(base, segment),
            "{segment}: {body}"
        );
    }
}

/// The canonical of the artifact declaration, and its two sub-extensions.
const ARTIFACT: &str = "https://ferroterm.eu/fhir/StructureDefinition/terminology-artifact";

/// The artifact directory names `start_with_every_loader` writes, by system.
const ARTIFACTS: [(&str, &str); 3] = [
    ("http://snomed.info/sct", "snomed"),
    ("http://loinc.org", "loinc"),
    ("http://www.nlm.nih.gov/research/umls/rxnorm", "rxnorm"),
];

// NOTE: no FHIR or SNOMED specification records which index a server read, so
// the declaration is an extension on `codeSystem.version`, a `BackboneElement`
// that admits one in every version (<https://hl7.org/fhir/R4B/extensibility.html>).
#[tokio::test]
async fn every_version_declares_the_artifact_each_index_backed_system_came_from() {
    let server = Server::start_with_every_loader();
    for version in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server
            .get(&format!("/{version}/metadata?mode=terminology"))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        let systems = body["codeSystem"].as_array().expect("codeSystem is a list");
        for entry in systems {
            let uri = entry["uri"].as_str().unwrap_or_default();
            let declared = ARTIFACTS.iter().find(|(system, _)| *system == uri);
            for served in entry["version"].as_array().expect("version is a list") {
                let extension = &served["extension"][0];
                match declared {
                    Some((_, name)) => {
                        assert_eq!(extension["url"], ARTIFACT, "{version} {uri}: {body}");
                        assert_eq!(
                            extension["extension"][0]["valueString"], *name,
                            "{version} {uri} names its artifact: {body}"
                        );
                        assert!(
                            extension["extension"][1]["valueString"].is_string(),
                            "{version} {uri} names its release: {body}"
                        );
                    }
                    None => assert!(
                        served["extension"]
                            .as_array()
                            .map(Vec::as_slice)
                            .unwrap_or_default()
                            .iter()
                            .all(|extension| extension["url"] != ARTIFACT),
                        "{version} {uri} came from no artifact and declares none: {body}"
                    ),
                }
            }
        }
    }
}

/// The canonical of the implicit value set declaration.
const IMPLICIT: &str = "https://ferroterm.eu/fhir/StructureDefinition/implicit-value-set";

/// The implicit value set forms each served system declares, as `pattern` and
/// `argument`, in the order the server writes them.
const IMPLICIT_FORMS: [(&str, &[(&str, &str)]); 4] = [
    (
        "http://snomed.info/sct",
        &[
            ("http://snomed.info/sct?fhir_vs", "none"),
            ("http://snomed.info/sct?fhir_vs=isa/[sctid]", "code"),
            ("http://snomed.info/sct?fhir_vs=refset", "none"),
            ("http://snomed.info/sct?fhir_vs=refset/[sctid]", "code"),
            ("http://snomed.info/sct?fhir_vs=ecl/[ecl]", "expression"),
        ],
    ),
    (
        "http://loinc.org",
        &[
            ("http://loinc.org/vs", "none"),
            ("http://loinc.org/vs/[id]", "code"),
            ("http://loinc.org/vs/[partcode]", "code"),
        ],
    ),
    (
        "http://www.nlm.nih.gov/research/umls/rxnorm",
        &[("http://www.nlm.nih.gov/research/umls/rxnorm/vs", "none")],
    ),
    (
        "http://unitsofmeasure.org",
        &[
            ("http://unitsofmeasure.org/vs", "none"),
            ("http://unitsofmeasure.org/vs/[expression]", "expression"),
        ],
    ),
];

// NOTE: no FHIR/SNOMED spec governs this: our own design, since no element of
// `TerminologyCapabilities` states which implicit value sets a server resolves
// (<https://hl7.org/fhir/R5/terminologycapabilities.html>).
#[tokio::test]
async fn every_version_declares_the_implicit_value_set_forms_each_system_resolves() {
    let server = Server::start_with_every_loader();
    for version in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server
            .get(&format!("/{version}/metadata?mode=terminology"))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        for entry in body["codeSystem"].as_array().expect("codeSystem is a list") {
            let uri = entry["uri"].as_str().unwrap_or_default();
            let expected: Vec<(&str, &str)> = IMPLICIT_FORMS
                .iter()
                .find(|(system, _)| *system == uri)
                .map(|(_, forms)| forms.to_vec())
                .unwrap_or_default();
            for served in entry["version"].as_array().expect("version is a list") {
                let declared: Vec<(&str, &str)> = served["extension"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    .filter(|extension| extension["url"] == IMPLICIT)
                    .map(|extension| {
                        let sub = |url: &str, key: &str| {
                            extension["extension"]
                                .as_array()
                                .and_then(|subs| subs.iter().find(|s| s["url"] == url))
                                .and_then(|s| s[key].as_str())
                                .unwrap_or_default()
                        };
                        (sub("pattern", "valueString"), sub("argument", "valueCode"))
                    })
                    .collect();
                assert_eq!(declared, expected, "{version} {uri}: {body}");
            }
        }
    }
}
/// The canonical the server declares an operation's invocation levels under.
const OPERATION_LEVEL: &str = "https://ferroterm.eu/fhir/StructureDefinition/operation-level";

/// The levels one declared operation carries, in the order the server wrote them.
fn levels_of(operation: &serde_json::Value) -> Vec<String> {
    operation["extension"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|extension| extension["url"] == OPERATION_LEVEL)
        .filter_map(|extension| extension["valueCode"].as_str())
        .map(str::to_owned)
        .collect()
}

// NOTE: a `CapabilityStatement` records that a server answers an operation and never at
// which level, so the levels the version's own definition declares are an extension
// (<https://hl7.org/fhir/R4B/operationdefinition-definitions.html#OperationDefinition.instance>).
#[tokio::test]
async fn every_declared_operation_states_the_levels_its_definition_declares() {
    let server = Server::start();
    for version in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server.get(&format!("/{version}/metadata")).await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        for resource in body["rest"][0]["resource"]
            .as_array()
            .expect("resource is a list")
        {
            for operation in resource["operation"]
                .as_array()
                .expect("operation is a list")
            {
                assert!(
                    levels_of(operation).contains(&"type".to_owned()),
                    "{version} answers {} {} at the type level: {body}",
                    resource["type"],
                    operation["name"]
                );
            }
        }
    }
}

// NOTE: R4 and R4B declare `$lookup` at the type level only, and R5 added the instance
// level (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>); the routes that
// follow it are pinned by `instance_lookup_answers_exactly_where_the_version_declares_it`.
#[tokio::test]
async fn only_the_versions_that_define_an_instance_lookup_declare_one() {
    let server = Server::start();
    for (version, instance) in [("r4", false), ("r4b", false), ("r5", true), ("r6", true)] {
        let (status, body) = server.get(&format!("/{version}/metadata")).await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        let lookup = &body["rest"][0]["resource"][0]["operation"][0];
        assert_eq!(lookup["name"], "lookup", "{version}: {body}");
        let expected = if instance {
            vec!["type".to_owned(), "instance".to_owned()]
        } else {
            vec!["type".to_owned()]
        };
        assert_eq!(levels_of(lookup), expected, "{version}: {body}");
    }
}

// NOTE: every served version declares `$expand` and `$translate` at the instance level
// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>,
// <https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>).
#[tokio::test]
async fn expand_and_translate_declare_the_instance_level_on_every_version() {
    let server = Server::start();
    for version in ["r4", "r4b", "r5", "r6"] {
        let (body_status, body) = server.get(&format!("/{version}/metadata")).await;
        assert_eq!(body_status, StatusCode::OK, "{version}: {body}");
        for (resource_type, name) in [("ValueSet", "expand"), ("ConceptMap", "translate")] {
            let declared = body["rest"][0]["resource"]
                .as_array()
                .expect("resource is a list")
                .iter()
                .find(|resource| resource["type"] == resource_type)
                .and_then(|resource| resource["operation"].as_array())
                .expect("operation is a list")
                .iter()
                .find(|operation| operation["name"] == name)
                .map(levels_of)
                .unwrap_or_default();
            assert!(
                declared.contains(&"instance".to_owned()),
                "{version} declares {resource_type}/${name} at the instance level: {declared:?}"
            );
        }
    }
}
