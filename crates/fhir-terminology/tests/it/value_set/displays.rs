// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `$validate-code` on displays, languages and codeable concepts.

use ferroterm_testkit::fhir::{ANIMALS, COLOURS, VS_ALL, VS_ENUMERATED, VS_PETS};
use fhir_terminology::operations::expand::ExpandInput;
use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;
use fhir_terminology::operations::{CodeableConceptRef, CodingRef};
use fhir_terminology::operations::{OperationError, expand, value_set_validate_code};
use fhir_terminology::valueset;
use fhir_types::r4b::extension::{Extension, ExtensionValue};
use fhir_types::r4b::value_set::{ValueSet, ValueSetCompose, ValueSetComposeInclude};

use super::{World, codeable, entry};

#[test]
fn validate_code_checks_display_case_and_inactive_codes() {
    let world = World::load();
    let wrong_display = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        display: Some(String::from("Hamster")),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &wrong_display)
        .expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.issues[0].kind, "invalid-display");
    assert_eq!(validation.display.as_deref(), Some("Cat"));
    let synonym = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        display: Some(String::from("domestic CAT")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &synonym).expect("validates");
    assert!(validation.result, "a designation, case folded");
    // NOTE: whitespace is not folded: a display that differs only there is wrong,
    // and says so (the ecosystem's `bad-display-ws` case).
    let spaced = ValueSetValidateInput {
        display: Some(String::from("domestic  CAT")),
        ..synonym
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &spaced).expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.issues[0].kind, "invalid-display");
    assert!(
        validation.issues[0].text.contains("the whitespace differs"),
        "{}",
        validation.issues[0].text
    );
    assert_eq!(
        validation.issues[0].message_id(),
        "Display_Name_WS_for__should_be_one_of__instead_of"
    );
    let case = ValueSetValidateInput {
        url: Some(VS_ENUMERATED.to_owned()),
        code: Some(String::from("red")),
        system: Some(COLOURS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &case).expect("validates");
    assert!(validation.result, "colours is case-insensitive");
    // The ecosystem asks for `normalized-code` on a case-insensitive system: the
    // request's spelling is echoed and the system's comes beside it.
    assert_eq!(
        validation.code.as_deref(),
        Some("red"),
        "the request's spelling is echoed"
    );
    assert_eq!(validation.normalized_code.as_deref(), Some("RED"));
    let inactive = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        value_set_version: Some(String::from("1.0")),
        code: Some(String::from("dodo")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &inactive).expect("validates");
    assert!(validation.result, "inactive is not invalid");
    assert_eq!(validation.issues[0].kind, "code-comment");
    assert_eq!(validation.issues[0].severity, "warning");
    let abstract_refused = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("pet")),
        system: Some(ANIMALS.to_owned()),
        abstract_ok: Some(false),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &abstract_refused)
        .expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.issues[0].kind, "code-rule");
}

// NOTE: `inferSystem` finds a bare code's system by its membership in the value
// set, and `lenient-display-validation` keeps a wrong display from failing the
// result (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-validate-code.html>).
#[test]
fn validate_code_infers_the_system_by_membership_and_can_be_lenient_on_displays() {
    let world = World::load();
    let bare = |code: &str, infer: Option<bool>| ValueSetValidateInput {
        url: Some(VS_ENUMERATED.to_owned()),
        code: Some(code.to_owned()),
        infer_system: infer,
        ..ValueSetValidateInput::default()
    };
    let red = value_set_validate_code::validate_code(&world.sources(), &bare("RED", Some(true)))
        .expect("validates");
    assert!(red.result);
    assert_eq!(red.system.as_deref(), Some(COLOURS));
    let cat = value_set_validate_code::validate_code(&world.sources(), &bare("cat", Some(true)))
        .expect("validates");
    assert_eq!(cat.system.as_deref(), Some(ANIMALS));
    let dodo = value_set_validate_code::validate_code(&world.sources(), &bare("dodo", Some(true)))
        .expect("a false result");
    assert!(!dodo.result);
    let kinds: Vec<&str> = dodo.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "cannot-infer"]);
    assert_eq!(dodo.issues[1].code, "not-found");
    assert_eq!(dodo.code.as_deref(), Some("dodo"));
    assert_eq!(
        dodo.message.as_deref(),
        Some(format!("The System URI could not be determined for the code 'dodo' in the ValueSet '{VS_ENUMERATED}|1.0'; The provided code '#dodo' was not found in the value set '{VS_ENUMERATED}|1.0'").as_str())
    );
    // Without inferSystem a bare code is a request error: "If a code is
    // provided, a system or a context must be provided"
    // (<https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>, `code`).
    assert!(matches!(
        value_set_validate_code::validate_code(&world.sources(), &bare("cat", None)),
        Err(OperationError::Invalid(_))
    ));
    let wrong_display = |lenient: Option<bool>| ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        display: Some(String::from("Puppy")),
        lenient_display_validation: lenient,
        ..ValueSetValidateInput::default()
    };
    let strict = value_set_validate_code::validate_code(&world.sources(), &wrong_display(None))
        .expect("validates");
    assert!(!strict.result);
    assert_eq!(strict.issues[0].severity, "error");
    let lenient =
        value_set_validate_code::validate_code(&world.sources(), &wrong_display(Some(true)))
            .expect("validates");
    assert!(lenient.result, "a warning does not fail the result");
    assert_eq!(lenient.issues[0].severity, "warning");
    assert_eq!(lenient.issues[0].kind, "invalid-display");
    assert_eq!(lenient.display.as_deref(), Some("Kitten"));
}

// NOTE: a CodeableConcept is judged coding by coding, the ecosystem's shape (its
// test cases): the first coding in the value set answers the code outputs, the
// input is echoed, and issues name the coding at fault by index.
#[test]
fn validate_code_judges_a_codeable_concept_coding_by_coding() {
    let world = World::load();
    let coding = |system: &str, code: &str, display: Option<&str>| CodingRef {
        system: Some(system.to_owned()),
        version: None,
        code: Some(code.to_owned()),
        display: display.map(str::to_owned),
    };
    let mixed = ValueSetValidateInput {
        url: Some(VS_ENUMERATED.to_owned()),
        codeable_concept: Some(codeable(vec![
            coding(COLOURS, "blue", None),
            coding(ANIMALS, "cat", None),
        ])),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &mixed).expect("validates");
    assert!(validation.result, "{validation:?}");
    assert_eq!(validation.code.as_deref(), Some("cat"));
    assert_eq!(validation.system.as_deref(), Some(ANIMALS));
    let echoed = validation.codeable_concept.as_ref().map(|c| c.coding.len());
    assert_eq!(echoed, Some(2), "echoed");
    let kinds: Vec<(&str, &str, Option<&str>)> = validation
        .issues
        .iter()
        .map(|i| (i.severity, i.kind, i.expression.as_deref()))
        .collect();
    assert_eq!(
        kinds,
        [(
            "information",
            "this-code-not-in-vs",
            Some("CodeableConcept.coding[0].code")
        )]
    );
    let wrong_display = ValueSetValidateInput {
        url: Some(VS_ENUMERATED.to_owned()),
        codeable_concept: Some(codeable(vec![coding(ANIMALS, "cat", Some("Hamster"))])),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &wrong_display)
        .expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.code.as_deref(), Some("cat"));
    assert_eq!(
        validation.issues[0].expression.as_deref(),
        Some("CodeableConcept.coding[0].display")
    );
    let none = ValueSetValidateInput {
        url: Some(VS_ENUMERATED.to_owned()),
        codeable_concept: Some(codeable(vec![
            coding(ANIMALS, "dodo", None),
            coding("http://example.org/none", "x", None),
        ])),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &none).expect("a false result");
    assert!(!validation.result);
    assert_eq!(
        validation.code, None,
        "no coding in the value set answers the code outputs"
    );
    assert_eq!(validation.system, None);
    let kinds: Vec<(&str, &str, Option<&str>)> = validation
        .issues
        .iter()
        .map(|i| (i.severity, i.kind, i.expression.as_deref()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("error", "not-in-vs", None),
            (
                "error",
                "not-found",
                Some("CodeableConcept.coding[1].system")
            ),
            (
                "information",
                "this-code-not-in-vs",
                Some("CodeableConcept.coding[0].code")
            ),
            (
                "information",
                "this-code-not-in-vs",
                Some("CodeableConcept.coding[1].code")
            ),
        ]
    );
    assert_eq!(validation.x_unknown_systems, ["http://example.org/none"]);
    assert_eq!(validation.unknown_systems, Vec::<String>::new());
    // The missing system speaks first, then the concept's verdict (the
    // ecosystem's `codeableconcept-bad-version2` case).
    assert_eq!(
        validation.message.as_deref(),
        Some(format!("A definition for CodeSystem 'http://example.org/none' could not be found, so the code cannot be validated; No valid coding was found for the value set '{VS_ENUMERATED}|1.0'").as_str())
    );
}

// NOTE: the issue texts follow the ecosystem's test cases (spec-silent, #183), so
// a validator shows its user the wording the reference server would.
#[test]
fn validate_code_speaks_the_ecosystems_words_for_displays_codes_and_membership() {
    let world = World::load();
    let cat = |display: &str, language: Option<&str>| ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        display: Some(display.to_owned()),
        display_language: language.map(str::to_owned),
        ..ValueSetValidateInput::default()
    };
    let run = |input: &ValueSetValidateInput| {
        value_set_validate_code::validate_code(&world.sources(), input).expect("validates")
    };
    // No displayLanguage: every display of the concept is valid, each with its language.
    let wrong = run(&cat("Hamster", None));
    assert!(!wrong.result);
    assert_eq!(
        wrong.issues[0].text,
        format!(
            "Wrong Display Name 'Hamster' for {ANIMALS}#cat. Valid display is one of 3 choices: 'Cat' (en), 'Domestic cat' (en) or 'Katze' (de) (for the language(s) '--')"
        )
    );
    assert_eq!(
        wrong.message.as_deref(),
        Some(wrong.issues[0].text.as_str())
    );
    // A displayLanguage with displays: only those count.
    assert!(run(&cat("Katze", Some("de"))).result);
    let english_in_german = run(&cat("Cat", Some("de")));
    assert!(!english_in_german.result);
    assert_eq!(
        english_in_german.issues[0].text,
        format!(
            "Wrong Display Name 'Cat' for {ANIMALS}#cat. Valid display is 'Katze' (de) (for the language(s) 'de')"
        )
    );
    // A displayLanguage without displays: the default language's are a note, others an error.
    let default_only = run(&cat("Cat", Some("nl")));
    assert!(default_only.result);
    assert_eq!(default_only.issues[0].severity, "information");
    assert_eq!(
        default_only.issues[0].text,
        format!(
            "There are no valid display names found for the code {ANIMALS}#cat for language(s) 'nl'. The display is 'Cat' which is a valid display for the default language"
        )
    );
    let nothing = run(&cat("Hamster", Some("nl")));
    assert!(!nothing.result);
    assert_eq!(
        nothing.issues[0].text,
        format!(
            "Wrong Display Name 'Hamster' for {ANIMALS}#cat. There are no valid display names found for language(s) 'nl'. Default display is 'Cat'"
        )
    );
}

// NOTE: "The server returns true if one of the coding values is in the value
// set" (`codeableConcept`, <https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>).
#[test]
fn validate_code_passes_a_codeable_concept_whose_other_coding_names_an_unknown_system() {
    let world = World::load();
    let nowhere = "http://example.org/fhir/CodeSystem/nowhere";
    let coding = |system: &str, code: &str| CodingRef {
        system: Some(system.to_owned()),
        version: None,
        code: Some(code.to_owned()),
        display: None,
    };
    let validation = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            url: Some(VS_ENUMERATED.to_owned()),
            codeable_concept: Some(codeable(vec![
                coding(nowhere, "cat"),
                coding(ANIMALS, "cat"),
            ])),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("validates");
    assert!(validation.result, "{validation:?}");
    assert_eq!(validation.code.as_deref(), Some("cat"));
    assert_eq!(validation.system.as_deref(), Some(ANIMALS));
    let shape: Vec<(&str, &str, &str, Option<&str>)> = validation
        .issues
        .iter()
        .map(|i| (i.severity, i.kind, i.message_id(), i.expression.as_deref()))
        .collect();
    assert_eq!(
        shape,
        [
            (
                "information",
                "this-code-not-in-vs",
                "None_of_the_provided_codes_are_in_the_value_set_one",
                Some("CodeableConcept.coding[0].code")
            ),
            (
                "warning",
                "not-found",
                "UNKNOWN_CODESYSTEM_CODING_NOT_CHECKED",
                Some("CodeableConcept.coding[0].system")
            ),
        ]
    );
    assert!(validation.x_unknown_systems.is_empty(), "{validation:?}");
    assert!(validation.unknown_systems.is_empty(), "{validation:?}");
    assert!(
        validation
            .message
            .as_deref()
            .is_some_and(|m| m.contains(nowhere)),
        "{:?}",
        validation.message
    );
}

#[test]
fn validate_code_notes_a_deprecated_concept_and_a_withdrawn_display() {
    let world = World::load();
    // `plant` carries a `deprecated` standards status: a warning, a `status`
    // output, and the message, while the concept stays active.
    let deprecated = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("plant")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &deprecated).expect("validates");
    assert!(validation.result);
    assert_eq!(validation.inactive, None);
    assert_eq!(validation.status.as_deref(), Some("deprecated"));
    assert_eq!(
        validation.message.as_deref(),
        Some("The concept 'plant' is deprecated and its use should be reviewed")
    );
    // `Hound` is a withdrawn designation of `dog`: an error, or a warning under
    // lenient display validation, never a valid display.
    let withdrawn = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("dog")),
        system: Some(ANIMALS.to_owned()),
        display: Some(String::from("Hound")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &withdrawn).expect("validates");
    assert!(!validation.result);
    let issue = &validation.issues[0];
    assert_eq!((issue.severity, issue.kind), ("error", "display-comment"));
    assert_eq!(
        issue.text,
        "'Hound' is no longer considered a correct display for code 'dog' (status = withdrawn). The correct display is 'Dog'"
    );
    assert_eq!(validation.display.as_deref(), Some("Dog"));
    let lenient = ValueSetValidateInput {
        lenient_display_validation: Some(true),
        ..withdrawn
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &lenient).expect("validates");
    assert!(validation.result);
    assert_eq!(validation.issues[0].severity, "warning");
    assert_eq!(
        validation.message.as_deref(),
        Some(
            "'Hound' is no longer considered a correct display for code 'dog' (status = withdrawn). The correct display is 'Dog'"
        )
    );
}

#[test]
fn validate_code_judges_displays_in_the_value_sets_language() {
    let world = World::load();
    let german = |compose_default: bool| {
        let mut inline = ValueSet {
            url: Some("http://example.org/german".into()),
            version: Some("1".into()),
            status: "draft".into(),
            compose: Some(ValueSetCompose {
                include: vec![ValueSetComposeInclude {
                    system: Some(ANIMALS.into()),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        if compose_default {
            // `valueset-expansion-parameter` displayLanguage = de on the compose.
            inline.compose.as_mut().expect("compose").extension = vec![Extension {
                url: "http://hl7.org/fhir/StructureDefinition/valueset-expansion-parameter".into(),
                extension: vec![
                    Extension {
                        url: "name".into(),
                        value: Some(ExtensionValue::Code("displayLanguage".into())),
                        ..Default::default()
                    },
                    Extension {
                        url: "value".into(),
                        value: Some(ExtensionValue::Code("de".into())),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }];
        } else {
            inline.language = Some("de".into());
        }
        valueset::convert::r4b::convert(&inline)
    };
    for compose_default in [false, true] {
        let english = ValueSetValidateInput {
            inline_value_set: Some(german(compose_default)),
            code: Some(String::from("cat")),
            system: Some(ANIMALS.to_owned()),
            display: Some(String::from("Cat")),
            ..ValueSetValidateInput::default()
        };
        // NOTE: the value set's language is the one displays are judged in, so
        // the English display is wrong here (the ecosystem's `bad-language-vs`).
        let validation =
            value_set_validate_code::validate_code(&world.sources(), &english).expect("validates");
        assert!(
            !validation.result,
            "compose default {compose_default}: {validation:?}"
        );
        assert_eq!(validation.issues[0].kind, "invalid-display");
        assert_eq!(validation.display.as_deref(), Some("Katze"));
        let german_display = ValueSetValidateInput {
            display: Some(String::from("Katze")),
            ..english
        };
        let validation = value_set_validate_code::validate_code(&world.sources(), &german_display)
            .expect("validates");
        assert!(validation.result, "compose default {compose_default}");
        assert!(validation.issues.is_empty(), "{:?}", validation.issues);
        let requested = ValueSetValidateInput {
            display_language: Some(String::from("en")),
            display: Some(String::from("Cat")),
            ..german_display
        };
        let validation = value_set_validate_code::validate_code(&world.sources(), &requested)
            .expect("validates");
        assert!(validation.result, "the request's language wins");
        assert!(validation.issues.is_empty(), "{:?}", validation.issues);
    }
}

#[test]
fn expand_filters_designations_by_a_bcp47_language() {
    let world = World::load();
    let request = ExpandInput {
        url: Some(VS_ALL.to_owned()),
        include_designations: Some(true),
        designation: vec![String::from("urn:ietf:bcp:47|de")],
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &request).expect("expands");
    let cat = entry(&vs.contains, "cat").expect("cat");
    let values: Vec<&str> = cat.designations.iter().map(|d| d.value.as_str()).collect();
    assert_eq!(values, ["Katze"]);
}

#[test]
fn an_invalid_display_language_is_refused_on_every_operation() {
    let world = World::load();
    for language in ["-", "zz", "en-AB", "de,zz"] {
        let expand_request = ExpandInput {
            url: Some(VS_PETS.to_owned()),
            display_language: Some(String::from(language)),
            ..ExpandInput::default()
        };
        let error = expand::expand(&world.sources(), &expand_request).expect_err(language);
        assert!(
            matches!(&error, OperationError::InvalidLanguage(_)),
            "{language}: {error}"
        );
        assert_eq!(error.status(), http::StatusCode::BAD_REQUEST);
        assert_eq!(error.issue_code(), "processing");
        assert_eq!(error.tx_issue_type(), "invalid-display");
        let validate_request = ValueSetValidateInput {
            url: Some(VS_PETS.to_owned()),
            code: Some(String::from("cat")),
            system: Some(ANIMALS.to_owned()),
            display_language: Some(String::from(language)),
            ..ValueSetValidateInput::default()
        };
        let error = value_set_validate_code::validate_code(&world.sources(), &validate_request)
            .expect_err(language);
        assert_eq!(
            error.to_string(),
            format!(
                "Invalid displayLanguage: '{}'",
                language.split(',').find(|l| *l != "de").unwrap_or(language)
            )
        );
    }
    // Valid ranges, quality values, and the wildcard pass.
    for language in ["en", "de-AT", "nl, en;q=0.5", "*", "zh-Hant-TW"] {
        let request = ExpandInput {
            url: Some(VS_PETS.to_owned()),
            display_language: Some(String::from(language)),
            ..ExpandInput::default()
        };
        expand::expand(&world.sources(), &request).expect(language);
    }
}

/// A designation the concept accepts is not always one to suggest.
///
/// A designation whose `use` is a project's own code, with no language, is a
/// term the concept carries for another purpose: it validates a display that
/// matches it, and the message that lists the valid displays leaves it out.
/// No FHIR version fixes this, and the terminology ecosystem IG says the
/// behaviour "is effectively specified by the test cases"
/// (<https://hl7.org/fhir/uv/tx-ecosystem/languages.html>), which is where the
/// rule comes from (#290).
#[test]
fn a_designation_with_its_own_use_validates_a_display_without_being_offered() {
    let world = World::load();
    let fish = |display: &str| ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("fish")),
        system: Some(ANIMALS.to_owned()),
        display: Some(String::from(display)),
        ..ValueSetValidateInput::default()
    };
    let run = |input: &ValueSetValidateInput| {
        value_set_validate_code::validate_code(&world.sources(), input).expect("validates")
    };
    assert!(
        run(&fish("Old fish name")).result,
        "the designation still validates the display"
    );
    let wrong = run(&fish("Trout"));
    assert!(!wrong.result);
    let display_issue = wrong
        .issues
        .iter()
        .find(|issue| issue.text.starts_with("Wrong Display Name"))
        .expect("the display issue");
    assert_eq!(
        display_issue.text,
        format!(
            "Wrong Display Name 'Trout' for {ANIMALS}#fish. Valid display is 'Fish' (en) (for the language(s) '--')"
        ),
        "the suggestion names the display alone"
    );
}

// NOTE: `codeableConcept` is echoed as "a codeableConcept containing codings for
// all the validated codes", so the `text` the request sent stays on it
// (<https://hl7.org/fhir/R5/valueset-operation-validate-code.html>).
#[test]
fn the_codeable_concept_echo_keeps_the_text_the_request_sent() {
    let world = World::load();
    let input = ValueSetValidateInput {
        url: Some(VS_ENUMERATED.to_owned()),
        codeable_concept: Some(CodeableConceptRef {
            coding: vec![CodingRef {
                system: Some(ANIMALS.to_owned()),
                code: Some(String::from("cat")),
                ..CodingRef::default()
            }],
            text: Some(String::from("some plain text")),
        }),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(validation.result, "{validation:?}");
    let echoed = validation.codeable_concept.expect("the echo");
    assert_eq!(echoed.text.as_deref(), Some("some plain text"));
    assert_eq!(echoed.coding.len(), 1);
}
