// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Value set references, excludes, cycles and contained value sets.

use ferroterm_testkit::fhir::{ANIMALS, VS_LOOP_A, VS_PETS, VS_PETS_REF};
use fhir_terminology::operations::CodingRef;
use fhir_terminology::operations::expand::ExpandInput;
use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;
use fhir_terminology::operations::{OperationError, expand, value_set_validate_code};
use fhir_terminology::valueset;
use fhir_terminology::valueset::expansion::ParameterValue;
use fhir_types::r4b::value_set::{
    ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
};

use super::{
    LANGUAGES, TWO_VERSION, VS_ABSENT, VS_BAD_EXCLUDE, VS_CIRCLE_1, VS_EXCLUDING_V1, VS_INNER,
    VS_ITSELF, VS_OUTER, World, circular_world, codes, entry, parameter, referencing_world,
    self_referencing_world, two_version_world, unresolvable_exclude_world,
};

#[test]
fn expand_follows_value_set_references_and_refuses_a_cycle() {
    let world = World::load();
    let referenced = ExpandInput {
        url: Some(VS_PETS_REF.to_owned()),
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &referenced).expect("expands");
    // The referenced expansion states an order and this one keeps it: `pet` is
    // the root of the referenced hierarchy, `kitten` its child (#276).
    assert_eq!(codes(&vs), ["pet", "kitten"]);
    let looped = ExpandInput {
        url: Some(VS_LOOP_A.to_owned()),
        ..ExpandInput::default()
    };
    let error = expand::expand(&world.sources(), &looped).expect_err("cycle");
    assert!(
        matches!(error, OperationError::ValueSetCyclic(_)),
        "{error}"
    );
    // A cycle is a processing failure, "there is no point resubmitting the same
    // content unchanged" (<https://hl7.org/fhir/R4B/valueset-issue-type.html>).
    assert_eq!(error.issue_code(), "processing");
    assert_eq!(error.tx_issue_type(), "vs-invalid");
}

#[test]
fn validate_code_flags_a_local_system_reference() {
    let world = World::load();
    let local = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(String::from("Location1")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &local).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "invalid-data", "not-found"]);
    assert_eq!(
        validation.message.as_deref(),
        Some(
            "A definition for CodeSystem 'Location1' could not be found, so the code cannot be validated; Coding.system must be an absolute reference, not a local reference; The provided code 'Location1#kitten' was not found in the value set 'http://example.org/fhir/ValueSet/pets|1.0'"
        )
    );
    assert_eq!(validation.x_unknown_systems, ["Location1"]);
}

// NOTE: an exclude's version says which version its codes are selected from, so
// it removes the code whatever version an include contributed it at
// (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.compose.exclude>).
#[test]
fn an_exclude_at_another_version_removes_the_code_the_include_contributed() {
    let world = two_version_world();
    let input = ValueSetValidateInput {
        url: Some(VS_EXCLUDING_V1.to_owned()),
        coding: Some(CodingRef {
            system: Some(TWO_VERSION.to_owned()),
            code: Some(String::from("code1")),
            ..CodingRef::default()
        }),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(!validation.result, "{validation:?}");
    assert_eq!(validation.version.as_deref(), Some("2.0.0"));
    assert_eq!(validation.display.as_deref(), Some("Display 1"));
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs"]);
    let kept = ValueSetValidateInput {
        url: Some(VS_EXCLUDING_V1.to_owned()),
        coding: Some(CodingRef {
            system: Some(TWO_VERSION.to_owned()),
            code: Some(String::from("code4")),
            ..CodingRef::default()
        }),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &kept).expect("validates");
    assert!(
        validation.result,
        "a code the excluded version does not have stays in: {validation:?}"
    );
}

// NOTE: the ecosystem's `default-valueset-version` suite answers one
// unresolvable reference with an error from `$expand` and `result = false` from
// `$validate-code` (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
#[test]
fn an_unresolvable_exclude_is_named_whether_or_not_an_include_held_the_code() {
    let world = unresolvable_exclude_world();
    let error = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_BAD_EXCLUDE.to_owned()),
            ..ExpandInput::default()
        },
    )
    .expect_err("an expansion has to evaluate every exclude");
    assert!(
        error.to_string().contains(VS_ABSENT),
        "the refusal names what could not be resolved: `{error}`"
    );

    // `code1` is the code the include holds and `code2` is a code of the same
    // system that it does not, so the two runs differ only in whether an
    // include matched before the exclude was reached.
    let validate = |code: &str| {
        value_set_validate_code::validate_code(
            &world.sources(),
            &ValueSetValidateInput {
                url: Some(VS_BAD_EXCLUDE.to_owned()),
                coding: Some(CodingRef {
                    system: Some(TWO_VERSION.to_owned()),
                    code: Some(code.to_owned()),
                    ..CodingRef::default()
                }),
                ..ValueSetValidateInput::default()
            },
        )
        .expect("validate-code answers rather than refusing")
    };
    for code in ["code1", "code2"] {
        let validation = validate(code);
        assert!(
            !validation.result,
            "a definition this server cannot fully evaluate is not a membership: {validation:?}"
        );
        let named = validation
            .issues
            .iter()
            .any(|issue| issue.kind == "not-found" && issue.text.contains(VS_ABSENT));
        assert!(
            named,
            "`{code}` must name the value set the server could not resolve, so a client can tell \
             a missing resource from a code that is genuinely out: {:?}",
            validation.issues
        );
    }
}

// NOTE: a reference naming no version means the version the server resolves to
// (<https://hl7.org/fhir/R4B/references.html#canonical>), so a bare reference
// re-enters a value set stored with a version.
#[test]
fn a_value_set_that_references_itself_is_refused_and_named() {
    let world = self_referencing_world();
    let error = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_ITSELF.to_owned()),
            ..ExpandInput::default()
        },
    )
    .expect_err("a value set that reaches itself cannot be expanded");
    let OperationError::ValueSetCyclic(reported) = &error else {
        panic!("a cycle, not {error}");
    };
    assert!(
        reported.contains(VS_ITSELF),
        "the error names the value set that closed the loop: `{reported}`"
    );

    let error = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            url: Some(VS_ITSELF.to_owned()),
            coding: Some(CodingRef {
                system: Some(TWO_VERSION.to_owned()),
                code: Some(String::from("code1")),
                ..CodingRef::default()
            }),
            ..ValueSetValidateInput::default()
        },
    )
    .expect_err("a value set that reaches itself cannot be validated against");
    let OperationError::ValueSetCyclic(reported) = &error else {
        panic!("a cycle, not {error}");
    };
    assert!(
        reported.contains(VS_ITSELF),
        "both operations name the same value set: `{reported}`"
    );
    assert_eq!(error.tx_issue_type(), "vs-invalid");
}

// NOTE: a value set that reaches itself cannot be evaluated, so both operations
// refuse it with the ecosystem's `vs-invalid`
// (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
#[test]
fn a_cycle_through_an_exclude_refuses_both_expand_and_validate_code() {
    let world = circular_world();
    let error = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_CIRCLE_1.to_owned()),
            ..ExpandInput::default()
        },
    )
    .expect_err("a cycle");
    let OperationError::ValueSetCyclic(reported) = &error else {
        panic!("a cycle, not {error}");
    };
    assert!(
        reported.contains(VS_CIRCLE_1),
        "the error names the value set the loop closed on, which is the one the \
         request asked about: `{reported}`"
    );
    assert_eq!(error.issue_code(), "processing");
    assert_eq!(error.tx_issue_type(), "vs-invalid");
    // The code is of a system the first include misses, so the membership walk
    // stops before the exclude that closes the cycle.
    let error = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            url: Some(VS_CIRCLE_1.to_owned()),
            coding: Some(CodingRef {
                system: Some(String::from("http://example.org/fhir/CodeSystem/elsewhere")),
                code: Some(String::from("code9")),
                ..CodingRef::default()
            }),
            ..ValueSetValidateInput::default()
        },
    )
    .expect_err("a cycle");
    assert!(
        matches!(error, OperationError::ValueSetCyclic(_)),
        "{error}"
    );
    assert_eq!(error.issue_code(), "processing");
    assert_eq!(error.tx_issue_type(), "vs-invalid");
    assert_eq!(error.status(), http::StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn a_code_reached_through_a_value_set_reference_carries_the_requested_display() {
    // `displayLanguage` is "the language to use when generating the display"
    // of an expansion's concepts
    // (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>), so the hop
    // through a referenced value set answers in it too.
    let world = referencing_world();
    let expansion = |url: &str, language: &str| {
        let request = ExpandInput {
            url: Some(url.to_owned()),
            display_language: Some(language.to_owned()),
            ..ExpandInput::default()
        };
        let vs = expand::expand(&world.sources(), &request).expect("expands");
        entry(&vs.contains, "cat")
            .expect("cat")
            .display
            .clone()
            .expect("a display")
    };
    assert_eq!(expansion(VS_INNER, "nl"), "Kat", "the first hop");
    assert_eq!(expansion(VS_OUTER, "nl"), "Kat", "through the reference");
    // A language the system holds nothing in answers the display it does
    // have, at both hops, rather than the requested tag over another language.
    assert_eq!(expansion(VS_INNER, "de"), "Cat");
    assert_eq!(expansion(VS_OUTER, "de"), "Cat");
    let validate = |url: &str| {
        let input = ValueSetValidateInput {
            url: Some(url.to_owned()),
            system: Some(LANGUAGES.to_owned()),
            code: Some(String::from("cat")),
            display_language: Some(String::from("nl")),
            ..ValueSetValidateInput::default()
        };
        let validation =
            value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
        assert!(validation.result, "{validation:?}");
        validation.display.expect("a display")
    };
    assert_eq!(validate(VS_INNER), "Kat");
    assert_eq!(validate(VS_OUTER), "Kat");
}

#[test]
fn a_compose_reaches_a_contained_value_set_through_its_fragment() {
    let world = World::load();
    let contained = ValueSet {
        id: Some(String::from("vs1")),
        url: Some("http://example.org/inline/contained".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                concept: vec![
                    ValueSetComposeIncludeConcept {
                        code: "cat".into(),
                        ..Default::default()
                    },
                    ValueSetComposeIncludeConcept {
                        code: "kitten".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let inline = ValueSet {
        url: Some("http://example.org/inline/container".into()),
        status: "active".into(),
        contained: vec![fhir_types::r4b::resource::Resource::ValueSet(Box::new(
            contained,
        ))],
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                value_set: vec!["#vs1".into(), VS_PETS.into()],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let request = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &request).expect("expands");
    // Two value sets in one include intersect, so only `kitten` survives.
    assert_eq!(codes(&vs), ["kitten"]);
    assert_eq!(
        parameter(&vs, "used-valueset"),
        [ParameterValue::Uri(format!("{VS_PETS}|1.0"))],
        "a contained value set has no canonical of its own to report"
    );
    let validate = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &validate).expect("validates");
    assert!(
        validation.result,
        "the contained value set resolves here too"
    );
}
