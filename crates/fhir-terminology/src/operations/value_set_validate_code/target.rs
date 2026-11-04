// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The code system a coding of `ValueSet/$validate-code` is checked against:
//! the include that names it, the version negotiated for it, and the answer
//! when the system is not served.

use super::super::{Issue, OperationError};
use super::refusal::{failed, not_in_vs, system_code};
use super::{Context, DisplayCheck, InactivePolicy, Subject, Target, Validation, message_of};
use crate::compose::Item;
use crate::compose::negotiation::Negotiation;
use crate::provider::{CodeSystemProvider, Located};
use crate::valueset::model::ValueSetModel;
use std::sync::Arc;

/// The version of `system` the value set uses for this subject, negotiated,
/// and the issues a subject version that disagrees or a version the server
/// does not serve raise (the ecosystem's `version` cases).
///
/// Returns the unserved-system validation when neither the include's version
/// nor the subject's nor a default resolves.
pub(super) fn resolve_target(
    context: &Context<'_>,
    system: &str,
    subject: &Subject<'_>,
) -> Result<Target, Box<Validation>> {
    let Context {
        sources,
        model,
        negotiation,
        ..
    } = *context;
    let registry = sources.registry;
    let valid: Vec<String> = registry
        .versions(system)
        .map(|p| p.identity().version.clone())
        .collect();
    let subject_served = subject
        .version
        .is_none_or(|v| registry.resolve(system, Some(v)).is_ok());
    let include_literal = include_literal_for(model, system, subject.version);
    let original = model
        .compose
        .include
        .iter()
        .filter_map(|i| i.system.as_ref())
        .find(|s| s.url == system)
        .map(|s| negotiation_original(negotiation, system, s.version.as_deref()))
        .unwrap_or_default();
    let literal = include_literal.map(str::to_owned);
    let resolved = match &literal {
        Some(l) => match subject.version {
            Some(sv) if subject_served && crate::versioned::version_matches(l, sv) => {
                registry.resolve(system, Some(sv)).ok()
            }
            _ => registry.resolve(system, Some(l)).ok(),
        },
        None => registry.resolve(system, None).ok(),
    };
    let mut issues = Vec::new();
    let mut unknown_systems = Vec::new();
    let expression = subject.expression;
    let Some(resolved) = resolved else {
        return unresolvable_include(context, system, subject, subject_served, literal, &valid);
    };
    let version = resolved.provider.identity().version.clone();
    if let Err(error) = negotiation.check_system(system, &version) {
        issues.push(Issue {
            severity: "error",
            code: "exception",
            kind: "version-error",
            message: crate::operations::MessageId::ValueSetVersionCheck,
            text: error.to_string(),
            expression: super::super::at(expression, "version"),
        });
    }
    disagreement(
        system,
        subject,
        &version,
        literal.as_deref(),
        original.as_deref(),
        subject_served,
        &valid,
        &mut issues,
        &mut unknown_systems,
    );
    // NOTE: a value set that includes one system at several versions admits a
    // versionless subject from any of them (the ecosystem's `overload` cases).
    let alternatives = if subject.version.is_none() {
        model
            .compose
            .include
            .iter()
            .filter_map(|i| i.system.as_ref())
            .filter(|s| s.url == system && s.version.as_deref() != include_literal)
            .filter_map(|s| registry.resolve(system, s.version.as_deref()).ok())
            .map(|r| r.provider)
            .filter(|p| p.identity().version != version)
            .collect()
    } else {
        Vec::new()
    };
    Ok(Target {
        provider: resolved.provider,
        alternatives,
        resolvable: true,
        issues,
        unknown_systems,
    })
}

/// The version literal of the include for `system` this subject falls under:
/// one whose version admits the subject's when several do (`version-mixed`),
/// else the first.
pub(super) fn include_literal_for<'m>(
    model: &'m ValueSetModel,
    system: &str,
    subject_version: Option<&str>,
) -> Option<&'m str> {
    let includes: Vec<Option<&str>> = model
        .compose
        .include
        .iter()
        .filter_map(|i| i.system.as_ref())
        .filter(|s| s.url == system)
        .map(|s| s.version.as_deref())
        .collect();
    includes
        .iter()
        .copied()
        .find(|v| match (v, subject_version) {
            (Some(pattern), Some(sv)) => crate::versioned::version_matches(pattern, sv),
            _ => false,
        })
        .or_else(|| includes.first().copied())
        .flatten()
}

/// The issues a subject version raises when it differs from the version the
/// value set uses: the `vs-invalid` disagreement (an error, or a warning for
/// a versionless include and an unserved subject version) and, for an
/// unserved subject version, the not-found issue, in the ecosystem's order.
#[expect(
    clippy::too_many_arguments,
    reason = "none of these is call-invariant, so the context carries none of them: seven describe one disagreement and two are the itemised outputs it adds to"
)]
pub(super) fn disagreement(
    system: &str,
    subject: &Subject<'_>,
    version: &str,
    literal: Option<&str>,
    original: Option<&str>,
    subject_served: bool,
    valid: &[String],
    issues: &mut Vec<Issue>,
    unknown_systems: &mut Vec<String>,
) {
    let Some(sv) = subject.version else {
        return;
    };
    if crate::versioned::version_matches(sv, version) {
        return;
    }
    let expression = subject.expression;
    let (severity, message, text) = match (literal, original) {
        (Some(named), Some(orig)) if named != orig => (
            "error",
            crate::operations::MessageId::ValueSetValueMismatchChanged,
            format!(
                "The code system '{system}' version '{named}' resulting from the version '{orig}' in the ValueSet include is different to the one in the value ('{sv}')"
            ),
        ),
        (Some(named), _) => (
            "error",
            crate::operations::MessageId::ValueSetValueMismatch,
            format!(
                "The code system '{system}' version '{named}' in the ValueSet include is different to the one in the value ('{sv}')"
            ),
        ),
        (None, _) if subject_served => (
            "error",
            crate::operations::MessageId::ValueSetValueMismatch,
            format!(
                "The code system '{system}' version '{version}' in the ValueSet include is different to the one in the value ('{sv}')"
            ),
        ),
        (None, _) => (
            "warning",
            crate::operations::MessageId::ValueSetValueMismatchDefault,
            format!(
                "The code system '{system}' version '{version}' for the versionless include in the ValueSet include is different to the one in the value ('{sv}')"
            ),
        ),
    };
    let disagreement = vs_invalid(severity, message, text, expression);
    if subject_served {
        issues.push(disagreement);
        return;
    }
    let (canonical, not_found) = super::super::unknown_system(
        system,
        Some(sv),
        super::super::at(expression, "system"),
        valid,
    );
    unknown_systems.push(canonical);
    if severity == "error" {
        issues.push(disagreement);
        issues.push(not_found);
    } else {
        issues.push(not_found);
        issues.push(disagreement);
    }
}

/// The target when the include's version is not served: the subject's version
/// when served, else the default, and the validation fails regardless.
pub(super) fn unresolvable_include(
    context: &Context<'_>,
    system: &str,
    subject: &Subject<'_>,
    subject_served: bool,
    literal: Option<String>,
    valid: &[String],
) -> Result<Target, Box<Validation>> {
    let Context {
        sources,
        negotiation,
        ..
    } = *context;
    let registry = sources.registry;
    let expression = subject.expression;
    // NOTE: a versionless subject falls back to the negotiated default
    // (`system-version`), else the server's (the ecosystem's `vs1wb` cases).
    let default = negotiation.system_version(system, None).ok().flatten();
    let fallback = subject.version.filter(|_| subject_served).map_or_else(
        || {
            registry
                .resolve(system, default.as_deref())
                .or_else(|_| registry.resolve(system, None))
        },
        |v| registry.resolve(system, Some(v)),
    );
    let Ok(fallback) = fallback else {
        return Err(Box::new(unserved_subject(context, system, subject, valid)));
    };
    let bad = literal.unwrap_or_default();
    let mut issues = Vec::new();
    if let Some(sv) = subject.version {
        issues.push(vs_invalid(
            "error",
            crate::operations::MessageId::ValueSetValueMismatch,
            format!(
                "The code system '{system}' version '{bad}' in the ValueSet include is different to the one in the value ('{sv}')"
            ),
            expression,
        ));
    }
    let (canonical, not_found) = super::super::unknown_system(
        system,
        Some(&bad),
        super::super::at(expression, "system"),
        valid,
    );
    issues.push(not_found);
    Ok(Target {
        provider: fallback.provider,
        alternatives: Vec::new(),
        resolvable: false,
        issues,
        unknown_systems: vec![canonical],
    })
}

/// The include's version before the negotiation touched it, when the
/// negotiation changed it (`Some(original)`, `Some("")` for none).
pub(super) fn negotiation_original(
    negotiation: &Negotiation,
    system: &str,
    pinned: Option<&str>,
) -> Option<String> {
    let now = pinned?;
    negotiation
        .include_version(system)
        .filter(|stated| *stated != now)
        .map(str::to_owned)
}

/// A `vs-invalid` issue about the include's version, at `Coding.version`.
pub(super) fn vs_invalid(
    severity: &'static str,
    message: crate::operations::MessageId,
    text: String,
    expression: &str,
) -> Issue {
    Issue {
        severity,
        code: "invalid",
        kind: "vs-invalid",
        message,
        text,
        expression: super::super::at(expression, "version"),
    }
}

/// The failed validation of a subject whose system is a supplement, which
/// defines no codes of its own (the ecosystem's `bad-supplement-url` case).
pub(super) fn supplement_as_system(
    system: &str,
    version: Option<&str>,
    subject: &Subject<'_>,
) -> Validation {
    let canonical = match version {
        Some(version) => format!("{system}|{version}"),
        None => system.to_owned(),
    };
    let mut validation = failed(
        Some(system.to_owned()),
        None,
        Issue {
            severity: "error",
            code: "invalid",
            kind: "invalid-data",
            message: crate::operations::MessageId::CodeSystemCsNoSupplement,
            text: format!(
                "CodeSystem {canonical} is a supplement, so can't be used as a value in Coding.system"
            ),
            expression: super::super::at(subject.expression, "system"),
        },
    );
    validation.code = Some(subject.code.to_owned());
    validation.unknown_systems.push(system.to_owned());
    validation
}

/// The failed validation of a subject whose system or version the server does
/// not serve at all.
pub(super) fn unserved_subject(
    context: &Context<'_>,
    system: &str,
    subject: &Subject<'_>,
    valid: &[String],
) -> Validation {
    let Context { sources, model, .. } = *context;
    let registry = sources.registry;
    if let Some(supplement_version) = registry.supplement_named(system) {
        return supplement_as_system(system, supplement_version.version.as_deref(), subject);
    }
    let served = registry.resolve(system, None).is_ok();
    let version = subject.version.filter(|_| served);
    let at = super::super::at(subject.expression, "system");
    let (canonical, not_found) = match subject.version {
        Some(asked) if !served => super::super::unknown_system_version_none(system, asked, at),
        _ => super::super::unknown_system(system, version, at, valid),
    };
    let referenced = model
        .compose
        .include
        .iter()
        .chain(&model.compose.exclude)
        .any(|i| i.system.as_ref().is_some_and(|s| s.url == system));
    // NOTE: the ecosystem's shape: the membership issue first, then the missing
    // system; the system is "caused by" the value set only when the value set
    // names it, else it is the input's own unknown system (`x-unknown-system`).

    // NOTE: a value set that selects from the missing system says nothing about
    // membership, so the server reports only that it cannot check
    // (<https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>).
    let mut validation = failed(
        Some(system.to_owned()),
        version.map(str::to_owned),
        if referenced {
            not_found.clone()
        } else {
            not_in_vs(
                model,
                system,
                subject.code,
                subject.display,
                subject.expression,
            )
        },
    );
    if let Some(local) = local_system(system, subject.expression) {
        validation.issues.push(local);
    }
    // NOTE: a `Coding.system` that names a value set is bad data, and no
    // missing system (the ecosystem's `bad-system2` case).
    let names_value_set = sources.value_sets.resolve(system, None).is_some();
    if names_value_set {
        validation.issues.push(Issue {
            severity: "error",
            code: "invalid",
            kind: "invalid-data",
            message: crate::operations::MessageId::TerminologyTxSystemValueSet2,
            text: format!("The Coding references a value set, not a code system ('{system}')"),
            expression: super::super::at(subject.expression, "system"),
        });
        validation.message = message_of(&validation.issues);
        validation.code = Some(subject.code.to_owned());
        return validation;
    }
    if !referenced {
        validation.issues.push(not_found);
    }
    validation.message = message_of(&validation.issues);
    validation.code = Some(subject.code.to_owned());
    if referenced || version.is_some() {
        validation.unknown_systems.push(canonical);
    } else {
        validation.x_unknown_systems.push(canonical);
    }
    validation
}

/// The `invalid-data` issue for a system that is a local reference: a
/// `Coding.system` must be an absolute URI (`Coding` in the FHIR data types,
/// <https://hl7.org/fhir/R4B/datatypes.html#Coding>).
pub(super) fn local_system(system: &str, expression: &str) -> Option<Issue> {
    let absolute = system.contains(':');
    (!absolute).then(|| Issue {
        severity: "error",
        code: "invalid",
        kind: "invalid-data",
        message: crate::operations::MessageId::TerminologyTxSystemRelative,
        text: String::from("Coding.system must be an absolute reference, not a local reference"),
        expression: super::super::at(expression, "system"),
    })
}

/// The failed validation over an include whose version the server does not
/// serve, answered against `version` (the subject's or the default).
pub(super) fn failed_target(system: &str, version: String, target: Target) -> Validation {
    let mut validation = Validation {
        result: false,
        message: None,
        display: None,
        system: Some(system.to_owned()),
        version: Some(version).filter(|v| !v.is_empty()),
        code: None,
        normalized_code: None,
        issues: target.issues,
        unknown_systems: target.unknown_systems,
        x_unknown_systems: Vec::new(),
        codeable_concept: None,
        inactive: None,
        status: None,
    };
    validation.message = message_of(&validation.issues);
    validation
}

/// `validation` with the located code and its display filled in.
pub(super) fn with_target(
    mut validation: Validation,
    located: &Located,
    display: Option<String>,
) -> Validation {
    validation.code = Some(located.code.clone());
    validation.display = display;
    validation
}

/// The issues of a code the value set contains: abstract, inactive, and the
/// display check.
pub(super) fn assess(
    context: &Context<'_>,
    provider: &Arc<dyn CodeSystemProvider>,
    located: &Located,
    item: &Item,
    subject: &Subject<'_>,
) -> Result<Vec<Issue>, OperationError> {
    let Context { model, policy, .. } = *context;
    let language = policy.language;
    let mut issues = Vec::new();
    if !policy.membership_only
        && let Some(note) = super::super::display::case_note(
            provider,
            subject.code,
            &located.code,
            super::super::at(subject.expression, "code"),
        )
    {
        issues.push(note);
    }
    if item.abstract_concept && !policy.abstract_ok {
        // NOTE: an abstract code is refused and, the ecosystem's shape, also reported
        // as outside the value set (its `notSelectable` cases).
        issues.push(Issue {
            severity: "error",
            code: "business-rule",
            kind: "code-rule",
            message: crate::operations::MessageId::AbstractCodeNotAllowed,
            text: format!(
                "Code '{}' is abstract, and not allowed in this context",
                system_code(&item.system, &located.code)
            ),
            expression: super::super::at(subject.expression, "code"),
        });
        issues.push(not_in_vs(
            model,
            &item.system,
            &located.code,
            subject.display,
            subject.expression,
        ));
    }
    let status = provider.status(located.concept)?;
    if item.inactive {
        // NOTE: `activeOnly`, or a compose that excludes inactive codes
        // (`compose.inactive = false`), refuses an inactive concept and reports it
        // outside the value set, the ecosystem's shape (its `inactive` cases).
        if policy.inactive == InactivePolicy::Refused || model.compose.inactive == Some(false) {
            issues.push(Issue {
                severity: "error",
                code: "business-rule",
                kind: "code-rule",
                message: crate::operations::MessageId::StatusCodeWarningCode,
                text: format!("The concept '{}' is valid but is not active", located.code),
                expression: super::super::at(subject.expression, "code"),
            });
            issues.push(not_in_vs(
                model,
                &item.system,
                &located.code,
                subject.display,
                subject.expression,
            ));
        }
        if !policy.membership_only
            && let Some((note, _)) = super::super::inactive_note(
                &located.code,
                &status,
                super::super::whole(subject.expression),
            )
        {
            issues.push(note);
        }
    } else if !policy.membership_only
        && let Some((note, _)) = super::super::deprecated_note(
            &located.code,
            &status,
            super::super::whole(subject.expression),
        )
    {
        issues.push(note);
    }
    // NOTE: under `lenient-display-validation` a wrong display does not fail the
    // result (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-validate-code.html>).
    if let Some(given) = subject.display
        && policy.display != DisplayCheck::Skipped
        && let Some(issue) = super::super::display::judge(
            provider,
            located.concept,
            super::super::display::Asserted {
                system: &item.system,
                code: &located.code,
                given,
                requested: language,
                lenient: policy.display == DisplayCheck::Lenient,
            },
            super::super::at(subject.expression, "display"),
        )?
    {
        issues.push(issue);
    }
    Ok(issues)
}
