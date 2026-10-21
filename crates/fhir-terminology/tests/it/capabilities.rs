// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `TerminologyCapabilities` from the registry, rendered per FHIR version.

use std::sync::Arc;

use fhir_terminology::artifact::{SOURCE_EXTENSION, SOURCE_NAME, SOURCE_RELEASE};
use fhir_terminology::capabilities::{
    IMPLICIT_ARGUMENT, IMPLICIT_EXTENSION, IMPLICIT_PATTERN, Summary,
};
use fhir_terminology::fhir_codesystem::load::{FhirVersion, load_file};
use fhir_terminology::fhir_codesystem::provider::FhirCodeSystem;
use fhir_terminology::provider::{CodeSystemProvider, Compositional};
use fhir_terminology::registries::ucum::provider::{URL as UCUM, UcumProvider};
use fhir_terminology::registry::Registry;
use fhir_terminology::snomed::{SYSTEM as SNOMED, SnomedProvider};
use fhir_types::codec::Json;
use serde_json::Value;

use ferroterm_testkit::snomed;
use ferroterm_testkit::snomed::{CAT, item, sctid};

use crate::fixture::{FLAT_URL, URL, registry};

#[test]
fn the_summary_reads_every_declaration_and_marks_the_default() {
    let mut registry = registry();
    registry.set_default(URL, "2024").expect("sets default");
    let summary = Summary::of(&registry);
    assert_eq!(summary.systems.len(), 2);
    let fixture = summary
        .systems
        .iter()
        .find(|s| s.url == URL)
        .expect("fixture");
    assert!(fixture.subsumption);
    let defaults: Vec<(&str, bool)> = fixture
        .versions
        .iter()
        .map(|v| (v.code.as_str(), v.is_default))
        .collect();
    assert_eq!(defaults, [("2024", true), ("2025", false)]);
    assert_eq!(fixture.versions[0].properties, ["legs", "kingdom"]);
    assert_eq!(fixture.versions[0].filters[0].operators.len(), 11);
    let flat = summary
        .systems
        .iter()
        .find(|s| s.url == FLAT_URL)
        .expect("flat");
    assert!(!flat.subsumption);
    assert_eq!(flat.versions[0].filters[0].operators.len(), 5);
}

#[test]
fn r4_r4b_and_r5_render_their_own_shapes() {
    let summary = Summary::of(&registry());
    let r4 = document(
        &summary
            .to_r4("2026-09-02T00:00:00Z")
            .to_json()
            .expect("encodes"),
    );
    let r4b = document(
        &summary
            .to_r4b("2026-09-02T00:00:00Z")
            .to_json()
            .expect("encodes"),
    );
    let r5_object = summary
        .to_r5("2026-09-02T00:00:00Z")
        .to_json()
        .expect("encodes");
    let r5 = document(&r5_object);
    for (version, json) in [("r4", &r4), ("r4b", &r4b), ("r5", &r5)] {
        assert_eq!(json["resourceType"], "TerminologyCapabilities", "{version}");
        assert_eq!(json["status"], "active", "{version}");
        assert_eq!(json["kind"], "instance", "{version}");
        assert_eq!(json["codeSystem"][0]["uri"], URL, "{version}");
        assert_eq!(json["codeSystem"][0]["subsumption"], true, "{version}");
        assert_eq!(
            json["codeSystem"][0]["version"][1]["isDefault"], true,
            "{version}"
        );
        assert_eq!(
            json["codeSystem"][0]["version"][0]["filter"][0]["code"], "concept",
            "{version}"
        );
        assert_eq!(
            json["codeSystem"][0]["version"][0]["language"],
            serde_json::json!(["en", "nl"]),
            "{version}"
        );
        assert_eq!(json["expansion"]["paging"], true, "{version}");
        assert!(json["expansion"]["textFilter"].is_string(), "{version}");
    }
    // R5 adds the mandatory codeSystem.content; R4 and R4B have no such element.
    assert_eq!(r5["codeSystem"][1]["content"], "not-present");
    assert_eq!(r5["codeSystem"][1]["uri"], FLAT_URL);
    assert!(r4b["codeSystem"][1].get("content").is_none());
    assert!(r4["codeSystem"][1].get("content").is_none());
    assert_eq!(r4, r4b, "R4 and R4B fill the same elements");
    // The rendered resources decode again through the generated codec.
    let mut path = fhir_types::codec::Path::root("TerminologyCapabilities");
    let decoded = fhir_types::r5::terminology_capabilities::TerminologyCapabilities::from_json(
        &r5_object, &mut path,
    )
    .expect("decodes");
    assert_eq!(decoded, summary.to_r5("2026-09-02T00:00:00Z"));
}

/// A `CodeSystem` resource that declares a compositional grammar, written to a
/// file so the generic provider loads it the way a deployment's package does.
const GRAMMAR_SYSTEM: &str = "http://example.org/fhir/CodeSystem/grammar";

fn declares_a_grammar() -> (tempfile::TempDir, FhirCodeSystem) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("CodeSystem-grammar.json");
    let resource = serde_json::json!({
        "resourceType": "CodeSystem",
        "url": GRAMMAR_SYSTEM,
        "version": "1.0",
        "status": "active",
        "content": "complete",
        "caseSensitive": true,
        "compositional": true,
        "concept": [{"code": "a", "display": "A"}]
    });
    std::fs::write(&path, resource.to_string()).expect("writes");
    let model = load_file(&path, FhirVersion::R5).expect("loads");
    assert!(model.compositional, "the resource declares the grammar");
    let provider = FhirCodeSystem::new(model).expect("builds");
    (dir, provider)
}

#[test]
fn a_declared_grammar_and_a_supported_grammar_are_two_different_statements() {
    // `CodeSystem.compositional` is "The code system defines a compositional
    // (post-coordination) grammar"
    // (<https://hl7.org/fhir/R4B/codesystem-definitions.html#CodeSystem.compositional>);
    // `TerminologyCapabilities.codeSystem.version.compositional` is "If the
    // compositional grammar defined by the code system is supported"
    // (<https://hl7.org/fhir/R4B/terminologycapabilities-definitions.html#TerminologyCapabilities.codeSystem.version.compositional>).
    // The generic provider serves the concepts a resource enumerates and
    // evaluates no grammar, so the two disagree for it.
    let (_dir, grammar) = declares_a_grammar();
    assert_eq!(grammar.declaration().compositional, Compositional::Defined);
    assert!(grammar.declaration().compositional.defined());
    assert!(!grammar.declaration().compositional.supported());
    // UCUM's own provider parses the unit expression grammar, so both hold.
    let ucum = UcumProvider::new();
    assert_eq!(ucum.declaration().compositional, Compositional::Supported);
    assert!(ucum.declaration().compositional.defined());
    assert!(ucum.declaration().compositional.supported());

    let mut registry = Registry::new();
    registry.register(Arc::new(grammar)).expect("registers");
    registry.register(Arc::new(ucum)).expect("registers");
    let summary = Summary::of(&registry);
    let flags: Vec<(&str, bool)> = summary
        .systems
        .iter()
        .map(|system| (system.url.as_str(), system.versions[0].compositional))
        .collect();
    assert_eq!(flags, [(GRAMMAR_SYSTEM, false), (UCUM, true)]);
    let json = document(
        &summary
            .to_r4b("2026-09-05T00:00:00Z")
            .to_json()
            .expect("encodes"),
    );
    assert_eq!(json["codeSystem"][0]["uri"], GRAMMAR_SYSTEM);
    assert_eq!(json["codeSystem"][0]["version"][0]["compositional"], false);
    assert_eq!(json["codeSystem"][1]["uri"], UCUM);
    assert_eq!(json["codeSystem"][1]["version"][0]["compositional"], true);
}

/// The `codeSystem` entry of `uri` in a rendered statement.
fn code_system<'a>(statement: &'a Value, uri: &str) -> &'a Value {
    statement["codeSystem"]
        .as_array()
        .expect("codeSystem is a list")
        .iter()
        .find(|entry| entry["uri"] == uri)
        .expect("the system is declared")
}

/// The artifact directory name this file writes the synthetic edition into.
const ARTIFACT_NAME: &str = "snomed-int";

/// A registry holding the synthetic edition written under `dir` and UCUM.
fn served(dir: &std::path::Path) -> Registry {
    let artifact = dir.join(ARTIFACT_NAME);
    std::fs::create_dir_all(&artifact).expect("creates the artifact directory");
    snomed::write(&artifact).expect("writes the fixture");
    let provider = SnomedProvider::open(&artifact, "en").expect("opens");
    assert_eq!(
        provider.artifact().map(|source| source.release.as_str()),
        Some(snomed::DATE),
        "the provider carries the release the manifest recorded"
    );
    let mut registry = Registry::new();
    registry.register(Arc::new(provider)).expect("registers");
    registry
        .register(Arc::new(UcumProvider::new()))
        .expect("registers");
    registry
}

#[test]
fn every_version_declares_the_artifact_an_index_backed_system_was_read_from() {
    // No FHIR or SNOMED specification records which index a server read, so
    // the declaration is an extension, the form FHIR defines for exactly that
    // (<https://hl7.org/fhir/R4B/extensibility.html>). Every version's
    // `TerminologyCapabilities.codeSystem.version` is a `BackboneElement` and
    // admits `extension` 0..*, from R4 4.0.1 through the R6 ballot.
    let dir = tempfile::tempdir().expect("tempdir");
    let summary = Summary::of(&served(dir.path()));
    let date = "2026-09-06T00:00:00Z";
    let statements = [
        ("r4", summary.to_r4(date).to_json().expect("encodes")),
        ("r4b", summary.to_r4b(date).to_json().expect("encodes")),
        ("r5", summary.to_r5(date).to_json().expect("encodes")),
        ("r6", summary.to_r6(date).to_json().expect("encodes")),
    ];
    for (version, object) in statements {
        let statement = document(&object);
        assert_eq!(
            artifact_extensions(code_system(&statement, SNOMED)),
            serde_json::json!([{
                "url": SOURCE_EXTENSION,
                "extension": [
                    {"url": SOURCE_NAME, "valueString": ARTIFACT_NAME},
                    {"url": SOURCE_RELEASE, "valueString": snomed::DATE},
                ],
            }]),
            "{version} declares the artifact of the served edition"
        );
        assert_eq!(
            artifact_extensions(code_system(&statement, UCUM)),
            serde_json::json!([]),
            "{version} declares no artifact for the UCUM registry"
        );
    }
}

/// The artifact extensions of the first version entry of `system`.
fn artifact_extensions(system: &Value) -> Value {
    Value::Array(
        system
            .pointer("/version/0/extension")
            .and_then(Value::as_array)
            .map(|extensions| {
                extensions
                    .iter()
                    .filter(|extension| extension["url"] == SOURCE_EXTENSION)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default(),
    )
}

#[test]
fn every_version_declares_the_implicit_value_set_forms_it_resolves() {
    // No element of `TerminologyCapabilities` states which implicit value sets
    // a server resolves (<https://hl7.org/fhir/R5/terminologycapabilities.html>),
    // so each form is an extension on `codeSystem.version`, the patterns spelled
    // as <https://hl7.org/fhir/R4B/snomedct.html> and
    // <https://terminology.hl7.org/UCUM.html> spell them.
    let dir = tempfile::tempdir().expect("tempdir");
    let summary = Summary::of(&served(dir.path()));
    let form = |pattern: &str, argument: &str| {
        serde_json::json!({
            "url": IMPLICIT_EXTENSION,
            "extension": [
                {"url": IMPLICIT_PATTERN, "valueString": pattern},
                {"url": IMPLICIT_ARGUMENT, "valueCode": argument},
            ],
        })
    };
    for (version, statement) in rendered(&summary) {
        let implicit = |system: &str| -> Vec<Value> {
            code_system(&statement, system)["version"][0]["extension"]
                .as_array()
                .expect("extensions")
                .iter()
                .filter(|extension| extension["url"] == IMPLICIT_EXTENSION)
                .cloned()
                .collect()
        };
        assert_eq!(
            implicit(SNOMED),
            [
                form("http://snomed.info/sct?fhir_vs", "none"),
                form("http://snomed.info/sct?fhir_vs=isa/[sctid]", "code"),
                form("http://snomed.info/sct?fhir_vs=refset", "none"),
                form("http://snomed.info/sct?fhir_vs=refset/[sctid]", "code"),
                form("http://snomed.info/sct?fhir_vs=ecl/[ecl]", "expression"),
            ],
            "{version} declares the SNOMED CT forms"
        );
        assert_eq!(
            implicit(UCUM),
            [
                form("http://unitsofmeasure.org/vs", "none"),
                form("http://unitsofmeasure.org/vs/[expression]", "expression"),
            ],
            "{version} declares the UCUM forms"
        );
    }
}

#[test]
fn a_system_declaring_no_implicit_form_carries_no_implicit_extension() {
    let summary = Summary::of(&registry());
    for (version, statement) in rendered(&summary) {
        for system in statement["codeSystem"].as_array().expect("codeSystem") {
            for entry in system["version"].as_array().expect("version") {
                assert!(
                    entry.get("extension").is_none(),
                    "{version} declares an extension for {}",
                    system["uri"]
                );
            }
        }
    }
}

/// The four rendered statements of `summary`, one per served FHIR version.
fn rendered(summary: &Summary) -> Vec<(&'static str, Value)> {
    let date = "2026-09-06T00:00:00Z";
    vec![
        (
            "r4",
            document(&summary.to_r4(date).to_json().expect("encodes")),
        ),
        (
            "r4b",
            document(&summary.to_r4b(date).to_json().expect("encodes")),
        ),
        (
            "r5",
            document(&summary.to_r5(date).to_json().expect("encodes")),
        ),
        (
            "r6",
            document(&summary.to_r6(date).to_json().expect("encodes")),
        ),
    ]
}

#[test]
fn a_filter_code_is_declared_once_with_the_operators_of_both_declarations() {
    // `TerminologyCapabilities.codeSystem.version.filter` carries `code` 1..1
    // and `op` 1..* "Operations supported for the property"
    // (<https://hl7.org/fhir/R5/terminologycapabilities-definitions.html#TerminologyCapabilities.codeSystem.version.filter.op>),
    // so one code states one operator list. SNOMED CT declares its own
    // `concept` filter over the set the engine answers generically.
    let dir = tempfile::tempdir().expect("tempdir");
    let summary = Summary::of(&served(dir.path()));
    for (version, statement) in rendered(&summary) {
        for system in statement["codeSystem"].as_array().expect("codeSystem") {
            for entry in system["version"].as_array().expect("version") {
                let codes: Vec<&str> = entry["filter"]
                    .as_array()
                    .map(|filters| {
                        filters
                            .iter()
                            .filter_map(|filter| filter["code"].as_str())
                            .collect()
                    })
                    .unwrap_or_default();
                let mut distinct = codes.clone();
                distinct.sort_unstable();
                distinct.dedup();
                assert_eq!(
                    codes.len(),
                    distinct.len(),
                    "{version} repeats a filter code for {}: {codes:?}",
                    system["uri"]
                );
            }
        }
        let concept = &code_system(&statement, SNOMED)["version"][0]["filter"][0];
        assert_eq!(concept["code"], "concept", "{version}");
        let operators: Vec<&str> = concept["op"]
            .as_array()
            .expect("op")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        for expected in [
            "=",
            "in",
            "not-in",
            "regex",
            "exists",
            "is-a",
            "descendent-of",
        ] {
            assert!(
                operators.contains(&expected),
                "{version} drops `{expected}` from the SNOMED CT `concept` filter: {operators:?}"
            );
        }
    }
}

/// The operator codes the version's `filter-operator` code system defines,
/// read from the core bundle the server itself loads for that version.
fn defined_operators(version: &str) -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("data/fhir/{version}/code-systems.json"));
    let text = std::fs::read_to_string(&path).expect("reads the core bundle");
    let bundle: Vec<Value> = serde_json::from_str(&text).expect("parses");
    bundle
        .iter()
        .find(|system| system.get("url").and_then(Value::as_str) == Some(FILTER_OPERATOR))
        .expect("the bundle carries the filter-operator code system")
        .get("concept")
        .and_then(Value::as_array)
        .expect("the code system enumerates its concepts")
        .iter()
        .filter_map(|concept| concept.get("code").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}

/// The code system whose codes `ValueSet.compose.include.filter.op` is bound to
/// (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.compose.include.filter.op>).
const FILTER_OPERATOR: &str = "http://hl7.org/fhir/filter-operator";

#[test]
fn no_version_advertises_a_filter_operator_its_own_value_set_does_not_define() {
    // `ValueSet.compose.include.filter.op` is required-bound to the version's
    // own `filter-operator` value set
    // (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.compose.include.filter.op>),
    // so a statement offering an operator the version lacks invites a resource
    // that version refuses. `filter.op` is 1..*, so an entry states one at least.
    let dir = tempfile::tempdir().expect("tempdir");
    let summary = Summary::of(&served(dir.path()));
    for (version, statement) in rendered(&summary) {
        let defined = defined_operators(version);
        for system in statement["codeSystem"].as_array().expect("codeSystem") {
            for entry in system["version"].as_array().expect("version") {
                for filter in entry["filter"].as_array().into_iter().flatten() {
                    let operators = filter["op"].as_array().expect("op");
                    assert!(
                        !operators.is_empty(),
                        "{version} states the filter {} with no operator",
                        filter["code"]
                    );
                    for operator in operators.iter().filter_map(Value::as_str) {
                        assert!(
                            defined.iter().any(|code| code == operator),
                            "{version} advertises `{operator}`, which it does not define"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn the_hierarchy_operators_r5_added_reach_the_r5_family_alone() {
    // `child-of` and `descendent-leaf` enter `filter-operator` in 5.0.0; R4
    // 4.0.1 and R4B 4.3.0 define nine operators without them.
    let dir = tempfile::tempdir().expect("tempdir");
    let summary = Summary::of(&served(dir.path()));
    for (version, statement) in rendered(&summary) {
        let concept = &code_system(&statement, SNOMED)["version"][0]["filter"][0];
        assert_eq!(concept["code"], "concept", "{version}");
        let operators: Vec<&str> = concept["op"]
            .as_array()
            .expect("op")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        let r5_family = version == "r5" || version == "r6";
        for added in ["child-of", "descendent-leaf"] {
            assert_eq!(
                operators.contains(&added),
                r5_family,
                "{version} states `{added}`: {operators:?}"
            );
        }
        assert!(
            operators.contains(&"is-a") && operators.contains(&"generalizes"),
            "{version} keeps the operators every version defines: {operators:?}"
        );
    }
}

#[test]
fn a_declared_code_filter_states_the_operators_the_engine_answers_on_it() {
    // `evaluate` reads `concept` and `code` as the code itself, so a system
    // declaring `code` answers the generic set there beside its own. ISO 3166-1
    // declares `code` with `regex`, `=`, and `in`.
    let mut registry = Registry::new();
    registry
        .register(Arc::new(
            fhir_terminology::registries::iso3166::provider().expect("builds"),
        ))
        .expect("registers");
    let summary = Summary::of(&registry);
    let filters = &summary.systems[0].versions[0].filters;
    let code = filters
        .iter()
        .find(|filter| filter.code == "code")
        .expect("the declared filter");
    let operators: Vec<&str> = code.operators.iter().map(|op| op.code()).collect();
    assert_eq!(
        operators,
        ["=", "in", "not-in", "regex", "exists"],
        "the generic set, the declared operators folded into it"
    );
}

#[test]
fn the_artifact_declaration_names_no_directory_above_it_and_no_content() {
    // The artifact sits under a temporary parent the declaration must not
    // carry: an operator's layout is not wire content, and neither is any
    // concept of a licensed release.
    let dir = tempfile::tempdir().expect("tempdir");
    let statement = document(
        &Summary::of(&served(dir.path()))
            .to_r4b("2026-09-06T00:00:00Z")
            .to_json()
            .expect("encodes"),
    )
    .to_string();
    let parent = dir.path().to_string_lossy().into_owned();
    assert!(
        !statement.contains(&parent),
        "the statement names no directory above the artifact"
    );
    assert!(
        !statement.contains(&sctid(item(CAT))),
        "the statement carries no concept of the release"
    );
}

/// A rendered resource as a `serde_json` document, so a test can index into it.
fn document(object: &fhir_types::codec::Object) -> Value {
    let text = serde_json::to_string(object).expect("the document writes");
    serde_json::from_str(&text).expect("the document parses")
}
