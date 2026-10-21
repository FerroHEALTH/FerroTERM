// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The three R4B operations over the synthetic provider: every parameter
//! branch the R4B definitions admit, and every refusal.

use std::sync::Arc;

use fhir_terminology::operations::lookup::{LookupInput, lookup};
use fhir_terminology::operations::subsumes::{SubsumesInput, subsumes};
use fhir_terminology::operations::validate_code::{ValidateCodeInput, validate_code};
use fhir_terminology::operations::{CodeableConceptRef, CodingRef, Invocation, OperationError};
use fhir_terminology::provider::PropertyValue;
use fhir_terminology::registry::Registry;
use http::StatusCode;

use crate::fixture::{FLAT_URL, URL, registry};

/// A codeable concept of `coding`, with no text.
fn codeable(coding: Vec<CodingRef>) -> CodeableConceptRef {
    CodeableConceptRef { coding, text: None }
}

/// A coding as the engine names one.
fn coding_ref(system: Option<&str>, code: &str) -> CodingRef {
    CodingRef {
        system: system.map(str::to_owned),
        code: Some(code.to_owned()),
        ..CodingRef::default()
    }
}

fn instance(registry: &Registry, url: &str) -> Invocation {
    Invocation::Instance(registry.resolve(url, None).expect("resolves"))
}

// ---- $lookup ------------------------------------------------------------

#[test]
fn lookup_by_system_and_code_returns_name_version_display_designations_properties() {
    let registry = registry();
    let input = LookupInput {
        system: Some(URL.to_owned()),
        code: Some(String::from("cat")),
        ..LookupInput::default()
    };
    let outcome = lookup(&registry, &Invocation::Type, &input).expect("looks up");
    assert_eq!(outcome.name, "Fixture");
    assert_eq!(outcome.version.as_deref(), Some("2025"));
    assert_eq!(outcome.display, "Cat");
    assert_eq!(outcome.code, "cat");
    assert_eq!(outcome.system, URL);
    assert!(!outcome.abstract_concept);
    let languages: Vec<&str> = outcome
        .designations
        .iter()
        .filter_map(|d| d.language.as_deref())
        .collect();
    assert_eq!(languages, ["en", "nl"]);
    let codes: Vec<&str> = outcome.properties.iter().map(|p| p.code.as_str()).collect();
    assert_eq!(codes, ["legs", "kingdom"]);
    assert_eq!(outcome.properties[0].value, PropertyValue::Integer(4));
}

#[test]
fn lookup_by_coding_with_version_and_display_language() {
    let registry = registry();
    let input = LookupInput {
        coding: Some(CodingRef {
            system: Some(URL.to_owned()),
            version: Some(String::from("2024")),
            code: Some(String::from("dog")),
            display: None,
        }),
        display_language: Some(String::from("nl")),
        ..LookupInput::default()
    };
    let outcome = lookup(&registry, &Invocation::Type, &input).expect("looks up");
    assert_eq!(outcome.version.as_deref(), Some("2024"));
    assert_eq!(outcome.display, "Hond");
}

#[test]
fn lookup_property_selects_properties_and_lang_x_selects_designations() {
    let registry = registry();
    let input = LookupInput {
        system: Some(URL.to_owned()),
        code: Some(String::from("cat")),
        properties: vec![
            String::from("kingdom"),
            String::from("lang.nl"),
            String::from("display"),
        ],
        ..LookupInput::default()
    };
    let outcome = lookup(&registry, &Invocation::Type, &input).expect("looks up");
    let codes: Vec<&str> = outcome.properties.iter().map(|p| p.code.as_str()).collect();
    assert_eq!(
        codes,
        ["kingdom"],
        "display is a named parameter, legs was not asked"
    );
    assert_eq!(outcome.designations.len(), 1);
    assert_eq!(outcome.designations[0].value, "Kat");
}

#[test]
fn lookup_property_star_asks_for_every_property_and_the_designations() {
    let registry = registry();
    let input = LookupInput {
        system: Some(URL.to_owned()),
        code: Some(String::from("cat")),
        properties: vec![String::from("*")],
        ..LookupInput::default()
    };
    let outcome = lookup(&registry, &Invocation::Type, &input).expect("looks up");
    let codes: Vec<&str> = outcome.properties.iter().map(|p| p.code.as_str()).collect();
    assert_eq!(codes, ["legs", "kingdom"]);
    let values: Vec<&str> = outcome
        .designations
        .iter()
        .map(|d| d.value.as_str())
        .collect();
    assert_eq!(
        values,
        ["Cat", "Kat"],
        "the display is already a designation, so it is not repeated"
    );
}

#[test]
fn lookup_naming_other_properties_only_leaves_the_designations_out() {
    let registry = registry();
    let input = LookupInput {
        system: Some(URL.to_owned()),
        code: Some(String::from("cat")),
        properties: vec![String::from("legs")],
        ..LookupInput::default()
    };
    let outcome = lookup(&registry, &Invocation::Type, &input).expect("looks up");
    let codes: Vec<&str> = outcome.properties.iter().map(|p| p.code.as_str()).collect();
    assert_eq!(codes, ["legs"]);
    assert_eq!(
        outcome.designations,
        Vec::<fhir_terminology::provider::Designation>::new()
    );
    let input = LookupInput {
        properties: vec![String::from("designation")],
        ..input
    };
    let outcome = lookup(&registry, &Invocation::Type, &input).expect("looks up");
    assert_eq!(
        outcome.properties,
        Vec::<fhir_terminology::provider::Property>::new()
    );
    assert_eq!(outcome.designations.len(), 2);
}

#[test]
fn lookup_refusals_carry_their_issue_code_and_status() {
    let registry = registry();
    let run = |input: LookupInput| lookup(&registry, &Invocation::Type, &input).err();
    // Nothing named.
    let error = run(LookupInput::default()).expect("refused");
    assert!(matches!(error, OperationError::Required(_)));
    assert_eq!(
        (error.issue_code(), error.status()),
        ("required", StatusCode::BAD_REQUEST)
    );
    // A code without a system.
    let error = run(LookupInput {
        code: Some(String::from("cat")),
        ..LookupInput::default()
    })
    .expect("refused");
    assert!(matches!(error, OperationError::Required(_)));
    // Both forms at once.
    let error = run(LookupInput {
        code: Some(String::from("cat")),
        system: Some(URL.to_owned()),
        coding: Some(coding_ref(Some(URL), "cat")),
        ..LookupInput::default()
    })
    .expect("refused");
    assert!(matches!(error, OperationError::Invalid(_)));
    // An unknown code.
    let error = run(LookupInput {
        code: Some(String::from("unicorn")),
        system: Some(URL.to_owned()),
        ..LookupInput::default()
    })
    .expect("refused");
    assert!(matches!(error, OperationError::UnknownCode { ref code, .. } if code == "unicorn"));
    // The code "was not valid in the context"; the unknown system below is a
    // reference that was not found
    // (<https://hl7.org/fhir/R4B/valueset-issue-type.html>).
    assert_eq!(
        (error.issue_code(), error.status()),
        ("code-invalid", StatusCode::BAD_REQUEST)
    );
    // An unknown system.
    let error = run(LookupInput {
        code: Some(String::from("cat")),
        system: Some(String::from("http://example.org/nowhere")),
        ..LookupInput::default()
    })
    .expect("refused");
    assert_eq!(
        (error.issue_code(), error.status()),
        ("not-found", StatusCode::NOT_FOUND)
    );
    // An unknown version.
    let error = run(LookupInput {
        code: Some(String::from("cat")),
        system: Some(URL.to_owned()),
        version: Some(String::from("1999")),
        ..LookupInput::default()
    })
    .expect("refused");
    assert!(matches!(error, OperationError::UnknownVersion { .. }));
    // The instance is the system, so a request naming another one disagrees
    // with it (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>).
    let error = lookup(
        &registry,
        &instance(&registry, URL),
        &LookupInput {
            code: Some(String::from("cat")),
            system: Some(String::from("http://example.org/nowhere")),
            ..LookupInput::default()
        },
    )
    .expect_err("refused");
    assert!(matches!(error, OperationError::Invalid(_)));
    assert_eq!(error.issue_code(), "invalid");
}

#[test]
fn lookup_on_an_instance_reads_the_instance_system() {
    // R5 and the R6 ballot declare `$lookup` at the instance level, where the
    // instance names the code system
    // (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>).
    let registry = registry();
    let outcome = lookup(
        &registry,
        &instance(&registry, URL),
        &LookupInput {
            code: Some(String::from("cat")),
            ..LookupInput::default()
        },
    )
    .expect("looks up");
    assert_eq!(outcome.system, URL);
    assert_eq!(outcome.display, "Cat");
}

// ---- $validate-code -------------------------------------------------------

#[test]
fn validate_code_by_code_coding_and_codeable_concept() {
    let registry = registry();
    let by_code = validate_code(
        &registry,
        &Invocation::Type,
        &ValidateCodeInput {
            url: Some(URL.to_owned()),
            code: Some(String::from("cat")),
            ..ValidateCodeInput::default()
        },
    )
    .expect("validates");
    assert!(by_code.result);
    assert!(by_code.message.is_none());
    assert_eq!(by_code.display.as_deref(), Some("Cat"));
    assert_eq!(by_code.code.as_deref(), Some("cat"));
    assert_eq!(by_code.system.as_deref(), Some(URL));
    assert_eq!(by_code.version.as_deref(), Some("2025"));

    let by_coding = validate_code(
        &registry,
        &Invocation::Type,
        &ValidateCodeInput {
            coding: Some(coding_ref(Some(URL), "dog")),
            ..ValidateCodeInput::default()
        },
    )
    .expect("validates");
    assert!(by_coding.result);

    // A CodeableConcept: codings of other systems are skipped, one valid
    // coding of the system makes the result true.
    let concept = vec![
        coding_ref(Some("http://loinc.org"), "1234-5"),
        coding_ref(Some(URL), "unicorn"),
        coding_ref(Some(URL), "cat"),
    ];
    let by_concept = validate_code(
        &registry,
        &Invocation::Type,
        &ValidateCodeInput {
            url: Some(URL.to_owned()),
            codeable_concept: Some(codeable(concept)),
            ..ValidateCodeInput::default()
        },
    )
    .expect("validates");
    assert!(by_concept.result);
    let none = vec![coding_ref(Some("http://loinc.org"), "1234-5")];
    let by_none = validate_code(
        &registry,
        &Invocation::Type,
        &ValidateCodeInput {
            url: Some(URL.to_owned()),
            codeable_concept: Some(codeable(none)),
            ..ValidateCodeInput::default()
        },
    )
    .expect("validates");
    assert!(!by_none.result);
}

#[test]
fn validate_code_wrong_display_unknown_code_and_inactive_code() {
    let registry = registry();
    let wrong = validate_code(
        &registry,
        &Invocation::Type,
        &ValidateCodeInput {
            url: Some(URL.to_owned()),
            code: Some(String::from("cat")),
            display: Some(String::from("Dog")),
            ..ValidateCodeInput::default()
        },
    )
    .expect("validates");
    assert!(!wrong.result);
    assert!(
        wrong
            .message
            .as_deref()
            .is_some_and(|m| m.contains("display"))
    );
    assert_eq!(
        wrong.display.as_deref(),
        Some("Cat"),
        "the correct display is returned"
    );
    let dutch = validate_code(
        &registry,
        &Invocation::Type,
        &ValidateCodeInput {
            url: Some(URL.to_owned()),
            code: Some(String::from("cat")),
            display: Some(String::from("Kat")),
            ..ValidateCodeInput::default()
        },
    )
    .expect("validates");
    assert!(
        dutch.result,
        "a designation in another language is a valid display"
    );
    let unknown = validate_code(
        &registry,
        &Invocation::Type,
        &ValidateCodeInput {
            url: Some(URL.to_owned()),
            code: Some(String::from("unicorn")),
            ..ValidateCodeInput::default()
        },
    )
    .expect("an invalid code is a false result, not an error");
    assert!(!unknown.result);
    assert!(unknown.display.is_none());
    let inactive = validate_code(
        &registry,
        &Invocation::Type,
        &ValidateCodeInput {
            url: Some(URL.to_owned()),
            code: Some(String::from("fish")),
            ..ValidateCodeInput::default()
        },
    )
    .expect("validates");
    assert!(inactive.result, "inactive is not invalid");
    assert!(
        inactive
            .message
            .as_deref()
            .is_some_and(|m| m.contains("inactive"))
    );
}

#[test]
fn validate_code_refusals() {
    let registry = registry();
    let run = |input: ValidateCodeInput| {
        validate_code(&registry, &Invocation::Type, &input).expect_err("refused")
    };
    // No code input at all.
    assert!(matches!(
        run(ValidateCodeInput {
            url: Some(URL.to_owned()),
            ..ValidateCodeInput::default()
        }),
        OperationError::Invalid(_)
    ));
    // Two code inputs.
    assert!(matches!(
        run(ValidateCodeInput {
            url: Some(URL.to_owned()),
            code: Some(String::from("cat")),
            coding: Some(coding_ref(Some(URL), "cat")),
            ..ValidateCodeInput::default()
        }),
        OperationError::Invalid(_)
    ));
    // A code without a system.
    assert!(matches!(
        run(ValidateCodeInput {
            code: Some(String::from("cat")),
            ..ValidateCodeInput::default()
        }),
        OperationError::Required(_)
    ));
    // A coding whose system contradicts the URL.
    assert!(matches!(
        run(ValidateCodeInput {
            url: Some(URL.to_owned()),
            coding: Some(coding_ref(Some("http://loinc.org"), "1")),
            ..ValidateCodeInput::default()
        }),
        OperationError::Invalid(_)
    ));
    // An inline code system.
    assert!(matches!(
        run(ValidateCodeInput {
            inline_code_system: true,
            code: Some(String::from("cat")),
            ..ValidateCodeInput::default()
        }),
        OperationError::NotSupported(_)
    ));
    // The instance level names the system; a contradicting URL is refused.
    let ok = validate_code(
        &registry,
        &instance(&registry, URL),
        &ValidateCodeInput {
            code: Some(String::from("cat")),
            ..ValidateCodeInput::default()
        },
    )
    .expect("validates");
    assert!(ok.result);
    let mismatch = validate_code(
        &registry,
        &instance(&registry, URL),
        &ValidateCodeInput {
            url: Some(FLAT_URL.to_owned()),
            code: Some(String::from("cat")),
            ..ValidateCodeInput::default()
        },
    )
    .expect_err("refused");
    assert!(matches!(mismatch, OperationError::Invalid(_)));
}

// ---- $subsumes ------------------------------------------------------------

#[test]
fn subsumes_outcomes_follow_the_closure() {
    let registry = registry();
    let run = |a: &str, b: &str| {
        subsumes(
            &registry,
            &Invocation::Type,
            &SubsumesInput {
                code_a: Some(a.to_owned()),
                code_b: Some(b.to_owned()),
                system: Some(URL.to_owned()),
                ..SubsumesInput::default()
            },
        )
        .expect("subsumes")
        .code()
    };
    assert_eq!(run("animal", "cat"), "subsumes");
    assert_eq!(run("cat", "animal"), "subsumed-by");
    assert_eq!(run("cat", "cat"), "equivalent");
    assert_eq!(run("cat", "dog"), "not-subsumed");
    let by_codings = subsumes(
        &registry,
        &Invocation::Type,
        &SubsumesInput {
            coding_a: Some(coding_ref(Some(URL), "root")),
            coding_b: Some(coding_ref(Some(URL), "kitten")),
            ..SubsumesInput::default()
        },
    )
    .expect("subsumes");
    assert_eq!(by_codings.code(), "subsumes");
    let on_instance = subsumes(
        &registry,
        &instance(&registry, URL),
        &SubsumesInput {
            code_a: Some(String::from("kitten")),
            code_b: Some(String::from("cat")),
            ..SubsumesInput::default()
        },
    )
    .expect("subsumes");
    assert_eq!(on_instance.code(), "subsumed-by");
}

#[test]
fn subsumes_refusals_are_errors_never_not_subsumed() {
    let registry = registry();
    let run =
        |input: SubsumesInput| subsumes(&registry, &Invocation::Type, &input).expect_err("refused");
    assert!(matches!(
        run(SubsumesInput::default()),
        OperationError::Required(_)
    ));
    // A code and a coding mixed.
    assert!(matches!(
        run(SubsumesInput {
            code_a: Some(String::from("cat")),
            coding_b: Some(coding_ref(Some(URL), "dog")),
            system: Some(URL.to_owned()),
            ..SubsumesInput::default()
        }),
        OperationError::Invalid(_)
    ));
    // Codes without a system.
    assert!(matches!(
        run(SubsumesInput {
            code_a: Some(String::from("cat")),
            code_b: Some(String::from("dog")),
            ..SubsumesInput::default()
        }),
        OperationError::Required(_)
    ));
    let unknown = run(SubsumesInput {
        code_a: Some(String::from("cat")),
        code_b: Some(String::from("unicorn")),
        system: Some(URL.to_owned()),
        ..SubsumesInput::default()
    });
    assert!(matches!(unknown, OperationError::UnknownCode { .. }));
    // Codings of two systems.
    let foreign = run(SubsumesInput {
        coding_a: Some(coding_ref(Some(URL), "cat")),
        coding_b: Some(coding_ref(Some("http://loinc.org"), "1")),
        ..SubsumesInput::default()
    });
    assert!(matches!(foreign, OperationError::NotSupported(_)));
    // A system without subsumption.
    let flat = run(SubsumesInput {
        code_a: Some(String::from("cat")),
        code_b: Some(String::from("dog")),
        system: Some(FLAT_URL.to_owned()),
        ..SubsumesInput::default()
    });
    assert!(matches!(flat, OperationError::NotSupported(_)));
    assert_eq!(flat.status(), StatusCode::BAD_REQUEST);
    let mut registry_without = Registry::new();
    registry_without
        .register(Arc::new(crate::fixture::Fixture::flat()))
        .expect("registers");
    assert!(matches!(
        subsumes(
            &registry_without,
            &Invocation::Type,
            &SubsumesInput {
                code_a: Some(String::from("a")),
                code_b: Some(String::from("b")),
                system: Some(URL.to_owned()),
                ..SubsumesInput::default()
            }
        ),
        Err(OperationError::UnknownSystem(_))
    ));
}

// NOTE: the terminology ecosystem answers an unknown system as `result = false`
// with `x-caused-by-unknown-system`, never as an HTTP error
// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>, `$validate-code` return parameters).
#[test]
fn validate_code_names_an_unknown_system_or_version_instead_of_failing() {
    let registry = registry();
    let unknown = validate_code(
        &registry,
        &Invocation::Type,
        &ValidateCodeInput {
            url: Some(String::from("http://example.org/nowhere")),
            code: Some(String::from("cat")),
            ..ValidateCodeInput::default()
        },
    )
    .expect("a false result, not an error");
    assert!(!unknown.result);
    assert_eq!(unknown.unknown_systems, ["http://example.org/nowhere"]);
    assert_eq!(unknown.code.as_deref(), Some("cat"));
    assert_eq!(
        unknown.system.as_deref(),
        Some("http://example.org/nowhere")
    );
    assert_eq!(unknown.version, None);
    assert_eq!(unknown.issues.len(), 1);
    assert_eq!(
        (unknown.issues[0].code, unknown.issues[0].kind),
        ("not-found", "not-found")
    );
    assert_eq!(
        unknown.message.as_deref(),
        Some(
            "A definition for CodeSystem 'http://example.org/nowhere' could not be found, so the code cannot be validated"
        )
    );
    let version = validate_code(
        &registry,
        &Invocation::Type,
        &ValidateCodeInput {
            coding: Some(CodingRef {
                system: Some(URL.to_owned()),
                version: Some(String::from("1999")),
                code: Some(String::from("cat")),
                display: None,
            }),
            ..ValidateCodeInput::default()
        },
    )
    .expect("a false result, not an error");
    assert!(!version.result);
    assert_eq!(version.unknown_systems, [format!("{URL}|1999")]);
    assert_eq!(version.version.as_deref(), Some("1999"));
    assert_eq!(
        version.issues[0].expression.as_deref(),
        Some("Coding.system")
    );
    assert!(
        version
            .message
            .as_deref()
            .unwrap()
            .contains("version '1999'"),
        "{:?}",
        version.message
    );
}

// NOTE: R5 `$validate-code` declares `issues`, an OperationOutcome whose issues
// carry `tx-issue-type` codings (<https://hl7.org/fhir/R5/codesystem-operation-validate-code.html>).
#[test]
fn validate_code_itemises_its_issues() {
    let registry = registry();
    let unknown = validate_code(
        &registry,
        &Invocation::Type,
        &ValidateCodeInput {
            url: Some(URL.to_owned()),
            code: Some(String::from("unicorn")),
            ..ValidateCodeInput::default()
        },
    )
    .expect("validates");
    assert_eq!(unknown.issues.len(), 1);
    assert_eq!(unknown.issues[0].severity, "error");
    assert_eq!(unknown.issues[0].code, "code-invalid");
    assert_eq!(unknown.issues[0].kind, "invalid-code");
    assert_eq!(unknown.issues[0].expression.as_deref(), Some("code"));
    let wrong = validate_code(
        &registry,
        &Invocation::Type,
        &ValidateCodeInput {
            url: Some(URL.to_owned()),
            coding: Some(coding_ref(Some(URL), "cat")).map(|mut c| {
                c.display = Some(String::from("Dog"));
                c
            }),
            ..ValidateCodeInput::default()
        },
    )
    .expect("validates");
    assert_eq!(wrong.issues.len(), 1);
    assert_eq!(wrong.issues[0].kind, "invalid-display");
    assert_eq!(
        wrong.issues[0].expression.as_deref(),
        Some("Coding.display")
    );
    assert!(
        wrong.issues[0]
            .text
            .starts_with("Wrong Display Name 'Dog' for")
            && wrong.issues[0].text.contains("'Cat'"),
        "{}",
        wrong.issues[0].text
    );
    let inactive = validate_code(
        &registry,
        &Invocation::Type,
        &ValidateCodeInput {
            url: Some(URL.to_owned()),
            code: Some(String::from("fish")),
            ..ValidateCodeInput::default()
        },
    )
    .expect("validates");
    assert!(inactive.result);
    assert_eq!(inactive.issues.len(), 1);
    assert_eq!(inactive.issues[0].severity, "warning");
    assert_eq!(inactive.issues[0].kind, "code-comment");
    let valid = validate_code(
        &registry,
        &Invocation::Type,
        &ValidateCodeInput {
            url: Some(URL.to_owned()),
            code: Some(String::from("cat")),
            ..ValidateCodeInput::default()
        },
    )
    .expect("validates");
    assert_eq!(
        valid.issues,
        Vec::<fhir_terminology::operations::Issue>::new()
    );
}
