// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `_elements` and `_summary` on the wire: the subset a client asked for, on
//! every version, on search and on read.
//!
//! The unit tests in `ferroterm_server::elements` pin the projection itself.
//! These pin what a client sees: that the parameter is answered rather than
//! refused, that the answer says it is a subset, and that a search still
//! matches the same resources when it narrows what they carry.

use fhir_types::schema::Schemas;
use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::Server;

/// The served versions and the element table of each.
const VERSIONS: [(&str, &Schemas); 4] = [
    ("r4", &fhir_types::r4::schema::SCHEMAS),
    ("r4b", &fhir_types::r4b::schema::SCHEMAS),
    ("r5", &fhir_types::r5::schema::SCHEMAS),
    ("r6", &fhir_types::r6::schema::SCHEMAS),
];

/// The three resource types the server serves.
const TYPES: [&str; 3] = ["CodeSystem", "ValueSet", "ConceptMap"];

/// The metadata elements every terminology resource of R4 and R4B marks
/// `isSummary`, in definition order.
const R4_METADATA: [&str; 15] = [
    "id",
    "meta",
    "implicitRules",
    "url",
    "identifier",
    "version",
    "name",
    "title",
    "status",
    "experimental",
    "date",
    "publisher",
    "contact",
    "useContext",
    "jurisdiction",
];

/// The same for R5 and the R6 ballot, which add `modifierExtension` and
/// `versionAlgorithm`.
const R5_METADATA: [&str; 17] = [
    "id",
    "meta",
    "implicitRules",
    "modifierExtension",
    "url",
    "identifier",
    "version",
    "versionAlgorithm",
    "name",
    "title",
    "status",
    "experimental",
    "date",
    "publisher",
    "contact",
    "useContext",
    "jurisdiction",
];

/// The `CodeSystem` summary elements after the metadata, R4 and R4B.
const R4_CODE_SYSTEM: [&str; 10] = [
    "caseSensitive",
    "valueSet",
    "hierarchyMeaning",
    "compositional",
    "versionNeeded",
    "content",
    "supplements",
    "count",
    "filter",
    "property",
];

/// The `CodeSystem` summary elements after the metadata, R5 and R6.
const R5_CODE_SYSTEM: [&str; 11] = [
    "effectivePeriod",
    "caseSensitive",
    "valueSet",
    "hierarchyMeaning",
    "compositional",
    "versionNeeded",
    "content",
    "supplements",
    "count",
    "filter",
    "property",
];

/// The summary elements of each type after its metadata, as the pinned
/// `StructureDefinition` resources of each served version flag them.
const SUMMARY_SETS: [(&str, &str, &[&str], &[&str]); 12] = [
    ("r4", "CodeSystem", &R4_METADATA, &R4_CODE_SYSTEM),
    ("r4", "ValueSet", &R4_METADATA, &["immutable"]),
    ("r4", "ConceptMap", &R4_METADATA, &["source", "target"]),
    ("r4b", "CodeSystem", &R4_METADATA, &R4_CODE_SYSTEM),
    ("r4b", "ValueSet", &R4_METADATA, &["immutable"]),
    ("r4b", "ConceptMap", &R4_METADATA, &["source", "target"]),
    ("r5", "CodeSystem", &R5_METADATA, &R5_CODE_SYSTEM),
    (
        "r5",
        "ValueSet",
        &R5_METADATA,
        &["immutable", "effectivePeriod"],
    ),
    (
        "r5",
        "ConceptMap",
        &R5_METADATA,
        &[
            "effectivePeriod",
            "property",
            "additionalAttribute",
            "sourceScope",
            "targetScope",
        ],
    ),
    ("r6", "CodeSystem", &R5_METADATA, &R5_CODE_SYSTEM),
    (
        "r6",
        "ValueSet",
        &R5_METADATA,
        &["immutable", "effectivePeriod"],
    ),
    (
        "r6",
        "ConceptMap",
        &R5_METADATA,
        &[
            "effectivePeriod",
            "property",
            "additionalAttribute",
            "allowedRelationship",
            "sourceScope",
            "targetScope",
        ],
    ),
];

/// The system of the tag a subsetted resource carries.
const TAG_SYSTEM: &str = "http://terminology.hl7.org/CodeSystem/v3-ObservationValue";

/// The elements every entry of `body` carries, as one sorted list.
fn elements_of(body: &Value) -> Vec<String> {
    let Some(entries) = body["entry"].as_array() else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .iter()
        .filter_map(|entry| entry["resource"].as_object())
        .flat_map(|resource| resource.keys().cloned())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Whether every entry of `body` is marked as a subset.
fn every_entry_is_subsetted(body: &Value) -> bool {
    let Some(entries) = body.get("entry").and_then(Value::as_array) else {
        return false;
    };
    !entries.is_empty()
        && entries.iter().all(|entry| {
            entry
                .pointer("/resource/meta/tag")
                .and_then(Value::as_array)
                .is_some_and(|tags| {
                    tags.iter().any(|tag| {
                        tag.get("system").and_then(Value::as_str) == Some(TAG_SYSTEM)
                            && tag.get("code").and_then(Value::as_str) == Some("SUBSETTED")
                    })
                })
        })
}

#[tokio::test]
async fn a_search_returns_the_elements_it_was_asked_for_on_every_served_version() {
    let server = Server::start_with_every_loader();
    for version in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server
            .get(&format!("/{version}/CodeSystem?_elements=url,title"))
            .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{version} refuses a parameter its own release defines: {body}"
        );
        let names = elements_of(&body);
        assert!(
            !names.is_empty(),
            "{version} answered a search with no resources in it: {body}"
        );
        for name in &names {
            assert!(
                ["id", "meta", "resourceType", "title", "url"].contains(&name.as_str()),
                "{version} returned `{name}`, which the search did not ask for"
            );
        }
        assert!(
            names.contains(&"url".to_owned()),
            "{version} left out an element the search did ask for: {names:?}"
        );
    }
}

#[tokio::test]
async fn a_subsetted_resource_says_that_it_is_one() {
    let server = Server::start_with_every_loader();
    let (status, body) = server.get("/r5/CodeSystem?_elements=url").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        every_entry_is_subsetted(&body),
        "a client must not mistake a subset for the whole resource: {body}"
    );
}

#[tokio::test]
async fn the_mandatory_elements_come_back_whether_they_were_asked_for_or_not() {
    let server = Server::start_with_every_loader();
    let (status, body) = server.get("/r5/CodeSystem?_elements=title").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let names = elements_of(&body);
    for mandatory in ["resourceType", "id"] {
        assert!(
            names.contains(&mandatory.to_owned()),
            "the specification says a server returns mandatory elements whether they are requested or not, and `{mandatory}` is missing: {names:?}"
        );
    }
}

#[tokio::test]
async fn narrowing_what_a_resource_carries_does_not_change_what_the_search_matched() {
    let server = Server::start_with_every_loader();
    let (_, whole) = server.get("/r5/CodeSystem?url=http://loinc.org").await;
    let (status, subset) = server
        .get("/r5/CodeSystem?url=http://loinc.org&_elements=url")
        .await;
    assert_eq!(status, StatusCode::OK, "{subset}");
    assert_eq!(
        subset["total"], whole["total"],
        "_elements says what to return, not what to match"
    );
    assert_eq!(
        subset["entry"][0]["fullUrl"], whole["entry"][0]["fullUrl"],
        "the entry still names the resource it matched"
    );
}

#[tokio::test]
async fn an_element_no_resource_carries_is_not_an_error() {
    let server = Server::start_with_every_loader();
    let (status, body) = server
        .get("/r5/CodeSystem?_elements=url,neverAnElementOfCodeSystem")
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an element a resource does not have is absent, not a refusal: {body}"
    );
    let names = elements_of(&body);
    assert!(names.contains(&"url".to_owned()), "{names:?}");
    assert!(!names.contains(&"neverAnElementOfCodeSystem".to_owned()));
}

#[tokio::test]
async fn a_search_naming_no_elements_is_the_search_it_always_was() {
    let server = Server::start_with_every_loader();
    let (_, whole) = server.get("/r5/CodeSystem?url=http://loinc.org").await;
    let (status, empty) = server
        .get("/r5/CodeSystem?url=http://loinc.org&_elements=")
        .await;
    assert_eq!(status, StatusCode::OK, "{empty}");
    assert_eq!(
        empty["entry"], whole["entry"],
        "a client that named nothing asked for everything"
    );
    assert_eq!(empty["total"], whole["total"], "{empty}");
    // The `self` link is the URL the search was made at
    // (<https://hl7.org/fhir/R4B/http.html#paging>), so it carries each
    // request's own query.
    assert_eq!(
        empty["link"][0]["url"],
        "http://ferroterm.test/r5/CodeSystem?url=http://loinc.org&_elements="
    );
    assert_eq!(
        whole["link"][0]["url"],
        "http://ferroterm.test/r5/CodeSystem?url=http://loinc.org"
    );
}

#[tokio::test]
async fn the_value_set_and_concept_map_searches_answer_it_too() {
    let server = Server::start_with_every_loader();
    for resource in ["ValueSet", "ConceptMap"] {
        let (status, body) = server.get(&format!("/r5/{resource}?_elements=url")).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "{resource} refuses a parameter every resource type defines: {body}"
        );
    }
}

#[tokio::test]
async fn the_capability_statement_declares_the_parameter_it_answers() {
    let server = Server::start_with_every_loader();
    for version in ["r4", "r4b", "r5", "r6"] {
        let (status, body) = server.get(&format!("/{version}/metadata")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let declared = body["rest"][0]["resource"]
            .as_array()
            .expect("a rest entry lists its resource types")
            .iter()
            .filter(|resource| resource["type"] == "CodeSystem")
            .flat_map(|resource| {
                resource["searchParam"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .any(|param| {
                param["name"] == "_elements"
                    && param["definition"]
                        == "http://hl7.org/fhir/SearchParameter/Resource-elements"
            });
        assert!(
            declared,
            "{version} answers _elements without declaring it, so a client cannot discover it"
        );
    }
}

#[tokio::test]
async fn a_parameter_no_version_defines_is_still_refused() {
    let server = Server::start_with_every_loader();
    let (status, body) = server.get("/r5/CodeSystem?_summaryish=true").await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "accepting _elements does not loosen the rest: {body}"
    );
    assert_eq!(body["issue"][0]["code"], "not-supported");
}

/// Whether `resource` carries the `SUBSETTED` tag.
fn is_subsetted(resource: &Value) -> bool {
    resource
        .pointer("/meta/tag")
        .and_then(Value::as_array)
        .is_some_and(|tags| {
            tags.iter()
                .any(|tag| tag["system"] == TAG_SYSTEM && tag["code"] == "SUBSETTED")
        })
}

/// The top-level elements of `resource_type` its definition makes mandatory.
fn mandatory(schemas: &Schemas, resource_type: &str) -> Vec<&'static str> {
    schemas
        .type_named(resource_type)
        .expect("the version defines the type")
        .fields
        .iter()
        .filter(|field| field.min > 0)
        .map(|field| field.name)
        .collect()
}

/// The top-level elements of `resource_type` its definition marks
/// `isSummary`, in definition order.
fn summary(schemas: &Schemas, resource_type: &str) -> Vec<&'static str> {
    schemas
        .type_named(resource_type)
        .expect("the version defines the type")
        .fields
        .iter()
        .filter(|field| field.is_summary)
        .map(|field| field.name)
        .collect()
}

/// The sorted member names of `resource` that `_summary=true` keeps: those
/// the definition marks `isSummary`, plus `resourceType`, `id` and `meta`.
fn summary_members(resource: &Value, schemas: &Schemas, resource_type: &str) -> Vec<String> {
    let flagged = summary(schemas, resource_type);
    let mut names: Vec<String> = resource
        .as_object()
        .expect("a resource")
        .keys()
        .filter(|name| flagged.contains(&name.as_str()))
        .cloned()
        .chain(["id", "meta", "resourceType"].map(str::to_owned))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The sorted member names of `resource`.
fn members(resource: &Value) -> Vec<String> {
    let mut names: Vec<String> = resource
        .as_object()
        .expect("a resource")
        .keys()
        .cloned()
        .collect();
    names.sort();
    names
}

/// The element each type carries that is not a summary element and that the
/// fixture fills.
fn content_element(resource_type: &str) -> &'static str {
    match resource_type {
        "CodeSystem" => "concept",
        "ValueSet" => "compose",
        _ => "group",
    }
}

/// A persisted resource of `resource_type` carrying a narrative and content
/// that `_summary=text` leaves out.
fn with_narrative(resource_type: &str, version: &str) -> Value {
    let url = format!("http://ferroterm.test/{resource_type}/summary-{version}");
    let text = json!({
        "status": "generated",
        "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">A narrative</div>"
    });
    match resource_type {
        "CodeSystem" => json!({
            "resourceType": "CodeSystem", "url": url, "version": "1", "name": "Summary", "text": text,
            "status": "active", "content": "complete", "title": "Summary",
            "concept": [{"code": "a", "display": "A"}]
        }),
        "ValueSet" => json!({
            "resourceType": "ValueSet", "url": url, "version": "1", "name": "Summary", "text": text,
            "status": "active", "title": "Summary",
            "compose": {"include": [{"system": "http://ferroterm.test/CodeSystem/summary"}]}
        }),
        _ if version == "r4" || version == "r4b" => json!({
            "resourceType": "ConceptMap", "url": url, "version": "1", "name": "Summary", "text": text,
            "status": "active", "title": "Summary",
            "group": [{"source": "http://a.test", "target": "http://b.test",
                "element": [{"code": "a", "target": [{"code": "b", "equivalence": "equivalent"}]}]}]
        }),
        _ => json!({
            "resourceType": "ConceptMap", "url": url, "version": "1", "name": "Summary", "text": text,
            "status": "active", "title": "Summary",
            "group": [{"source": "http://a.test", "target": "http://b.test",
                "element": [{"code": "a", "target": [{"code": "b", "relationship": "equivalent"}]}]}]
        }),
    }
}

/// A server holding one persisted resource of every type on every version,
/// each at the id `summary-{version}`.
async fn persisting_with_narratives() -> Server {
    let server = Server::start_persisting();
    for (version, _) in VERSIONS {
        for resource_type in TYPES {
            let response = server
                .put(
                    &format!("/{version}/{resource_type}/summary-{version}"),
                    &with_narrative(resource_type, version),
                )
                .await;
            assert!(
                response.status().is_success(),
                "{version} {resource_type}: {}",
                response.status()
            );
        }
    }
    server
}

#[test]
fn the_mandatory_elements_are_the_ones_the_pinned_definitions_name() {
    // `_summary=text` keeps the top-level mandatory elements
    // (<https://hl7.org/fhir/R4B/search.html#summary>): `min` of 1 in the
    // pinned StructureDefinitions, which the generated element table carries.
    for (version, schemas) in VERSIONS {
        assert_eq!(
            mandatory(schemas, "CodeSystem"),
            ["status", "content"],
            "{version}"
        );
        assert_eq!(mandatory(schemas, "ValueSet"), ["status"], "{version}");
        assert_eq!(mandatory(schemas, "ConceptMap"), ["status"], "{version}");
    }
}

#[tokio::test]
async fn summary_text_on_read_keeps_the_narrative_and_the_mandatory_elements() {
    let server = persisting_with_narratives().await;
    for (version, schemas) in VERSIONS {
        for resource_type in TYPES {
            let uri = format!("/{version}/{resource_type}/summary-{version}?_summary=text");
            let (status, body) = server.get(&uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}: {body}");
            let mut names: Vec<&str> = body
                .as_object()
                .expect("a resource")
                .keys()
                .map(String::as_str)
                .collect();
            names.sort_unstable();
            let mut expected = vec!["id", "meta", "resourceType", "text"];
            expected.extend(mandatory(schemas, resource_type));
            expected.sort_unstable();
            assert_eq!(names, expected, "{uri}");
            assert!(is_subsetted(&body), "{uri}: {body}");
        }
    }
}

#[tokio::test]
async fn summary_data_on_read_drops_the_narrative_alone() {
    let server = persisting_with_narratives().await;
    for (version, _) in VERSIONS {
        for resource_type in TYPES {
            let (_, whole) = server
                .get(&format!("/{version}/{resource_type}/summary-{version}"))
                .await;
            let uri = format!("/{version}/{resource_type}/summary-{version}?_summary=data");
            let (status, mut body) = server.get(&uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}: {body}");
            assert!(body.get("text").is_none(), "{uri}: {body}");
            assert!(is_subsetted(&body), "{uri}: {body}");
            let data = body.as_object_mut().expect("a resource");
            data.remove("meta");
            let mut whole = whole.as_object().expect("a resource").clone();
            whole.remove("meta");
            whole.remove("text");
            assert_eq!(*data, whole, "{uri}: only `text` is removed");
        }
    }
}

#[tokio::test]
async fn summary_false_on_read_is_the_whole_resource() {
    let server = persisting_with_narratives().await;
    for (version, _) in VERSIONS {
        for resource_type in TYPES {
            let (_, whole) = server
                .get(&format!("/{version}/{resource_type}/summary-{version}"))
                .await;
            let uri = format!("/{version}/{resource_type}/summary-{version}?_summary=false");
            let (status, body) = server.get(&uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}: {body}");
            assert_eq!(body, whole, "{uri}");
            assert!(!is_subsetted(&body), "{uri}: nothing was left out");
        }
    }
}

#[tokio::test]
async fn elements_on_read_projects_like_a_search_does() {
    let server = persisting_with_narratives().await;
    for (version, _) in VERSIONS {
        for resource_type in TYPES {
            let uri = format!("/{version}/{resource_type}/summary-{version}?_elements=url,title");
            let (status, body) = server.get(&uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}: {body}");
            let mut names: Vec<&str> = body
                .as_object()
                .expect("a resource")
                .keys()
                .map(String::as_str)
                .collect();
            names.sort_unstable();
            assert_eq!(
                names,
                ["id", "meta", "resourceType", "title", "url"],
                "{uri}"
            );
            assert!(is_subsetted(&body), "{uri}: {body}");
        }
    }
}

#[tokio::test]
async fn a_loaded_code_system_reads_as_a_summary_and_as_a_subset() {
    let server = Server::start_with_every_loader();
    let id = server.instance_id_of("http://loinc.org");
    for (version, schemas) in VERSIONS {
        let (status, body) = server
            .get(&format!("/{version}/CodeSystem/{id}?_summary=text"))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        for name in body.as_object().expect("a resource").keys() {
            assert!(
                ["id", "meta", "resourceType", "text"].contains(&name.as_str())
                    || mandatory(schemas, "CodeSystem").contains(&name.as_str()),
                "{version} returned `{name}` in the text summary"
            );
        }
        assert!(is_subsetted(&body), "{version}: {body}");
        let (status, body) = server
            .get(&format!("/{version}/CodeSystem/{id}?_elements=url"))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(body["url"], "http://loinc.org", "{version}");
        assert!(body.get("version").is_none(), "{version}: {body}");
        assert!(is_subsetted(&body), "{version}: {body}");
    }
}

#[tokio::test]
async fn summary_text_and_data_on_search_project_every_match() {
    let server = persisting_with_narratives().await;
    for (version, schemas) in VERSIONS {
        for resource_type in TYPES {
            let uri = format!(
                "/{version}/{resource_type}?url=http://ferroterm.test/{resource_type}/summary-{version}&_summary=text"
            );
            let (status, body) = server.get(&uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}: {body}");
            assert_eq!(body["total"], 1, "{uri}: {body}");
            let resource = &body["entry"][0]["resource"];
            let mut names: Vec<&str> = resource
                .as_object()
                .expect("a resource")
                .keys()
                .map(String::as_str)
                .collect();
            names.sort_unstable();
            let mut expected = vec!["id", "meta", "resourceType", "text"];
            expected.extend(mandatory(schemas, resource_type));
            expected.sort_unstable();
            assert_eq!(names, expected, "{uri}");
            assert!(is_subsetted(resource), "{uri}");

            let uri = format!(
                "/{version}/{resource_type}?url=http://ferroterm.test/{resource_type}/summary-{version}&_summary=data"
            );
            let (status, body) = server.get(&uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}: {body}");
            let resource = &body["entry"][0]["resource"];
            assert!(resource.get("text").is_none(), "{uri}: {body}");
            assert!(resource.get("status").is_some(), "{uri}: {body}");
            assert!(is_subsetted(resource), "{uri}");
        }
    }
}

#[tokio::test]
async fn summary_count_on_search_answers_the_total_and_no_entries() {
    let server = Server::start_with_every_loader();
    for (version, _) in VERSIONS {
        for resource_type in TYPES {
            let (_, whole) = server.get(&format!("/{version}/{resource_type}")).await;
            let uri = format!("/{version}/{resource_type}?_summary=count");
            let (status, body) = server.get(&uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}: {body}");
            assert_eq!(body["type"], "searchset", "{uri}");
            assert_eq!(body["total"], whole["total"], "{uri}: {body}");
            assert!(
                body.get("entry").is_none(),
                "{uri} returned matches with the count: {body}"
            );
        }
    }
}

#[tokio::test]
async fn summary_count_on_a_read_and_a_value_the_specification_does_not_define_are_refused() {
    let server = persisting_with_narratives().await;
    for (version, _) in VERSIONS {
        for uri in [
            format!("/{version}/CodeSystem/summary-{version}?_summary=count"),
            format!("/{version}/CodeSystem/summary-{version}?_summary=everything"),
            format!("/{version}/CodeSystem?_summary=TEXT"),
            format!("/{version}/CodeSystem?_summary=text&_summary=data"),
        ] {
            let (status, body) = server.get(&uri).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
            assert_eq!(body["issue"][0]["code"], "invalid", "{uri}");
        }
    }
}

#[test]
fn the_summary_elements_are_the_ones_the_pinned_definitions_flag() {
    // `_summary=true` keeps the elements marked "summary" in the base
    // definition (<https://hl7.org/fhir/R4B/search.html#summary>): the
    // `isSummary` flag of the pinned StructureDefinitions, carried by the
    // generated element table.
    for (version, schemas) in VERSIONS {
        for resource_type in TYPES {
            let flagged = summary(schemas, resource_type);
            for kept in ["id", "meta", "url", "version", "name", "status"] {
                assert!(
                    flagged.contains(&kept),
                    "{version} {resource_type} has no summary `{kept}`: {flagged:?}"
                );
            }
            for dropped in [
                "text",
                "contained",
                "extension",
                content_element(resource_type),
            ] {
                assert!(
                    !flagged.contains(&dropped),
                    "{version} {resource_type} marks `{dropped}` as summary: {flagged:?}"
                );
            }
        }
    }
    for (version, resource_type, metadata, rest) in SUMMARY_SETS {
        let schemas = VERSIONS
            .iter()
            .find(|(served, _)| *served == version)
            .map(|(_, schemas)| *schemas)
            .expect("a served version");
        let expected: Vec<&str> = metadata.iter().chain(rest).copied().collect();
        assert_eq!(
            summary(schemas, resource_type),
            expected,
            "{version} {resource_type}"
        );
    }
}

#[tokio::test]
async fn summary_true_on_read_keeps_the_summary_elements() {
    let server = persisting_with_narratives().await;
    for (version, schemas) in VERSIONS {
        for resource_type in TYPES {
            let (_, whole) = server
                .get(&format!("/{version}/{resource_type}/summary-{version}"))
                .await;
            let uri = format!("/{version}/{resource_type}/summary-{version}?_summary=true");
            let (status, body) = server.get(&uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}: {body}");
            assert_eq!(
                members(&body),
                summary_members(&whole, schemas, resource_type),
                "{uri}"
            );
            for kept in ["url", "version", "name", "status"] {
                assert_eq!(
                    body[kept], whole[kept],
                    "{uri}: `{kept}` is a summary element"
                );
            }
            for dropped in ["text", content_element(resource_type)] {
                assert!(
                    body.get(dropped).is_none(),
                    "{uri} kept `{dropped}`: {body}"
                );
            }
            assert!(is_subsetted(&body), "{uri}: {body}");
        }
    }
}

#[tokio::test]
async fn summary_true_on_search_keeps_the_summary_elements_of_every_match() {
    let server = persisting_with_narratives().await;
    for (version, schemas) in VERSIONS {
        for resource_type in TYPES {
            let url = format!("http://ferroterm.test/{resource_type}/summary-{version}");
            let (_, whole) = server
                .get(&format!("/{version}/{resource_type}?url={url}"))
                .await;
            let uri = format!("/{version}/{resource_type}?url={url}&_summary=true");
            let (status, body) = server.get(&uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}: {body}");
            assert_eq!(body["total"], 1, "{uri}: {body}");
            let resource = &body["entry"][0]["resource"];
            assert_eq!(
                members(resource),
                summary_members(&whole["entry"][0]["resource"], schemas, resource_type),
                "{uri}"
            );
            assert!(
                resource.get(content_element(resource_type)).is_none(),
                "{uri}: {body}"
            );
            assert_eq!(resource["status"], "active", "{uri}: {body}");
            assert!(is_subsetted(resource), "{uri}");
        }
    }
}

#[tokio::test]
async fn a_loaded_code_system_reads_as_a_summary_of_its_summary_elements() {
    let server = Server::start_with_every_loader();
    let id = server.instance_id_of("http://loinc.org");
    for (version, schemas) in VERSIONS {
        let (_, whole) = server.get(&format!("/{version}/CodeSystem/{id}")).await;
        let (status, body) = server
            .get(&format!("/{version}/CodeSystem/{id}?_summary=true"))
            .await;
        assert_eq!(status, StatusCode::OK, "{version}: {body}");
        assert_eq!(
            members(&body),
            summary_members(&whole, schemas, "CodeSystem"),
            "{version}"
        );
        assert_eq!(body["url"], "http://loinc.org", "{version}");
        assert!(body.get("concept").is_none(), "{version}: {body}");
        let kept = summary_members(&whole, schemas, "CodeSystem");
        let lost = members(&whole).iter().any(|name| !kept.contains(name));
        assert_eq!(
            is_subsetted(&body),
            lost,
            "{version}: the tag marks exactly a resource that lost an element"
        );
    }
}

#[tokio::test]
async fn a_summary_reads_in_xml_too() {
    let server = persisting_with_narratives().await;
    let (status, content_type, body) = server
        .get_text(
            "/r4b/CodeSystem/summary-r4b?_summary=text&_format=xml",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(content_type, "application/fhir+xml; charset=utf-8");
    assert!(body.contains("SUBSETTED"), "{body}");
    assert!(!body.contains("<concept>"), "{body}");
}

#[tokio::test]
async fn the_capability_statement_declares_summary() {
    let server = Server::start_with_every_loader();
    for (version, _) in VERSIONS {
        let (status, body) = server.get(&format!("/{version}/metadata")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        for resource in body["rest"][0]["resource"]
            .as_array()
            .expect("a rest entry lists its resource types")
        {
            let declared = resource["searchParam"].as_array().is_some_and(|params| {
                params.iter().any(|param| {
                    param["name"] == "_summary"
                        && param["definition"]
                            == "http://hl7.org/fhir/SearchParameter/Resource-summary"
                        && param["type"] == "token"
                })
            });
            assert!(declared, "{version} {}: {resource}", resource["type"]);
        }
    }
}

/// A `CodeSystem` whose `title` and `version` carry an extension in their
/// `_name` member (<https://hl7.org/fhir/R4B/json.html#primitive>).
fn with_primitive_extensions(version: &str) -> Value {
    let extension = json!({"extension": [{
        "url": "http://hl7.org/fhir/StructureDefinition/translation",
        "extension": [{"url": "lang", "valueCode": "nl"}]
    }]});
    json!({
        "resourceType": "CodeSystem",
        "url": format!("http://ferroterm.test/CodeSystem/extended-{version}"),
        "version": "1",
        "_version": extension,
        "title": "Extended",
        "_title": extension,
        "status": "active",
        "content": "complete",
        "concept": [{"code": "a", "display": "A"}]
    })
}

#[tokio::test]
async fn elements_keep_the_extension_member_of_a_named_primitive_on_every_version() {
    let server = Server::start_persisting();
    for (version, _) in VERSIONS {
        let id = format!("extended-{version}");
        let written = with_primitive_extensions(version);
        let response = server
            .put(&format!("/{version}/CodeSystem/{id}"), &written)
            .await;
        assert!(
            response.status().is_success(),
            "{version}: {}",
            response.status()
        );

        let uri = format!("/{version}/CodeSystem/{id}?_elements=title");
        let (status, body) = server.get(&uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        assert_eq!(body["_title"], written["_title"], "{uri}: {body}");
        assert!(
            body.get("_version").is_none(),
            "{uri}: the member of an element not asked for is left out: {body}"
        );

        let uri = format!(
            "/{version}/CodeSystem?url=http://ferroterm.test/CodeSystem/extended-{version}&_elements=title"
        );
        let (status, body) = server.get(&uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        let resource = &body["entry"][0]["resource"];
        assert_eq!(resource["_title"], written["_title"], "{uri}: {body}");
        assert!(resource.get("_version").is_none(), "{uri}: {body}");
    }
}
