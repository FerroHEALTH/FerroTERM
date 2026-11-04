// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Code systems with `fragment` content under the value set operations.

use fhir_terminology::operations::CodingRef;
use fhir_terminology::operations::value_set_validate_code;
use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;

use super::{FRAGMENT, VS_FRAGMENT, VS_FRAGMENT_ENUM, codeable, fragment_world};

#[test]
fn a_code_a_fragment_resource_does_not_carry_is_not_a_code_the_system_lacks() {
    // A `fragment` resource holds "a subset of the code system concepts"
    // (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>), and the
    // ecosystem asks a server to "reflect `content = fragment` in an error
    // message if the code is not valid against a fragment"
    // (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
    let world = fragment_world();
    let input = ValueSetValidateInput {
        url: Some(VS_FRAGMENT.to_owned()),
        system: Some(FRAGMENT.to_owned()),
        code: Some(String::from("code1x")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(validation.result, "{validation:?}");
    assert_eq!(validation.code.as_deref(), Some("code1x"));
    assert_eq!(validation.system.as_deref(), Some(FRAGMENT));
    assert_eq!(validation.version.as_deref(), Some("0.1.0"));
    assert_eq!(validation.display, None, "the system has no display for it");
    assert_eq!(validation.message, None, "a result of true says nothing");
    assert_eq!(validation.issues.len(), 1, "{:?}", validation.issues);
    let issue = &validation.issues[0];
    assert_eq!(
        (issue.severity, issue.code, issue.kind),
        ("warning", "code-invalid", "invalid-code")
    );
    assert_eq!(
        issue.text,
        format!(
            "Unknown Code 'code1x' in the CodeSystem '{FRAGMENT}' version '0.1.0' - note that \
the code system is labeled as a fragment, so the code may be valid in some other fragment"
        )
    );
    assert_eq!(issue.expression.as_deref(), Some("code"));
}

#[test]
fn a_codeable_concept_over_a_fragment_holds_on_a_code_the_resource_lacks() {
    // A `CodeableConcept` is judged coding by coding, and a coding the fragment
    // leaves open is not one the value set can be said to refuse, so the
    // warning travels with a `result` of true.
    let world = fragment_world();
    let input = ValueSetValidateInput {
        url: Some(VS_FRAGMENT.to_owned()),
        codeable_concept: Some(codeable(vec![CodingRef {
            system: Some(FRAGMENT.to_owned()),
            code: Some(String::from("code1x")),
            ..CodingRef::default()
        }])),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(validation.result, "{validation:?}");
    assert_eq!(validation.issues.len(), 1, "{:?}", validation.issues);
    assert_eq!(validation.issues[0].severity, "warning");
    assert_eq!(
        validation.issues[0].expression.as_deref(),
        Some("CodeableConcept.coding[0].code")
    );
    assert!(
        validation.codeable_concept.is_some(),
        "the concept is echoed"
    );
}

#[test]
fn an_include_that_enumerates_a_fragments_codes_still_decides_membership() {
    // The include lists the codes it selects, so one it does not list is
    // outside the value set whatever else the system defines.
    let world = fragment_world();
    let input = ValueSetValidateInput {
        url: Some(VS_FRAGMENT_ENUM.to_owned()),
        system: Some(FRAGMENT.to_owned()),
        code: Some(String::from("code1x")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(!validation.result, "{validation:?}");
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "invalid-code"]);
}
