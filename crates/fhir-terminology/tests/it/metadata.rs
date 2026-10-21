// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The descriptive elements a loaded `CodeSystem` and `ConceptMap` carry back
//! out on a read: `experimental`, `date`, `publisher` and `description`, which
//! every served version defines on both resources
//! (<https://hl7.org/fhir/R4/codesystem.html>, <https://hl7.org/fhir/R5/conceptmap.html>).

use fhir_types::codec::{Json, Path, Value};

/// The descriptive values every case states, beside `experimental = true`.
const DATE: &str = "2026-10-01";
const PUBLISHER: &str = "A synthetic publisher";
const DESCRIPTION: &str = "Synthetic content with **Markdown** in it.";

fn document(resource_type: &str) -> Value {
    let mut object = serde_json::Map::new();
    for (key, value) in [
        ("resourceType", serde_json::json!(resource_type)),
        (
            "url",
            serde_json::json!(format!("http://example.org/fhir/{resource_type}/described")),
        ),
        ("version", serde_json::json!("1")),
        ("status", serde_json::json!("active")),
        ("experimental", serde_json::json!(true)),
        ("date", serde_json::json!(DATE)),
        ("publisher", serde_json::json!(PUBLISHER)),
        ("description", serde_json::json!(DESCRIPTION)),
    ] {
        object.insert(key.to_owned(), value);
    }
    if resource_type == "CodeSystem" {
        object.insert("content".to_owned(), serde_json::json!("complete"));
        object.insert(
            "concept".to_owned(),
            serde_json::json!([{"code": "a", "display": "A"}]),
        );
    }
    let object = serde_json::Value::Object(object);
    serde_json::from_str(&object.to_string()).expect("the document parses")
}

macro_rules! round_trip {
    ($name:ident, $module:ident) => {
        #[test]
        fn $name() {
            let source = document("CodeSystem");
            let resource = fhir_types::$module::code_system::CodeSystem::from_json(
                source.as_object().expect("object"),
                &mut Path::root("CodeSystem"),
            )
            .expect("decodes");
            let model = fhir_terminology::fhir_codesystem::convert::$module::convert(&resource)
                .expect("converts");
            let served = fhir_terminology::fhir_codesystem::render::$module::code_system(&model);
            assert_eq!(served.experimental.and_then(|b| b.value), Some(true));
            assert_eq!(served.date.and_then(|d| d.value).as_deref(), Some(DATE));
            assert_eq!(
                served.publisher.and_then(|p| p.value).as_deref(),
                Some(PUBLISHER)
            );
            assert_eq!(
                served.description.and_then(|d| d.value).as_deref(),
                Some(DESCRIPTION)
            );

            let source = document("ConceptMap");
            let resource = fhir_types::$module::concept_map::ConceptMap::from_json(
                source.as_object().expect("object"),
                &mut Path::root("ConceptMap"),
            )
            .expect("decodes");
            let model = fhir_terminology::conceptmap::convert::$module::convert(&resource)
                .expect("converts");
            let served = fhir_terminology::conceptmap::render::$module::concept_map(&model);
            assert_eq!(served.experimental.and_then(|b| b.value), Some(true));
            assert_eq!(served.date.and_then(|d| d.value).as_deref(), Some(DATE));
            assert_eq!(
                served.publisher.and_then(|p| p.value).as_deref(),
                Some(PUBLISHER)
            );
            assert_eq!(
                served.description.and_then(|d| d.value).as_deref(),
                Some(DESCRIPTION)
            );
        }
    };
}

round_trip!(r4_keeps_the_descriptive_elements, r4);
round_trip!(r4b_keeps_the_descriptive_elements, r4b);
round_trip!(r5_keeps_the_descriptive_elements, r5);
round_trip!(r6_keeps_the_descriptive_elements, r6);
