// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One answer of `ValueSet/$validate-code` from the validations of each coding
//! a `CodeableConcept` carries: the first in the value set, else the version
//! mismatch, the failed import, or none.

use super::super::{CodingRef, Issue};
use super::refusal::{failed, named_code};
use super::target::include_literal_for;
use super::{Validation, message_of};
use crate::valueset::model::ValueSetModel;
use crate::versioned::Versioned;

/// Whether a judged coding is in the value set (its display may still be
/// wrong, and a warning is a note about the code, never a verdict against it).
pub(super) fn in_value_set(validation: &Validation) -> bool {
    !validation.issues.iter().any(|issue| {
        issue.severity == "error"
            && matches!(
                issue.kind,
                "not-in-vs" | "invalid-code" | "not-found" | "cannot-infer" | "vs-invalid"
            )
    })
}

/// The systems a failed coding named that the server does not serve, kept
/// apart as the coding's own judgement left them.
pub(super) fn take_unknown(from: &Validation, into: &mut Validation) {
    into.unknown_systems
        .extend(from.unknown_systems.iter().cloned());
    into.x_unknown_systems
        .extend(from.x_unknown_systems.iter().cloned());
}

/// The `information` issue that a coding is not in the value set.
pub(super) fn this_code_not_in_vs(model: &ValueSetModel, coding: &CodingRef, base: &str) -> Issue {
    let system = match (&coding.system, &coding.version) {
        (Some(system), Some(version)) => format!("{system}|{version}"),
        (Some(system), None) => system.clone(),
        (None, _) => String::new(),
    };
    Issue {
        severity: "information",
        code: "code-invalid",
        kind: "this-code-not-in-vs",
        message: crate::operations::MessageId::NoneOfTheProvidedCodesAreInTheValueSetOne,
        text: format!(
            "The provided code '{}' was not found in the value set '{}'",
            named_code(
                &system,
                coding.code.as_deref().unwrap_or_default(),
                coding.display.as_deref()
            ),
            model.canonical()
        ),
        expression: super::super::at(base, "code"),
    }
}

/// The failed answer that no coding of a `CodeableConcept` is a valid code of
/// the value set, with that one error and no code outputs.
pub(super) fn no_valid_coding(model: &ValueSetModel) -> Validation {
    failed(
        None,
        None,
        Issue {
            severity: "error",
            code: "code-invalid",
            kind: "not-in-vs",
            message: crate::operations::MessageId::TxGeneralCcErrorMessage,
            text: format!(
                "No valid coding was found for the value set '{}'",
                model.canonical()
            ),
            expression: None,
        },
    )
}

/// One answer for a `CodeableConcept` from its codings' judgements, the
/// ecosystem's shape (its test cases): the first coding in the value set
/// answers the code outputs and the other codings' issues join it; when none
/// is, one `not-in-vs` error, each coding's own errors, and an `information`
/// per coding, with no code outputs.
pub(super) fn combine(model: &ValueSetModel, judged: &[(&CodingRef, Validation)]) -> Validation {
    if let Some(failed) = failed_import(judged) {
        return failed;
    }
    if let Some(failed) = include_version_unserved(model, judged) {
        return failed;
    }
    if let Some(resolved) = version_mismatch(judged) {
        return resolved;
    }
    first_in_value_set(model, judged).unwrap_or_else(|| none_in_value_set(model, judged))
}

/// The answer when the one coding named a code system version the value set
/// include disagrees with.
///
/// The concept is resolved, so the coding answers alone and keeps the `system`
/// and `code` "against which validation was based", which the ecosystem
/// requires of every answer
/// (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
pub(super) fn version_mismatch(judged: &[(&CodingRef, Validation)]) -> Option<Validation> {
    let [(_, only)] = judged else {
        return None;
    };
    only.issues
        .iter()
        .any(|issue| issue.kind == "vs-invalid")
        .then(|| only.clone())
}

/// The answer when one coding names an import the server does not hold.
///
/// That fails the whole concept with that one issue, with no code outputs
/// (the ecosystem's `validation/simple-codeableconcept-bad-import`).
pub(super) fn failed_import(judged: &[(&CodingRef, Validation)]) -> Option<Validation> {
    let (_, import) = judged.iter().find(|(_, v)| {
        v.issues
            .iter()
            .any(|i| i.kind == "not-found" && i.expression.is_none())
    })?;
    let mut validation = import.clone();
    validation.code = None;
    validation.system = None;
    validation.version = None;
    validation.display = None;
    Some(validation)
}

/// The answer when a value set include names a version the server does not
/// serve.
///
/// That fails the whole concept on that coding alone, the code and the system
/// dropped (the ecosystem's `codeableconcept-*-vs1wb` cases).
pub(super) fn include_version_unserved(
    model: &ValueSetModel,
    judged: &[(&CodingRef, Validation)],
) -> Option<Validation> {
    let (_, unresolvable) = judged.iter().find(|(coding, v)| {
        coding.system.as_deref().is_some_and(|system| {
            include_literal_for(model, system, coding.version.as_deref()).is_some_and(|literal| {
                v.unknown_systems
                    .iter()
                    .any(|c| *c == format!("{system}|{literal}"))
            })
        })
    })?;
    let mut validation = unresolvable.clone();
    validation.code = None;
    validation.system = None;
    validation.message = message_of(&validation.issues);
    Some(validation)
}

/// The answer when a coding is in the value set: the first one answers the
/// code outputs and every other coding's issues join it.
pub(super) fn first_in_value_set(
    model: &ValueSetModel,
    judged: &[(&CodingRef, Validation)],
) -> Option<Validation> {
    let base = |index: usize| format!("CodeableConcept.coding[{index}]");
    let (primary, (_, found)) = judged
        .iter()
        .enumerate()
        .find(|(_, (_, v))| in_value_set(v))?;
    let mut answer = found.clone();
    for (index, (coding, other)) in judged.iter().enumerate() {
        if index == primary {
            continue;
        }
        for issue in &other.issues {
            if issue.kind == "not-in-vs" {
                answer
                    .issues
                    .push(this_code_not_in_vs(model, coding, &base(index)));
            } else {
                answer.issues.push(issue.clone());
            }
        }
        take_unknown(other, &mut answer);
    }
    answer.result = !answer.issues.iter().any(|i| i.severity == "error");
    answer.message = message_of(&answer.issues);
    Some(answer)
}

/// The answer when no coding is in the value set: one `not-in-vs` error, each
/// coding's own errors, and an `information` per coding, with no code outputs.
pub(super) fn none_in_value_set(
    model: &ValueSetModel,
    judged: &[(&CodingRef, Validation)],
) -> Validation {
    let base = |index: usize| format!("CodeableConcept.coding[{index}]");
    let mut answer = no_valid_coding(model);
    for (_, other) in judged {
        for issue in &other.issues {
            if issue.kind != "not-in-vs" && issue.severity == "error" {
                answer.issues.push(issue.clone());
            }
        }
        take_unknown(other, &mut answer);
    }
    for (index, (coding, _)) in judged.iter().enumerate() {
        answer
            .issues
            .push(this_code_not_in_vs(model, coding, &base(index)));
    }
    answer.message = message_of(&answer.issues);
    answer
}
