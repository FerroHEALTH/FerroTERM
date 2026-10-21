// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ConceptMap/$translate` over the stored and inline maps
//! (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>).

use ferroterm_testkit::fhir::{
    ANIMALS, CM_ANIMALS_COLOURS, CM_FALLBACK, COLOURS, VS_ALL, VS_COLOURS,
};
use fhir_terminology::conceptmap::convert;
use fhir_terminology::conceptmap::model::Relationship;
use fhir_terminology::operations::translate::{Match, TranslateInput, Translation, translate};
use fhir_terminology::operations::{CodeableConceptRef, CodingRef, OperationError};
use fhir_types::codec::Json;
use fhir_types::r4b::concept_map::{
    ConceptMap, ConceptMapGroup, ConceptMapGroupElement, ConceptMapGroupElementTarget,
};

use crate::value_set::World;

fn equivalence(m: &Match) -> &'static str {
    m.relationship.equivalence()
}

fn target_code(m: &Match) -> Option<&str> {
    m.concept.as_ref().and_then(|c| c.code.as_deref())
}

/// A codeable concept of `coding`, with no text.
fn codeable(coding: Vec<CodingRef>) -> CodeableConceptRef {
    CodeableConceptRef { coding, text: None }
}

fn coding_ref(system: &str, code: &str) -> CodingRef {
    CodingRef {
        system: Some(system.to_owned()),
        code: Some(code.to_owned()),
        ..CodingRef::default()
    }
}

fn run(world: &World, input: &TranslateInput) -> Translation {
    translate(&world.sources(), input).expect("translates")
}

#[test]
fn relationship_vocabularies_meet_in_one_enum() {
    assert_eq!(
        Relationship::from_relationship("source-is-broader-than-target"),
        Some(Relationship::Narrower)
    );
    assert_eq!(
        Relationship::from_equivalence("wider").map(Relationship::relationship),
        Some("source-is-narrower-than-target")
    );
    assert_eq!(Relationship::Narrower.inverse(), Relationship::Wider);
    assert!(!Relationship::Unmatched.translates());
    assert!(Relationship::RelatedTo.translates());
    assert_eq!(Relationship::from_equivalence("kinda"), None);
}

#[test]
fn the_store_loads_both_maps_with_the_r5_shapes_reduced() {
    let world = World::load();
    assert_eq!(world.concept_maps().len(), 2);
    let map = world
        .concept_maps()
        .resolve(CM_ANIMALS_COLOURS, None)
        .expect("map");
    assert_eq!(map.source_scope.as_deref(), Some(VS_ALL));
    assert_eq!(map.target_scope.as_deref(), Some(VS_COLOURS));
    let group = &map.groups[0];
    assert_eq!(group.source.as_deref(), Some(ANIMALS));
    let dog = group
        .elements
        .iter()
        .find(|e| e.code.as_deref() == Some("dog"))
        .expect("dog");
    assert_eq!(dog.targets[0].relationship, Relationship::Narrower);
    assert_eq!(dog.targets[0].comment.as_deref(), Some("roughly"));
    let fish = group
        .elements
        .iter()
        .find(|e| e.code.as_deref() == Some("fish"))
        .expect("fish");
    assert!(fish.no_map);
    assert_eq!(fish.comment.as_deref(), Some("fish have no colour"));
    assert_eq!(
        group.unmapped.as_ref().and_then(|u| u.relationship),
        Some(Relationship::RelatedTo)
    );
}

#[test]
fn translate_by_url_code_coding_and_codeable_concept() {
    let world = World::load();
    let by_code = TranslateInput {
        url: Some(CM_ANIMALS_COLOURS.to_owned()),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        ..TranslateInput::default()
    };
    let translation = run(&world, &by_code);
    assert!(translation.result);
    let m = &translation.matches[0];
    assert_eq!(equivalence(m), "equivalent");
    assert_eq!(target_code(m), Some("RED"));
    assert_eq!(
        m.concept.as_ref().and_then(|c| c.system.as_deref()),
        Some(COLOURS)
    );
    assert_eq!(
        m.source.as_deref(),
        Some(format!("{CM_ANIMALS_COLOURS}|1.0").as_str())
    );
    assert_eq!(
        translation.matches[0].origin.origin_map,
        format!("{CM_ANIMALS_COLOURS}|1.0")
    );
    assert_eq!(
        translation.matches[0]
            .origin
            .source_concept
            .as_ref()
            .and_then(|c| c.code.as_deref()),
        Some("cat")
    );
    let by_coding = TranslateInput {
        url: Some(CM_ANIMALS_COLOURS.to_owned()),
        coding: Some(coding_ref(ANIMALS, "dog")),
        ..TranslateInput::default()
    };
    let translation = run(&world, &by_coding);
    assert_eq!(equivalence(&translation.matches[0]), "narrower");
    assert_eq!(
        translation.matches[0].origin.source_comment, None,
        "the comment on the target is not the element's"
    );
    let by_concept = TranslateInput {
        url: Some(CM_ANIMALS_COLOURS.to_owned()),
        codeable_concept: Some(codeable(vec![
            coding_ref(ANIMALS, "cat"),
            coding_ref(ANIMALS, "dog"),
        ])),
        ..TranslateInput::default()
    };
    let translation = run(&world, &by_concept);
    assert_eq!(translation.matches.len(), 2);
}

#[test]
fn no_map_and_unmapped_rules_still_answer() {
    let world = World::load();
    let fish = TranslateInput {
        url: Some(CM_ANIMALS_COLOURS.to_owned()),
        code: Some(String::from("fish")),
        system: Some(ANIMALS.to_owned()),
        ..TranslateInput::default()
    };
    let translation = run(&world, &fish);
    // NOTE: an explicit `noMap` is an answer, so `result` is true and there is no
    // message (the ecosystem's `translate-4`, <https://hl7.org/fhir/uv/tx-ecosystem/>).
    assert!(translation.result);
    assert_eq!(equivalence(&translation.matches[0]), "unmatched");
    assert!(translation.matches[0].origin.no_map);
    assert_eq!(
        translation.matches[0].origin.source_comment.as_deref(),
        Some("fish have no colour")
    );
    assert!(translation.message.is_none());
    let kitten = TranslateInput {
        url: Some(CM_ANIMALS_COLOURS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        ..TranslateInput::default()
    };
    let translation = run(&world, &kitten);
    assert!(translation.result, "fixed unmapped");
    assert_eq!(equivalence(&translation.matches[0]), "relatedto");
    assert_eq!(target_code(&translation.matches[0]), Some("blue"));
    let via_other = TranslateInput {
        url: Some(CM_FALLBACK.to_owned()),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        ..TranslateInput::default()
    };
    let translation = run(&world, &via_other);
    assert_eq!(
        target_code(&translation.matches[0]),
        Some("RED"),
        "other-map defers"
    );
    assert_eq!(
        translation.matches[0].origin.origin_map,
        format!("{CM_FALLBACK}|1.0"),
        "a chained match is the referring map's"
    );
    assert_eq!(
        translation.used_concept_maps,
        [format!("{CM_ANIMALS_COLOURS}|1.0")],
        "the chained map is reported as used"
    );
}

#[test]
fn maps_are_chosen_by_scope_and_target_system_and_read_in_reverse() {
    let world = World::load();
    let chosen = TranslateInput {
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        source: Some(VS_ALL.to_owned()),
        target_system: Some(COLOURS.to_owned()),
        ..TranslateInput::default()
    };
    let translation = run(&world, &chosen);
    assert!(translation.result);
    assert_eq!(
        translation.matches.len(),
        1,
        "the fallback map has no matching scope"
    );
    let wrong_target = TranslateInput {
        url: Some(CM_ANIMALS_COLOURS.to_owned()),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        target_system: Some(String::from("http://example.org/fhir/CodeSystem/nowhere")),
        ..TranslateInput::default()
    };
    let translation = run(&world, &wrong_target);
    assert!(!translation.result);
    assert_eq!(translation.matches, Vec::<Match>::new());
    let reverse = TranslateInput {
        url: Some(CM_ANIMALS_COLOURS.to_owned()),
        code: Some(String::from("Green")),
        system: Some(COLOURS.to_owned()),
        reverse: Some(true),
        ..TranslateInput::default()
    };
    let translation = run(&world, &reverse);
    assert_eq!(target_code(&translation.matches[0]), Some("dog"));
    assert_eq!(
        equivalence(&translation.matches[0]),
        "wider",
        "the relationship is inverted"
    );
}

#[test]
fn an_inline_map_translates_and_malformed_requests_are_refused() {
    let world = World::load();
    let inline = ConceptMap {
        url: Some("http://example.org/inline-map".into()),
        status: "draft".into(),
        group: vec![ConceptMapGroup {
            source: Some(ANIMALS.into()),
            target: Some(COLOURS.into()),
            element: vec![ConceptMapGroupElement {
                code: Some("plant".into()),
                target: vec![ConceptMapGroupElementTarget {
                    code: Some("Green".into()),
                    equivalence: "wider".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let request = TranslateInput {
        inline_concept_map: Some(convert::r4b::convert(&inline)),
        code: Some(String::from("plant")),
        system: Some(ANIMALS.to_owned()),
        ..TranslateInput::default()
    };
    let translation = run(&world, &request);
    assert_eq!(target_code(&translation.matches[0]), Some("Green"));
    let both = TranslateInput {
        inline_concept_map: Some(convert::r4b::convert(&inline)),
        url: Some(CM_ANIMALS_COLOURS.to_owned()),
        code: Some(String::from("plant")),
        system: Some(ANIMALS.to_owned()),
        ..TranslateInput::default()
    };
    assert!(matches!(
        translate(&world.sources(), &both),
        Err(OperationError::Invalid(_))
    ));
    let no_system = TranslateInput {
        url: Some(CM_ANIMALS_COLOURS.to_owned()),
        code: Some(String::from("cat")),
        ..TranslateInput::default()
    };
    assert!(matches!(
        translate(&world.sources(), &no_system),
        Err(OperationError::Required(_))
    ));
    let two = TranslateInput {
        url: Some(CM_ANIMALS_COLOURS.to_owned()),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        coding: Some(CodingRef::default()),
        ..TranslateInput::default()
    };
    assert!(matches!(
        translate(&world.sources(), &two),
        Err(OperationError::Invalid(_))
    ));
    let unknown = TranslateInput {
        url: Some(String::from("http://example.org/fhir/ConceptMap/nowhere")),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        ..TranslateInput::default()
    };
    let error = translate(&world.sources(), &unknown).expect_err("unknown");
    assert!(matches!(error, OperationError::UnknownConceptMap(_)));
    assert_eq!(error.status(), http::StatusCode::NOT_FOUND);
    let other_system = TranslateInput {
        url: Some(CM_ANIMALS_COLOURS.to_owned()),
        code: Some(String::from("x")),
        system: Some(String::from("http://example.org/fhir/CodeSystem/nowhere")),
        ..TranslateInput::default()
    };
    let translation = run(&world, &other_system);
    assert!(!translation.result);
    assert!(translation.matches.is_empty(), "no group covers the system");
}

#[test]
fn an_r6_element_comment_reads_from_the_field_and_from_the_extension() {
    // R6 declares `ConceptMap.group.element.comment`; R5 and earlier carry the
    // same fact in an extension, so a document written either way has to read
    // the same (<https://hl7.org/fhir/6.0.0-ballot5/conceptmap-definitions.html>).
    let native = serde_json::json!({
        "resourceType": "ConceptMap",
        "url": "http://example.org/fhir/ConceptMap/native-comment",
        "status": "active",
        "group": [{
            "source": "http://example.org/fhir/CodeSystem/animals",
            "element": [{
                "code": "dog",
                "comment": "stated the R6 way",
                "target": [{"code": "brown", "relationship": "equivalent"}]
            }]
        }]
    });
    let extended = serde_json::json!({
        "resourceType": "ConceptMap",
        "url": "http://example.org/fhir/ConceptMap/extended-comment",
        "status": "active",
        "group": [{
            "source": "http://example.org/fhir/CodeSystem/animals",
            "element": [{
                "code": "dog",
                "extension": [{
                    "url": "http://hl7.org/fhir/6.0/StructureDefinition/extension-ConceptMap.group.element.comment",
                    "valueString": "stated the R5 way"
                }],
                "target": [{"code": "brown", "relationship": "equivalent"}]
            }]
        }]
    });
    for (document, expected) in [
        (native, "stated the R6 way"),
        (extended, "stated the R5 way"),
    ] {
        let parsed: fhir_types::codec::Value =
            serde_json::from_str(&document.to_string()).expect("the document parses");
        let resource = fhir_types::r6::concept_map::ConceptMap::from_json(
            parsed.as_object().expect("object"),
            &mut fhir_types::codec::Path::root("ConceptMap"),
        )
        .expect("an R6 ConceptMap");
        let model = convert::r6::convert(&resource).expect("converts");
        assert_eq!(
            model.groups[0].elements[0].comment.as_deref(),
            Some(expected),
            "the comment reads whichever way the document stated it"
        );
    }
}
