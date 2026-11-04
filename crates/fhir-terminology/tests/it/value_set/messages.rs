// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `$validate-code`'s messages, issues, standing notes and refusals.

use ferroterm_testkit::fhir::{ANIMALS, COLOURS, VS_ALL, VS_COLOURS, VS_ENUMERATED, VS_PETS};
use fhir_terminology::fhir_codesystem::load::FhirVersion;
use fhir_terminology::operations::expand::ExpandInput;
use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;
use fhir_terminology::operations::{CodingRef, Issue};
use fhir_terminology::operations::{OperationError, expand, value_set_validate_code};
use fhir_terminology::valueset;
use fhir_types::r4b::extension::{Extension, ExtensionValue};
use fhir_types::r4b::value_set::{
    ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
};

use super::{TWO_VERSION, VS_BOTH_VERSIONS, World, two_version_world};

#[test]
fn an_unknown_filter_operator_is_refused_at_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("ValueSet-bad.json"),
        serde_json::json!({
            "resourceType": "ValueSet", "url": "http://example.org/bad", "status": "active",
            "compose": {"include": [{"system": ANIMALS, "filter": [{"property": "concept", "op": "equals", "value": "cat"}]}]}
        })
        .to_string(),
    )
    .expect("writes");
    let error = valueset::load::load_dir(dir.path(), FhirVersion::R4B).expect_err("refused");
    assert!(error.to_string().contains("ValueSet-bad.json"), "{error}");
}

#[test]
fn validate_code_answers_the_membership_the_echo_and_the_issues() {
    let world = World::load();
    let good = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &good).expect("validates");
    assert!(validation.result);
    assert_eq!(validation.display.as_deref(), Some("Kitten"));
    assert_eq!(validation.system.as_deref(), Some(ANIMALS));
    assert_eq!(validation.version.as_deref(), Some("2.0"));
    assert_eq!(validation.code.as_deref(), Some("kitten"));
    assert_eq!(validation.issues, Vec::<Issue>::new());
    let inferred = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        ..ValueSetValidateInput::default()
    };
    // "If a code is provided, a system or a context must be provided"
    // (<https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>, `code`).
    assert!(
        matches!(
            value_set_validate_code::validate_code(&world.sources(), &inferred),
            Err(OperationError::Invalid(_))
        ),
        "a bare code with no system and no inferSystem is a request error"
    );
    let outside = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        coding: Some(CodingRef {
            system: Some(ANIMALS.to_owned()),
            code: Some(String::from("dog")),
            ..CodingRef::default()
        }),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &outside).expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.issues.len(), 1);
    assert_eq!(validation.issues[0].kind, "not-in-vs");
    assert_eq!(validation.issues[0].code, "code-invalid");
    assert_eq!(
        validation.code.as_deref(),
        Some("dog"),
        "a valid code outside the set is still echoed"
    );
    let unknown_code = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("unicorn")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &unknown_code).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "invalid-code"]);
}

#[test]
fn validate_code_refuses_malformed_requests() {
    let world = World::load();
    let none = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        ..ValueSetValidateInput::default()
    };
    assert!(matches!(
        value_set_validate_code::validate_code(&world.sources(), &none),
        Err(OperationError::Invalid(_))
    ));
    let two = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("cat")),
        coding: Some(CodingRef::default()),
        ..ValueSetValidateInput::default()
    };
    assert!(matches!(
        value_set_validate_code::validate_code(&world.sources(), &two),
        Err(OperationError::Invalid(_))
    ));
    let no_value_set = ValueSetValidateInput {
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    assert!(matches!(
        value_set_validate_code::validate_code(&world.sources(), &no_value_set),
        Err(OperationError::Required(_))
    ));
    let unknown = ValueSetValidateInput {
        url: Some(String::from("http://example.org/fhir/ValueSet/nowhere")),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    assert!(matches!(
        value_set_validate_code::validate_code(&world.sources(), &unknown),
        Err(OperationError::UnknownValueSet(_))
    ));
    let ambiguous = ValueSetValidateInput {
        url: Some(VS_ENUMERATED.to_owned()),
        code: Some(String::from("cat")),
        ..ValueSetValidateInput::default()
    };
    // A bare code needs a system, or `inferSystem` to infer one
    // (<https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>, `code`).
    assert!(matches!(
        value_set_validate_code::validate_code(&world.sources(), &ambiguous),
        Err(OperationError::Invalid(_))
    ));
}

// NOTE: an include naming a version the server does not serve fails the
// validation as an invalid value set naming the system (the ecosystem's test
// cases, `version/coding-v10-vs1wb`); `$expand` keeps refusing it.
#[test]
fn validate_code_fails_over_an_include_the_server_cannot_resolve_and_names_the_system() {
    let world = World::load();
    let inline = ValueSet {
        url: Some("http://example.org/inline-bad-version".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                version: Some("9".into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let input = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("a false result");
    assert!(!validation.result);
    // NOTE: without a subject version only the include's unknown version is itemised
    // (the ecosystem's `coding-vnn-vs1wb`); with one, the disagreement comes first.
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-found"]);
    assert_eq!(validation.unknown_systems, [format!("{ANIMALS}|9")]);
    assert_eq!(validation.code.as_deref(), Some("kitten"));
    assert_eq!(validation.display.as_deref(), Some("Kitten"));
    assert_eq!(
        validation.version.as_deref(),
        Some("2.0"),
        "the served default answers"
    );
    let with_version = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        system_version: Some(String::from("2.0")),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &with_version)
        .expect("a false result");
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["vs-invalid", "not-found"]);
    assert_eq!(
        validation.version.as_deref(),
        Some("2.0"),
        "the subject's served version"
    );
    let expand = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&world.sources(), &expand),
        Err(OperationError::UnknownVersion { .. })
    ));
    // A wildcard include version names the greatest match.
    let wild = ValueSet {
        url: Some("http://example.org/inline-wild".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                version: Some("2.x".into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let input = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&wild)),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(validation.result);
    assert_eq!(validation.version.as_deref(), Some("2.0"));
}

#[test]
fn validate_code_speaks_the_ecosystems_words_for_codes_membership_abstract_and_case() {
    let world = World::load();
    let run = |input: &ValueSetValidateInput| {
        value_set_validate_code::validate_code(&world.sources(), input).expect("validates")
    };
    // An unknown code, and a code outside the value set with its asserted display.
    let unknown = run(&ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("unicorn")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    });
    let kinds: Vec<(&str, String)> = unknown
        .issues
        .iter()
        .map(|i| (i.kind, i.text.clone()))
        .collect();
    assert_eq!(
        kinds,
        [
            (
                "not-in-vs",
                format!(
                    "The provided code '{ANIMALS}#unicorn' was not found in the value set '{VS_ALL}|2.0'"
                )
            ),
            (
                "invalid-code",
                format!("Unknown code 'unicorn' in the CodeSystem '{ANIMALS}' version '2.0'")
            ),
        ]
    );
    let outside = run(&ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("dog")),
        system: Some(ANIMALS.to_owned()),
        display: Some(String::from("Doggo")),
        ..ValueSetValidateInput::default()
    });
    assert_eq!(
        outside.issues[0].text,
        format!(
            "The provided code '{ANIMALS}#dog ('Doggo')' was not found in the value set '{VS_PETS}|1.0'"
        )
    );
    // An abstract code refused: the rule, then the membership, the rule as message.
    let abstract_code = run(&ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("living")),
        system: Some(ANIMALS.to_owned()),
        abstract_ok: Some(false),
        ..ValueSetValidateInput::default()
    });
    let kinds: Vec<&str> = abstract_code.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["code-rule", "not-in-vs"]);
    assert_eq!(
        abstract_code.message.as_deref(),
        Some(
            format!(
                "Code '{ANIMALS}#living' is abstract, and not allowed in this context; The provided code '{ANIMALS}#living' was not found in the value set '{VS_ALL}|2.0'"
            )
            .as_str()
        )
    );
    // A case-insensitive system located the code under another spelling: an information note.
    let cased = run(&ValueSetValidateInput {
        url: Some(VS_COLOURS.to_owned()),
        code: Some(String::from("red")),
        system: Some(COLOURS.to_owned()),
        ..ValueSetValidateInput::default()
    });
    assert!(cased.result, "{cased:?}");
    assert_eq!(cased.code.as_deref(), Some("red"));
    assert_eq!(cased.normalized_code.as_deref(), Some("RED"));
    assert_eq!(cased.issues[0].kind, "code-rule");
    assert_eq!(cased.issues[0].severity, "information");
    assert_eq!(
        cased.issues[0].text,
        format!(
            "The code 'red' differs from the correct code 'RED' by case. Although the code system '{COLOURS}' version '1' is case insensitive, implementers are strongly encouraged to use the correct case anyway"
        )
    );
}

// NOTE: an inactive concept validates with `inactive`, `status`, and a code-comment
// warning; a draft, experimental, deprecated, or withdrawn resource earns a
// status-check note (the ecosystem's requirements, #185).
#[test]
fn validate_code_reports_inactive_concepts_and_resource_standing() {
    let world = World::load();
    let run = |code: &str| {
        value_set_validate_code::validate_code(
            &world.sources(),
            &ValueSetValidateInput {
                url: Some(VS_ALL.to_owned()),
                value_set_version: Some(String::from("1.0")),
                coding: Some(CodingRef {
                    system: Some(ANIMALS.to_owned()),
                    version: None,
                    code: Some(code.to_owned()),
                    display: None,
                }),
                ..ValueSetValidateInput::default()
            },
        )
        .expect("validates")
    };
    let active = run("cat");
    assert_eq!((active.inactive, active.status.as_deref()), (None, None));
    assert!(active.issues.is_empty(), "{active:?}");
    let retired = run("fish");
    assert!(retired.result, "inactive is a warning");
    assert_eq!(retired.inactive, Some(true));
    assert_eq!(retired.status.as_deref(), Some("retired"));
    assert_eq!(retired.issues.len(), 1);
    assert_eq!(
        (retired.issues[0].severity, retired.issues[0].kind),
        ("warning", "code-comment")
    );
    assert_eq!(
        retired.issues[0].text,
        "The concept 'fish' has a status of retired and inactive and its use should be reviewed"
    );
    assert_eq!(retired.issues[0].expression.as_deref(), Some("Coding"));
    assert_eq!(
        retired.message.as_deref(),
        Some(retired.issues[0].text.as_str())
    );
    let flagged = run("dodo");
    assert_eq!(flagged.inactive, Some(true));
    assert_eq!(
        flagged.status, None,
        "no status property, only the inactive flag"
    );
    assert_eq!(
        flagged.issues[0].text,
        "The concept 'dodo' has a status of inactive and its use should be reviewed"
    );
}

#[test]
fn a_draft_experimental_deprecated_or_withdrawn_resource_earns_a_status_note() {
    use fhir_terminology::operations::standing_note;
    use fhir_terminology::provider::Standing;
    let world = World::load();
    let draft = Standing {
        status: String::from("draft"),
        ..Standing::default()
    };
    let note = standing_note("CodeSystem", "http://example.org/cs|1", &draft).expect("noted");
    assert_eq!((note.severity, note.kind), ("information", "status-check"));
    assert_eq!(
        note.text,
        "Reference to draft CodeSystem http://example.org/cs|1"
    );
    let experimental = Standing {
        experimental: true,
        ..Standing::default()
    };
    assert_eq!(
        standing_note("CodeSystem", "http://example.org/cs|1", &experimental)
            .expect("noted")
            .text,
        "Reference to experimental CodeSystem http://example.org/cs|1"
    );
    let deprecated = Standing {
        standards_status: Some(String::from("deprecated")),
        ..Standing::default()
    };
    assert_eq!(
        standing_note("CodeSystem", "http://example.org/cs|1", &deprecated)
            .expect("noted")
            .text,
        "Reference to deprecated CodeSystem http://example.org/cs|1"
    );
    assert!(standing_note("CodeSystem", "x", &Standing::default()).is_none());
    // A withdrawn value set is noted on every validation against it.
    let withdrawn = ValueSet {
        url: Some("http://example.org/inline-withdrawn".into()),
        version: Some("1".into()),
        status: "active".into(),
        extension: vec![Extension {
            url: "http://hl7.org/fhir/StructureDefinition/structuredefinition-standards-status"
                .into(),
            value: Some(ExtensionValue::Code("withdrawn".into())),
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
    let noted = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            inline_value_set: Some(valueset::convert::r4b::convert(&withdrawn)),
            code: Some(String::from("cat")),
            system: Some(ANIMALS.to_owned()),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("validates");
    assert!(noted.result);
    let shape: Vec<(&str, &str, &str)> = noted
        .issues
        .iter()
        .map(|i| (i.severity, i.kind, i.text.as_str()))
        .collect();
    assert_eq!(
        shape,
        [(
            "information",
            "status-check",
            "Reference to withdrawn ValueSet http://example.org/inline-withdrawn|1"
        )]
    );
    assert_eq!(noted.message, None, "notes carry no message");
}

// NOTE: every issue carries the ecosystem's message id, the message joins the
// errors in the ecosystem's order, and an unknown system fails with the
// membership issue first (the ecosystem's test cases, #189).
#[test]
fn validate_code_names_its_messages_and_shapes_the_unknown_system_answer() {
    let world = World::load();
    let unknown = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            url: Some(VS_ALL.to_owned()),
            code: Some(String::from("x")),
            system: Some(String::from("http://example.org/fhir/CodeSystem/nowhere")),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("a false result");
    let shape: Vec<(&str, &str, Option<&str>)> = unknown
        .issues
        .iter()
        .map(|i| (i.kind, i.message_id(), i.expression.as_deref()))
        .collect();
    assert_eq!(
        shape,
        [
            (
                "not-in-vs",
                "None_of_the_provided_codes_are_in_the_value_set_one",
                Some("code")
            ),
            ("not-found", "UNKNOWN_CODESYSTEM", Some("system")),
        ]
    );
    assert_eq!(
        unknown.x_unknown_systems,
        ["http://example.org/fhir/CodeSystem/nowhere"]
    );
    assert!(
        unknown.unknown_systems.is_empty(),
        "the value set never named the system"
    );
    assert!(
        unknown
            .message
            .as_deref()
            .unwrap()
            .starts_with("A definition for CodeSystem"),
        "{:?}",
        unknown.message
    );
    // A system the value set does name is caused by the value set.
    let inline = ValueSet {
        url: Some("http://example.org/inline-unknown".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some("http://example.org/fhir/CodeSystem/nowhere".into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let caused = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
            code: Some(String::from("x")),
            system: Some(String::from("http://example.org/fhir/CodeSystem/nowhere")),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("a false result");
    assert_eq!(
        caused.unknown_systems,
        ["http://example.org/fhir/CodeSystem/nowhere"]
    );
    assert_eq!(caused.x_unknown_systems, Vec::<String>::new());
}

#[test]
fn the_message_joins_the_errors_in_the_ecosystems_order() {
    let world = World::load();
    let pinned = ValueSet {
        url: Some("http://example.org/inline-pinned".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                version: Some("2.0".into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let checked = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            inline_value_set: Some(valueset::convert::r4b::convert(&pinned)),
            coding: Some(CodingRef {
                system: Some(ANIMALS.to_owned()),
                version: Some(String::from("9.9")),
                code: Some(String::from("cat")),
                display: None,
            }),
            check_system_version: vec![format!("{ANIMALS}|1.x")],
            ..ValueSetValidateInput::default()
        },
    )
    .expect("a false result");
    let ids: Vec<&str> = checked.issues.iter().map(Issue::message_id).collect();
    assert_eq!(
        ids,
        [
            "VALUESET_VERSION_CHECK",
            "VALUESET_VALUE_MISMATCH",
            "UNKNOWN_CODESYSTEM_VERSION"
        ]
    );
    let message = checked.message.as_deref().unwrap();
    let order = [
        message.find("A definition for CodeSystem").unwrap(),
        message.find("The code system").unwrap(),
        message.find("The version").unwrap(),
    ];
    assert!(order[0] < order[1] && order[1] < order[2], "{message}");
}

#[test]
fn validate_code_names_a_value_set_used_as_a_system_as_bad_data() {
    let world = World::load();
    let input = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(VS_ALL.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "invalid-data"]);
    assert_eq!(
        validation.message.as_deref(),
        Some(
            format!(
                "The Coding references a value set, not a code system ('{VS_ALL}'); The provided code '{VS_ALL}#kitten' was not found in the value set '{VS_PETS}|1.0'"
            )
            .as_str()
        )
    );
    assert!(validation.x_unknown_systems.is_empty(), "no missing system");
    assert_eq!(validation.unknown_systems, Vec::<String>::new());
}

#[test]
fn validate_code_notes_a_concept_the_value_set_deprecates_without_a_message() {
    let world = World::load();
    let inline = ValueSet {
        url: Some("http://example.org/deprecating".into()),
        version: Some("3".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                concept: vec![ValueSetComposeIncludeConcept {
                    code: "cat".into(),
                    extension: vec![Extension {
                        url: "http://hl7.org/fhir/StructureDefinition/valueset-deprecated".into(),
                        value: Some(ExtensionValue::Boolean(true.into())),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let input = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(validation.result);
    let note = validation
        .issues
        .iter()
        .find(|i| i.kind == "code-comment")
        .expect("the deprecation note");
    assert_eq!(note.severity, "warning");
    assert_eq!(
        note.text,
        format!(
            "The presence of the concept 'cat' in the system '{ANIMALS}' in the value set http://example.org/deprecating|3 is marked with a status of deprecated and its use should be reviewed"
        )
    );
    assert_eq!(
        validation.message, None,
        "the note stays out of the message"
    );
}

#[test]
fn validate_code_refuses_an_inactive_concept_under_active_only_and_a_coding_without_a_system() {
    let world = World::load();
    // Version 1.0 of the value set keeps the dodo; 2.0 excludes it.
    let inactive = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        value_set_version: Some(String::from("1.0")),
        code: Some(String::from("dodo")),
        system: Some(ANIMALS.to_owned()),
        active_only: Some(true),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &inactive).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["code-rule", "not-in-vs", "code-comment"]);
    assert_eq!(validation.inactive, Some(true));
    assert_eq!(
        validation.message.as_deref(),
        Some(
            format!(
                "The concept 'dodo' has a status of inactive and its use should be reviewed; The concept 'dodo' is valid but is not active; The provided code '{ANIMALS}#dodo' was not found in the value set '{VS_ALL}|1.0'"
            )
            .as_str()
        )
    );
    let allowed = ValueSetValidateInput {
        active_only: None,
        ..inactive
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &allowed).expect("validates");
    assert!(
        validation.result,
        "inactive codes are members unless refused"
    );
    // A coding without a system is not inferred: the membership verdict and a
    // data warning, no system echo.
    let no_system = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        coding: Some(CodingRef {
            code: Some(String::from("cat")),
            ..CodingRef::default()
        }),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &no_system).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<(&str, &str)> = validation
        .issues
        .iter()
        .map(|i| (i.severity, i.kind))
        .collect();
    assert_eq!(kinds, [("error", "not-in-vs"), ("warning", "invalid-data")]);
    assert_eq!(validation.system, None);
    assert_eq!(validation.code.as_deref(), Some("cat"));
    assert_eq!(
        validation.message.as_deref(),
        Some(
            format!(
                "Coding has no system. A code with no system has no defined meaning, and it cannot be validated. A system should be provided; The provided code '#cat' was not found in the value set '{VS_ALL}|2.0'"
            )
            .as_str()
        )
    );
}

#[test]
fn validate_code_checks_membership_only_when_asked() {
    let world = World::load();
    let input = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("dodo")),
        system: Some(ANIMALS.to_owned()),
        value_set_version: Some(String::from("1.0")),
        display: Some(String::from("Wrong")),
        membership_only: Some(true),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(validation.result);
    assert!(
        validation.issues.is_empty(),
        "no display, case, or status notes: {:?}",
        validation.issues
    );
    assert_eq!(validation.inactive, Some(true), "the outputs still tell");
    let checked = ValueSetValidateInput {
        membership_only: None,
        ..input
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &checked).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert!(kinds.contains(&"invalid-display"), "{kinds:?}");
}
// NOTE: under `valueset-membership-only` the server performs no "validation tasks
// beyond validating membership", so an unknown code carries the membership issue
// alone (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-validate-code.html>).
#[test]
fn membership_only_reports_membership_without_the_codes_standing_in_its_system() {
    let world = two_version_world();
    let input = |membership_only: Option<bool>| ValueSetValidateInput {
        url: Some(VS_BOTH_VERSIONS.to_owned()),
        coding: Some(CodingRef {
            system: Some(TWO_VERSION.to_owned()),
            code: Some(String::from("nosuch")),
            ..CodingRef::default()
        }),
        membership_only,
        ..ValueSetValidateInput::default()
    };
    let full =
        value_set_validate_code::validate_code(&world.sources(), &input(None)).expect("validates");
    let kinds: Vec<&str> = full.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "invalid-code"]);
    let membership = value_set_validate_code::validate_code(&world.sources(), &input(Some(true)))
        .expect("validates");
    assert!(!membership.result);
    let kinds: Vec<&str> = membership.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs"]);
}

#[test]
fn a_complete_resource_keeps_answering_an_absent_code_as_one_the_system_lacks() {
    // `complete` means "all the concepts defined by the code system are
    // included in the code system resource"
    // (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>), so absence
    // from the resource is absence from the system.
    let world = World::load();
    let input = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        system: Some(ANIMALS.to_owned()),
        code: Some(String::from("zebra")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(!validation.result, "{validation:?}");
    let severities: Vec<&str> = validation.issues.iter().map(|i| i.severity).collect();
    assert_eq!(severities, ["error", "error"]);
}
