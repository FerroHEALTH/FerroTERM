// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answers of `ValueSet/$validate-code` when a coding is not in the value
//! set: the system inferred or not, the code unknown, the fragment, and the
//! import that failed.

use super::super::{Issue, OperationError};
use super::{Context, InactivePolicy, Policy, Subject, Validation, inactive_outputs, message_of};
use crate::provider::declaration::ContentMode;
use crate::provider::{CodeSystemProvider, Located};
use crate::valueset::model::ValueSetModel;
use crate::versioned::Versioned;
use std::sync::Arc;

/// The one system a value set draws on, when there is exactly one.
pub(super) fn infer_system(model: &ValueSetModel) -> Option<String> {
    let mut systems: Vec<&str> = model
        .compose
        .include
        .iter()
        .filter_map(|include| include.system.as_ref().map(|s| s.url.as_str()))
        .collect();
    systems.sort_unstable();
    systems.dedup();
    match systems.as_slice() {
        [one] => Some((*one).to_owned()),
        _ => None,
    }
}

/// The systems of the value set whose `subject.code` the value set contains
/// (`inferSystem`), in system order; one is the inferred system and several
/// leave it undetermined.
pub(super) fn infer_by_membership(
    context: &Context<'_>,
    subject: &Subject<'_>,
    language: Option<&str>,
) -> Result<Vec<String>, OperationError> {
    let Context {
        sources,
        model,
        resolver,
        ..
    } = *context;
    let mut systems: Vec<&str> = model
        .compose
        .include
        .iter()
        .filter_map(|include| include.system.as_ref().map(|s| s.url.as_str()))
        .collect();
    systems.sort_unstable();
    systems.dedup();
    let mut matches = Vec::new();
    for system in systems {
        let Ok(served) = sources.registry.resolve(system, None) else {
            continue;
        };
        let Some(located) = served.provider.locate(subject.code)? else {
            continue;
        };
        let contained = resolver.contains_compose(
            &model.canonical(),
            &model.compose,
            system,
            None,
            &located.code,
            language,
        );
        match contained {
            Ok(Some(_)) => matches.push(system.to_owned()),
            Ok(None) | Err(crate::compose::ComposeError::UnknownValueSet(_)) => {}
            Err(error) => {
                return Err(OperationError::of_compose(
                    error,
                    context.inline.then_some(&model.compose),
                ));
            }
        }
    }
    Ok(matches)
}

/// The failed validation of a bare code whose system cannot be determined:
/// not in the value set, and the system unknown (the ecosystem's wording).
/// The failed validation of a `Coding` that names no system: the membership
/// verdict and the data warning (the ecosystem's `no-system` case).
pub(super) fn no_system(model: &ValueSetModel, subject: &Subject<'_>) -> Validation {
    let mut validation = failed(
        None,
        None,
        not_in_vs(model, "", subject.code, subject.display, subject.expression),
    );
    validation.issues.push(Issue {
        severity: "warning",
        code: "invalid",
        kind: "invalid-data",
        message: crate::operations::MessageId::CodingHasNoSystemCannotValidate,
        text: String::from(super::super::NO_SYSTEM),
        expression: Some(super::super::element(subject.expression)),
    });
    validation.message = message_of(&validation.issues);
    validation.code = Some(subject.code.to_owned());
    validation
}

pub(super) fn cannot_infer(
    model: &ValueSetModel,
    subject: &Subject<'_>,
    matches: &[String],
) -> Validation {
    // NOTE: `inferSystem` needs one system of the value set to hold the code;
    // the ecosystem names the competing systems when several do
    // (<https://hl7.org/fhir/R5/valueset-operation-validate-code.html>).
    let competing = if matches.len() > 1 {
        format!(
            ": value set expansion has multiple matches: [{}]",
            matches.join(", ")
        )
    } else {
        String::new()
    };
    let text = format!(
        "The System URI could not be determined for the code '{}' in the ValueSet '{}'{competing}",
        subject.code,
        model.canonical()
    );
    let message = if matches.len() > 1 {
        crate::operations::MessageId::UnableToResolveSystemValueSetHasMultipleMatches
    } else {
        crate::operations::MessageId::UnableToInferCodeSystem
    };
    let mut validation = failed(
        None,
        None,
        not_in_vs(model, "", subject.code, subject.display, subject.expression),
    );
    validation.issues.push(Issue {
        severity: "error",
        code: "not-found",
        kind: "cannot-infer",
        message,
        text,
        expression: super::super::at(subject.expression, "code"),
    });
    validation.code = Some(subject.code.to_owned());
    validation.message = message_of(&validation.issues);
    validation
}

pub(super) fn system_code(system: &str, code: &str) -> String {
    format!("{system}#{code}")
}

/// `S#C`, or `S#C ('display')` when the client asserted a display, as the
/// ecosystem names a code in its texts.
pub(super) fn named_code(system: &str, code: &str, display: Option<&str>) -> String {
    match display {
        Some(display) => format!("{system}#{code} ('{display}')"),
        None => format!("{system}#{code}"),
    }
}

/// The issue for a code the value set does not contain.
pub(super) fn not_in_vs(
    model: &ValueSetModel,
    system: &str,
    code: &str,
    display: Option<&str>,
    expression: &str,
) -> Issue {
    Issue {
        severity: "error",
        code: "code-invalid",
        kind: "not-in-vs",
        message: crate::operations::MessageId::NoneOfTheProvidedCodesAreInTheValueSetOne,
        text: format!(
            "The provided code '{}' was not found in the value set '{}'",
            named_code(system, code, display),
            model.canonical()
        ),
        expression: super::super::at(expression, "code"),
    }
}

/// A `result = false` validation carrying `issue`.
pub(super) fn failed(system: Option<String>, version: Option<String>, issue: Issue) -> Validation {
    let version = version.filter(|v| !v.is_empty());
    Validation {
        result: false,
        message: Some(issue.text.clone()),
        display: None,
        system,
        version,
        code: None,
        normalized_code: None,
        issues: vec![issue],
        unknown_systems: Vec::new(),
        x_unknown_systems: Vec::new(),
        codeable_concept: None,
        inactive: None,
        status: None,
    }
}

/// The failed validation over a value set the compose imports but the server
/// does not hold: the ecosystem's shape (its test cases), where an unknown
/// top-level value set stays an error.
pub(super) fn unknown_import(system: &str, version: String, url: &str, code: &str) -> Validation {
    let mut validation = failed(
        Some(system.to_owned()),
        Some(version),
        Issue {
            severity: "error",
            code: "not-found",
            kind: "not-found",
            message: crate::operations::MessageId::UnableToResolveValueSet,
            text: format!("A definition for the value Set '{url}' could not be found"),
            expression: None,
        },
    );
    validation.code = Some(code.to_owned());
    validation
}

/// Whether a code the resource does not carry could still be in the value set:
/// the system is served from a `fragment`, and an include over it enumerates
/// no concepts of its own.
///
/// A fragment carries "a subset of the code system concepts"
/// (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>), so absence from
/// the resource is not absence from the system. No FHIR specification says how
/// that meets an include that lists its concepts, and such an include decides
/// membership on its own, so leaving it out here is our own design.
pub(super) fn fragment_may_admit(
    provider: &Arc<dyn CodeSystemProvider>,
    model: &ValueSetModel,
    system: &str,
) -> bool {
    provider.declaration().content == ContentMode::Fragment
        && model.compose.include.iter().any(|include| {
            include
                .system
                .as_ref()
                .is_some_and(|named| named.url == system)
                && include.concepts.is_empty()
        })
}

/// The validation of a code a `fragment` resource does not carry.
///
/// The server cannot say the code is invalid, so the answer holds and states
/// the fragment beside it, which the ecosystem requires of a fragment
/// ("Servers SHALL reflect `content = fragment` in an error message if the
/// code is not valid against a fragment",
/// <https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
pub(super) fn unlisted_in_fragment(
    provider: &Arc<dyn CodeSystemProvider>,
    system: &str,
    version: String,
    subject: &Subject<'_>,
    policy: &Policy<'_>,
) -> Validation {
    let (message, text) = super::super::display::unknown_code(provider.as_ref(), subject.code);
    // NOTE: under `valueset-membership-only` the server performs no "validation
    // tasks beyond validating membership", so the fragment note stays unsaid
    // (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-validate-code.html>).
    let issues = if policy.membership_only {
        Vec::new()
    } else {
        vec![Issue {
            severity: "warning",
            code: "code-invalid",
            kind: "invalid-code",
            message,
            text,
            expression: super::super::at(subject.expression, "code"),
        }]
    };
    Validation {
        result: true,
        message: message_of(&issues),
        display: None,
        system: Some(system.to_owned()),
        version: Some(version).filter(|v| !v.is_empty()),
        code: Some(subject.code.to_owned()),
        normalized_code: None,
        issues,
        unknown_systems: Vec::new(),
        x_unknown_systems: Vec::new(),
        codeable_concept: None,
        inactive: None,
        status: None,
    }
}

/// The failed validation of a code the system does not have: not in the value
/// set, and unknown in the system.
pub(super) fn unknown_code(
    model: &ValueSetModel,
    provider: &Arc<dyn CodeSystemProvider>,
    system: &str,
    version: String,
    subject: &Subject<'_>,
    policy: &Policy<'_>,
) -> Validation {
    let (message, text) = super::super::display::unknown_code(provider.as_ref(), subject.code);
    let unknown = Issue {
        severity: "error",
        code: "code-invalid",
        kind: "invalid-code",
        message,
        text,
        expression: super::super::at(subject.expression, "code"),
    };
    // NOTE: the membership issue names the code the way the request stated it,
    // the version included, which the ecosystem requires of every answer
    // (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
    let named = match subject.version {
        Some(asserted) => format!("{system}|{asserted}"),
        None => system.to_owned(),
    };
    let mut validation = failed(
        Some(system.to_owned()),
        Some(version),
        not_in_vs(
            model,
            &named,
            subject.code,
            subject.display,
            subject.expression,
        ),
    );
    // NOTE: under `valueset-membership-only` the server performs no "validation
    // tasks beyond validating membership", so the code's standing in its system
    // stays unreported (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-validate-code.html>).
    if !policy.membership_only {
        validation.issues.push(unknown);
    }
    // NOTE: the submitted code is echoed even when the system does not have it, the
    // shape the ecosystem expects (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/>).
    validation.code = Some(subject.code.to_owned());
    validation.message = message_of(&validation.issues);
    validation
}

/// The failed validation of a code the system has but the value set does
/// not: the code and its display are still echoed.
pub(super) fn outside_value_set(
    context: &Context<'_>,
    system: &str,
    version: String,
    located: &Located,
    display: Option<&str>,
    subject: &Subject<'_>,
    concept_status: &crate::provider::Status,
) -> Validation {
    let Context { model, policy, .. } = *context;
    let (given, expression) = (subject.display, subject.expression);
    let mut issues = Vec::new();
    // NOTE: a concept the value set refuses because it is inactive is valid in
    // its code system, which the ecosystem reports as its own rule
    // (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
    if !concept_status.active
        && (policy.inactive == InactivePolicy::Refused || model.compose.inactive == Some(false))
    {
        issues.push(Issue {
            severity: "error",
            code: "business-rule",
            kind: "code-rule",
            message: crate::operations::MessageId::StatusCodeWarningCode,
            text: format!("The concept '{}' is valid but is not active", located.code),
            expression: super::super::at(expression, "code"),
        });
    }
    issues.push(not_in_vs(model, system, &located.code, given, expression));
    if !policy.membership_only {
        if let Some((note, _)) = super::super::inactive_note(
            &located.code,
            concept_status,
            super::super::whole(expression),
        ) {
            issues.push(note);
        } else if let Some((note, _)) = super::super::deprecated_note(
            &located.code,
            concept_status,
            super::super::whole(expression),
        ) {
            issues.push(note);
        }
    }
    let (inactive, status) = inactive_outputs(concept_status);
    let mut validation = Validation {
        result: false,
        message: None,
        display: display.map(str::to_owned),
        system: Some(system.to_owned()),
        version: Some(version).filter(|v| !v.is_empty()),
        code: Some(located.code.clone()),
        normalized_code: None,
        issues,
        unknown_systems: Vec::new(),
        x_unknown_systems: Vec::new(),
        codeable_concept: None,
        inactive,
        status,
    };
    validation.message = message_of(&validation.issues);
    validation
}
