// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The versions an expansion or a validation resolves, negotiates and names.

use ferroterm_testkit::fhir::{ANIMALS, COLOURS, VS_ALL, VS_PETS, VS_PETS_REF};
use fhir_terminology::operations::CodingRef;
use fhir_terminology::operations::expand::ExpandInput;
use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;
use fhir_terminology::operations::{OperationError, expand, value_set_validate_code};
use fhir_terminology::valueset;
use fhir_terminology::valueset::expansion::ParameterValue;
use fhir_types::r4b::value_set::{ValueSet, ValueSetCompose, ValueSetComposeInclude};

use super::{TWO_VERSION, VS_BOTH_VERSIONS, World, codeable, codes, two_version_world};

#[test]
fn the_store_loads_every_value_set_and_defaults_to_the_greatest_version() {
    let world = World::load();
    assert_eq!(world.value_sets.len(), 8);
    let default = world.value_sets.resolve(VS_ALL, None).expect("default");
    assert_eq!(default.version.as_deref(), Some("2.0"));
    let pinned = world
        .value_sets
        .resolve(&format!("{VS_ALL}|1.0"), None)
        .expect("1.0");
    assert_eq!(pinned.version.as_deref(), Some("1.0"));
    assert!(world.value_sets.resolve(VS_ALL, Some("3.0")).is_none());
}

#[test]
fn one_coding_answers_for_the_concept_when_the_include_pins_another_version() {
    let world = World::load();
    let inline = ValueSet {
        url: Some("http://example.org/inline/pinned".into()),
        status: "active".into(),
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
    let request = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        codeable_concept: Some(codeable(vec![CodingRef {
            system: Some(ANIMALS.to_owned()),
            version: Some(String::from("1.0")),
            code: Some(String::from("cat")),
            ..CodingRef::default()
        }])),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &request).expect("validates");
    assert!(!validation.result);
    // "the server SHALL return the code system and code against which
    // validation was based (`system` and `code` parameters)"
    // (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
    assert_eq!(validation.code.as_deref(), Some("cat"));
    assert_eq!(validation.system.as_deref(), Some(ANIMALS));
    assert_eq!(validation.version.as_deref(), Some("2.0"));
    assert!(
        validation.issues.iter().any(|i| i.kind == "vs-invalid"),
        "the version disagreement is the reason"
    );
    assert!(
        !validation.issues.iter().any(|i| i.kind == "not-in-vs"),
        "one coding needs no summary of the codings that failed"
    );
}

#[test]
fn expand_takes_an_inline_value_set_and_pins_system_versions() {
    let world = World::load();
    let inline = ValueSet {
        url: Some("http://example.org/inline".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(COLOURS.into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let request = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        include_definition: Some(true),
        system_version: vec![format!("{COLOURS}|1")],
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &request).expect("expands");
    assert_eq!(codes(&vs), ["Green", "RED", "blue"]);
    assert!(
        vs.include_definition,
        "includeDefinition returns the compose"
    );
    let both = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        url: Some(VS_ALL.to_owned()),
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&world.sources(), &both),
        Err(OperationError::Invalid(_))
    ));
    let wrong_version = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        check_system_version: vec![format!("{COLOURS}|9")],
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&world.sources(), &wrong_version),
        Err(OperationError::UnknownVersion { .. })
    ));
    let excluded = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        exclude_system: vec![COLOURS.to_owned()],
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &excluded).expect("expands");
    assert_eq!(codes(&vs), Vec::<String>::new());
}

#[test]
fn validate_code_names_an_unknown_system_and_an_unknown_version() {
    let world = World::load();
    let unknown_system = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(String::from("http://example.org/fhir/CodeSystem/nowhere")),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &unknown_system)
        .expect("validates");
    assert!(!validation.result);
    // NOTE: the membership issue leads, then the missing system, which the input
    // alone named (`x-unknown-system`), the ecosystem's shape (#189).
    assert_eq!(validation.issues[0].kind, "not-in-vs");
    assert_eq!(validation.issues[1].kind, "not-found");
    assert_eq!(validation.issues[1].code, "not-found");
    assert_eq!(
        validation.x_unknown_systems,
        ["http://example.org/fhir/CodeSystem/nowhere"],
        "x-unknown-system names the system"
    );
    assert_eq!(
        validation.code.as_deref(),
        Some("kitten"),
        "the code is echoed"
    );
    // NOTE: the message joins the missing system and the membership verdict, in
    // that order (the ecosystem's `bad-system` cases).
    assert_eq!(
        validation.message.as_deref(),
        Some(
            "A definition for CodeSystem 'http://example.org/fhir/CodeSystem/nowhere' could not be found, so the code cannot be validated; The provided code 'http://example.org/fhir/CodeSystem/nowhere#kitten' was not found in the value set 'http://example.org/fhir/ValueSet/pets|1.0'"
        )
    );
    let unknown_version = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        system_version: Some(String::from("9.9")),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &unknown_version)
        .expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.unknown_systems, [format!("{ANIMALS}|9.9")]);
    // NOTE: the code is still resolved against the version the value set uses, and
    // the versionless include's choice is a warning beside the not-found error
    // (the ecosystem's `version` cases, #177).
    assert_eq!(validation.version.as_deref(), Some("2.0"));
    assert_eq!(validation.display.as_deref(), Some("Kitten"));
    let shape: Vec<(&str, &str)> = validation
        .issues
        .iter()
        .map(|i| (i.severity, i.kind))
        .collect();
    assert_eq!(shape, [("error", "not-found"), ("warning", "vs-invalid")]);
    assert_eq!(validation.issues[0].expression.as_deref(), Some("system"));
    assert_eq!(validation.issues[1].expression.as_deref(), Some("version"));
    assert!(
        validation.issues[0].text.ends_with(". Valid versions: 2.0"),
        "{}",
        validation.issues[0].text
    );
    assert!(
        validation.issues[1]
            .text
            .contains("for the versionless include"),
        "{}",
        validation.issues[1].text
    );
}

// NOTE: a coding version that differs from the include's is validated against the
// include's version and itemised as vs-invalid; a check-system-version that
// forbids the include's version is a version-error, never a refusal (#177).
#[test]
fn validate_code_itemises_version_disagreements_against_the_value_set_version() {
    let world = World::load();
    let inline = |version: Option<&str>| ValueSet {
        url: Some("http://example.org/inline-pinned".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                version: version.map(Into::into),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let coding = |version: &str| CodingRef {
        system: Some(ANIMALS.to_owned()),
        version: Some(version.to_owned()),
        code: Some(String::from("cat")),
        display: None,
    };
    // An unserved coding version against a pinned include: vs-invalid, then not-found.
    let unserved = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline(Some("2.0")))),
        coding: Some(coding("9.9")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &unserved).expect("validates");
    let shape: Vec<(&str, &str)> = validation
        .issues
        .iter()
        .map(|i| (i.severity, i.kind))
        .collect();
    assert_eq!(shape, [("error", "vs-invalid"), ("error", "not-found")]);
    assert_eq!(validation.unknown_systems, [format!("{ANIMALS}|9.9")]);
    assert_eq!(validation.display.as_deref(), Some("Cat"));
    assert_eq!(
        validation.version.as_deref(),
        Some("2.0"),
        "the include's version answers"
    );
    assert_eq!(
        validation.issues[0].text,
        format!(
            "The code system '{ANIMALS}' version '2.0' in the ValueSet include is different to the one in the value ('9.9')"
        )
    );
    assert_eq!(
        validation.issues[0].expression.as_deref(),
        Some("Coding.version")
    );
    assert!(
        validation
            .message
            .as_deref()
            .unwrap()
            .starts_with("A definition for CodeSystem"),
        "the not-found text leads the message: {:?}",
        validation.message
    );
    // A check that forbids the include's version: a version-error, result false.
    let checked = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline(Some("2.0")))),
        coding: Some(coding("2.0")),
        check_system_version: vec![format!("{ANIMALS}|1.x")],
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &checked).expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.issues[0].kind, "version-error");
    assert_eq!(validation.issues[0].code, "exception");
    assert_eq!(
        validation.issues[0].text,
        format!(
            "The version '2.0' is not allowed for system '{ANIMALS}': required to be '1.x' by a version-check parameter"
        )
    );
    assert_eq!(
        validation.message.as_deref(),
        Some(validation.issues[0].text.as_str())
    );
    // On $expand the same check is refused as an exception.
    let expand = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline(Some("2.0")))),
        check_system_version: vec![format!("{ANIMALS}|1.x")],
        ..ExpandInput::default()
    };
    let error = expand::expand(&world.sources(), &expand).expect_err("refused");
    assert!(
        matches!(error, OperationError::VersionCheck(_)),
        "{error:?}"
    );
    assert_eq!(error.issue_code(), "exception");
    assert_eq!(error.tx_issue_type(), "version-error");
}

// NOTE: the value set version trio and the system trio negotiate the versions an
// operation touches, top-level and imported alike
// (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-expand.html>).
#[test]
fn expand_negotiates_value_set_versions_and_names_the_value_sets_it_used() {
    let world = World::load();
    let pinned = ExpandInput {
        url: Some(VS_PETS_REF.to_owned()),
        default_valueset_version: vec![format!("{VS_PETS}|1.0")],
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &pinned).expect("expands");
    let echoed: Vec<(&str, String)> = vs
        .parameters
        .iter()
        .filter_map(|p| match &p.value {
            ParameterValue::Uri(u) => Some((p.name.as_str(), u.clone())),
            _ => None,
        })
        .collect();
    assert!(echoed.contains(&("default-valueset-version", format!("{VS_PETS}|1.0"))));
    assert!(
        echoed.contains(&("used-valueset", format!("{VS_PETS}|1.0"))),
        "{echoed:?}"
    );
    let missing = ExpandInput {
        url: Some(VS_PETS_REF.to_owned()),
        default_valueset_version: vec![format!("{VS_PETS}|9.9")],
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&world.sources(), &missing),
        Err(OperationError::UnknownImport(url)) if url == format!("{VS_PETS}|9.9")
    ));
    let forced = ExpandInput {
        url: Some(VS_ALL.to_owned()),
        force_valueset_version: vec![format!("{VS_ALL}|1.0")],
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &forced).expect("expands");
    assert!(
        codes(&vs).iter().any(|c| c == "dodo"),
        "1.0 has no exclude, so the forced version keeps the dodo: {:?}",
        codes(&vs)
    );
    let checked = ExpandInput {
        url: Some(VS_ALL.to_owned()),
        value_set_version: Some(String::from("2.0")),
        check_valueset_version: vec![format!("{VS_ALL}|1.0")],
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&world.sources(), &checked),
        Err(OperationError::Invalid(_))
    ));
}

#[test]
fn validate_code_negotiates_system_and_value_set_versions() {
    let world = World::load();
    let defaulted = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        default_system_version: vec![format!("{ANIMALS}|2.0")],
        check_system_version: vec![format!("{ANIMALS}|2.0")],
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &defaulted).expect("validates");
    assert!(validation.result);
    assert_eq!(validation.version.as_deref(), Some("2.0"));
    let forced_away = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        force_system_version: vec![format!("{ANIMALS}|9.9")],
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &forced_away)
        .expect("a false result");
    assert!(!validation.result);
    assert_eq!(validation.unknown_systems, [format!("{ANIMALS}|9.9")]);
    let mismatch = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        coding: Some(CodingRef {
            system: Some(ANIMALS.to_owned()),
            version: Some(String::from("2.0")),
            code: Some(String::from("kitten")),
            display: None,
        }),
        check_system_version: vec![format!("{ANIMALS}|1.0")],
        ..ValueSetValidateInput::default()
    };
    // NOTE: on a versionless include the check acts as the default, so a version
    // the server lacks is the unresolvable-include shape, never a refusal (#177).
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &mismatch).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["vs-invalid", "not-found"]);
    assert_eq!(validation.unknown_systems, [format!("{ANIMALS}|1.0")]);
    let imported = ValueSetValidateInput {
        url: Some(VS_PETS_REF.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        default_valueset_version: vec![format!("{VS_PETS}|1.0")],
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &imported).expect("validates");
    assert!(validation.result);
    let unknown_import = ValueSetValidateInput {
        url: Some(VS_PETS_REF.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        default_valueset_version: vec![format!("{VS_PETS}|9.9")],
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &unknown_import)
        .expect("a false result, not an error");
    assert!(!validation.result);
    assert_eq!(validation.issues[0].kind, "not-found");
    assert_eq!(
        validation.message.as_deref(),
        Some(format!("A definition for the value Set '{VS_PETS}|9.9' could not be found").as_str())
    );
    assert_eq!(validation.code.as_deref(), Some("kitten"));
}

#[test]
fn validate_code_fails_a_codeable_concept_on_an_include_version_the_server_lacks() {
    let world = World::load();
    let inline = ValueSet {
        url: Some("http://example.org/pinned-badly".into()),
        version: Some("1".into()),
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
        codeable_concept: Some(codeable(vec![CodingRef {
            system: Some(ANIMALS.to_owned()),
            code: Some(String::from("cat")),
            ..CodingRef::default()
        }])),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-found"], "the concept fails on that alone");
    assert_eq!(validation.code, None);
    assert_eq!(validation.system, None);
    assert_eq!(validation.display.as_deref(), Some("Cat"));
    assert_eq!(validation.version.as_deref(), Some("2.0"));
    assert_eq!(validation.unknown_systems, [format!("{ANIMALS}|9")]);
    assert_eq!(validation.message, Some(validation.issues[0].text.clone()));
}

// NOTE: `version` is "the version of the system of the code that was validated",
// so a value set including one system twice picks the version the coding is valid
// in (<https://hl7.org/fhir/R5/valueset-operation-validate-code.html>).
#[test]
fn validate_code_picks_the_included_version_the_display_is_valid_in() {
    let world = two_version_world();
    let coding = |display: Option<&str>| CodingRef {
        system: Some(TWO_VERSION.to_owned()),
        version: None,
        code: Some(String::from("code2")),
        display: display.map(str::to_owned),
    };
    let run = |coding: CodingRef| {
        let input = ValueSetValidateInput {
            url: Some(VS_BOTH_VERSIONS.to_owned()),
            coding: Some(coding),
            ..ValueSetValidateInput::default()
        };
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates")
    };
    let first = run(coding(Some("Display 2")));
    assert!(first.result, "{first:?}");
    assert_eq!(first.version.as_deref(), Some("1.0.0"));
    assert_eq!(first.display.as_deref(), Some("Display 2"));
    let second = run(coding(Some("Display #2")));
    assert!(second.result, "{second:?}");
    assert_eq!(second.version.as_deref(), Some("2.0.0"));
    assert_eq!(second.display.as_deref(), Some("Display #2"));
    let none = run(coding(None));
    assert!(none.result, "{none:?}");
    assert_eq!(
        none.version.as_deref(),
        Some("2.0.0"),
        "with no display asserted the greatest included version answers"
    );
}

// NOTE: the membership issue names the code the way the request stated it, the
// version included, which the ecosystem requires of every answer
// (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
#[test]
fn an_unknown_code_at_an_asserted_version_names_that_version() {
    let world = two_version_world();
    let input = ValueSetValidateInput {
        url: Some(VS_BOTH_VERSIONS.to_owned()),
        coding: Some(CodingRef {
            system: Some(TWO_VERSION.to_owned()),
            version: Some(String::from("1.0.0")),
            code: Some(String::from("code4")),
            display: None,
        }),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "invalid-code"]);
    assert_eq!(
        validation.issues[0].text,
        format!(
            "The provided code '{TWO_VERSION}|1.0.0#code4' was not found in the value set '{VS_BOTH_VERSIONS}|1.0'"
        )
    );
}
