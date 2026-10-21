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

use std::sync::Arc;

use super::{CodeableConceptRef, CodingRef, Issue, OperationError, Sources};
use crate::compose::Item;
use crate::language;
use crate::provider::{CodeSystemProvider, ContentMode, Located};
use crate::valueset::model::{ModelError, ValueSetModel};
use crate::valueset::negotiation::Negotiation;
use crate::valueset::store::Resolver;
use crate::versioned::Versioned;

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
        let code = coding
            .code
            .as_deref()
            .ok_or_else(|| OperationError::Required(String::from("`coding.code` is required")))?;
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
    if codings.is_empty() {
        return Err(OperationError::Required(String::from(
            "`codeableConcept` carries no `coding`",
        )));
    }
    let mut judged = Vec::with_capacity(codings.len());
    for (index, coding) in codings.iter().enumerate() {
        let Some(code) = coding.code.as_deref() else {
            continue;
        };
        let base = format!("CodeableConcept.coding[{index}]");
        let subject = Subject {
            system: coding.system.as_deref(),
            version: coding.version.as_deref(),
            code,
            display: coding.display.as_deref(),
            expression: &base,
        };
        judged.push((coding, check(&subject)?));
    }
    if judged.is_empty() {
        return Err(OperationError::Required(String::from(
            "`codeableConcept.coding.code` is required",
        )));
    }
    let mut validation = combine(&model, &judged);
    validation.codeable_concept = Some(concept.clone());
    Ok(validation)
}

/// Whether a judged coding is in the value set (its display may still be
/// wrong, and a warning is a note about the code, never a verdict against it).
fn in_value_set(validation: &Validation) -> bool {
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
fn take_unknown(from: &Validation, into: &mut Validation) {
    into.unknown_systems
        .extend(from.unknown_systems.iter().cloned());
    into.x_unknown_systems
        .extend(from.x_unknown_systems.iter().cloned());
}

/// The `information` issue that a coding is not in the value set.
fn this_code_not_in_vs(model: &ValueSetModel, coding: &CodingRef, base: &str) -> Issue {
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
        expression: super::at(base, "code"),
    }
}

/// One answer for a `CodeableConcept` from its codings' judgements, the
/// ecosystem's shape (its test cases): the first coding in the value set
/// answers the code outputs and the other codings' issues join it; when none
/// is, one `not-in-vs` error, each coding's own errors, and an `information`
/// per coding, with no code outputs.
fn combine(model: &ValueSetModel, judged: &[(&CodingRef, Validation)]) -> Validation {
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
fn version_mismatch(judged: &[(&CodingRef, Validation)]) -> Option<Validation> {
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
fn failed_import(judged: &[(&CodingRef, Validation)]) -> Option<Validation> {
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
fn include_version_unserved(
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
fn first_in_value_set(
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
fn none_in_value_set(model: &ValueSetModel, judged: &[(&CodingRef, Validation)]) -> Validation {
    let base = |index: usize| format!("CodeableConcept.coding[{index}]");
    let mut answer = failed(
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
    );
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

/// The `message`: the issues that speak for the outcome, joined with `; `.
///
/// Every error and the warnings about the data, the display, or the concept's
/// standing join, in the ecosystem's order (the missing system first, the
/// membership verdict late); an information about the display speaks alone
/// when nothing else does. Status notes and code warnings stay issues.
fn message_of(issues: &[Issue]) -> Option<String> {
    const ORDER: [&str; 11] = [
        "not-found",
        "invalid-data",
        "vs-invalid",
        "version-error",
        "cannot-infer",
        "code-comment",
        "code-rule",
        "not-in-vs",
        "invalid-code",
        "display-comment",
        "invalid-display",
    ];
    const WARNINGS: [&str; 4] = [
        "invalid-data",
        "code-comment",
        "display-comment",
        "invalid-display",
    ];
    let speaks = |issue: &&Issue| {
        issue.severity == "error" || (issue.severity == "warning" && WARNINGS.contains(&issue.kind))
    };
    let rank = |kind: &str| ORDER.iter().position(|k| *k == kind).unwrap_or(ORDER.len());
    let mut speaking: Vec<&Issue> = issues.iter().filter(speaks).collect();
    speaking.sort_by_key(|issue| rank(issue.kind));
    if speaking.is_empty() {
        return issues
            .iter()
            .find(|i| i.severity == "information" && i.kind == "invalid-display")
            .map(|i| i.text.clone());
    }
    let texts: Vec<&str> = speaking.iter().map(|i| i.text.as_str()).collect();
    Some(texts.join("; "))
}

/// The version of `system` the value set uses for this subject, negotiated,
/// and the issues a subject version that disagrees or a version the server
/// does not serve raise (the ecosystem's `version` cases).
///
/// Returns the unserved-system validation when neither the include's version
/// nor the subject's nor a default resolves.
fn resolve_target(
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
            expression: super::at(expression, "version"),
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
fn include_literal_for<'m>(
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
fn disagreement(
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
    let (canonical, not_found) =
        super::unknown_system(system, Some(sv), super::at(expression, "system"), valid);
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
fn unresolvable_include(
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
    let (canonical, not_found) =
        super::unknown_system(system, Some(&bad), super::at(expression, "system"), valid);
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
fn negotiation_original(
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
fn vs_invalid(
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
        expression: super::at(expression, "version"),
    }
}

/// The failed validation of a subject whose system is a supplement, which
/// defines no codes of its own (the ecosystem's `bad-supplement-url` case).
fn supplement_as_system(system: &str, version: Option<&str>, subject: &Subject<'_>) -> Validation {
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
            expression: super::at(subject.expression, "system"),
        },
    );
    validation.code = Some(subject.code.to_owned());
    validation.unknown_systems.push(system.to_owned());
    validation
}

/// The failed validation of a subject whose system or version the server does
/// not serve at all.
fn unserved_subject(
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
    let version = subject
        .version
        .filter(|_| registry.resolve(system, None).is_ok());
    let (canonical, not_found) = super::unknown_system(
        system,
        version,
        super::at(subject.expression, "system"),
        valid,
    );
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
            expression: super::at(subject.expression, "system"),
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
fn local_system(system: &str, expression: &str) -> Option<Issue> {
    let absolute = system.contains(':');
    (!absolute).then(|| Issue {
        severity: "error",
        code: "invalid",
        kind: "invalid-data",
        message: crate::operations::MessageId::TerminologyTxSystemRelative,
        text: String::from("Coding.system must be an absolute reference, not a local reference"),
        expression: super::at(expression, "system"),
    })
}

/// The failed validation over an include whose version the server does not
/// serve, answered against `version` (the subject's or the default).
fn failed_target(system: &str, version: String, target: Target) -> Validation {
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
fn with_target(
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
fn assess(
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
        && let Some(note) = super::display::case_note(
            provider,
            subject.code,
            &located.code,
            super::at(subject.expression, "code"),
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
            expression: super::at(subject.expression, "code"),
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
                expression: super::at(subject.expression, "code"),
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
            && let Some((note, _)) =
                super::inactive_note(&located.code, &status, super::whole(subject.expression))
        {
            issues.push(note);
        }
    } else if !policy.membership_only
        && let Some((note, _)) =
            super::deprecated_note(&located.code, &status, super::whole(subject.expression))
    {
        issues.push(note);
    }
    // NOTE: under `lenient-display-validation` a wrong display does not fail the
    // result (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-validate-code.html>).
    if let Some(given) = subject.display
        && policy.display != DisplayCheck::Skipped
        && let Some(issue) = super::display::judge(
            provider,
            located.concept,
            super::display::Asserted {
                system: &item.system,
                code: &located.code,
                given,
                requested: language,
                lenient: policy.display == DisplayCheck::Lenient,
            },
            super::at(subject.expression, "display"),
        )?
    {
        issues.push(issue);
    }
    Ok(issues)
}

/// The one system a value set draws on, when there is exactly one.
fn infer_system(model: &ValueSetModel) -> Option<String> {
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
fn infer_by_membership(
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
fn no_system(model: &ValueSetModel, subject: &Subject<'_>) -> Validation {
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
        text: String::from(
            "Coding has no system. A code with no system has no defined meaning, and it cannot be validated. A system should be provided",
        ),
        expression: super::at(subject.expression, "system"),
    });
    validation.message = message_of(&validation.issues);
    validation.code = Some(subject.code.to_owned());
    validation
}

fn cannot_infer(model: &ValueSetModel, subject: &Subject<'_>, matches: &[String]) -> Validation {
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
        expression: super::at(subject.expression, "code"),
    });
    validation.code = Some(subject.code.to_owned());
    validation.message = message_of(&validation.issues);
    validation
}

fn system_code(system: &str, code: &str) -> String {
    format!("{system}#{code}")
}

/// `S#C`, or `S#C ('display')` when the client asserted a display, as the
/// ecosystem names a code in its texts.
fn named_code(system: &str, code: &str, display: Option<&str>) -> String {
    match display {
        Some(display) => format!("{system}#{code} ('{display}')"),
        None => format!("{system}#{code}"),
    }
}

/// The issue for a code the value set does not contain.
fn not_in_vs(
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
        expression: super::at(expression, "code"),
    }
}

/// A `result = false` validation carrying `issue`.
fn failed(system: Option<String>, version: Option<String>, issue: Issue) -> Validation {
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
fn unknown_import(system: &str, version: String, url: &str, code: &str) -> Validation {
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
fn fragment_may_admit(
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
fn unlisted_in_fragment(
    provider: &Arc<dyn CodeSystemProvider>,
    system: &str,
    version: String,
    subject: &Subject<'_>,
    policy: &Policy<'_>,
) -> Validation {
    let (message, text) = super::display::unknown_code(provider.as_ref(), subject.code);
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
            expression: super::at(subject.expression, "code"),
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
fn unknown_code(
    model: &ValueSetModel,
    provider: &Arc<dyn CodeSystemProvider>,
    system: &str,
    version: String,
    subject: &Subject<'_>,
    policy: &Policy<'_>,
) -> Validation {
    let (message, text) = super::display::unknown_code(provider.as_ref(), subject.code);
    let unknown = Issue {
        severity: "error",
        code: "code-invalid",
        kind: "invalid-code",
        message,
        text,
        expression: super::at(subject.expression, "code"),
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
fn outside_value_set(
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
            expression: super::at(expression, "code"),
        });
    }
    issues.push(not_in_vs(model, system, &located.code, given, expression));
    if !policy.membership_only {
        if let Some((note, _)) =
            super::inactive_note(&located.code, concept_status, super::whole(expression))
        {
            issues.push(note);
        } else if let Some((note, _)) =
            super::deprecated_note(&located.code, concept_status, super::whole(expression))
        {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(severity: &'static str, kind: &'static str, text: &str) -> Issue {
        Issue {
            severity,
            code: "invalid",
            kind,
            message: crate::operations::MessageId::TxGeneralError,
            text: text.to_owned(),
            expression: None,
        }
    }

    #[test]
    fn the_message_orders_the_issues_the_ecosystems_way() {
        let issues = [
            issue("error", "not-in-vs", "not in vs"),
            issue("error", "invalid-code", "unknown code"),
        ];
        assert_eq!(
            message_of(&issues).as_deref(),
            Some("not in vs; unknown code")
        );
        let issues = [
            issue("error", "not-in-vs", "not in vs"),
            issue("error", "invalid-data", "bad data"),
            issue("error", "not-found", "missing"),
            issue("warning", "vs-invalid", "quiet warning"),
            issue("information", "status-check", "draft"),
        ];
        assert_eq!(
            message_of(&issues).as_deref(),
            Some("missing; bad data; not in vs")
        );
        let issues = [issue("warning", "invalid-code", "fragment")];
        assert_eq!(message_of(&issues), None);
    }

    #[test]
    fn the_message_falls_back_to_the_first_information_issue() {
        let issues = [Issue {
            severity: "information",
            code: "invalid",
            kind: "invalid-display",
            message: crate::operations::MessageId::NoValidDisplayFoundNoneForLangOk,
            text: String::from("There are no valid display names found"),
            expression: None,
        }];
        assert_eq!(
            message_of(&issues).as_deref(),
            Some("There are no valid display names found")
        );
    }
}
