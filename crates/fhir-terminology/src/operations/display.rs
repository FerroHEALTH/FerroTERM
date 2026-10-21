// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The judgement of a display a client asserts, in the terminology
//! ecosystem's words.
//!
//! No FHIR version fixes the wording of a display mismatch; the ecosystem's
//! test cases do, and a validator shows them to its user, so the texts follow
//! the reference server's (spec-silent, recorded on #183).

use std::sync::Arc;

use super::Issue;
use crate::language;
use crate::provider::{CodeSystemProvider, Concept, ContentMode, ProviderError};
use crate::text_match;

/// One display a concept accepts, with the language it is in.
struct Candidate {
    text: String,
    language: Option<String>,
    /// Whether the message offers it as a display to use. A designation the
    /// concept accepts is not always one to suggest.
    offered: bool,
}

/// The designation uses that make a designation a display to offer.
///
/// A designation with no use is a display; a use of `preferredForLanguage`, or
/// SNOMED CT's fully specified name, preferred term, or synonym, is a display;
/// any other use is a term the concept carries for some other purpose, so it is
/// accepted as a display and never suggested as one. No FHIR version fixes
/// this, and the terminology ecosystem IG says the behaviour "is effectively
/// specified by the test cases"
/// (<https://hl7.org/fhir/uv/tx-ecosystem/languages.html>), which is where the
/// list comes from (#290).
const DISPLAY_USES: [(&str, &str); 4] = [
    (
        "http://terminology.hl7.org/CodeSystem/hl7TermMaintInfra",
        "preferredForLanguage",
    ),
    ("http://snomed.info/sct", "900000000000003001"),
    ("http://snomed.info/sct", "900000000000548007"),
    ("http://snomed.info/sct", "900000000000013009"),
];

/// Whether a designation's use makes it a display to offer.
fn offered(use_: Option<&crate::provider::DesignationUse>) -> bool {
    let Some(use_) = use_ else {
        return true;
    };
    DISPLAY_USES
        .iter()
        .any(|(system, code)| *system == use_.system && *code == use_.code)
}

impl Candidate {
    /// `'text' (lang)`, the tag only when the display has a language.
    fn quoted(&self) -> String {
        match &self.language {
            Some(lang) => format!("'{}' ({lang})", self.text),
            None => format!("'{}'", self.text),
        }
    }
}

/// Compares displays without case and with whitespace collapsed.
fn fold(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The displays `concept` accepts: its display in the system's language and
/// every designation in its own.
fn candidates(
    provider: &Arc<dyn CodeSystemProvider>,
    concept: Concept,
) -> Result<Vec<Candidate>, ProviderError> {
    let mut out: Vec<Candidate> = Vec::new();
    if let Some(display) = provider.display(concept, None)? {
        out.push(Candidate {
            text: display,
            language: provider.language().map(str::to_owned),
            offered: true,
        });
    }
    for designation in provider.designations(concept, None)? {
        if designation.standards_status.is_some() {
            continue;
        }
        let duplicate = out
            .iter()
            .any(|c| c.text == designation.value && c.language == designation.language);
        if !duplicate {
            out.push(Candidate {
                offered: offered(designation.use_.as_ref()),
                text: designation.value,
                language: designation.language,
            });
        }
    }
    Ok(out)
}

/// `Valid display is 'a' (en)`, or `one of N choices: 'a' (en) or 'b' (de)`.
fn valid_display(list: &[&Candidate]) -> String {
    // NOTE: a display the concept accepts is not always one to suggest; the
    // offered ones are the displays, and the rest stay valid (#290).
    let offered: Vec<&Candidate> = list.iter().filter(|c| c.offered).copied().collect();
    let list: &[&Candidate] = if offered.is_empty() { list } else { &offered };
    match list {
        [one] => format!("Valid display is {}", one.quoted()),
        many => {
            let quoted: Vec<String> = many.iter().map(|c| c.quoted()).collect();
            let (leading, final_one) = quoted.split_at(quoted.len() - 1);
            format!(
                "Valid display is one of {} choices: {} or {}",
                many.len(),
                leading.join(", "),
                final_one.join("")
            )
        }
    }
}

/// A display the client asserted, and how to judge it.
#[derive(Debug, Clone, Copy)]
pub struct Asserted<'a> {
    /// The code system URI, for the text.
    pub system: &'a str,
    /// The code as the system spells it, for the text.
    pub code: &'a str,
    /// The display the client sent.
    pub given: &'a str,
    /// The `displayLanguage` as the client sent it.
    pub requested: Option<&'a str>,
    /// Whether a wrong display is a warning (`lenient-display-validation`).
    pub lenient: bool,
}

/// Judges an asserted display.
///
/// `None` when the display is one the concept accepts in the requested
/// language, else the issue to raise at `expression`: an `information` when
/// it is valid in the default language alone.
///
/// # Errors
///
/// Returns the provider's error when the displays cannot be read.
pub fn judge(
    provider: &Arc<dyn CodeSystemProvider>,
    concept: Concept,
    asserted: Asserted<'_>,
    expression: Option<String>,
) -> Result<Option<Issue>, ProviderError> {
    let Some((severity, kind, message, text)) = judgement(provider, concept, asserted)? else {
        return Ok(None);
    };
    Ok(Some(Issue {
        severity,
        code: "invalid",
        kind,
        message,
        text,
        expression,
    }))
}

/// The `display-comment` for a display that matches a designation the system
/// has withdrawn or deprecated (the ecosystem's `INACTIVE_DISPLAY_FOUND`):
/// a warning under lenient validation, an error otherwise.
fn retired_designation(
    provider: &Arc<dyn CodeSystemProvider>,
    concept: Concept,
    asserted: Asserted<'_>,
) -> Result<Option<(&'static str, &'static str, super::MessageId, String)>, ProviderError> {
    let wanted = fold(asserted.given);
    let retired = provider
        .designations(concept, None)?
        .into_iter()
        .find(|d| d.standards_status.is_some() && fold(&d.value) == wanted);
    let Some(retired) = retired else {
        return Ok(None);
    };
    let preferred = provider
        .display(
            concept,
            language::for_provider(provider.as_ref(), asserted.requested).as_deref(),
        )?
        .unwrap_or_default();
    let severity = if asserted.lenient { "warning" } else { "error" };
    Ok(Some((
        severity,
        "display-comment",
        super::MessageId::InactiveDisplayFound,
        format!(
            "'{}' is no longer considered a correct display for code '{}' (status = {}). The correct display is '{preferred}'",
            asserted.given,
            asserted.code,
            retired.standards_status.unwrap_or_default()
        ),
    )))
}

/// The severity and text of a wrong display, `None` for a right one.
fn judgement(
    provider: &Arc<dyn CodeSystemProvider>,
    concept: Concept,
    asserted: Asserted<'_>,
) -> Result<Option<(&'static str, &'static str, super::MessageId, String)>, ProviderError> {
    if let Some(retired) = retired_designation(provider, concept, asserted)? {
        return Ok(Some(retired));
    }
    let Asserted {
        system,
        code,
        given,
        requested,
        lenient,
    } = asserted;
    let all = candidates(provider, concept)?;
    let wanted = fold(given);
    let matches = |c: &&Candidate| fold(&c.text) == wanted;
    // NOTE: a display that differs only in its whitespace is still wrong, and
    // said so (the ecosystem's `bad-display-ws` case).
    let exact = |c: &&Candidate| c.text.to_lowercase() == given.to_lowercase();
    let whitespace = |valid: &[&Candidate]| {
        format!(
            "Wrong Display Name '{given}' for {system}#{code}: the whitespace differs. {}",
            valid_display(valid)
        )
    };
    let severity = if lenient { "warning" } else { "error" };
    let Some(requested) = requested else {
        if all.iter().any(|c| exact(&c)) {
            return Ok(None);
        }
        let valid: Vec<&Candidate> = all.iter().collect();
        if all.iter().any(|c| matches(&c)) {
            return Ok(Some((
                severity,
                "invalid-display",
                super::MessageId::DisplayNameWsForShouldBeOneOfInsteadOf,
                whitespace(&valid),
            )));
        }
        return Ok(Some((
            severity,
            "invalid-display",
            super::MessageId::DisplayNameForShouldBeOneOfInsteadOf,
            format!(
                "Wrong Display Name '{given}' for {system}#{code}. {} (for the language(s) '--')",
                valid_display(&valid)
            ),
        )));
    };
    // NOTE: `displayLanguage` is a BCP 47 range list (`de,it,zh`); a display in any
    // of its languages counts.
    let wanted_languages: Vec<String> = language::ranges(requested)
        .into_iter()
        .map(|range| range.tag)
        .collect();
    let in_language: Vec<&Candidate> = all
        .iter()
        .filter(|c| {
            c.language.as_deref().is_none_or(|l| {
                wanted_languages
                    .iter()
                    .any(|w| w == "*" || text_match::same_language(l, w))
            })
        })
        .collect();
    if !in_language.is_empty() {
        if in_language.iter().any(exact) {
            return Ok(None);
        }
        if in_language.iter().any(matches) {
            return Ok(Some((
                severity,
                "invalid-display",
                super::MessageId::DisplayNameWsForShouldBeOneOfInsteadOf,
                whitespace(&in_language),
            )));
        }
        return Ok(Some((
            severity,
            "invalid-display",
            super::MessageId::DisplayNameForShouldBeOneOfInsteadOf,
            format!(
                "Wrong Display Name '{given}' for {system}#{code}. {} (for the language(s) '{requested}')",
                valid_display(&in_language)
            ),
        )));
    }
    if all.iter().any(|c| matches(&c)) {
        return Ok(Some((
            "information",
            "invalid-display",
            super::MessageId::NoValidDisplayFoundNoneForLangOk,
            format!(
                "There are no valid display names found for the code {system}#{code} for language(s) '{requested}'. The display is '{given}' which is a valid display for the default language"
            ),
        )));
    }
    no_display_for_language(provider, concept, severity, given, system, code, requested)
}

/// The `invalid-display` for a display that fits no designation, when the
/// requested language has none: the default display is named.
fn no_display_for_language(
    provider: &Arc<dyn CodeSystemProvider>,
    concept: Concept,
    severity: &'static str,
    given: &str,
    system: &str,
    code: &str,
    requested: &str,
) -> Result<Option<(&'static str, &'static str, super::MessageId, String)>, ProviderError> {
    let default = provider
        .display(
            concept,
            language::for_provider(provider.as_ref(), None).as_deref(),
        )?
        .unwrap_or_default();
    Ok(Some((
        severity,
        "invalid-display",
        super::MessageId::NoValidDisplayFoundNoneForLangErr,
        format!(
            "Wrong Display Name '{given}' for {system}#{code}. There are no valid display names found for language(s) '{requested}'. Default display is '{default}'"
        ),
    )))
}

/// The `code-rule` note for a code a case-insensitive system located under
/// another spelling or form; `None` when the spelling matches or the system is
/// case-sensitive.
#[must_use]
pub fn case_note(
    provider: &Arc<dyn CodeSystemProvider>,
    given: &str,
    located: &str,
    expression: Option<String>,
) -> Option<Issue> {
    if given == located {
        return None;
    }
    if given.to_lowercase() != located.to_lowercase() {
        // NOTE: an alternate form the system admits (the ecosystem's icd-11
        // `cs-validate-uri`: a URI for a code) is normalized with an information note.
        return Some(Issue {
            severity: "information",
            code: "business-rule",
            kind: "code-rule",
            message: super::MessageId::CodeCaseDifference,
            text: format!(
                "The code '{given}' is an alternate form of '{located}', the code as the code system '{}' spells it",
                provider.identity().url
            ),
            expression,
        });
    }
    if provider.declaration().case_sensitive {
        return None;
    }
    // NOTE: the ecosystem's `case-coding-insensitive-code1-2` and `validate-lang-case-language`
    // answer a case difference as `information`.
    Some(Issue {
        severity: "information",
        code: "business-rule",
        kind: "code-rule",
        message: super::MessageId::CodeCaseDifference,
        text: format!(
            "The code '{given}' differs from the correct code '{located}' by case. Although the code system '{}' is case insensitive, implementers are strongly encouraged to use the correct case anyway",
            provider.identity().url
        ),
        expression,
    })
}

/// The `invalid-code` message and text for a code the system does not have.
///
/// The text names the version when the system has one, the fragment caveat
/// when the content is a fragment, and the compositional grammar when the code
/// is an expression this server does not evaluate.
#[must_use]
pub fn unknown_code(provider: &dyn CodeSystemProvider, code: &str) -> (super::MessageId, String) {
    let identity = provider.identity();
    let version = if identity.version.is_empty() {
        String::new()
    } else {
        format!(" version '{}'", identity.version)
    };
    // NOTE: a code the system's compositional grammar defines is not a code
    // the system lacks, so the caveat says which of the two this is
    // (<https://hl7.org/fhir/R4B/terminologycapabilities-definitions.html#TerminologyCapabilities.codeSystem.version.compositional>).
    let grammar = match provider.declaration().compositional {
        crate::provider::Compositional::Defined if provider.is_expression(code) => String::from(
            " - note that the code is an expression in the compositional grammar of the code system, which this server does not evaluate",
        ),
        crate::provider::Compositional::Supported => provider
            .rejection(code)
            .map(|reason| format!(" - {reason}"))
            .unwrap_or_default(),
        _ => String::new(),
    };
    if matches!(provider.declaration().content, ContentMode::Fragment) {
        return (
            super::MessageId::UnknownCodeInFragment,
            format!(
                "Unknown Code '{code}' in the CodeSystem '{}'{version} - note that the code system is labeled as a fragment, so the code may be valid in some other fragment",
                identity.url
            ),
        );
    }
    let message = if identity.version.is_empty() {
        super::MessageId::UnknownCodeIn
    } else {
        super::MessageId::UnknownCodeInVersion
    };
    (
        message,
        format!(
            "Unknown code '{code}' in the CodeSystem '{}'{version}{grammar}",
            identity.url
        ),
    )
}
