// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What an [`OperationError`] answers on the wire: its issue code, its
//! `tx-issue-type`, its message id and its status, and the failures of the
//! layers below it that become one.

use super::{OperationError, Refused, RefusedInput};
use crate::provider::ProviderError;
use crate::registry::ResolveError;
use http::StatusCode;

impl OperationError {
    /// A value set that cannot be expanded as defined, for `message`.
    #[must_use]
    pub fn value_set_invalid(message: impl Into<String>) -> Self {
        Self::ValueSetInvalid {
            message: message.into(),
            refused: None,
        }
    }

    /// The request value this failure is about, when it knows one.
    #[must_use]
    pub fn refused(&self) -> Option<&Refused> {
        match self {
            Self::ValueSetInvalid { refused, .. } => refused.as_deref(),
            _ => None,
        }
    }

    /// This failure pointing at `refused`, when it is a value set refusal.
    #[must_use]
    pub(super) fn pointing(self, refused: Refused) -> Self {
        match self {
            Self::ValueSetInvalid { message, .. } => Self::ValueSetInvalid {
                message,
                refused: Some(Box::new(refused)),
            },
            other => other,
        }
    }

    /// The failure of an inline value set's compose, pointing at the filter
    /// value it refused.
    ///
    /// The compose an operation evaluates has been pinned and may have lost
    /// includes to `exclude-system`, so the filter is found again in the
    /// compose the request carried, `inline`. A refusal from a value set the
    /// compose references names a filter `inline` does not hold, and points
    /// nowhere.
    pub(crate) fn of_compose(
        error: crate::compose::ComposeError,
        inline: Option<&crate::provider::compose::Compose>,
    ) -> Self {
        let refused = match (&error, inline) {
            (
                crate::compose::ComposeError::Provider {
                    source: ProviderError::InvalidFilterValue(invalid),
                    ..
                },
                Some(compose),
            ) => compose
                .filter_at(&invalid.property, &invalid.value)
                .map(|at| Refused {
                    input: RefusedInput::Filter(at),
                    text: invalid.value.clone(),
                    position: invalid.position,
                }),
            _ => None,
        };
        let converted = Self::from(error);
        match refused {
            Some(refused) => converted.pointing(refused),
            None => converted,
        }
    }

    /// The `OperationOutcome.issue.code`.
    #[must_use]
    pub const fn issue_code(&self) -> &'static str {
        match self {
            Self::Required(_) => "required",
            // NOTE: a code the system does not have "was not valid in the
            // context", where `not-found` is for a reference
            // (<https://hl7.org/fhir/R4B/valueset-issue-type.html>).
            Self::InvalidCode { .. } | Self::UnknownCode { .. } => "code-invalid",
            Self::Invalid(_) | Self::ValueSetInvalid { .. } => "invalid",
            // NOTE: a cycle is a processing failure, "no point resubmitting the
            // same content unchanged" (<https://hl7.org/fhir/R4B/valueset-issue-type.html>).
            Self::ValueSetCyclic(_) | Self::InvalidLanguage(_) => "processing",
            Self::NotSupported(_) | Self::CannotDetermine(_) | Self::UnsupportedGrammar { .. } => {
                "not-supported"
            }
            Self::UnknownSystem(_)
            | Self::UnknownVersion { .. }
            | Self::UnknownValueSet(_)
            | Self::UnknownImport(_)
            | Self::UnknownSupplement(_)
            | Self::UnknownConceptMap(_) => "not-found",
            Self::TooCostly(_) => "too-costly",
            Self::VersionCheck(_) | Self::Provider(_) => "exception",
        }
    }

    /// The `tx-issue-type` code of the failure, for `issue.details.coding`
    /// (<https://build.fhir.org/ig/FHIR/fhir-tools-ig/CodeSystem-tx-issue-type.html>).
    #[must_use]
    pub const fn tx_issue_type(&self) -> &'static str {
        match self {
            Self::Required(_) | Self::Invalid(_) => "invalid-data",
            Self::InvalidLanguage(_) => "invalid-display",
            Self::NotSupported(_) | Self::Provider(_) | Self::UnsupportedGrammar { .. } => {
                "not-supported"
            }
            Self::UnknownSystem(_)
            | Self::UnknownVersion { .. }
            | Self::UnknownValueSet(_)
            | Self::UnknownImport(_)
            | Self::UnknownSupplement(_)
            | Self::UnknownConceptMap(_) => "not-found",
            Self::UnknownCode { .. } | Self::InvalidCode { .. } => "invalid-code",
            Self::ValueSetInvalid { .. } | Self::ValueSetCyclic(_) => "vs-invalid",
            Self::VersionCheck(_) => "version-error",
            Self::TooCostly(_) => "too-costly",
            Self::CannotDetermine(_) => "cannot-determine",
        }
    }

    /// The message id the terminology ecosystem attaches to an outcome of
    /// this failure (the reference server's message key; #189).
    #[must_use]
    pub fn message_id(&self) -> &'static str {
        match self {
            // NOTE: the ecosystem names a pinned import it cannot find on `$expand`
            // by its own key (its `default-valueset-version` cases).
            Self::UnknownImport(url) if url.contains('|') => "VS_EXP_IMPORT_UNK_PINNED",
            Self::UnknownValueSet(_) | Self::UnknownImport(_) => "Unable_to_resolve_value_Set_",
            Self::UnknownVersion { .. } => "UNKNOWN_CODESYSTEM_VERSION_EXP",
            Self::UnknownSystem(_) => "UNKNOWN_CODESYSTEM",
            Self::UnknownSupplement(_) => "VALUESET_SUPPLEMENT_MISSING",
            Self::VersionCheck(_) => "VALUESET_VERSION_CHECK",
            Self::TooCostly(_) => "VALUESET_TOO_COSTLY",
            Self::NotSupported(_) => "CODESYSTEM_NOT_ENUMERABLE",
            Self::UnknownCode { .. } | Self::InvalidCode { .. } => "Unknown_Code_in_Version",
            Self::ValueSetCyclic(_) => "VALUESET_CIRCULAR_REFERENCE",
            Self::InvalidLanguage(_) => "INVALID_DISPLAY_NAME",
            Self::Required(_)
            | Self::Invalid(_)
            | Self::ValueSetInvalid { .. }
            | Self::CannotDetermine(_)
            | Self::UnknownConceptMap(_)
            | Self::UnsupportedGrammar { .. }
            | Self::Provider(_) => "TX_GENERAL_ERROR",
        }
    }

    /// The HTTP status.
    ///
    /// An unknown code answers `400` as the R4B `$lookup` page's own example
    /// does; the normative text asks only for a 4xx or 5xx with an
    /// `OperationOutcome` (no spec fixes the number: our own choice).
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        match self {
            Self::Required(_)
            | Self::Invalid(_)
            | Self::InvalidLanguage(_)
            | Self::VersionCheck(_)
            | Self::UnknownCode { .. }
            | Self::InvalidCode { .. }
            | Self::UnsupportedGrammar { .. }
            | Self::NotSupported(_) => StatusCode::BAD_REQUEST,
            Self::UnknownSystem(_)
            | Self::UnknownVersion { .. }
            | Self::UnknownValueSet(_)
            | Self::UnknownImport(_)
            | Self::UnknownSupplement(_)
            | Self::UnknownConceptMap(_) => StatusCode::NOT_FOUND,
            // NOTE: 422 is the status for a resource that breaks the server's
            // rules (<https://hl7.org/fhir/R4B/http.html#status-codes>); a compose
            // the layer cannot evaluate is that resource.
            Self::ValueSetInvalid { .. }
            | Self::ValueSetCyclic(_)
            | Self::TooCostly(_)
            | Self::CannotDetermine(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Provider(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<ResolveError> for OperationError {
    fn from(error: ResolveError) -> Self {
        match error {
            ResolveError::UnknownSystem(url) => Self::UnknownSystem(url),
            ResolveError::UnknownVersion {
                url,
                version,
                available,
            } => Self::UnknownVersion {
                url,
                version,
                available,
            },
        }
    }
}

impl From<ProviderError> for OperationError {
    #[expect(
        clippy::match_same_arms,
        reason = "the arms group by what the failure is, and each spec citation sits on the variants it explains"
    )]
    fn from(error: ProviderError) -> Self {
        match error {
            ProviderError::IncompleteContent { .. }
            | ProviderError::NotEnumerable
            | ProviderError::FilterNotEnumerable => Self::NotSupported(error.to_string()),
            // NOTE: a filter the system cannot evaluate, an unknown filter value, or
            // a bad regular expression is a defect of the value set, a 422
            // (<https://hl7.org/fhir/R4B/http.html#status-codes>), never a 500.
            ProviderError::UnsupportedFilter { .. }
            | ProviderError::InvalidFilterValue { .. }
            | ProviderError::Regex(_)
            | ProviderError::UnknownCode(_)
            | ProviderError::MalformedImplicitValueSet { .. } => {
                Self::value_set_invalid(error.to_string())
            }
            // NOTE: an implicit concept map URI the system does not answer is the
            // unknown-map outcome `$translate` already has for a `url` nothing serves
            // (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>).
            ProviderError::MalformedImplicitConceptMap { .. } => Self::Invalid(error.to_string()),
            ProviderError::UnknownImplicitConceptMap { url } => Self::UnknownConceptMap(url),
            // NOTE: an implicit URI whose base names an edition version the server
            // does not hold is a version it cannot find, not a malformed value set
            // (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets").
            ProviderError::UnservedImplicitVersion { url, version } => Self::UnknownVersion {
                url,
                version,
                available: Vec::new(),
            },
            // NOTE: R4B needs `ConceptMap.group.target` whenever the targets are real
            // codes, so a map the server cannot state one for is `not-supported`
            // (<https://hl7.org/fhir/R4B/conceptmap-definitions.html#ConceptMap.group.target>).
            ProviderError::UnnamedConceptMapTarget { .. } => Self::NotSupported(error.to_string()),
            ProviderError::CannotDetermine(_) => Self::CannotDetermine(error.to_string()),
            ProviderError::InvalidCode { code, reason } => Self::InvalidCode { code, reason },
            other @ ProviderError::Storage(_) => Self::Provider(other),
        }
    }
}

/// The value of an optional R4B `code`.
/// The value of an optional R4B `string`.
/// The value of an optional R4B `uri`.
/// A `Coding`'s system, version, code, and display as text.
impl From<crate::compose::ComposeError> for OperationError {
    fn from(error: crate::compose::ComposeError) -> Self {
        use crate::compose::ComposeError;
        match error {
            ComposeError::Resolve(error) => error.into(),
            ComposeError::Provider {
                system,
                source: ProviderError::NotEnumerable,
            } => Self::NotSupported(format!(
                "The code system '{system}' cannot be expanded because its codes cannot be iterated or enumerated in any meaningful sense"
            )),
            // NOTE: the ecosystem words a filter the system will not expand apart from a
            // system that enumerates nothing at all (its `langcodes/expand-lang-script`
            // case), so a validator recognises which of the two it got.
            ComposeError::Provider {
                system,
                source: ProviderError::FilterNotEnumerable,
            } => Self::NotSupported(format!("This filter on {system} cannot be expanded")),
            ComposeError::Provider { source, .. } => source.into(),
            ComposeError::UnknownValueSet(url) => Self::UnknownImport(url),
            ComposeError::UnknownCode { .. }
            | ComposeError::NoSystemOrValueSet
            | ComposeError::CriteriaWithoutSystem
            | ComposeError::ConceptsAndFilters => Self::value_set_invalid(error.to_string()),
            ComposeError::Cycle(_) => Self::ValueSetCyclic(error.to_string()),
            ComposeError::NoResolver(_) => Self::NotSupported(error.to_string()),
            ComposeError::Negotiation(error) => error.into(),
        }
    }
}

impl From<crate::compose::negotiation::NegotiationError> for OperationError {
    fn from(error: crate::compose::negotiation::NegotiationError) -> Self {
        match error {
            crate::compose::negotiation::NegotiationError::SystemVersion { .. } => {
                Self::VersionCheck(error.to_string())
            }
            crate::compose::negotiation::NegotiationError::ValueSetVersion { .. } => {
                Self::Invalid(error.to_string())
            }
        }
    }
}
