// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The standalone ICF releases and their qualifier grammar, over a synthetic
//! release shaped as the terminology ecosystem IG describes one
//! (`HL7/fhir-tx-ecosystem-ig` at `9c2bbce`, `input/pagecontent/icf.md`). No
//! WHO content: the categories are invented, and the scale meanings are the
//! IG page's own tables.

use std::sync::Arc;

use concept_graph::subsumption::Outcome;
use fhir_terminology::fhir_codesystem::load::{FhirVersion, load_file};
use fhir_terminology::operations::expand::ExpandInput;
use fhir_terminology::operations::lookup::LookupInput;
use fhir_terminology::operations::sources::Sources;
use fhir_terminology::operations::subsumes::SubsumesInput;
use fhir_terminology::operations::validate_code::{ValidateCodeInput, ValidationOutcome};
use fhir_terminology::operations::{Invocation, expand, lookup, subsumes, validate_code};
use fhir_terminology::provider::PropertyValue;
use fhir_terminology::registry::Registry;
use fhir_terminology::standalone_icf::{self, URL};
use fhir_types::r4b::value_set::{
    ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeFilter,
};
use serde_json::{Value, json};

const VERSION: &str = "0.0.1-synthetic";

fn category(code: &str, display: &str, children: &[Value]) -> Value {
    nested(
        json!({"code": code, "display": display,
               "property": [{"code": "kind", "valueCode": "primary"}]}),
        children,
    )
}

/// `concept` with `children` nested under it; FHIR JSON admits no empty array.
fn nested(mut concept: Value, children: &[Value]) -> Value {
    if !children.is_empty()
        && let Some(object) = concept.as_object_mut()
    {
        object.insert(String::from("concept"), Value::Array(children.to_vec()));
    }
    concept
}

fn heading(code: &str, display: &str, kind: &str, children: &[Value]) -> Value {
    nested(
        json!({"code": code, "display": display,
               "property": [{"code": "kind", "valueCode": kind},
                            {"code": "notSelectable", "valueBoolean": true}]}),
        children,
    )
}

/// A scale heading and its values, `(digit, meaning)`.
fn scale(code: &str, kind: &str, values: &[(char, &str)]) -> Value {
    let children: Vec<Value> = values
        .iter()
        .map(|(digit, meaning)| {
            json!({"code": format!("{code}-{digit}"), "display": meaning,
                   "property": [{"code": "kind", "valueCode": kind}]})
        })
        .collect();
    heading(code, &format!("Scale {code}"), kind, &children)
}

fn release() -> Value {
    let extent = [
        ('0', "NO impairment"),
        ('1', "MILD impairment"),
        ('2', "MODERATE impairment"),
        ('3', "SEVERE impairment"),
        ('4', "COMPLETE impairment"),
        ('8', "Not specified"),
        ('9', "Not applicable"),
    ];
    let digits = |meanings: [&'static str; 10]| -> Vec<(char, &'static str)> {
        "0123456789".chars().zip(meanings).collect()
    };
    let problem = [
        ('0', "NO problem"),
        ('1', "MILD problem"),
        ('2', "MODERATE problem"),
        ('3', "SEVERE problem"),
        ('4', "COMPLETE problem"),
        ('8', "not specified"),
        ('9', "not applicable"),
    ];
    let barrier = [
        ('0', "NO barrier"),
        ('1', "MILD barrier"),
        ('2', "MODERATE barrier"),
        ('3', "SEVERE barrier"),
        ('4', "COMPLETE barrier"),
        ('8', "barrier, not specified"),
        ('9', "not applicable"),
    ];
    let facilitator = [
        ('0', "NO facilitator"),
        ('1', "MILD facilitator"),
        ('2', "MODERATE facilitator"),
        ('3', "SUBSTANTIAL facilitator"),
        ('4', "COMPLETE facilitator"),
        ('8', "facilitator, not specified"),
    ];
    json!({
        "resourceType": "CodeSystem",
        "url": URL,
        "version": VERSION,
        "status": "active",
        "language": "en",
        "content": "complete",
        "versionNeeded": true,
        "hierarchyMeaning": "classified-with",
        "property": [
            {"code": "kind", "type": "code"},
            {"code": "notSelectable", "type": "boolean",
             "uri": "http://hl7.org/fhir/concept-properties#notSelectable"}
        ],
        "concept": [
            heading("b", "Synthetic body functions", "primary", &[heading(
                "b9", "Synthetic chapter", "primary",
                &[category("b990", "Synthetic function", &[category(
                    "b9900", "Synthetic subfunction", &[])])])]),
            heading("s", "Synthetic body structures", "primary",
                &[category("s990", "Synthetic structure", &[])]),
            heading("d", "Synthetic activities", "primary",
                &[category("d990", "Synthetic activity", &[])]),
            heading("e", "Synthetic factors", "primary",
                &[category("e990", "Synthetic factor", &[])]),
            scale("q1", "extent", &extent),
            scale("q2", "nature", &digits(["no change in structure", "total absence",
                "partial absence", "additional part", "aberrant dimensions", "discontinuity",
                "deviating position", "qualitative changes in structure", "not specified",
                "not applicable"])),
            scale("q3", "localization", &digits(["more than one region", "right", "left",
                "both sides", "front", "back", "proximal", "distal", "not specified",
                "not applicable"])),
            scale("q4", "problem", &problem),
            scale("q5", "barrier", &barrier),
            scale("q6", "facilitator", &facilitator),
        ]
    })
}

fn registry() -> Registry {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("CodeSystem-icf.json");
    std::fs::write(&path, release().to_string()).expect("writes");
    let model = load_file(&path, FhirVersion::R5).expect("loads");
    let mut registry = Registry::default();
    registry
        .register(standalone_icf::serve(model).expect("serves"))
        .expect("registers");
    registry
}

fn validated(registry: &Registry, code: &str, display: Option<&str>) -> ValidationOutcome {
    let input = ValidateCodeInput {
        url: Some(URL.to_owned()),
        version: Some(VERSION.to_owned()),
        code: Some(code.to_owned()),
        display: display.map(str::to_owned),
        ..ValidateCodeInput::default()
    };
    validate_code::validate_code(registry, &Invocation::Type, &input).expect("validates")
}

/// The generated displays `icf.md` lists under Display Terms, on synthetic
/// categories.
#[test]
fn a_qualified_code_is_valid_with_its_generated_display() {
    let registry = registry();
    for (code, display) in [
        ("b990", "Synthetic function"),
        ("b990.2", "Synthetic function: MODERATE impairment"),
        ("b990.9", "Synthetic function: Not applicable"),
        ("b9900.1", "Synthetic subfunction: MILD impairment"),
        (
            "s990.183",
            "Synthetic structure: MILD impairment, both sides",
        ),
        ("s990.81", "Synthetic structure: total absence"),
        ("s990.888", "Synthetic structure"),
        (
            "d990.12",
            "Synthetic activity: performance MILD problem, capacity MODERATE problem",
        ),
        (
            "d990.8834",
            "Synthetic activity: capacity with assistance SEVERE problem, performance without assistance COMPLETE problem",
        ),
        ("e990.2", "Synthetic factor: MODERATE barrier"),
        ("e990+3", "Synthetic factor: SUBSTANTIAL facilitator"),
        ("e990+8", "Synthetic factor"),
    ] {
        let outcome = validated(&registry, code, None);
        assert!(outcome.result, "{code}: {outcome:?}");
        assert_eq!(outcome.display.as_deref(), Some(display), "{code}");
    }
}

/// The rules of `icf.md`, Qualifiers: positions per component, scale values,
/// a facilitator only on `e`, no `+9`, and only a selectable category.
#[test]
fn a_code_that_breaks_the_qualifier_rules_is_unknown() {
    let registry = registry();
    for code in [
        "b990.21",
        "d990.12345",
        "e990.23",
        "b990+2",
        "b990.5",
        "d990.15",
        "e990+9",
        "b990.",
        "b990.x",
        "e990-4",
        "b9.2",
        "b.2",
        "b999.2",
        "q1.2",
    ] {
        let outcome = validated(&registry, code, None);
        assert!(!outcome.result, "{code} is no code: {outcome:?}");
    }
}

/// "The category's own title is not a valid display for a qualified code
/// (unless every qualifier is `8`)" (`icf.md`, Display Terms).
#[test]
fn the_category_title_is_no_display_of_a_qualified_code() {
    let registry = registry();
    let wrong = validated(&registry, "s990.183", Some("Synthetic structure"));
    assert!(!wrong.result, "{wrong:?}");
    assert_eq!(
        wrong.issues.first().map(|i| i.kind),
        Some("invalid-display")
    );
    let eights = validated(&registry, "s990.888", Some("Synthetic structure"));
    assert!(eights.result, "{eights:?}");
}

/// "A qualified code has its category's properties as well, other than
/// `parent`", and "A qualifier that is `8` is still reported" (`icf.md`,
/// Properties); "a qualified code is subsumed by its category" (Summary).
#[test]
fn a_qualified_code_reports_its_qualifiers_and_sits_under_its_category() {
    let registry = registry();
    let outcome = lookup::lookup(
        &registry,
        &Invocation::Type,
        &LookupInput {
            system: Some(URL.to_owned()),
            version: Some(VERSION.to_owned()),
            code: Some(String::from("s990.183")),
            properties: vec![String::from("*")],
            ..LookupInput::default()
        },
    )
    .expect("looks up");
    let value = |name: &str| {
        outcome
            .properties
            .iter()
            .find(|p| p.code == name)
            .map(|p| p.value.clone())
    };
    assert_eq!(
        value("parent"),
        Some(PropertyValue::Code(String::from("s990")))
    );
    assert_eq!(
        value("extent"),
        Some(PropertyValue::Code(String::from("q1-1")))
    );
    assert_eq!(
        value("nature"),
        Some(PropertyValue::Code(String::from("q2-8")))
    );
    assert_eq!(
        value("localization"),
        Some(PropertyValue::Code(String::from("q3-3")))
    );
    assert_eq!(
        value("kind"),
        Some(PropertyValue::Code(String::from("primary")))
    );
    let subsumed = |a: &str, b: &str| {
        subsumes::subsumes(
            &registry,
            &Invocation::Type,
            &SubsumesInput {
                system: Some(URL.to_owned()),
                version: Some(VERSION.to_owned()),
                code_a: Some(a.to_owned()),
                code_b: Some(b.to_owned()),
                ..SubsumesInput::default()
            },
        )
        .expect("decides")
    };
    assert_eq!(subsumed("b990", "b990.2"), Outcome::Subsumes);
    assert_eq!(subsumed("b9", "b9900.1"), Outcome::Subsumes);
    assert_eq!(subsumed("b990.2", "b990"), Outcome::SubsumedBy);
    assert_eq!(subsumed("s990", "b990.2"), Outcome::NotSubsumed);
}

fn include(filters: &[(&str, &str, &str)]) -> ValueSetComposeInclude {
    ValueSetComposeInclude {
        system: Some(URL.into()),
        version: Some(VERSION.into()),
        filter: filters
            .iter()
            .map(|(property, op, value)| ValueSetComposeIncludeFilter {
                property: (*property).into(),
                op: (*op).into(),
                value: (*value).into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn member(registry: &Registry, include: &ValueSetComposeInclude, code: &str) -> bool {
    let value_set = ValueSet {
        url: Some("http://example.org/fhir/ValueSet/icf-test".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![include.clone()],
            ..Default::default()
        }),
        ..Default::default()
    };
    let input = fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(
            &value_set,
        )),
        code: Some(code.to_owned()),
        system: Some(URL.to_owned()),
        system_version: Some(VERSION.to_owned()),
        ..Default::default()
    };
    let value_sets = fhir_terminology::valueset::store::ValueSetStore::default();
    let concept_maps = fhir_terminology::conceptmap::store::ConceptMapStore::default();
    let sources = Sources {
        registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    fhir_terminology::operations::value_set_validate_code::validate_code(&sources, &input)
        .expect("validates")
        .result
}

/// The qualifier filters of `icf.md`, Filter Properties, with its examples on
/// synthetic categories.
#[test]
fn the_qualifier_filters_select_by_what_a_code_states() {
    let registry = registry();
    let full = include(&[
        ("concept", "is-a", "s"),
        ("extent", "exists", "true"),
        ("nature", "exists", "true"),
        ("localization", "exists", "true"),
    ]);
    assert!(member(&registry, &full, "s990.123"));
    for code in ["s990.12", "s990.128", "s990.183", "s990"] {
        assert!(!member(&registry, &full, code), "{code}");
    }
    let capacity = include(&[("concept", "is-a", "d"), ("capacity", "exists", "true")]);
    assert!(member(&registry, &capacity, "d990.12"));
    assert!(!member(&registry, &capacity, "d990.1"));
    assert!(!member(&registry, &capacity, "d990.18"));
    let isa = include(&[("concept", "is-a", "b9")]);
    assert!(member(&registry, &isa, "b9900.2"));
    assert!(!member(&registry, &isa, "s990.1"));
    let plain = include(&[("qualified", "=", "false")]);
    assert!(member(&registry, &plain, "b990"));
    assert!(!member(&registry, &plain, "s990.888"));
}

/// "an include that admits qualified codes - any include without
/// `qualified = false` - expands to the plain codes it contains, and the
/// expansion is marked as incomplete" (`icf.md`, Filter Properties).
#[test]
fn an_expansion_that_admits_qualified_codes_is_unclosed() {
    let registry = registry();
    let value_sets = fhir_terminology::valueset::store::ValueSetStore::default();
    let concept_maps = fhir_terminology::conceptmap::store::ConceptMapStore::default();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let expanded = |include: ValueSetComposeInclude| {
        let value_set = ValueSet {
            url: Some("http://example.org/fhir/ValueSet/icf-expand".into()),
            status: "active".into(),
            compose: Some(ValueSetCompose {
                include: vec![include],
                ..Default::default()
            }),
            ..Default::default()
        };
        expand::expand(
            &sources,
            &ExpandInput {
                inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(
                    &value_set,
                )),
                ..ExpandInput::default()
            },
        )
        .expect("expands")
    };
    let admitting = expanded(include(&[("concept", "is-a", "b990")]));
    assert!(admitting.unclosed);
    assert_eq!(admitting.total, 2, "the plain codes b990 and b9900");
    let plain = expanded(include(&[
        ("concept", "is-a", "b990"),
        ("qualified", "=", "false"),
    ]));
    assert!(!plain.unclosed);
}

#[test]
fn every_other_code_system_is_served_as_a_fhir_code_system() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("CodeSystem-other.json");
    std::fs::write(
        &path,
        json!({"resourceType": "CodeSystem", "url": "http://example.org/fhir/CodeSystem/other",
               "status": "active", "content": "complete",
               "concept": [{"code": "a.1", "display": "A one"}]})
        .to_string(),
    )
    .expect("writes");
    let provider: Arc<dyn fhir_terminology::provider::CodeSystemProvider> =
        standalone_icf::serve(load_file(&path, FhirVersion::R5).expect("loads")).expect("serves");
    assert!(provider.locate("a.1").expect("reads").is_some());
    assert!(provider.locate("a.2").expect("reads").is_none());
}
