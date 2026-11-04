// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ValueSet/$validate-code` in the terms every served FHIR version shares.
//!
//! The operation pages: <https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>
//! and <https://hl7.org/fhir/R5/valueset-operation-validate-code.html>.
//!
//! The value set is inline, stored, or implicit, and is expanded whole; the
//! code is one of `code` with `system`, `coding`, or `codeableConcept`. A code
//! outside the value set is `result = false` with a message, never an error.
//! Beside `result`, `message`, and `display`, the validation carries the
//! `system`, `version`, and `code` echo and the `issues` a general-purpose
//! server of the terminology ecosystem returns (`tx-issue-type` codings); the
//! wire layer of each version emits what that version declares.

mod combine;
mod message;
mod refusal;
mod target;

use std::sync::Arc;

use super::{CodeableConceptRef, CodingRef, Issue, OperationError, Sources};
use crate::compose::Item;
use crate::compose::negotiation::Negotiation;
use crate::language;
use crate::provider::{CodeSystemProvider, Located};
use crate::valueset::model::{ModelError, ValueSetModel};
use crate::valueset::store::Resolver;
use crate::versioned::Versioned;
use combine::combine;
use message::message_of;
use refusal::{
    cannot_infer, fragment_may_admit, infer_by_membership, infer_system, no_system,
    outside_value_set, unknown_code, unknown_import, unlisted_in_fragment,
};
use target::{assess, failed_target, resolve_target, with_target};

/// The `tx-issue-type` code system
/// (<https://build.fhir.org/ig/FHIR/fhir-tools-ig/CodeSystem-tx-issue-type.html>).
pub const TX_ISSUE_TYPE: &str = "http://hl7.org/fhir/tools/CodeSystem/tx-issue-type";

/// The outcome of a validation, in no version's types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validation {
    /// Whether the code is in the value set (and its display, when given, is valid).
    pub result: bool,
    /// Why the result is what it is, when there is something to say.
    pub message: Option<String>,
    /// The display the system prefers, when the code was found.
    pub display: Option<String>,
    /// The system the code was checked in, when one was determined.
    pub system: Option<String>,
    /// The served version of that system.
    pub version: Option<String>,
    /// The code as the system spells it, when the code is known.
    pub code: Option<String>,
    /// The code as the system spells it when `code` is spelled otherwise
    /// (`normalized-code`, the ecosystem's output).
    pub normalized_code: Option<String>,
    /// The issues behind the result.
    pub issues: Vec<Issue>,
    /// The canonicals of the systems the server does not serve
    /// (`x-caused-by-unknown-system`, the terminology ecosystem's output).
    pub unknown_systems: Vec<String>,
    /// The systems a `CodeableConcept`'s codings name that the server does not
    /// serve (`x-unknown-system`, the ecosystem's output for that input).
    pub x_unknown_systems: Vec<String>,
    /// The `codeableConcept` input, echoed (an R5 output, pre-adopted).
    pub codeable_concept: Option<CodeableConceptRef>,
    /// `inactive`: whether the concept is inactive (the ecosystem's output).
    pub inactive: Option<bool>,
    /// `status`: the concept's status when its system states one (the
    /// ecosystem's output).
    pub status: Option<String>,
}

/// The code under validation, from whichever parameter carried it.
/// The input of `ValueSet/$validate-code`: the union of the parameters the
/// served versions declare.
#[derive(Debug, Default)]
pub struct ValueSetValidateInput {
    /// The names of parameters the version declares that the server does not
    /// implement; a request naming one is refused, never absorbed.
    pub unsupported: Vec<String>,
    /// The value set URL (`url`).
    pub url: Option<String>,
    /// The value set version (`valueSetVersion`).
    pub value_set_version: Option<String>,
    /// The inline `valueSet`, converted by the wire layer of its version.
    pub inline_value_set: Option<Result<ValueSetModel, ModelError>>,
    /// `useSupplement` (R5): the loaded supplements to apply, by canonical.
    pub use_supplement: Vec<String>,
    /// Whether `context` was given; not supported.
    pub context: bool,
    /// Whether `date` was given; not supported.
    pub date: bool,
    /// The code.
    pub code: Option<String>,
    /// The code system URI (`system`).
    pub system: Option<String>,
    /// The code system version (`systemVersion`).
    pub system_version: Option<String>,
    /// The display the client asserts.
    pub display: Option<String>,
    /// The coding, instead of `code`.
    pub coding: Option<CodingRef>,
    /// The `codeableConcept`, instead of `code`.
    pub codeable_concept: Option<CodeableConceptRef>,
    /// The language of the display (a BCP 47 range list).
    pub display_language: Option<String>,
    /// `abstract`: whether an abstract code may be selected; the default is true.
    pub abstract_ok: Option<bool>,
    /// `system-version`: the version to use for a system the value set does
    /// not pin, as `[system]|[version]` canonicals (pre-adopted from R6).
    pub default_system_version: Vec<String>,
    /// `check-system-version`: a version a differing value set is refused
    /// against (pre-adopted from R6).
    pub check_system_version: Vec<String>,
    /// `force-system-version`: a version that overrides the value set's
    /// (pre-adopted from R6).
    pub force_system_version: Vec<String>,
    /// `default-valueset-version`: the version to use for a value set
    /// reference that names none (pre-adopted from R6).
    pub default_valueset_version: Vec<String>,
    /// `check-valueset-version`: a value set version a differing reference is
    /// refused against (pre-adopted from R6).
    pub check_valueset_version: Vec<String>,
    /// `force-valueset-version`: a value set version that overrides the
    /// reference's (pre-adopted from R6).
    pub force_valueset_version: Vec<String>,
    /// `inferSystem`: find the system of a bare `code` in the value set
    /// (pre-adopted from R6).
    pub infer_system: Option<bool>,
    /// `lenient-display-validation`: a wrong display is a warning, not a
    /// failure (pre-adopted from R6).
    pub lenient_display_validation: Option<bool>,
    /// `activeOnly`: an inactive concept is outside the value set (the
    /// ecosystem's extension of this operation).
    pub active_only: Option<bool>,
    /// `valueset-membership-only`: only membership is checked, no display or
    /// status (pre-adopted from R6).
    pub membership_only: Option<bool>,
}

/// Refuses a request naming a parameter this server does not answer.
///
/// # Errors
///
/// Returns [`OperationError::NotSupported`] naming the parameter, or the
/// language error when the request's display language is not a language.
fn refuse_unsupported(input: &ValueSetValidateInput) -> Result<(), OperationError> {
    language::check(input.display_language.as_deref())?;
    if let Some(name) = input.unsupported.first() {
        return Err(OperationError::NotSupported(format!(
            "`{name}` is not supported by this server"
        )));
    }
    if input.context {
        return Err(OperationError::NotSupported(String::from(
            "`context` is not supported; name the value set with `url` or `valueSet`",
        )));
    }
    if input.date {
        return Err(OperationError::NotSupported(String::from(
            "`date` is not supported: codes are validated against the versions served now",
        )));
    }
    Ok(())
}

/// The value set at its negotiated version with its systems pinned, and the
/// negotiation itself, for the subjects and the imports.
fn prepare(
    sources: &Sources<'_>,
    input: &ValueSetValidateInput,
) -> Result<(Arc<ValueSetModel>, Negotiation), OperationError> {
    let negotiation = Negotiation::new(
        &input.default_system_version,
        &input.check_system_version,
        &input.force_system_version,
        &input.default_valueset_version,
        &input.check_valueset_version,
        &input.force_valueset_version,
    );
    let (url, version) = match input.url.as_deref() {
        Some(url) => {
            let (url, version) = negotiation.value_set(url, input.value_set_version.as_deref())?;
            (Some(url), version)
        }
        None => (None, input.value_set_version.clone()),
    };
    let model = sources.value_set(
        input.inline_value_set.clone(),
        url.as_deref(),
        version.as_deref(),
    )?;
    let negotiation = negotiation.with_include_versions(&model.compose);
    let model = Arc::new(ValueSetModel {
        compose: negotiation.pin_lenient(&model.compose),
        ..(*model).clone()
    });
    Ok((model, negotiation))
}

/// Refuses a value set that reaches itself through `include.valueSet` or
/// `exclude.valueSet`, the way `$expand` refuses it.
///
/// The walk runs on its own resolver, so it records no value set reference
/// the validation did not make.
///
/// # Errors
///
/// Returns [`OperationError::ValueSetCyclic`] for a circular reference, which
/// the ecosystem classifies `vs-invalid`
/// (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
fn refuse_cyclic(
    sources: &Sources<'_>,
    model: &ValueSetModel,
    negotiation: &Negotiation,
) -> Result<(), OperationError> {
    resolver_for(sources, model, negotiation, None)
        .check_acyclic(&model.canonical(), &model.compose)?;
    Ok(())
}

/// The resolver one request answers its `include.valueSet` references on: the
/// version negotiation, the value sets the resource contains, and the display
/// language every hop answers in.
fn resolver_for<'a>(
    sources: &Sources<'a>,
    model: &'a ValueSetModel,
    negotiation: &'a Negotiation,
    language: Option<&str>,
) -> Resolver<'a> {
    Resolver::new(sources.registry, sources.value_sets)
        .with_negotiation(negotiation)
        .with_contained(&model.contained)
        .with_language(language)
}

/// What one `validate_code` call resolves once, for every phase below to read.
///
/// These are invariant for a whole call: the same sources, the same value
/// set and whether the request carried it inline, the same resolver over it, the
/// same version negotiation, and the same policy. Threading them one by one
/// is what made this file's argument lists what they are.
#[derive(Debug, Clone, Copy)]
struct Context<'a> {
    /// The registries and stores this call reads.
    sources: &'a Sources<'a>,
    /// The value set being validated against.
    model: &'a ValueSetModel,
    /// Whether the request carried the value set inline, so a refused filter
    /// value is one of its own.
    inline: bool,
    /// The resolver the model's references are answered on.
    resolver: &'a Resolver<'a>,
    /// The version negotiation the request asked for.
    negotiation: &'a Negotiation,
    /// How this validation judges what it finds.
    policy: &'a Policy<'a>,
}

/// How one validation judges what it finds.
#[derive(Debug, Clone, Copy)]
struct Policy<'a> {
    /// The language of the display (a BCP 47 range list).
    language: Option<&'a str>,
    /// `abstract`: whether an abstract code may be selected.
    abstract_ok: bool,
    /// How a display is judged.
    display: DisplayCheck,
    /// `valueset-membership-only`: no display, case, or status notes.
    membership_only: bool,
    /// `inferSystem`: find the system of a bare code by its membership in the
    /// value set (pre-adopted from R6).
    infer_system: bool,
    /// What an inactive concept is to the value set.
    inactive: InactivePolicy,
}

/// The policy a request and its value set set for the validation.
fn policy_of<'a>(input: &'a ValueSetValidateInput, model: &'a ValueSetModel) -> Policy<'a> {
    // NOTE: a value set's own language, or its `displayLanguage` expansion
    // default, is the language its displays are judged in when the request
    // names none (the ecosystem's `bad-language-vs` cases).
    let default_language = model
        .expansion_parameters
        .iter()
        .find(|d| d.name == "displayLanguage")
        .map(|d| d.value.as_str())
        .or(model.language.as_deref());
    let membership_only = input.membership_only.unwrap_or(false);
    Policy {
        language: input.display_language.as_deref().or(default_language),
        abstract_ok: input.abstract_ok.unwrap_or(true),
        display: if membership_only {
            DisplayCheck::Skipped
        } else if input.lenient_display_validation.unwrap_or(false) {
            DisplayCheck::Lenient
        } else {
            DisplayCheck::Strict
        },
        membership_only,
        infer_system: input.infer_system.unwrap_or(false),
        inactive: InactivePolicy::of(input.active_only),
    }
}

/// How an asserted display is judged: strictly (an error), leniently
/// (`lenient-display-validation`, a warning), or not at all
/// (`valueset-membership-only`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayCheck {
    Strict,
    Lenient,
    Skipped,
}

/// Whether an inactive concept is a member (the default) or refused
/// (`activeOnly`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InactivePolicy {
    Member,
    Refused,
}

impl InactivePolicy {
    fn of(active_only: Option<bool>) -> Self {
        if active_only.unwrap_or(false) {
            Self::Refused
        } else {
            Self::Member
        }
    }
}

struct Subject<'a> {
    system: Option<&'a str>,
    version: Option<&'a str>,
    code: &'a str,
    display: Option<&'a str>,
    expression: &'a str,
}

/// Runs `$validate-code`.
///
/// # Errors
///
/// Returns [`OperationError`] when the validation cannot be performed: no or
/// several code inputs, no value set, an unsupported parameter, an unknown or
/// invalid value set, or a provider failure.
pub fn validate_code(
    sources: &Sources<'_>,
    input: &ValueSetValidateInput,
) -> Result<Validation, OperationError> {
    refuse_unsupported(input)?;
    let (model, negotiation) = prepare(sources, input)?;
    let mut wanted = input.use_supplement.clone();
    wanted.extend(model.supplements.iter().cloned());
    let registry = sources.with_supplements(&wanted)?;
    let sources = &Sources {
        registry: &registry,
        ..*sources
    };
    refuse_cyclic(sources, &model, &negotiation)?;
    let policy = policy_of(input, &model);
    let resolver = resolver_for(sources, &model, &negotiation, policy.language);
    let context = &Context {
        sources,
        model: &model,
        inline: input.inline_value_set.is_some(),
        resolver: &resolver,
        negotiation: &negotiation,
        policy: &policy,
    };
    let check =
        |subject: &Subject<'_>| -> Result<Validation, OperationError> { check(context, subject) };
    let inputs = usize::from(input.code.is_some())
        + usize::from(input.coding.is_some())
        + usize::from(input.codeable_concept.is_some());
    if inputs != 1 {
        return Err(OperationError::Invalid(String::from(
            "provide one and only one of `code`, `coding`, or `codeableConcept`",
        )));
    }
    if let Some(code) = input.code.as_deref() {
        // NOTE: "If a code is provided, a system or a context must be provided"
        // (<https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>, `code`); `inferSystem` asks for it.
        if input.system.is_none() && !policy.infer_system {
            return Err(OperationError::Invalid(String::from(
                "a `code` needs a `system`, or `inferSystem` to infer one from the value set",
            )));
        }
        let subject = Subject {
            system: input.system.as_deref(),
            version: input.system_version.as_deref(),
            code,
            display: input.display.as_deref(),
            expression: "code",
        };
        return check(&subject);
    }
    if let Some(coding) = &input.coding {
        let Some(code) = coding.code.as_deref() else {
            return Ok(refusal::failed(
                coding.system.clone(),
                None,
                super::codeless(coding, "coding"),
            ));
        };
        let subject = Subject {
            system: coding.system.as_deref().or(input.system.as_deref()),
            version: coding
                .version
                .as_deref()
                .or(input.system_version.as_deref()),
            code,
            display: coding.display.as_deref().or(input.display.as_deref()),
            expression: "coding",
        };
        return check(&subject);
    }
    let concept = input
        .codeable_concept
        .as_ref()
        .ok_or_else(|| OperationError::Invalid(String::from("no code input")))?;
    let codings = &concept.coding;
    let mut judged = Vec::with_capacity(codings.len());
    let mut codeless = Vec::new();
    for (index, coding) in codings.iter().enumerate() {
        let base = format!("CodeableConcept.coding[{index}]");
        let Some(code) = coding.code.as_deref() else {
            codeless.push(super::codeless(coding, &base));
            continue;
        };
        let subject = Subject {
            system: coding.system.as_deref(),
            version: coding.version.as_deref(),
            code,
            display: coding.display.as_deref(),
            expression: &base,
        };
        judged.push((coding, check(&subject)?));
    }
    // NOTE: a `CodeableConcept` with no coding to judge is a valid input that
    // holds no valid code, so it fails against the value set rather than
    // refusing the request (<https://hl7.org/fhir/R4B/datatypes.html#CodeableConcept>).
    let mut validation = if judged.is_empty() {
        combine::no_valid_coding(&model)
    } else {
        combine(&model, &judged)
    };
    if !codeless.is_empty() {
        validation.issues.extend(codeless);
        validation.message = message_of(&validation.issues);
    }
    validation.codeable_concept = Some(concept.clone());
    Ok(validation)
}

/// Validates one subject against the value set.
/// The subject's system: the one it names, else the one inferred from the
/// value set (by membership under `inferSystem`, else the value set's only
/// system); the failed validation when none can be found.
fn subject_system(
    context: &Context<'_>,
    subject: &Subject<'_>,
) -> Result<Result<String, Box<Validation>>, OperationError> {
    let Context { model, policy, .. } = *context;
    if let Some(system) = subject.system {
        return Ok(Ok(system.to_owned()));
    }
    // NOTE: a `Coding` without a system has no meaning to validate; only a bare
    // `code` has its system inferred (<https://hl7.org/fhir/R5/valueset-operation-validate-code.html>).
    if subject.expression != "code" {
        return Ok(Err(Box::new(no_system(model, subject))));
    }
    let inferred = if policy.infer_system {
        infer_by_membership(context, subject, policy.language)?
    } else {
        infer_system(model).into_iter().collect()
    };
    Ok(match inferred.as_slice() {
        [one] => Ok(one.clone()),
        several => Err(Box::new(cannot_infer(model, subject, several))),
    })
}

/// Checks one code against the value set: its system, the code, membership,
/// and the display, in order.
fn check(context: &Context<'_>, subject: &Subject<'_>) -> Result<Validation, OperationError> {
    let Context {
        model,
        resolver,
        policy,
        ..
    } = *context;
    let language = policy.language;
    let system = match subject_system(context, subject)? {
        Ok(system) => system,
        Err(unresolved) => return Ok(*unresolved),
    };
    let mut target = match resolve_target(context, &system, subject) {
        Ok(target) => target,
        Err(unserved) => return Ok(*unserved),
    };
    if target.resolvable && !target.alternatives.is_empty() {
        let alternatives = std::mem::take(&mut target.alternatives);
        let candidates = std::iter::once(Arc::clone(&target.provider)).chain(alternatives);
        if let Some(found) = containing_version(context, &system, subject, language, candidates) {
            target.provider = found;
        }
    }
    let provider: &Arc<dyn CodeSystemProvider> = &target.provider;
    let version = provider.identity().version.clone();
    let Some(located) = provider.locate(subject.code)? else {
        let mut validation = if fragment_may_admit(provider, model, &system) {
            unlisted_in_fragment(provider, &system, version, subject, policy)
        } else {
            unknown_code(model, provider, &system, version, subject, policy)
        };
        validation.issues.splice(0..0, target.issues);
        validation.result &= !validation
            .issues
            .iter()
            .any(|issue| issue.severity == "error");
        validation.message = message_of(&validation.issues);
        validation.unknown_systems.extend(target.unknown_systems);
        return Ok(validation);
    };
    let display = provider.display(
        located.concept,
        language::for_provider(provider.as_ref(), language).as_deref(),
    )?;
    if !target.resolvable {
        return Ok(with_target(
            failed_target(&system, version, target),
            &located,
            display,
        ));
    }
    let contained = match resolver.contains_compose(
        &model.canonical(),
        &model.compose,
        &system,
        Some(&version),
        &located.code,
        language,
    ) {
        Ok(contained) => contained,
        // NOTE: a value set the compose imports but the server does not hold is a
        // failed validation naming it, the ecosystem's shape (its test cases);
        // an unknown top-level value set stays an error.
        Err(crate::compose::ComposeError::UnknownValueSet(url)) => {
            return Ok(unknown_import(&system, version, &url, &located.code));
        }
        Err(error) => {
            return Err(OperationError::of_compose(
                error,
                context.inline.then_some(&model.compose),
            ));
        }
    };
    let Some(item) = contained else {
        let mut validation = outside_value_set(
            context,
            &system,
            version,
            &located,
            display.as_deref(),
            subject,
            &provider.status(located.concept)?,
        );
        validation.issues.splice(0..0, target.issues);
        validation.message = message_of(&validation.issues);
        validation.unknown_systems.extend(target.unknown_systems);
        return Ok(validation);
    };
    conclude(
        context,
        subject,
        target,
        &located,
        &item,
        display,
        Resolved { system, version },
    )
}

/// The validation of a code the value set contains: the notes about the code,
/// the value set, and the code system, then the outputs.
fn conclude(
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
        issues.extend(super::standing_note(
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
        expression: super::at(subject.expression, "code"),
    })
}

fn inactive_outputs(concept_status: &crate::provider::Status) -> (Option<bool>, Option<String>) {
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
        notes.extend(super::standing_note("ValueSet", canonical, &standing));
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

/// The version of `system` (among `candidates`, the ones the value set
/// includes) the subject is validated in, the greatest first, for a subject
/// that names no version (the ecosystem's `overload` cases).
///
/// A version that has the code in the value set AND accepts the asserted
/// display answers first, then one that has the code, then the greatest: the
/// `version` output is "the version of the system of the code that was
/// validated", so the choice among the versions a value set includes belongs
/// to the validation
/// (<https://hl7.org/fhir/R5/valueset-operation-validate-code.html>).
fn containing_version(
    context: &Context<'_>,
    system: &str,
    subject: &Subject<'_>,
    language: Option<&str>,
    candidates: impl Iterator<Item = Arc<dyn CodeSystemProvider>>,
) -> Option<Arc<dyn CodeSystemProvider>> {
    let Context { .. } = *context;
    let mut candidates: Vec<Arc<dyn CodeSystemProvider>> = candidates.collect();
    candidates.sort_by(|a, b| {
        crate::versioned::version_order(&b.identity().version, &a.identity().version)
    });
    let holding: Vec<&Arc<dyn CodeSystemProvider>> = candidates
        .iter()
        .filter(|candidate| holds_in_value_set(context, system, subject.code, language, candidate))
        .collect();
    holding
        .iter()
        .find(|candidate| accepts_display(candidate, subject, language))
        .or_else(|| holding.first())
        .map(|found| Arc::clone(found))
        .or_else(|| candidates.first().cloned())
}

/// Whether `candidate` has `code` and the value set contains it at that
/// version.
fn holds_in_value_set(
    context: &Context<'_>,
    system: &str,
    code: &str,
    language: Option<&str>,
    candidate: &Arc<dyn CodeSystemProvider>,
) -> bool {
    let Context {
        model, resolver, ..
    } = *context;
    // NOTE: a candidate whose provider or compose fails to answer is not the
    // version to validate in, and the chosen version answers for itself.
    candidate
        .locate(code)
        .ok()
        .flatten()
        .is_some_and(|located| {
            resolver
                .contains_compose(
                    &model.canonical(),
                    &model.compose,
                    system,
                    Some(&candidate.identity().version),
                    &located.code,
                    language,
                )
                .ok()
                .flatten()
                .is_some()
        })
}

/// Whether `candidate` accepts the display the subject asserted; true when
/// the subject asserted none.
fn accepts_display(
    candidate: &Arc<dyn CodeSystemProvider>,
    subject: &Subject<'_>,
    language: Option<&str>,
) -> bool {
    let Some(given) = subject.display else {
        return true;
    };
    let Ok(Some(located)) = candidate.locate(subject.code) else {
        return false;
    };
    super::display::judge(
        candidate,
        located.concept,
        super::display::Asserted {
            system: &candidate.identity().url,
            code: &located.code,
            given,
            requested: language,
            lenient: false,
        },
        None,
    )
    .is_ok_and(|issue| issue.is_none())
}

/// The code system version a subject was resolved to.
///
/// The two are decided together and are carried together from there on, so
/// they travel as one rather than as a pair of strings a caller could swap.
struct Resolved {
    /// The system the code was read in.
    system: String,
    /// The version of it the validation ran against.
    version: String,
}

/// The version a subject is validated against and what the choice cost.
struct Target {
    /// The provider at that version.
    provider: Arc<dyn CodeSystemProvider>,
    /// The other include versions of the system to try when the subject names
    /// none and the value set includes the system at several versions.
    alternatives: Vec<Arc<dyn CodeSystemProvider>>,
    /// Whether the include's own version resolved (else the subject's or the
    /// default stands in, and the validation fails regardless of membership).
    resolvable: bool,
    /// The version disagreements, in the order the ecosystem itemises them.
    issues: Vec<Issue>,
    /// The canonicals for `x-caused-by-unknown-system`.
    unknown_systems: Vec<String>,
}
