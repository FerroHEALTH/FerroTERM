// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The concept reader's tests: the hierarchy each root can walk, and the
//! `$lookup` request and answer.

use super::*;

/// The document `GET /r4b/metadata?mode=terminology` answered.
const R4B: &str = include_str!("../../../fixtures/terminology-capabilities-r4b.json");
/// The document `GET /r5/metadata?mode=terminology` answered.
const R5: &str = include_str!("../../../fixtures/terminology-capabilities-r5.json");
/// Every recorded document, named by the root that answered it.
const RECORDED: [(&str, &str); 4] = [
    (
        "r4",
        include_str!("../../../fixtures/terminology-capabilities-r4.json"),
    ),
    ("r4b", R4B),
    ("r5", R5),
    (
        "r6",
        include_str!("../../../fixtures/terminology-capabilities-r6.json"),
    ),
];

/// `json`, the document the root `root` answered, decoded at its version.
fn capabilities(root: &str, json: &str) -> crate::fhir::terminology::TerminologyCapabilities {
    root.parse::<FhirVersion>()
        .expect("a root the server mounts")
        .decode(json)
        .expect("the fixture is a valid document at its version")
}

fn hierarchy_of(root: &str, document: &str, system: &str) -> Hierarchy {
    let card = capabilities(root, document)
        .card(system)
        .expect("the fixture declares this system");
    let row = chosen_version(&card, None).expect("the fixture serves a version");
    Hierarchy::of(&row)
}

/// `json`, decoded as the R4B `Parameters` `$lookup` answers.
fn answer(json: &str) -> Concept {
    read(json).expect("the fixture is a valid answer at R4B")
}

/// `json`, read as the R4B answer of `$lookup`.
fn read(json: &str) -> Result<Concept, FhirError> {
    let request = LookupRequest {
        system: String::from("https://terminology.example/x"),
        code: String::from("a"),
        ..LookupRequest::default()
    }
    .request();
    let outcome = crate::fhir::operation::tests::read(FhirVersion::R4B, &request, json)?;
    Concept::read(outcome, "", FhirVersion::R4B)
}

/// The query `request` is sent with at R4B.
fn query(request: &LookupRequest) -> String {
    crate::fhir::operation::tests::query(FhirVersion::R4B, request.request())
}

/// The body `query` is sent with at R4B.
fn body(query: &ConceptQuery) -> serde_json::Value {
    crate::fhir::operation::tests::body(FhirVersion::R4B, query.request())
}

/// The roots whose `filter-operator` value set defines `child-of`.
///
/// R5 added `child-of` and `descendent-leaf` to the value set; the R4
/// family defines neither
/// (<https://hl7.org/fhir/R4B/codesystem-filter-operator.html>,
/// <https://hl7.org/fhir/R5/codesystem-filter-operator.html>). So a root of
/// the R4 family states a hierarchy it has no way to select a direct child
/// of, whatever the code system behind it can do.
const ROOTS_DEFINING_CHILD_OF: [&str; 2] = ["r5", "r6"];

#[test]
fn the_tree_is_walkable_exactly_where_the_root_can_express_a_direct_child_filter() {
    for (root, document) in RECORDED {
        let hierarchy = hierarchy_of(root, document, "http://example.org/fhir/CodeSystem/animals");
        if ROOTS_DEFINING_CHILD_OF.contains(&root) {
            assert_eq!(
                hierarchy,
                Hierarchy::Walkable {
                    property: String::from("concept")
                },
                "{root} declares the direct-child operator, so the tree walks the property it is declared on"
            );
        } else {
            assert_eq!(
                hierarchy,
                Hierarchy::WithoutChildren {
                    operators: vec![
                        String::from("descendent-of"),
                        String::from("generalizes"),
                        String::from("is-a"),
                        String::from("is-not-a"),
                    ]
                },
                "{root} states the hierarchy operators it does define, and none of them selects a direct child, so the screen lists rather than walks"
            );
        }
    }
}

#[test]
fn a_version_declaring_no_hierarchy_operator_has_no_tree_on_every_root() {
    for (root, document) in RECORDED {
        assert_eq!(
            hierarchy_of(root, document, "http://unitsofmeasure.org"),
            Hierarchy::Absent,
            "{root}: a grammar-defined system declares no hierarchy to walk"
        );
    }
}

#[test]
fn a_declared_parent_property_is_not_a_declared_hierarchy_filter() {
    // This system declares the `parent` and `child` concept properties and
    // no hierarchy filter operator, so the tree cannot select a level of it
    // and the screen says so rather than drawing an empty tree.
    let card = capabilities("r5", R5)
        .card("urn:iso:std:iso:3166")
        .expect("the fixture declares this system");
    let row = chosen_version(&card, None).expect("the fixture serves a version");
    assert!(
        row.properties.iter().any(|property| property == "parent"),
        "the version declares the property the tree would read"
    );
    assert_eq!(
        Hierarchy::of(&row),
        Hierarchy::Absent,
        "the pane is gated on the declared filter, which is what a walk needs"
    );
}

#[test]
fn two_systems_with_different_shapes_both_draw_from_the_same_reading() {
    let document = capabilities("r5", R5);
    let walked: Vec<Hierarchy> = ["http://loinc.org", "http://snomed.info/sct"]
        .into_iter()
        .map(|system| {
            let card = document.card(system).expect("the fixture declares it");
            let row = chosen_version(&card, None).expect("a version is served");
            Hierarchy::of(&row)
        })
        .collect();
    assert!(
        walked
            .iter()
            .all(|hierarchy| hierarchy.walk() == Some("concept")),
        "one reading covers both hierarchies: {walked:?}"
    );
}

#[test]
fn a_hierarchy_without_a_child_operator_says_what_it_does_declare() {
    let row = VersionRow {
        filters: vec![crate::fhir::terminology::FilterRow {
            code: String::from("concept"),
            operators: vec![String::from("is-a"), String::from("descendent-of")],
        }],
        ..VersionRow::default()
    };
    assert_eq!(
        Hierarchy::of(&row),
        Hierarchy::WithoutChildren {
            operators: vec![String::from("descendent-of"), String::from("is-a")]
        },
        "the screen names the operators the version declared"
    );
    assert_eq!(
        Hierarchy::of(&row).walk(),
        None,
        "no level of the tree can be selected"
    );
}

#[test]
fn a_child_operator_declared_on_no_property_is_not_a_walk() {
    let row = VersionRow {
        filters: vec![crate::fhir::terminology::FilterRow {
            code: String::new(),
            operators: vec![String::from(CHILD_OF_OPERATOR)],
        }],
        ..VersionRow::default()
    };
    assert_eq!(
        Hierarchy::of(&row).walk(),
        None,
        "a request cannot name a property the server did not"
    );
}

#[test]
fn the_address_names_the_version_it_reads() {
    let card = capabilities("r5", R5)
        .card("http://example.org/fhir/CodeSystem/animals")
        .expect("the fixture declares this system");
    assert_eq!(
        chosen_version(&card, Some("2.0")).and_then(|row| row.code),
        Some(String::from("2.0"))
    );
    assert_eq!(
        chosen_version(&card, Some("1.0")),
        None,
        "a version this root does not serve is stated, not silently replaced"
    );
    assert!(
        chosen_version(&card, Some("  ")).is_some(),
        "an empty version names none, so the default applies"
    );
}

#[test]
fn a_system_that_marks_no_default_reads_its_first_version() {
    let card = SystemCard {
        versions: vec![
            VersionRow {
                code: Some(String::from("a")),
                ..VersionRow::default()
            },
            VersionRow {
                code: Some(String::from("b")),
                ..VersionRow::default()
            },
        ],
        ..SystemCard::default()
    };
    assert_eq!(
        chosen_version(&card, None).and_then(|row| row.code),
        Some(String::from("a"))
    );
}

#[test]
fn a_lookup_address_carries_the_code_and_the_system_encoded() {
    let request = LookupRequest {
        system: String::from("https://terminology.example/x?edition=2031"),
        system_version: Some(String::from("2.0")),
        code: String::from("m/s2"),
        display_language: Some(String::from("nl-NL")),
    };
    assert_eq!(
        query(&request),
        "?code=m%2Fs2&system=https%3A%2F%2Fterminology.example%2Fx%3Fedition%3D2031\
         &version=2.0&displayLanguage=nl-NL",
        "a code carrying a separator stays inside the parameter it belongs to"
    );
}

#[test]
fn a_lookup_address_sends_only_what_the_address_names() {
    let request = LookupRequest {
        system: String::from("https://terminology.example/x"),
        code: String::from("a"),
        ..LookupRequest::default()
    };
    assert_eq!(
        query(&request),
        "?code=a&system=https%3A%2F%2Fterminology.example%2Fx",
        "an unset parameter leaves the server's own default in force"
    );
}

#[test]
fn a_search_body_sends_the_system_as_an_inline_value_set() {
    let query = ConceptQuery {
        system: String::from("https://terminology.example/x"),
        system_version: Some(String::from("2.0")),
        filter: Some(String::from("fever")),
        child_of: None,
        display_language: Some(String::from("nl")),
        count: 20,
    };
    assert_eq!(
        body(&query),
        serde_json::from_str::<serde_json::Value>(
            r#"{"parameter":[{"name":"valueSet","resource":{"compose":{"include":[{"system":"https://terminology.example/x","version":"2.0"}]},"resourceType":"ValueSet","status":"active"}},{"name":"filter","valueString":"fever"},{"name":"count","valueInteger":20},{"name":"excludeNested","valueBoolean":true},{"name":"displayLanguage","valueCode":"nl"}],"resourceType":"Parameters"}"#
        )
        .expect("JSON"),
        "the selection names the system and no value set canonical"
    );
}

#[test]
fn a_child_body_sends_the_declared_property_and_the_child_operator() {
    let query = ConceptQuery {
        system: String::from("https://terminology.example/x"),
        child_of: Some(ChildOf {
            property: String::from("concept"),
            code: String::from("root"),
        }),
        count: 100,
        ..ConceptQuery::default()
    };
    let sent = body(&query);
    assert_eq!(
        sent.pointer("/parameter/0/resource/compose/include/0/filter"),
        Some(&serde_json::json!([{"op":"child-of","property":"concept","value":"root"}])),
        "the walk sends the property the version declared: {sent}"
    );
}

#[test]
fn a_lookup_answer_reads_the_display_and_the_designations() {
    let concept = answer(
        r#"{"resourceType":"Parameters","parameter":[
            {"name":"name","valueString":"Animals"},
            {"name":"version","valueString":"2.0"},
            {"name":"display","valueString":"Kat"},
            {"name":"designation","part":[
                {"name":"language","valueCode":"nl"},
                {"name":"use","valueCoding":{"code":"900000000000013009","display":"Synonym"}},
                {"name":"value","valueString":"Kat"}]},
            {"name":"designation","part":[{"name":"value","valueString":"Cat"}]}]}"#,
    );
    assert_eq!(concept.name.as_deref(), Some("Animals"));
    assert_eq!(concept.version.as_deref(), Some("2.0"));
    assert_eq!(concept.display.as_deref(), Some("Kat"));
    assert_eq!(
        concept.designations.first().map(|row| (
            row.language.clone(),
            row.usage.clone(),
            row.value.clone()
        )),
        Some((
            Some(String::from("nl")),
            Some(String::from("Synonym")),
            String::from("Kat")
        ))
    );
    assert_eq!(
        concept
            .designations
            .get(1)
            .map(|row| (row.language.clone(), row.usage.clone())),
        Some((None, None)),
        "a designation with no language or use is drawn with the absence stated"
    );
}

#[test]
fn a_lookup_answer_reads_every_value_type_a_property_carries() {
    let concept = answer(
        r#"{"parameter":[
            {"name":"name","valueString":"Animals"},
            {"name":"display","valueString":"Cat"},
            {"name":"property","part":[
                {"name":"code","valueCode":"inactive"},
                {"name":"value","valueBoolean":false},
                {"name":"description","valueString":"Whether the concept is active"}]},
            {"name":"property","part":[
                {"name":"code","valueCode":"legs"},
                {"name":"value","valueInteger":4}]},
            {"name":"property","part":[
                {"name":"code","valueCode":"parent"},
                {"name":"value","valueCode":"mammal"}]}]}"#,
    );
    assert_eq!(
        concept
            .properties
            .iter()
            .map(|property| (property.code.clone(), property.value.clone()))
            .collect::<Vec<_>>(),
        vec![
            (String::from("inactive"), Some(String::from("false"))),
            (String::from("legs"), Some(String::from("4"))),
            (String::from("parent"), Some(String::from("mammal"))),
        ]
    );
    assert_eq!(
        concept
            .properties
            .first()
            .and_then(|property| property.description.clone()),
        Some(String::from("Whether the concept is active"))
    );
}

#[test]
fn the_hierarchy_links_are_read_from_the_standard_concept_properties() {
    let concept = answer(
        r#"{"parameter":[
            {"name":"name","valueString":"Animals"},
            {"name":"display","valueString":"Cat"},
            {"name":"property","part":[{"name":"code","valueCode":"parent"},
                {"name":"value","valueCode":"a"}]},
            {"name":"property","part":[{"name":"code","valueCode":"parent"},
                {"name":"value","valueCode":"b"}]},
            {"name":"property","part":[{"name":"code","valueCode":"child"},
                {"name":"value","valueCode":"c"}]}]}"#,
    );
    assert_eq!(
        concept.property_values(PARENT_PROPERTY),
        vec![String::from("a"), String::from("b")],
        "a concept with two parents keeps both, because a hierarchy may be a graph"
    );
    assert_eq!(
        concept.property_values(CHILD_PROPERTY),
        vec![String::from("c")]
    );
}

#[test]
fn an_answer_that_carries_nothing_is_no_concept_at_all() {
    let unread = read(r#"{"resourceType":"Parameters"}"#);
    assert!(
        matches!(unread, Err(FhirError::Decode { .. })),
        "`name` and `display` are 1..1, so an answer without them is refused: {unread:?}"
    );
}
