// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `CodeSystem/$validate-code` in the terms every served FHIR version shares.
//!
//! One of `code` (with `url`), `coding`, or `codeableConcept` is checked
//! against a served system; the outcome says whether the code is in the
//! system, why not, and the display the system prefers
//! (<https://hl7.org/fhir/R4B/codesystem-operation-validate-code.html>,
//! <https://hl7.org/fhir/R5/codesystem-operation-validate-code.html>).

use super::{CodeableConceptRef, CodingRef, Invocation, Issue, OperationError, resolve};
use crate::language;
use crate::provider::{CodeSystemProvider, ContentMode};
use crate::registry::Registry;

/// The input of `CodeSystem/$validate-code`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidateCodeInput {
    /// The code system URI (`url`).
    pub url: Option<String>,
    /// The code system version.
    pub version: Option<String>,
    /// Whether an inline `codeSystem` resource was given; not supported.
    pub inline_code_system: bool,
    /// The code.
    pub code: Option<String>,
    /// The display the client asserts.
    pub display: Option<String>,
    /// The coding, instead of `code`.
    pub coding: Option<CodingRef>,
    /// The `codeableConcept`, instead of `code`.
    pub codeable_concept: Option<CodeableConceptRef>,
    /// The language of the display (a BCP 47 range list).
    pub display_language: Option<String>,
    /// `lenient-display-validation`: a wrong display is a warning and the
    /// result stays true (the ecosystem's extension of this operation).
    pub lenient_display: bool,
}

/// The outcome of `CodeSystem/$validate-code`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationOutcome {
    /// Whether the code is valid in the system.
    pub result: bool,
    /// Why the result is what it is, when there is something to say.
    pub message: Option<String>,
    /// The display the system prefers, when the code was found.
    pub display: Option<String>,
    /// The code that was checked, when one was.
    pub code: Option<String>,
    /// The code as the system spells it when `code` is spelled otherwise
    /// (`normalized-code`, the ecosystem's output).
    pub normalized_code: Option<String>,
    /// The system it was checked against.
    pub system: Option<String>,
    /// The version it was checked against.
    pub version: Option<String>,
    /// The itemised issues, for the versions that declare `issues`
    /// (<https://hl7.org/fhir/R5/codesystem-operation-validate-code.html>).
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

/// The `result = false` outcome for a system or version the server does not
/// serve: the code is echoed, and the system is named for the validator.
fn unserved(
    registry: &Registry,
    url: &str,
    version: Option<&str>,
    code: Option<&str>,
    expression: &str,
) -> ValidationOutcome {
    let valid: Vec<String> = registry
        .versions(url)
        .map(|p| p.identity().version.clone())
        .collect();
    let (canonical, issue) =
        super::unknown_system(url, version, super::at(expression, "system"), &valid);
    ValidationOutcome {
        normalized_code: None,
        result: false,
        message: Some(issue.text.clone()),
        display: None,
        code: code.map(str::to_owned),
        system: Some(url.to_owned()),
        version: version.map(str::to_owned),
        issues: vec![issue],
        unknown_systems: vec![canonical],
        x_unknown_systems: Vec::new(),
        codeable_concept: None,
        inactive: None,
        status: None,
    }
}

/// The failed validation of a system that is a supplement: bad data, and the
/// supplement named as the system the client lacks (the ecosystem's
/// `bad-supplement-url` case).
fn supplement_as_system(
    url: &str,
    version: Option<&str>,
    code: Option<&str>,
    expression: &str,
) -> ValidationOutcome {
    let canonical = match version {
        Some(version) => format!("{url}|{version}"),
        None => url.to_owned(),
    };
    let text = format!(
        "CodeSystem {canonical} is a supplement, so can't be used as a value in Coding.system"
    );
    ValidationOutcome {
        normalized_code: None,
        result: false,
        message: Some(text.clone()),
        display: None,
        code: code.map(str::to_owned),
        system: Some(url.to_owned()),
        version: None,
        issues: vec![Issue {
            severity: "error",
            code: "invalid",
            kind: "invalid-data",
            message: super::MessageId::CodeSystemCsNoSupplement,
            text,
            expression: super::at(expression, "system"),
        }],
        unknown_systems: vec![url.to_owned()],
        x_unknown_systems: Vec::new(),
        codeable_concept: None,
        inactive: None,
        status: None,
    }
}

/// Resolves the system for a validation; a system or version the server does
/// not serve is a `false` result, never an error (the ecosystem's rule).
fn resolve_for(
    registry: &Registry,
    invocation: &Invocation,
    url: Option<&str>,
    version: Option<&str>,
    code: Option<&str>,
    expression: &str,
) -> Result<Result<super::Resolved, ValidationOutcome>, OperationError> {
    match resolve(registry, invocation, url, version) {
        Ok(resolved) => Ok(Ok(resolved)),
        Err(OperationError::UnknownSystem(url)) => {
            if let Some(supplement_version) = registry.supplement_named(&url) {
                return Ok(Err(supplement_as_system(
                    &url,
                    supplement_version.version.as_deref(),
                    code,
                    expression,
                )));
            }
            Ok(Err(unserved(registry, &url, None, code, expression)))
        }
        Err(OperationError::UnknownVersion { url, version, .. }) => Ok(Err(unserved(
            registry,
            &url,
            Some(&version),
            code,
            expression,
        ))),
        Err(error) => Err(error),
    }
}

/// Runs `$validate-code`.
///
/// # Errors
///
/// Returns [`OperationError`] for an inline `codeSystem`, none or more than
/// one of the code inputs, a coding whose system contradicts `url`, or a
/// provider failure. An unknown code, system, or version is a `false`
/// result, never an error.
pub fn validate_code(
    registry: &Registry,
    invocation: &Invocation,
    input: &ValidateCodeInput,
) -> Result<ValidationOutcome, OperationError> {
    language::check(input.display_language.as_deref())?;
    if input.inline_code_system {
        return Err(OperationError::NotSupported(String::from(
            "validating against an inline `codeSystem` resource is not supported; name a served system with `url`",
        )));
    }
    let inputs = usize::from(input.code.is_some())
        + usize::from(input.coding.is_some())
        + usize::from(input.codeable_concept.is_some());
    if inputs != 1 {
        return Err(OperationError::Invalid(String::from(
            "provide one and only one of `code`, `coding`, or `codeableConcept`",
        )));
    }
    let url = input.url.as_deref();
    let version = input.version.as_deref();
    let language = input.display_language.as_deref();
    if let Some(code) = input.code.as_deref() {
        let resolved = match resolve_for(registry, invocation, url, version, Some(code), "code")? {
            Ok(resolved) => resolved,
            Err(unserved) => return Ok(unserved),
        };
        return check(
            &resolved.provider,
            code,
            input.display.as_deref(),
            language,
            input.lenient_display,
            "code",
        );
    }
    if let Some(coding) = &input.coding {
        if let (Some(url), Some(system)) = (url, coding.system.as_deref())
            && url != system
        {
            return Err(OperationError::Invalid(format!(
                "`coding.system` `{system}` does not match `url` `{url}`"
            )));
        }
        let code = coding
            .code
            .as_deref()
            .ok_or_else(|| OperationError::Required(String::from("`coding.code` is required")))?;
        let resolved = match resolve_for(
            registry,
            invocation,
            url.or(coding.system.as_deref()),
            coding.version.as_deref().or(version),
            Some(code),
            "coding",
        )? {
            Ok(resolved) => resolved,
            Err(unserved) => return Ok(unserved),
        };
        return check(
            &resolved.provider,
            code,
            coding.display.as_deref().or(input.display.as_deref()),
            language,
            input.lenient_display,
            "coding",
        );
    }
    let Some(concept) = &input.codeable_concept else {
        return Err(OperationError::Required(String::from(
            "provide one of `code`, `coding`, or `codeableConcept`",
        )));
    };
    let resolved = match resolve_for(registry, invocation, url, version, None, "codeableConcept")? {
        Ok(resolved) => resolved,
        Err(unserved) => return Ok(unserved),
    };
    let mut outcome = check_codeable_concept(&resolved.provider, &concept.coding, language)?;
    outcome.codeable_concept = Some(concept.clone());
    Ok(outcome)
}

/// Validates the codings of a `CodeableConcept` that name the system: the
/// first valid one answers, else the failures are joined.
fn check_codeable_concept(
    provider: &std::sync::Arc<dyn CodeSystemProvider>,
    codings: &[CodingRef],
    language: Option<&str>,
) -> Result<ValidationOutcome, OperationError> {
    let identity = provider.identity();
    let target = identity.url.clone();
    let mut messages = Vec::new();
    let mut any = false;
    for (index, coding) in codings.iter().enumerate() {
        if coding.system.as_deref().is_some_and(|s| s != target) {
            continue;
        }
        let Some(code) = coding.code.as_deref() else {
            continue;
        };
        any = true;
        let base = format!("CodeableConcept.coding[{index}]");
        let outcome = check(
            provider,
            code,
            coding.display.as_deref(),
            language,
            false,
            &base,
        )?;
        if outcome.result {
            return Ok(outcome);
        }
        if let Some(message) = outcome.message {
            messages.push(message);
        }
    }
    let message = if any {
        messages.join("; ")
    } else {
        format!("no coding of the CodeableConcept is in code system `{target}`")
    };
    Ok(ValidationOutcome {
        result: false,
        message: Some(message.clone()),
        display: None,
        code: None,
        normalized_code: None,
        system: Some(target),
        version: Some(identity.version.clone()),
        issues: vec![Issue {
            severity: "error",
            code: "code-invalid",
            kind: "invalid-code",
            message: if identity.version.is_empty() {
                super::MessageId::UnknownCodeIn
            } else {
                super::MessageId::UnknownCodeInVersion
            },
            text: message,
            expression: None,
        }],
        unknown_systems: Vec::new(),
        x_unknown_systems: Vec::new(),
        codeable_concept: None,
        inactive: None,
        status: None,
    })
}

/// Checks one code against a system: found, its display matches when one was
/// asserted, and inactive is a warning, never a `false`.
fn check(
    provider: &std::sync::Arc<dyn CodeSystemProvider>,
    code: &str,
    display: Option<&str>,
    language: Option<&str>,
    lenient: bool,
    expression: &str,
) -> Result<ValidationOutcome, OperationError> {
    let identity = provider.identity();
    let version = Some(identity.version.clone()).filter(|v| !v.is_empty());
    let Some(located) = provider.locate(code)? else {
        let (id, text) = super::display::unknown_code(provider.as_ref(), code);
        // A `fragment` resource carries "a subset of the code system concepts"
        // (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>), so a code
        // it does not carry is one the server cannot call invalid.
        let fragment = provider.declaration().content == ContentMode::Fragment;
        return Ok(ValidationOutcome {
            result: fragment,
            message: (!fragment).then(|| text.clone()),
            display: None,
            code: Some(code.to_owned()),
            normalized_code: None,
            system: Some(identity.url.clone()),
            version,
            issues: vec![Issue {
                severity: if fragment { "warning" } else { "error" },
                code: "code-invalid",
                kind: "invalid-code",
                message: id,
                text,
                expression: super::at(expression, "code"),
            }],
            unknown_systems: Vec::new(),
            x_unknown_systems: Vec::new(),
            codeable_concept: None,
            inactive: None,
            status: None,
        });
    };
    let concept = located.concept;
    let requested_language = language;
    let language = language::for_provider(provider.as_ref(), language);
    let language = language.as_deref();
    let preferred = provider.display(concept, language)?;
    let mut messages = Vec::new();
    let mut issues = Vec::new();
    let mut result = true;
    if let Some(note) =
        super::display::case_note(provider, code, &located.code, super::at(expression, "code"))
    {
        issues.push(note);
    }
    if let Some(display) = display
        && let Some(issue) = super::display::judge(
            provider,
            concept,
            super::display::Asserted {
                system: &identity.url,
                code: &located.code,
                given: display,
                requested: requested_language,
                lenient,
            },
            super::at(expression, "display"),
        )?
    {
        if issue.severity == "error" {
            result = false;
        }
        messages.push(issue.text.clone());
        issues.push(issue);
    }
    let status = provider.status(concept)?;
    let mut inactive = None;
    let mut status_code = None;
    if let Some((note, code_status)) =
        super::inactive_note(&located.code, &status, super::whole(expression))
    {
        messages.push(note.text.clone());
        issues.push(note);
        inactive = Some(true);
        status_code = code_status;
    } else if let Some((note, code_status)) =
        super::deprecated_note(&located.code, &status, super::whole(expression))
    {
        messages.push(note.text.clone());
        issues.push(note);
        status_code = Some(code_status);
    }
    let canonical = format!("{}|{}", identity.url, identity.version);
    issues.extend(super::standing_note(
        "CodeSystem",
        &canonical,
        &provider.standing(),
    ));
    Ok(ValidationOutcome {
        result,
        message: (!messages.is_empty()).then(|| messages.join("; ")),
        display: preferred,
        code: Some(code.to_owned()),
        normalized_code: (located.code != code).then(|| located.code.clone()),
        system: Some(identity.url.clone()),
        version,
        issues,
        unknown_systems: Vec::new(),
        x_unknown_systems: Vec::new(),
        codeable_concept: None,
        inactive,
        status: status_code,
    })
}
