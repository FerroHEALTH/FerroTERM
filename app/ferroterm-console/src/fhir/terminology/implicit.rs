// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The implicit value set forms a served version declares: the extension that
//! carries each, its URL template, and what the template's placeholders take.

/// The canonical FerroTERM declares one implicit value set form under.
///
/// No element of `TerminologyCapabilities` says which implicit value sets a
/// server resolves, so the server states each form as an extension on
/// `codeSystem.version`: a URL template and what its placeholders take.
pub(super) const IMPLICIT_EXTENSION: &str =
    "https://ferroterm.eu/fhir/StructureDefinition/implicit-value-set";

/// The sub-extension carrying the URL template.
pub(super) const IMPLICIT_PATTERN: &str = "pattern";

/// The sub-extension carrying what the template's placeholders take.
pub(super) const IMPLICIT_ARGUMENT: &str = "argument";

/// One implicit value set form a served version resolves.
///
/// The viewer reads the template and the placeholders it holds; it never
/// knows which system a template belongs to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImplicitForm {
    /// The URL template, each placeholder in square brackets.
    pub(crate) pattern: String,
    /// What the placeholders take.
    pub(crate) argument: ImplicitArgument,
}

/// What the placeholders of an implicit value set template take.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImplicitArgument {
    /// The template has no placeholder and is the value set URL as it stands.
    None,
    /// The one placeholder takes a code of the system.
    Code,
    /// The placeholders take free text.
    Expression,
}

impl ImplicitArgument {
    /// The argument a declared `valueCode` names, or `None` for a code this
    /// viewer does not know.
    pub(super) fn of(code: &str) -> Option<Self> {
        match code {
            "none" => Some(Self::None),
            "code" => Some(Self::Code),
            "expression" => Some(Self::Expression),
            _ => None,
        }
    }
}

// NOTE: only the composer fills a template, and the reader bundle carries no
// composer, so the filling is editor-only (no FHIR/SNOMED spec governs this: our own design).
impl ImplicitForm {}
