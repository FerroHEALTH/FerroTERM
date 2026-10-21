// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! UCUM (<https://ucum.org/ucum>, <https://terminology.hl7.org/UCUM.html>):
//! the grammar, canonical forms, and the provider over the seam.

use std::sync::Arc;

use concept_graph::subsumption::Outcome;
use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::filter::{Filter, FilterOperator};
use fhir_terminology::operations::expand::ExpandInput;
use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;
use fhir_terminology::operations::{
    Invocation, OperationError, Sources, expand, subsumes, value_set_validate_code,
};
use fhir_terminology::provider::{CodeSystemProvider, ProviderError};
use fhir_terminology::registries::ucum::essence::ESSENCE_DATA;
use fhir_terminology::registries::ucum::grammar::{GrammarError, parse};
use fhir_terminology::registries::ucum::provider::{URL, UcumProvider};
use fhir_terminology::registry::Registry;
use fhir_terminology::valueset::store::ValueSetStore;
use fhir_types::r4b::value_set::{
    ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeFilter,
};

fn filter(property: &str, op: FilterOperator, value: &str) -> Filter {
    Filter {
        property: property.to_owned(),
        op,
        value: value.to_owned(),
    }
}

#[test]
fn the_grammar_accepts_the_specification_examples_and_refuses_the_rest() {
    let essence = &*ESSENCE_DATA;
    for ok in [
        "m",
        "mg",
        "mg/dL",
        "kg.m/s2",
        "1/min",
        "min-1",
        "10*3/uL",
        "mm[Hg]",
        "[in_i]",
        "%",
        "mg{total}",
        "{cells}",
        "/min",
        "(mg/dL)",
        "m2",
        "m-2",
        "cm3",
        "uL",
        "deg",
        "[degF]",
        "Cel",
        "mol/L",
        "10^3",
        "4.[pi].10*-7.N/A2",
    ] {
        assert!(parse(ok, essence).is_ok(), "{ok} should parse");
    }
    assert!(matches!(parse("", essence), Err(GrammarError::Empty)));
    assert!(matches!(
        parse("kgXX", essence),
        Err(GrammarError::UnknownAtom { .. })
    ));
    assert!(matches!(
        parse("not-a-unit", essence),
        Err(GrammarError::UnknownAtom { .. } | GrammarError::Unexpected { .. })
    ));
    assert!(
        matches!(
            parse("k[in_i]", essence),
            Err(GrammarError::PrefixOnNonMetric { .. })
        ),
        "inch is not metric"
    );
    assert!(matches!(
        parse("mg{open", essence),
        Err(GrammarError::UnclosedAnnotation)
    ));
    assert!(matches!(
        parse("(mg", essence),
        Err(GrammarError::UnclosedGroup)
    ));
    assert!(parse("Mg", essence).is_ok(), "megagram: case sensitive");
    assert!(
        parse("KG", essence).is_err(),
        "the case-insensitive spelling is not a code"
    );
}

#[test]
fn canonical_forms_reduce_to_the_base_units() {
    let provider = UcumProvider::new();
    let essence = &*ESSENCE_DATA;
    let ml = provider.canonical_of("mL").expect("mL");
    assert_eq!(ml.text(essence), "m3");
    assert!((ml.magnitude - 1e-6).abs() < 1e-15);
    let kg = provider.canonical_of("kg").expect("kg");
    assert_eq!(kg.text(essence), "g");
    assert!((kg.magnitude - 1000.0).abs() < 1e-9);
    let newton = provider.canonical_of("N").expect("N");
    let derived = provider.canonical_of("kg.m/s2").expect("kg.m/s2");
    assert!(newton.same_unit(&derived));
    assert_eq!(
        newton.text(essence),
        "m.s-2.g",
        "base units in essence order"
    );
    let per_min = provider.canonical_of("1/min").expect("1/min");
    let min_inv = provider.canonical_of("min-1").expect("min-1");
    assert!(per_min.same_unit(&min_inv));
    assert!(
        provider
            .canonical_of("m")
            .expect("m")
            .commensurable(&provider.canonical_of("cm").expect("cm"))
    );
    assert!(
        !provider
            .canonical_of("m")
            .expect("m")
            .same_unit(&provider.canonical_of("cm").expect("cm"))
    );
    let celsius = provider.canonical_of("Cel").expect("Cel");
    assert_eq!(celsius.special.as_deref(), Some("Cel"));
    assert!(
        !celsius.same_unit(&provider.canonical_of("K").expect("K")),
        "an offset unit is not its base"
    );
    assert!(provider.canonical_of("[iU]").expect("[iU]").arbitrary);
    assert_eq!(provider.canonical_of("%").expect("%").text(essence), "1");
}

#[test]
fn the_provider_locates_describes_and_filters_expressions() {
    let provider = UcumProvider::new();
    assert_eq!(provider.identity().url, URL);
    assert_eq!(provider.identity().version, "2.2");
    let located = provider.locate("mg/dL").expect("reads").expect("valid");
    assert_eq!(located.code, "mg/dL");
    assert_eq!(
        provider
            .display(located.concept, None)
            .expect("reads")
            .as_deref(),
        Some("mg/dL")
    );
    let designations = provider.designations(located.concept, None).expect("reads");
    assert_eq!(designations[0].value, "milligram per deciliter");
    let props: Vec<String> = provider
        .properties(located.concept)
        .expect("reads")
        .into_iter()
        .map(|p| format!("{}={}", p.code, p.value.as_text()))
        .collect();
    assert_eq!(props, ["canonical=m-3.g"]);
    let kg = provider.locate("kg").expect("reads").expect("valid");
    let props: Vec<String> = provider
        .properties(kg.concept)
        .expect("reads")
        .into_iter()
        .map(|p| format!("{}={}", p.code, p.value.as_text()))
        .collect();
    assert_eq!(props, ["canonical=g", "property=mass"]);
    assert!(provider.locate("kgXX").expect("reads").is_none());
    assert!(
        provider
            .filter_matches(kg.concept, &filter("canonical", FilterOperator::Equal, "g"))
            .expect("answers")
    );
    assert!(
        !provider
            .filter_matches(
                located.concept,
                &filter("canonical", FilterOperator::Equal, "g")
            )
            .expect("answers")
    );
    assert!(
        provider
            .filter_matches(
                kg.concept,
                &filter("property", FilterOperator::Equal, "mass")
            )
            .expect("answers")
    );
    assert!(matches!(
        provider.filter_matches(
            kg.concept,
            &filter("canonical", FilterOperator::Equal, "not a unit")
        ),
        Err(ProviderError::InvalidFilterValue(_))
    ));
    assert!(matches!(provider.all(), Err(ProviderError::NotEnumerable)));
    assert!(matches!(
        provider.filter(&filter("canonical", FilterOperator::Equal, "g")),
        Err(ProviderError::NotEnumerable)
    ));
    let annotated = provider
        .locate("mg/dL{milligram per deciliter}")
        .expect("reads")
        .expect("valid");
    assert_eq!(
        annotated.code, "mg/dL{milligram per deciliter}",
        "an annotated expression is its own code"
    );
    let all = provider
        .implicit_value_set("http://unitsofmeasure.org/vs")
        .expect("implicit")
        .expect("compose");
    assert!(all.include[0].filters.is_empty());
    let mass = provider
        .implicit_value_set("http://unitsofmeasure.org/vs/g")
        .expect("implicit")
        .expect("compose");
    assert_eq!(mass.include[0].filters[0].value, "g");
    assert!(
        provider
            .implicit_value_set("http://unitsofmeasure.org/vs/kgXX")
            .expect("implicit")
            .is_err()
    );
    assert!(
        provider
            .implicit_value_set("http://unitsofmeasure.org/other")
            .is_none()
    );
}

fn canonical_value_set(value: &str) -> ValueSet {
    ValueSet {
        url: Some("http://example.org/ucum-canonical".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(URL.into()),
                filter: vec![ValueSetComposeIncludeFilter {
                    property: "canonical".into(),
                    op: "=".into(),
                    value: value.into(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn ucum_answers_the_operations_the_ecosystem_suite_asks() {
    let mut registry = Registry::new();
    registry
        .register(Arc::new(UcumProvider::new()))
        .expect("registers");
    let sub = |a: &str, b: &str| {
        subsumes::subsumes(
            &registry,
            &Invocation::Type,
            &subsumes::SubsumesInput {
                system: Some(URL.to_owned()),
                code_a: Some(a.to_owned()),
                code_b: Some(b.to_owned()),
                ..subsumes::SubsumesInput::default()
            },
        )
        .map(|outcome| outcome.code().to_owned())
    };
    assert_eq!(
        sub("mg/dL", "mg/dL").expect("answers"),
        Outcome::Equivalent.code()
    );
    assert_eq!(
        sub("1/min", "min-1").expect("answers"),
        Outcome::Equivalent.code()
    );
    assert_eq!(
        sub("N", "kg.m/s2").expect("answers"),
        Outcome::Equivalent.code()
    );
    assert_eq!(
        sub("mg{total}", "mg").expect("answers"),
        Outcome::Equivalent.code()
    );
    assert_eq!(
        sub("mg/dL", "mm[Hg]").expect("answers"),
        Outcome::NotSubsumed.code()
    );
    assert_eq!(
        sub("m", "cm").expect("answers"),
        Outcome::NotSubsumed.code()
    );
    assert_eq!(
        sub("Cel", "K").expect("answers"),
        Outcome::NotSubsumed.code()
    );
    assert_eq!(
        sub("mg/dL", "g/L").expect("answers"),
        Outcome::NotSubsumed.code()
    );
    assert_eq!(
        sub("[in_i]", "cm").expect("answers"),
        Outcome::NotSubsumed.code()
    );
    assert_eq!(sub("%", "1").expect("answers"), Outcome::NotSubsumed.code());
    assert!(matches!(
        sub("not-a-unit", "mg/dL"),
        Err(OperationError::UnknownCode { .. })
    ));
}

#[test]
fn ucum_validates_by_canonical_membership_and_refuses_expansion() {
    let mut registry = Registry::new();
    registry
        .register(Arc::new(UcumProvider::new()))
        .expect("registers");
    let value_sets = ValueSetStore::new();
    let concept_maps = ConceptMapStore::new();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let good = ValueSetValidateInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(
            &canonical_value_set("g"),
        )),
        code: Some(String::from("kg")),
        system: Some(URL.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&sources, &good).expect("validates");
    assert!(validation.result);
    assert_eq!(validation.version.as_deref(), Some("2.2"));
    let bad = ValueSetValidateInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(
            &canonical_value_set("g"),
        )),
        code: Some(String::from("mL")),
        system: Some(URL.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&sources, &bad).expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.issues[0].kind, "not-in-vs");
    let unknown = ValueSetValidateInput {
        url: Some(String::from("http://unitsofmeasure.org/vs")),
        code: Some(String::from("kgXX")),
        system: Some(URL.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&sources, &unknown).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "invalid-code"]);
    let all = ExpandInput {
        url: Some(String::from("http://unitsofmeasure.org/vs")),
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&sources, &all),
        Err(OperationError::NotSupported(_))
    ));
    let mass = ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(
            &canonical_value_set("g"),
        )),
        ..ExpandInput::default()
    };
    assert!(
        matches!(
            expand::expand(&sources, &mass),
            Err(OperationError::NotSupported(_))
        ),
        "commensurable units are unbounded too"
    );
}

/// `$expand` names UCUM as a system whose value sets are "unbounded due to the
/// inclusion of post-coordinated value sets", the case the `valueset-unclosed`
/// extension marks
/// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>, Notes).
#[test]
fn a_unit_selection_is_unclosed() {
    let provider = UcumProvider::new();
    assert!(provider.unclosed(&[filter("canonical", FilterOperator::Equal, "g")]));
}
// NOTE: `displayLanguage` "Specifies the language to be used for description
// when validating the display property"
// (<https://hl7.org/fhir/R4B/codesystem-operation-validate-code.html>).
#[test]
fn a_unit_name_in_english_does_not_satisfy_a_request_for_another_language() {
    use fhir_terminology::operations::lookup::{LookupInput, lookup};
    use fhir_terminology::operations::validate_code::{ValidateCodeInput, validate_code};

    let provider = UcumProvider::new();
    assert_eq!(provider.language(), Some("en"));
    let mut registry = Registry::new();
    registry.register(Arc::new(provider)).expect("registers");
    let ask = |display: &str| {
        validate_code(
            &registry,
            &Invocation::Type,
            &ValidateCodeInput {
                url: Some(URL.to_owned()),
                code: Some(String::from("mg")),
                display: Some(display.to_owned()),
                display_language: Some(String::from("fr")),
                ..ValidateCodeInput::default()
            },
        )
        .expect("validates")
    };

    // The English unit name stands and the answer says French has none; it is
    // no longer refused as the wrong display for a French request.
    let name = ask("milligram");
    assert!(name.result, "{name:?}");
    let issue = name.issues.first().expect("an issue");
    assert_eq!(issue.severity, "information");
    assert_eq!(issue.kind, "invalid-display");
    assert_eq!(
        issue.message_id(),
        "NO_VALID_DISPLAY_FOUND_NONE_FOR_LANG_OK"
    );

    // The lookup states the language it answered in, not the one it was asked
    // for, so the English name is never labelled French.
    let outcome = lookup(
        &registry,
        &Invocation::Type,
        &LookupInput {
            system: Some(URL.to_owned()),
            code: Some(String::from("mg")),
            display_language: Some(String::from("fr")),
            ..LookupInput::default()
        },
    )
    .expect("looks up");
    assert_eq!(outcome.display, "mg");
    let designation = outcome
        .designations
        .iter()
        .find(|d| d.value == "mg")
        .expect("the display as a designation");
    assert_eq!(designation.language.as_deref(), Some("en"));
}

#[test]
fn every_declared_implicit_value_set_form_resolves() {
    // The `/vs` and `/vs/[expression]` forms of <https://terminology.hl7.org/UCUM.html>.
    let provider = UcumProvider::new();
    crate::implicit_forms::every_form_resolves(
        &provider,
        &[
            ("http://unitsofmeasure.org/vs", &[]),
            ("http://unitsofmeasure.org/vs/[expression]", &["mg/dL"]),
        ],
    );
}
