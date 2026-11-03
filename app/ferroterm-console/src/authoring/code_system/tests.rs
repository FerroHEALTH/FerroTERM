// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The code system editor's tests.

use super::*;

/// The property codes and values one lifecycle writes, in order.
fn written(lifecycle: &Lifecycle) -> Vec<String> {
    lifecycle
        .properties()
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn an_active_concept_carries_its_status_and_nothing_else() {
    assert_eq!(
        written(&Lifecycle::Active),
        vec![r#"{"code":"status","valueCode":"active"}"#.to_owned()],
        "an active concept is neither inactive nor dated"
    );
}

#[test]
fn an_experimental_concept_carries_its_status_and_nothing_else() {
    assert_eq!(
        written(&Lifecycle::Experimental),
        vec![r#"{"code":"status","valueCode":"experimental"}"#.to_owned()]
    );
}

/// A deprecated concept "can still be used, but their use is discouraged"
/// (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>), so it is
/// dated and never flagged inactive.
#[test]
fn deprecating_a_concept_dates_it_and_leaves_it_active() {
    let deprecated = Lifecycle::Deprecated {
        on: "2026-09-24".to_owned(),
    };
    assert!(!deprecated.inactive());
    assert_eq!(
        written(&deprecated),
        vec![
            r#"{"code":"status","valueCode":"deprecated"}"#.to_owned(),
            r#"{"code":"deprecationDate","valueDateTime":"2026-09-24"}"#.to_owned(),
        ]
    );
}

/// A `dateTime` is a date at minimum, so an empty one is not a value the
/// element admits (<https://hl7.org/fhir/R4B/datatypes.html#dateTime>).
/// The state itself is already in the `status` code.
#[test]
fn a_state_whose_date_nobody_typed_writes_no_date() {
    assert_eq!(
        written(&Lifecycle::Retired { on: String::new() }),
        vec![
            r#"{"code":"status","valueCode":"retired"}"#.to_owned(),
            r#"{"code":"inactive","valueBoolean":true}"#.to_owned(),
        ]
    );
    assert_eq!(
        written(&Lifecycle::Deprecated {
            on: "  ".to_owned()
        }),
        vec![r#"{"code":"status","valueCode":"deprecated"}"#.to_owned()]
    );
}

#[test]
fn retiring_a_concept_writes_exactly_the_three_standard_properties() {
    let retired = Lifecycle::Retired {
        on: "2026-09-24".to_owned(),
    };
    assert!(retired.inactive());
    assert_eq!(
        written(&retired),
        vec![
            r#"{"code":"status","valueCode":"retired"}"#.to_owned(),
            r#"{"code":"inactive","valueBoolean":true}"#.to_owned(),
            r#"{"code":"retirementDate","valueDateTime":"2026-09-24"}"#.to_owned(),
        ],
        "a retirement is a status, an inactive flag, and the date it took effect"
    );
}

#[test]
fn a_transition_writes_only_the_properties_the_lifecycle_owns() {
    for lifecycle in [
        Lifecycle::Active,
        Lifecycle::Experimental,
        Lifecycle::Deprecated {
            on: "2026-01-01".to_owned(),
        },
        Lifecycle::Retired {
            on: "2026-01-01".to_owned(),
        },
    ] {
        for property in lifecycle.properties() {
            let code = property
                .get("code")
                .and_then(Value::as_str)
                .expect("every property names the code it sets");
            assert!(
                Lifecycle::owned().contains(&code),
                "{lifecycle:?} writes `{code}`, which is not a standard concept property"
            );
        }
    }
}

#[test]
fn the_four_standard_properties_are_declared_with_their_own_identifiers() {
    let declared = Lifecycle::declarations();
    assert_eq!(declared.len(), Lifecycle::owned().len());
    for property in &declared {
        let code = property
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or_default();
        assert_eq!(
            property.get("uri").and_then(Value::as_str),
            Some(format!("{CONCEPT_PROPERTIES}#{code}").as_str()),
            "a standard property is identified by the system that defines it"
        );
    }
}

/// A draft carrying one retired concept, for the body cases below.
fn drafted() -> Draft {
    let mut draft = Draft::new();
    draft.url = "https://terminology.example/colours".to_owned();
    draft.version = "1.0.0".to_owned();
    let red = draft.keys.next();
    let designation = draft.keys.next();
    let vermilion = draft.keys.next();
    draft.concepts = vec![
        Concept {
            key: red,
            code: "red".to_owned(),
            display: "Red".to_owned(),
            definition: "The colour red".to_owned(),
            lifecycle: Lifecycle::Active,
            designations: vec![Designation {
                key: designation,
                language: "nl-NL".to_owned(),
                usage: "https://terms.example/use|syn".to_owned(),
                value: "Rood".to_owned(),
            }],
            values: Vec::new(),
        },
        Concept {
            key: vermilion,
            code: "vermilion".to_owned(),
            display: "Vermilion".to_owned(),
            lifecycle: Lifecycle::Retired {
                on: "2026-09-24".to_owned(),
            },
            ..Concept::default()
        },
    ];
    draft
}

/// The body of a draft, as a `Value` to read fields out of.
fn body_of(draft: &Draft) -> Value {
    serde_json::from_str(&draft.body()).expect("the editor writes JSON")
}

#[test]
fn a_save_writes_the_metadata_the_resource_requires() {
    let body = body_of(&drafted());
    assert_eq!(body["resourceType"], json!("CodeSystem"));
    assert_eq!(body["url"], json!("https://terminology.example/colours"));
    assert_eq!(body["version"], json!("1.0.0"));
    assert_eq!(body["status"], json!("draft"));
    assert_eq!(body["content"], json!("complete"));
    assert_eq!(body["caseSensitive"], json!(true));
    assert_eq!(
        body.get("id"),
        None,
        "a draft the server has never seen names no logical id"
    );
}

#[test]
fn a_save_declares_the_lifecycle_properties_whatever_the_concepts_carry() {
    let mut draft = drafted();
    draft.concepts.clear();
    let body = body_of(&draft);
    let declared: Vec<&str> = body["property"]
        .as_array()
        .expect("the declarations are an array")
        .iter()
        .filter_map(|property| property.get("code").and_then(Value::as_str))
        .collect();
    assert_eq!(
        declared,
        Lifecycle::owned().to_vec(),
        "a status the system does not declare cannot be read back"
    );
}

#[test]
fn a_retired_concept_reaches_the_wire_as_the_standard_properties() {
    let body = body_of(&drafted());
    let concepts = body["concept"]
        .as_array()
        .expect("the concepts are an array");
    let retired = concepts
        .iter()
        .find(|concept| concept["code"] == json!("vermilion"))
        .expect("the retired concept is written");
    assert_eq!(
        retired["property"],
        json!([
            {"code": "status", "valueCode": "retired"},
            {"code": "inactive", "valueBoolean": true},
            {"code": "retirementDate", "valueDateTime": "2026-09-24"},
        ])
    );
}

#[test]
fn a_designation_carries_its_language_its_use_and_its_value() {
    let body = body_of(&drafted());
    let concepts = body["concept"]
        .as_array()
        .expect("the concepts are an array");
    assert_eq!(
        concepts[0]["designation"],
        json!([{
            "language": "nl-NL",
            "use": {"system": "https://terms.example/use", "code": "syn"},
            "value": "Rood",
        }])
    );
}

#[test]
fn a_half_typed_row_is_left_out_rather_than_sent_empty() {
    let mut draft = drafted();
    let key = draft.keys.next();
    draft.concepts.push(Concept {
        key,
        ..Concept::default()
    });
    let property = draft.keys.next();
    draft.properties.push(Declared {
        key: property,
        ..Declared::default()
    });
    let body = body_of(&draft);
    assert_eq!(
        body["concept"].as_array().map(Vec::len),
        Some(2),
        "a concept with no code is not a concept yet"
    );
    assert_eq!(
        body["property"].as_array().map(Vec::len),
        Some(Lifecycle::owned().len()),
        "a declaration with no code is not a declaration yet"
    );
}

#[test]
fn a_property_value_is_written_under_the_element_its_declared_type_names() {
    for (kind, text, expected) in [
        ("code", "abc", json!({"code": "p", "valueCode": "abc"})),
        (
            "Coding",
            "https://x.example/s|c",
            json!({"code": "p", "valueCoding": {"system": "https://x.example/s", "code": "c"}}),
        ),
        (
            "string",
            "a phrase",
            json!({"code": "p", "valueString": "a phrase"}),
        ),
        ("integer", "-3", json!({"code": "p", "valueInteger": -3})),
        (
            "boolean",
            "true",
            json!({"code": "p", "valueBoolean": true}),
        ),
        (
            "dateTime",
            "2026-09-24",
            json!({"code": "p", "valueDateTime": "2026-09-24"}),
        ),
        ("decimal", "1.5", json!({"code": "p", "valueDecimal": 1.5})),
    ] {
        assert_eq!(value_body("p", kind, text), expected, "{kind}");
    }
}

#[test]
fn a_value_that_is_not_of_its_declared_type_reaches_the_server_as_typed() {
    assert_eq!(
        value_body("p", "integer", "seven"),
        json!({"code": "p", "valueString": "seven"}),
        "the server refuses it in its own words rather than the value being dropped"
    );
}

#[test]
fn a_stored_code_system_round_trips_into_the_form_and_back_out() {
    let stored: Value = serde_json::from_str(
        r#"{"resourceType":"CodeSystem","id":"colours","meta":{"versionId":"4"},
            "url":"https://terminology.example/colours","version":"1.0.0",
            "status":"active","content":"complete","caseSensitive":false,
            "property":[
              {"code":"status","uri":"http://hl7.org/fhir/concept-properties#status","type":"code"},
              {"code":"hue","uri":"https://terminology.example/hue","type":"integer"}],
            "concept":[
              {"code":"red","display":"Red","property":[
                {"code":"hue","valueInteger":0},
                {"code":"status","valueCode":"retired"},
                {"code":"inactive","valueBoolean":true},
                {"code":"retirementDate","valueDateTime":"2026-09-24"}]}]}"#,
    )
    .expect("the server's own answer parses");
    let draft = Draft::of(&stored);
    assert_eq!(draft.id, "colours");
    assert_eq!(draft.version_id, "4");
    assert!(draft.managed());
    assert!(!draft.case_sensitive);
    assert_eq!(
        draft.properties.len(),
        1,
        "the lifecycle's own declarations belong to the lifecycle control"
    );
    assert_eq!(draft.properties[0].code, "hue");
    assert_eq!(draft.concepts[0].values.len(), 1, "and so do its values");
    assert_eq!(draft.concepts[0].values[0].text, "0");
    assert_eq!(
        draft.concepts[0].lifecycle,
        Lifecycle::Retired {
            on: "2026-09-24".to_owned()
        }
    );

    let body = body_of(&draft);
    assert_eq!(body["id"], json!("colours"));
    assert_eq!(
        body["concept"][0]["property"],
        json!([
            {"code": "status", "valueCode": "retired"},
            {"code": "inactive", "valueBoolean": true},
            {"code": "retirementDate", "valueDateTime": "2026-09-24"},
            {"code": "hue", "valueInteger": 0},
        ]),
        "the value keeps the type its declaration names"
    );
}

/// A FHIR update replaces the whole resource
/// (<https://hl7.org/fhir/R4B/http.html#update>), so every element the form
/// does not draw has to reach the server again. A form that rebuilt the
/// resource from what it drew would delete the rest of it silently, which
/// is the failure class this case exists to catch.
#[test]
fn a_save_carries_back_every_element_the_form_does_not_draw() {
    let stored: Value = serde_json::from_str(
        r#"{"resourceType":"CodeSystem","id":"colours","meta":{"versionId":"4"},
            "url":"https://terminology.example/colours","version":"1.0.0",
            "name":"Colours","title":"The colours","publisher":"Someone",
            "hierarchyMeaning":"is-a","experimental":false,
            "text":{"status":"generated","div":"<div>a narrative</div>"},
            "status":"active","content":"complete","caseSensitive":true,
            "property":[{"code":"hue","uri":"https://terminology.example/hue",
                         "type":"integer","description":"the hue angle"}],
            "concept":[{"code":"red","display":"Red",
              "property":[{"code":"hue","valueInteger":0}],
              "concept":[{"code":"crimson","display":"Crimson"}]}]}"#,
    )
    .expect("the server's own answer parses");
    let draft = Draft::of(&stored);
    let body = body_of(&draft);

    for element in [
        "name",
        "title",
        "publisher",
        "hierarchyMeaning",
        "experimental",
        "text",
        "meta",
    ] {
        assert_eq!(
            body.get(element),
            stored.get(element),
            "`{element}` is not an element the form draws, so a save keeps it"
        );
    }
    assert_eq!(
        body["property"][4]["description"],
        json!("the hue angle"),
        "a declaration keeps what the form does not draw of it"
    );
    assert_eq!(
        body["concept"][0]["concept"],
        json!([{"code": "crimson", "display": "Crimson"}]),
        "and a concept keeps its own child concepts"
    );
    assert_eq!(
        body["concept"][0]["display"],
        json!("Red"),
        "while the elements the form does draw are the form's"
    );
}

#[test]
fn a_form_that_empties_an_element_removes_it_rather_than_sending_it_empty() {
    let stored: Value = serde_json::from_str(
        r#"{"resourceType":"CodeSystem","url":"https://terminology.example/colours",
            "version":"1.0.0","status":"active","content":"complete",
            "concept":[{"code":"red","display":"Red","definition":"The colour red",
              "designation":[{"language":"nl-NL","value":"Rood"}]}]}"#,
    )
    .expect("the server's own answer parses");
    let mut draft = Draft::of(&stored);
    draft.version = String::new();
    if let Some(concept) = draft.concepts.first_mut() {
        concept.display = String::new();
        concept.definition = String::from("  ");
        concept.designations.clear();
    }
    let body = body_of(&draft);
    assert_eq!(body.get("version"), None);
    assert_eq!(body["concept"][0].get("display"), None);
    assert_eq!(body["concept"][0].get("definition"), None);
    assert_eq!(
        body["concept"][0].get("designation"),
        None,
        "a designation the reader removed is gone rather than sent empty"
    );
}

#[test]
fn a_concept_with_no_status_reads_the_state_its_dates_imply() {
    let stored: Value = serde_json::from_str(
        r#"{"resourceType":"CodeSystem","status":"active","content":"complete","concept":[
             {"code":"a"},
             {"code":"b","property":[{"code":"retirementDate","valueDateTime":"2001-06-15"}]},
             {"code":"c","property":[{"code":"deprecationDate","valueDateTime":"2001-06-15"}]},
             {"code":"d","property":[{"code":"inactive","valueBoolean":true}]}]}"#,
    )
    .expect("the server's own answer parses");
    let draft = Draft::of(&stored);
    let states: Vec<Lifecycle> = draft
        .concepts
        .iter()
        .map(|concept| concept.lifecycle.clone())
        .collect();
    assert_eq!(
        states,
        vec![
            Lifecycle::Active,
            Lifecycle::Retired {
                on: "2001-06-15".to_owned()
            },
            Lifecycle::Deprecated {
                on: "2001-06-15".to_owned()
            },
            Lifecycle::Retired { on: String::new() },
        ]
    );
}

#[test]
fn every_row_of_one_draft_gets_a_key_of_its_own() {
    let draft = drafted();
    let mut keys: Vec<Key> = draft.properties.iter().map(|row| row.key).collect();
    for concept in &draft.concepts {
        keys.push(concept.key);
        keys.extend(concept.designations.iter().map(|row| row.key));
        keys.extend(concept.values.iter().map(|row| row.key));
    }
    let mut seen = keys.clone();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), keys.len(), "two rows share a key: {keys:?}");
}

#[test]
fn a_draft_names_what_a_save_needs_before_it_is_savable() {
    let mut draft = Draft::new();
    assert!(!draft.savable(), "a code system is named by its canonical");
    draft.url = "https://terminology.example/colours".to_owned();
    assert!(draft.savable());
    draft.status = String::new();
    assert!(!draft.savable(), "status is mandatory on a CodeSystem");
}
