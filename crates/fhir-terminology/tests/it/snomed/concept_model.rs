// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `$validate-code` of an expression against the concept model the edition
//! carries in its MRCM reference sets (SNOMED CT Machine Readable Concept
//! Model, §6 Considerations, "Expressions, Constraints and Queries").

use std::sync::Arc;

use fhir_terminology::operations::{Invocation, validate_code};
use fhir_terminology::provider::{CodeSystemProvider, Concept, PropertyValue};
use fhir_terminology::registry::Registry;
use fhir_terminology::snomed::{SYSTEM, SnomedProvider};

use ferroterm_testkit::snomed;
use ferroterm_testkit::snomed::{ANIMAL, CAT, COVERING, DOG, FUR, LEGS, code};

fn validated(code: &str) -> validate_code::ValidationOutcome {
    let dir = tempfile::tempdir().expect("tempdir");
    snomed::write_with_concept_model(dir.path()).expect("writes the fixture");
    let provider = SnomedProvider::open(dir.path(), "en").expect("opens");
    let mut registry = Registry::default();
    registry.register(Arc::new(provider)).expect("registers");
    let request = validate_code::ValidateCodeInput {
        url: Some(SYSTEM.to_owned()),
        code: Some(code.to_owned()),
        ..validate_code::ValidateCodeInput::default()
    };
    validate_code::validate_code(&registry, &Invocation::Type, &request).expect("validates")
}

/// The detail of a refused expression, the `information` issue beside its error.
fn refused(code: &str) -> String {
    let outcome = validated(code);
    assert!(!outcome.result, "{code}: {:?}", outcome.issues);
    let severities: Vec<&str> = outcome.issues.iter().map(|i| i.severity).collect();
    assert_eq!(severities, ["error", "information"], "{code}");
    let detail = outcome.issues.get(1).expect("the detail");
    assert_eq!(
        detail.message_id(),
        None,
        "a detail carries no message of its own"
    );
    detail.text.clone()
}

#[test]
fn an_expression_inside_every_rule_is_valid_with_no_issue() {
    for expression in [
        format!("{}:{{{}={}}}", code(CAT), code(COVERING), code(FUR)),
        format!("{}:{}=#4", code(CAT), code(LEGS)),
    ] {
        let outcome = validated(&expression);
        assert!(outcome.result, "{expression}: {:?}", outcome.issues);
        assert!(
            outcome.issues.is_empty(),
            "{expression}: {:?}",
            outcome.issues
        );
    }
}

#[test]
fn an_attribute_outside_its_domain_is_refused() {
    let detail = refused(&format!(
        "{}:{{{}={}}}",
        code(FUR),
        code(COVERING),
        code(FUR)
    ));
    assert!(
        detail.contains(&code(COVERING)) && detail.contains(&code(ANIMAL)),
        "{detail}"
    );
}

#[test]
fn a_value_outside_the_range_is_refused() {
    let detail = refused(&format!(
        "{}:{{{}={}}}",
        code(CAT),
        code(COVERING),
        code(DOG)
    ));
    assert!(
        detail.contains(&code(DOG)) && detail.contains(&code(FUR)),
        "{detail}"
    );
}

#[test]
fn an_attribute_beyond_its_cardinality_is_refused() {
    let detail = refused(&format!(
        "{}:{}=#4,{}=#4",
        code(CAT),
        code(LEGS),
        code(LEGS)
    ));
    assert!(detail.contains("2 times"), "{detail}");
}

#[test]
fn an_ungrouped_attribute_in_a_group_is_refused() {
    let detail = refused(&format!("{}:{{{}=#4}}", code(CAT), code(LEGS)));
    assert!(detail.contains("ungrouped"), "{detail}");
}

#[test]
fn a_concrete_value_outside_its_type_or_bounds_is_refused() {
    for value in ["#2.5", "#0"] {
        let detail = refused(&format!("{}:{}={value}", code(CAT), code(LEGS)));
        assert!(
            detail.contains(value) && detail.contains("int(>#0..)"),
            "{detail}"
        );
    }
    let detail = refused(&format!("{}:{}={}", code(CAT), code(LEGS), code(FUR)));
    assert!(
        detail.contains(&code(FUR)),
        "a concept where a number is wanted: {detail}"
    );
}

#[test]
fn a_concept_that_is_no_attribute_is_refused() {
    let detail = refused(&format!("{}:{{{}={}}}", code(CAT), code(ANIMAL), code(FUR)));
    assert!(detail.contains("not a concept model attribute"), "{detail}");
}

#[test]
fn every_mandatory_rule_broken_is_reported() {
    let detail = refused(&format!("{}:{{{}=#2.5}}", code(CAT), code(LEGS)));
    assert!(
        detail.contains("ungrouped") && detail.contains("#2.5"),
        "{detail}"
    );
}

#[test]
fn an_optional_rule_broken_is_a_warning_beside_a_true_result() {
    // "Rules with a ruleStrength of |Optional concept model rule| ... result in a
    // warning during validation" (MRCM §6).
    let expression = format!("{}:{{{}={}}}", code(DOG), code(COVERING), code(FUR));
    let outcome = validated(&expression);
    assert!(outcome.result, "{:?}", outcome.issues);
    let warnings: Vec<_> = outcome
        .issues
        .iter()
        .filter(|i| i.severity == "warning")
        .collect();
    assert_eq!(warnings.len(), 1, "{:?}", outcome.issues);
    let warning = warnings.first().expect("the warning");
    assert!(warning.text.contains(&code(CAT)), "{}", warning.text);
}

#[test]
fn an_edition_without_a_concept_model_checks_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    snomed::write(dir.path()).expect("writes the fixture");
    let provider = SnomedProvider::open(dir.path(), "en").expect("opens");
    let mut registry = Registry::default();
    registry.register(Arc::new(provider)).expect("registers");
    let request = validate_code::ValidateCodeInput {
        url: Some(SYSTEM.to_owned()),
        code: Some(format!("{}:{{{}=#4}}", code(CAT), code(LEGS))),
        ..validate_code::ValidateCodeInput::default()
    };
    let outcome =
        validate_code::validate_code(&registry, &Invocation::Type, &request).expect("validates");
    assert!(outcome.result, "{:?}", outcome.issues);
}

#[test]
fn a_loose_attribute_the_concept_model_groups_is_written_in_a_group_of_its_own() {
    // "Adding a Self-grouped Attribute" (SNOMED CT Postcoordination Guide,
    // Transformation Level 1), which the tx-ecosystem IG requires.
    let grouped = format!("{}:{{{}={}}}", code(CAT), code(COVERING), code(FUR));
    let outcome = validated(&format!("{}:{}={}", code(CAT), code(COVERING), code(FUR)));
    assert!(outcome.result, "{:?}", outcome.issues);
    assert_eq!(outcome.normalized_code.as_deref(), Some(grouped.as_str()));
    // An ungrouped attribute stays loose, and so does one stated twice.
    let loose = format!("{}:{}=#4", code(CAT), code(LEGS));
    assert_eq!(validated(&loose).normalized_code, None);
    let twice = format!(
        "{}:{}={},{}={}",
        code(CAT),
        code(COVERING),
        code(FUR),
        code(COVERING),
        code(FUR)
    );
    let outcome = validated(&twice);
    assert!(
        outcome
            .normalized_code
            .as_deref()
            .is_none_or(|normal| !normal.contains('{')),
        "{:?}",
        outcome.normalized_code
    );
}

#[test]
fn a_concrete_number_is_typed_by_the_attribute_range() {
    // tx-ecosystem IG, requirements, SNOMED CT: a number is "typed by the
    // attribute's range in the MRCM"; legs ranges over `int(>#0..)`.
    let dir = tempfile::tempdir().expect("tempdir");
    snomed::write_with_concept_model(dir.path()).expect("writes the fixture");
    let provider = SnomedProvider::open(dir.path(), "en").expect("opens");
    let legs: Vec<PropertyValue> = provider
        .properties(Concept::new(CAT))
        .expect("reads")
        .into_iter()
        .filter(|p| p.code == code(LEGS))
        .filter_map(|p| p.value)
        .collect();
    assert_eq!(legs, [PropertyValue::Integer(4)]);
}
