// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a reader typed into the command bar, and what can be done with it.
//!
//! The reading is of the string's shape alone. Nothing here asks the server a
//! question first, because the point of the bar is that a reader gets somewhere
//! before a request has been answered, and because a viewer that had to ask
//! could not offer anything on a root that is refusing.
//!
//! Every offer resolves to an address the reader could have typed, so an offer
//! is a link rather than a command.

use crate::fhir::version::FhirVersion;
use ferrohealth_ui::i18n::Locale;

use crate::locale::{Msg, tr, tr_with};
use crate::routes::BROWSE_PATH;
use crate::routes::EXPAND_PATH;
use crate::routes::LOOKUP_PATH;
use crate::routes::TRANSLATE_PATH;
use crate::routes::VALIDATE_PATH;
use crate::routes::VERSION_PARAM;
use crate::routes::base_url;
use crate::routes::system_link;
use serde::{Deserialize, Serialize};

/// The query parameter the command bar carries what was typed in.
pub(crate) const QUERY_PARAM: &str = "q";

/// The longest string still read as a code.
///
/// A code is an identifier a person types, and every code system this server
/// can hold uses short ones: an SCTID is at most 18 digits, a LOINC code 10
/// characters, an ICD-10 code 8. A longer run of non-space text is a phrase
/// with the spaces left out, and the text offer is the honest answer to it.
const LONGEST_CODE: usize = 64;

/// What the viewer reads a typed string as.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum Subject {
    /// A canonical: something the FHIR specification would call a `uri`.
    Canonical(String),
    /// A code: one short token with no space in it.
    Code(String),
    /// A phrase to search designations for.
    Text(String),
}

impl Subject {
    /// Reads a typed string, or `None` when nothing was typed.
    ///
    /// A canonical is recognised by its scheme, which is what makes a `uri`
    /// a `uri` (<https://www.rfc-editor.org/rfc/rfc3986#section-3.1>). The
    /// three schemes below are the ones a FHIR canonical uses in practice; a
    /// string carrying any other scheme still reads as a canonical, because a
    /// scheme followed by a colon is the shape the specification names.
    pub(crate) fn of(typed: &str) -> Option<Self> {
        let typed = typed.trim();
        if typed.is_empty() {
            return None;
        }
        if looks_like_uri(typed) {
            return Some(Self::Canonical(typed.to_owned()));
        }
        if !typed.contains(char::is_whitespace) && typed.chars().count() <= LONGEST_CODE {
            return Some(Self::Code(typed.to_owned()));
        }
        Some(Self::Text(typed.to_owned()))
    }

    /// What a reader is told this string was read as.
    pub(crate) fn label(&self, locale: &Locale) -> String {
        match self {
            Self::Canonical(_) => tr(locale, Msg::ACanonical),
            Self::Code(_) => tr(locale, Msg::ACode),
            Self::Text(_) => tr(locale, Msg::APhrase),
        }
    }

    /// The string itself.
    pub(crate) fn typed(&self) -> &str {
        match self {
            Self::Canonical(text) | Self::Code(text) | Self::Text(text) => text,
        }
    }
}

/// Whether a string carries a URI scheme.
///
/// A scheme starts with a letter and runs on letters, digits, `+`, `-` and `.`
/// up to the colon (<https://www.rfc-editor.org/rfc/rfc3986#section-3.1>). A
/// bare `word:` with nothing after it is not an address a screen can open, so
/// something has to follow the colon.
fn looks_like_uri(typed: &str) -> bool {
    let Some((scheme, rest)) = typed.split_once(':') else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    let mut characters = scheme.chars();
    characters.next().is_some_and(char::is_alphabetic)
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
}

/// The `$lookup` operation, as a capability statement names it.
pub(crate) const LOOKUP: &str = "lookup";

/// The `$validate-code` operation, as a capability statement names it.
pub(crate) const VALIDATE_CODE: &str = "validate-code";

/// The `$expand` operation, as a capability statement names it.
pub(crate) const EXPAND: &str = "expand";

/// The `$translate` operation, as a capability statement names it.
pub(crate) const TRANSLATE: &str = "translate";

/// One thing a reader can do with what they typed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Offer {
    /// What the offer does, as a reader reads it.
    pub(crate) label: String,
    /// Which operation or screen answers it, in one sentence.
    pub(crate) detail: String,
    /// The address it opens, which the reader could have typed.
    pub(crate) href: String,
}

/// What this root declares, as the offers are gated on it.
///
/// The facts travel in one struct rather than three booleans at a call site,
/// so a caller cannot pass them in the wrong order. `$validate-code` is one
/// fact rather than two, because the runner picks the resource type from what
/// the root declares once the reader is on it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Declared {
    /// The operations the root declares, by the name `crate::find` asks
    /// for: `lookup`, `validate-code`, `expand` and `translate`.
    operations: Vec<&'static str>,
}

impl Declared {
    /// What a root declaring `operations` offers.
    #[must_use]
    pub(crate) fn of(operations: Vec<&'static str>) -> Self {
        Self { operations }
    }

    /// Whether the root declares `operation`.
    fn has(&self, operation: &str) -> bool {
        self.operations.contains(&operation)
    }
}

/// Everything this root can do with `subject`, in the order it is offered.
///
/// An operation the root's capability statement does not declare is left out
/// rather than offered and refused, which is the same rule every screen
/// follows (<https://hl7.org/fhir/R5/capabilitystatement.html>).
pub(crate) fn offers(
    locale: &Locale,
    subject: &Subject,
    declared: &Declared,
    version: FhirVersion,
) -> Vec<Offer> {
    match subject {
        Subject::Canonical(canonical) => canonical_offers(locale, canonical, declared, version),
        Subject::Code(code) => code_offers(locale, code, declared, version),
        Subject::Text(text) => text_offers(locale, text, version),
    }
}

/// What a canonical can be: a code system to read, or a value set to expand.
///
/// The viewer cannot tell one from the other without asking the server, and
/// asking is what this module exists not to do, so both are offered and each
/// says which it treats the canonical as.
fn canonical_offers(
    locale: &Locale,
    canonical: &str,
    declared: &Declared,
    version: FhirVersion,
) -> Vec<Offer> {
    let mut found = vec![Offer {
        label: tr(locale, Msg::OpenTheCodeSystem),
        detail: tr(locale, Msg::ReadsWhatRootDeclaresAboutCodesystem),
        href: system_link(canonical, version),
    }];
    found.push(Offer {
        label: tr(locale, Msg::BrowseItsConcepts),
        detail: tr(locale, Msg::WalksHierarchyWhereServedVersionDeclares),
        href: screen_with(BROWSE_PATH, "system", canonical, version),
    });
    if declared.has(EXPAND) {
        found.push(Offer {
            label: tr(locale, Msg::ExpandValueSet),
            detail: tr(locale, Msg::SendsCanonicalUrlParameterValuesetExpand),
            href: screen_with(EXPAND_PATH, "url", canonical, version),
        });
    }
    if declared.has(VALIDATE_CODE) {
        found.push(Offer {
            label: tr(locale, Msg::ValidateCode),
            detail: tr(locale, Msg::OpensRunnerCanonicalFilledReadyCode),
            href: screen_with(VALIDATE_PATH, "system", canonical, version),
        });
    }
    found
}

/// What a code can be: something to check, or something to translate.
///
/// Every one of these needs a code system too, which the reader has not given,
/// so each offer opens the runner with the code filled in and the system left
/// for them.
fn code_offers(
    locale: &Locale,
    code: &str,
    declared: &Declared,
    version: FhirVersion,
) -> Vec<Offer> {
    let named = |msg: Msg| tr_with(locale, msg, &[("code", code.to_owned().into())]);
    let mut found = Vec::new();
    if declared.has(LOOKUP) {
        found.push(Offer {
            label: named(Msg::FindLookUp),
            detail: tr(locale, Msg::FindLookUpDetail),
            href: screen_with(LOOKUP_PATH, "code", code, version),
        });
    }
    if declared.has(VALIDATE_CODE) {
        found.push(Offer {
            label: named(Msg::FindCheck),
            detail: tr(locale, Msg::FindCheckDetail),
            href: screen_with(VALIDATE_PATH, "code", code, version),
        });
    }
    if declared.has(TRANSLATE) {
        found.push(Offer {
            label: named(Msg::FindTranslate),
            detail: tr(locale, Msg::FindTranslateDetail),
            href: screen_with(TRANSLATE_PATH, "code", code, version),
        });
    }
    found.push(Offer {
        label: named(Msg::FindSearch),
        detail: tr(locale, Msg::SearchesConceptBrowserCodeSystemYou),
        href: screen_with(BROWSE_PATH, "filter", code, version),
    });
    found
}

/// What a phrase can be: a search, in the one place that searches designations.
fn text_offers(locale: &Locale, text: &str, version: FhirVersion) -> Vec<Offer> {
    vec![Offer {
        label: tr_with(locale, Msg::FindSearch, &[("code", text.to_owned().into())]),
        detail: tr(locale, Msg::SendsPhraseFilterParameterCodeSystem),
        href: screen_with(BROWSE_PATH, "filter", text, version),
    }]
}

/// The address of one screen, carrying the subject as one parameter.
fn screen_with(path: &str, name: &str, value: &str, version: FhirVersion) -> String {
    base_url()
        .segment(path)
        .query(VERSION_PARAM, version.segment())
        .query(name, value)
        .render("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::UI_BASE;

    /// A root that declares everything, so a case about shape is about shape.
    /// A root that declares everything, so a case about shape is about shape.
    fn everything() -> Declared {
        Declared::of(vec![LOOKUP, VALIDATE_CODE, EXPAND, TRANSLATE])
    }

    #[test]
    fn a_string_carrying_a_scheme_is_a_canonical() {
        for typed in [
            "http://snomed.info/sct",
            "https://ferroterm.eu/fhir/ValueSet/v",
            "urn:iso:std:iso:3166",
            "urn:ietf:bcp:47",
        ] {
            assert_eq!(
                Subject::of(typed),
                Some(Subject::Canonical(typed.to_owned())),
                "`{typed}` names a resource, not a code"
            );
        }
    }

    #[test]
    fn a_scheme_with_nothing_after_it_is_not_an_address() {
        assert_eq!(
            Subject::of("code:"),
            Some(Subject::Code("code:".to_owned())),
            "a colon at the end opens no screen, so it is read as a token"
        );
    }

    #[test]
    fn one_token_is_a_code_and_a_phrase_is_a_phrase() {
        assert_eq!(
            Subject::of(" 404684003 "),
            Some(Subject::Code("404684003".to_owned())),
            "surrounding space is trimmed"
        );
        assert_eq!(
            Subject::of("clinical finding"),
            Some(Subject::Text("clinical finding".to_owned()))
        );
    }

    #[test]
    fn a_long_run_with_no_space_is_a_phrase_rather_than_a_code() {
        let long = "a".repeat(LONGEST_CODE + 1);
        assert_eq!(
            Subject::of(&long),
            Some(Subject::Text(long.clone())),
            "no code system this server holds uses a code that long"
        );
    }

    #[test]
    fn nothing_typed_is_no_subject() {
        assert_eq!(Subject::of(""), None);
        assert_eq!(Subject::of("   "), None, "space alone is nothing typed");
    }

    #[test]
    fn every_offer_carries_the_subject_into_the_address() {
        for typed in ["http://snomed.info/sct", "404684003", "clinical finding"] {
            let subject = Subject::of(typed).expect("the case types something");
            let found = offers(&Locale::english(), &subject, &everything(), FhirVersion::R5);
            assert!(!found.is_empty(), "`{typed}` reaches no screen at all");
            for offer in found {
                assert!(
                    offer.href.starts_with(UI_BASE),
                    "an offer leaves the viewer: {}",
                    offer.href
                );
                assert!(
                    offer.href.contains("fhir=r5"),
                    "an offer drops the version the reader is on: {}",
                    offer.href
                );
            }
        }
    }

    #[test]
    fn an_operation_this_root_does_not_declare_is_not_offered() {
        let code = Subject::of("404684003").expect("the case types a code");
        let nothing = offers(
            &Locale::english(),
            &code,
            &Declared::default(),
            FhirVersion::R4B,
        );
        assert!(
            nothing.iter().all(|offer| offer.label != "Check 404684003"),
            "a root that declares no $validate-code is not offered one"
        );
        assert!(
            nothing.iter().any(|offer| offer.href.contains("browse")),
            "the concept browser is a screen rather than an operation, so it stays"
        );
    }

    #[test]
    fn a_canonical_is_offered_as_both_a_code_system_and_a_value_set() {
        let canonical = Subject::of("http://snomed.info/sct").expect("the case types a canonical");
        let found = offers(
            &Locale::english(),
            &canonical,
            &everything(),
            FhirVersion::R5,
        );
        assert!(
            found
                .iter()
                .any(|offer| offer.href.contains(&format!("{UI_BASE}/systems/"))),
            "the viewer cannot tell the two apart without asking, so it offers both"
        );
        assert!(
            found
                .iter()
                .any(|offer| offer.href.contains(&format!("{UI_BASE}/expand")))
        );
    }
}
