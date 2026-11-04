// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Code system supplements under the value set operations.

use ferroterm_testkit::fhir::{ANIMALS, ANIMALS_NL, VS_ALL};
use fhir_terminology::operations::expand::ExpandInput;
use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;
use fhir_terminology::operations::{OperationError, expand, value_set_validate_code};
use fhir_terminology::valueset;
use fhir_terminology::valueset::expansion::ExpansionOutcome;
use fhir_types::r4b::extension::{Extension, ExtensionValue};
use fhir_types::r4b::value_set::{ValueSet, ValueSetCompose, ValueSetComposeInclude};

use super::World;

// NOTE: a loaded supplement applies only when the request names it in
// `useSupplement` or the value set asks for it (the ecosystem's requirements,
// #184); a supplement is never a code system of its own.
#[test]
fn supplements_apply_only_when_named() {
    let world = World::load();
    let kat = |supplements: Vec<String>| ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        display: Some(String::from("Kat")),
        use_supplement: supplements,
        ..ValueSetValidateInput::default()
    };
    let dormant = value_set_validate_code::validate_code(&world.sources(), &kat(Vec::new()))
        .expect("validates");
    assert!(
        !dormant.result,
        "the Dutch designation comes from the supplement: {dormant:?}"
    );
    assert_eq!(dormant.issues[0].kind, "invalid-display");
    let applied =
        value_set_validate_code::validate_code(&world.sources(), &kat(vec![ANIMALS_NL.to_owned()]))
            .expect("validates");
    assert!(applied.result, "{applied:?}");
    let versioned = value_set_validate_code::validate_code(
        &world.sources(),
        &kat(vec![format!("{ANIMALS_NL}|1")]),
    )
    .expect("validates");
    assert!(versioned.result);
    let unknown = value_set_validate_code::validate_code(
        &world.sources(),
        &kat(vec![String::from(
            "http://example.org/fhir/CodeSystem/no-such-supplement",
        )]),
    )
    .expect_err("refused");
    assert!(
        matches!(unknown, OperationError::UnknownSupplement(_)),
        "{unknown:?}"
    );
    assert_eq!(unknown.issue_code(), "not-found");
    assert_eq!(
        unknown.to_string(),
        "Required supplement not found: http://example.org/fhir/CodeSystem/no-such-supplement"
    );
}

#[test]
fn a_value_set_asks_for_its_supplement_and_a_supplement_is_no_code_system() {
    let world = World::load();
    // A value set that asks for the supplement applies it by itself.
    let asking = ValueSet {
        url: Some("http://example.org/inline-asking".into()),
        status: "draft".into(),
        extension: vec![Extension {
            url: "http://hl7.org/fhir/StructureDefinition/valueset-supplement".into(),
            value: Some(ExtensionValue::Canonical(ANIMALS_NL.into())),
            ..Default::default()
        }],
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let by_value_set = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            inline_value_set: Some(valueset::convert::r4b::convert(&asking)),
            code: Some(String::from("cat")),
            system: Some(ANIMALS.to_owned()),
            display: Some(String::from("Kat")),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("validates");
    assert!(by_value_set.result, "{by_value_set:?}");
    // The supplement's own URL is no code system.
    let as_system = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            url: Some(VS_ALL.to_owned()),
            code: Some(String::from("cat")),
            system: Some(ANIMALS_NL.to_owned()),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("a false result");
    assert!(!as_system.result);
    assert_eq!(as_system.issues[0].kind, "invalid-data");
    assert_eq!(
        as_system.issues[0].text,
        format!(
            "CodeSystem {ANIMALS_NL}|1 is a supplement, so can't be used as a value in Coding.system"
        )
    );
    assert_eq!(as_system.unknown_systems, [ANIMALS_NL.to_owned()]);
    // $expand applies a named supplement to the designations it returns.
    let kat_in = |vs: &ExpansionOutcome| {
        vs.contains
            .iter()
            .any(|c| c.designations.iter().any(|d| d.value == "Kat"))
    };
    let expanded = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_ALL.to_owned()),
            include_designations: Some(true),
            use_supplement: vec![ANIMALS_NL.to_owned()],
            ..ExpandInput::default()
        },
    )
    .expect("expands");
    assert!(
        kat_in(&expanded),
        "the named supplement's designation is in the expansion"
    );
    let plain = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_ALL.to_owned()),
            include_designations: Some(true),
            ..ExpandInput::default()
        },
    )
    .expect("expands");
    assert!(!kat_in(&plain), "a dormant supplement adds nothing");
}

#[test]
fn code_system_validate_code_refuses_a_supplement_as_the_system() {
    let world = World::load();
    let input = fhir_terminology::operations::validate_code::ValidateCodeInput {
        url: Some(ANIMALS_NL.to_owned()),
        code: Some(String::from("cat")),
        ..fhir_terminology::operations::validate_code::ValidateCodeInput::default()
    };
    let outcome = fhir_terminology::operations::validate_code::validate_code(
        world.sources().registry,
        &fhir_terminology::operations::Invocation::Type,
        &input,
    )
    .expect("validates");
    assert!(!outcome.result);
    assert_eq!(outcome.issues[0].kind, "invalid-data");
    assert_eq!(
        outcome.issues[0].text,
        format!(
            "CodeSystem {ANIMALS_NL}|1 is a supplement, so can't be used as a value in Coding.system"
        )
    );
    assert_eq!(
        outcome.issues[0].message_id(),
        "CODESYSTEM_CS_NO_SUPPLEMENT"
    );
    assert_eq!(outcome.unknown_systems, [ANIMALS_NL]);
    assert_eq!(outcome.code.as_deref(), Some("cat"));
}
