// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The outputs of a code the value set contains: the notes about the code,
//! the value set and the code system, and the inactive status.

use super::super::{Issue, OperationError};
use super::message::message_of;
use super::target::assess;
use super::{Context, Resolved, Subject, Target, Validation};
use crate::compose::Item;
use crate::provider::Located;
use crate::valueset::model::ValueSetModel;
use crate::versioned::Versioned;

/// The validation of a code the value set contains: the notes about the code,
/// the value set, and the code system, then the outputs.
pub(super) fn conclude(
    context: &Context<'_>,
    subject: &Subject<'_>,
    target: Target,
    located: &Located,
    item: &Item,
    display: Option<String>,
    resolved: Resolved,
) -> Result<Validation, OperationError> {
    let Resolved { system, version } = resolved;
    let Context { model, policy, .. } = *context;
    let provider = &target.provider;
    let mut issues = target.issues;
    issues.extend(assess(context, provider, located, item, subject)?);
    let (inactive, status) = inactive_outputs(&provider.status(located.concept)?);
    // NOTE: the value set's own deprecation note stays out of `message`, the
    // ecosystem's shape (its `CONCEPT_DEPRECATED_IN_VALUESET` cases).
    let message = message_of(&issues);
    if !policy.membership_only {
        issues.extend(deprecated_in_value_set(model, item, &located.code, subject));
        issues.extend(super::super::standing_note(
            "CodeSystem",
            &format!("{system}|{version}"),
            &provider.standing(),
        ));
        issues.extend(value_set_notes(context));
    }
    let result = !issues.iter().any(|issue| issue.severity == "error");
    Ok(Validation {
        result,
        message,
        display,
        system: Some(system),
        version: Some(version).filter(|v| !v.is_empty()),
        code: Some(subject.code.to_owned()),
        normalized_code: (located.code != subject.code).then(|| located.code.clone()),
        issues,
        unknown_systems: target.unknown_systems,
        x_unknown_systems: Vec::new(),
        codeable_concept: None,
        inactive,
        status,
    })
}

/// The `inactive` and `status` outputs of a concept: set when it is inactive,
/// the status only when the system states one beyond the inactive flag.
/// The `code-comment` warning for a concept the value set lists as deprecated
/// (`valueset-deprecated`).
fn deprecated_in_value_set(
    model: &ValueSetModel,
    item: &Item,
    code: &str,
    subject: &Subject<'_>,
) -> Option<Issue> {
    let deprecated = model
        .compose
        .include
        .iter()
        .filter(|i| i.system.as_ref().is_some_and(|s| s.url == item.system))
        .flat_map(|i| &i.concepts)
        .any(|c| c.deprecated && c.code == code);
    deprecated.then(|| Issue {
        severity: "warning",
        code: "business-rule",
        kind: "code-comment",
        message: crate::operations::MessageId::ConceptDeprecatedInValueSet,
        text: format!(
            "The presence of the concept '{code}' in the system '{}' in the value set {} is marked with a status of deprecated and its use should be reviewed",
            item.system,
            model.canonical()
        ),
        expression: super::super::at(subject.expression, "code"),
    })
}

pub(super) fn inactive_outputs(
    concept_status: &crate::provider::Status,
) -> (Option<bool>, Option<String>) {
    if concept_status.active {
        return (
            None,
            concept_status
                .standards_status
                .clone()
                .filter(|s| s == "deprecated"),
        );
    }
    (
        Some(true),
        concept_status
            .inactive_reason
            .clone()
            .filter(|reason| reason != "inactive"),
    )
}

/// The `status-check` notes for the value set validated against and every
/// value set it drew on that the ecosystem marks (a withdrawn value set).
fn value_set_notes(context: &Context<'_>) -> Vec<Issue> {
    let Context {
        sources,
        model,
        resolver,
        ..
    } = *context;
    let mut notes = Vec::new();
    let mut seen = Vec::new();
    let mut note = |canonical: &str, standards_status: Option<&str>| {
        if seen.iter().any(|c| c == canonical) {
            return;
        }
        seen.push(canonical.to_owned());
        let standing = crate::provider::Standing {
            status: String::from("active"),
            experimental: false,
            standards_status: standards_status.map(str::to_owned),
        };
        notes.extend(super::super::standing_note(
            "ValueSet", canonical, &standing,
        ));
    };
    for used in resolver.used_value_sets() {
        let (url, version) = match used.split_once('|') {
            Some((url, version)) => (url, Some(version)),
            None => (used.as_str(), None),
        };
        if let Some(referenced) = sources.value_sets.resolve(url, version) {
            note(&used, referenced.standards_status.as_deref());
        }
    }
    note(&model.canonical(), model.standards_status.as_deref());
    notes
}
