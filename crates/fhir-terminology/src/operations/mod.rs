// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR terminology operations over the seam, one module each.
//!
//! Each operation takes the generated request type of a FHIR version and
//! returns the generated response type; the parameter set is exactly what
//! that version's `OperationDefinition` declares because the types are
//! generated from it. Every client input error is an [`OperationError`] the
//! server maps to an `OperationOutcome` issue and an HTTP status.

pub mod closure;
pub mod display;
pub mod expand;
pub mod lookup;
pub mod subsumes;
pub mod translate;
pub mod validate_code;
pub mod value_set_validate_code;

use std::sync::Arc;

use http::StatusCode;

use crate::provider::{CodeSystemProvider, ProviderError};
use crate::registry::{Registry, ResolveError, Resolved};

/// A coding as an operation input names it, in no FHIR version's types: the
/// system, version, code, and display a `Coding` carries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodingRef {
    /// The code system URI.
    pub system: Option<String>,
    /// The code system version.
    pub version: Option<String>,
    /// The code.
    pub code: Option<String>,
    /// The display the client sent.
    pub display: Option<String>,
}

/// A `CodeableConcept` as an operation input names it, in no FHIR version's
/// types: the codings and the `text` a `CodeableConcept` carries
/// (<https://hl7.org/fhir/R4B/datatypes.html#CodeableConcept>).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeableConceptRef {
    /// The codings, in the order the client sent them.
    pub coding: Vec<CodingRef>,
    /// The plain text the client sent beside the codings.
    pub text: Option<String>,
}

/// Where an operation was invoked: on the type, or on one code system
/// instance the server already resolved from the URL.
#[derive(Debug, Clone)]
pub enum Invocation {
    /// `[base]/CodeSystem/$op`: the request names the system.
    Type,
    /// `[base]/CodeSystem/{id}/$op`: the instance is the system.
    Instance(Resolved),
}

/// A failure the server reports as an `OperationOutcome`.
///
/// Each variant knows its `issue.code` (<https://hl7.org/fhir/R4B/valueset-issue-type.html>)
/// and HTTP status (<https://hl7.org/fhir/R4B/operations.html#3.2.0.6.2>).
#[derive(Debug, thiserror::Error)]
pub enum OperationError {
    /// A required parameter (or one of a required group) is absent.
    #[error("{0}")]
    Required(String),
    /// Parameters contradict each other or the invocation.
    #[error("{0}")]
    Invalid(String),
    /// A `displayLanguage` that is not a valid BCP 47 language range list.
    #[error("Invalid displayLanguage: '{0}'")]
    InvalidLanguage(String),
    /// A `check-system-version` parameter forbids the version a value set
    /// uses (the ecosystem's `version-error`, an `exception`).
    #[error("{0}")]
    VersionCheck(String),
    /// A `useSupplement` (or a value set's `valueset-supplement`) names no
    /// loaded supplement.
    #[error(transparent)]
    UnknownSupplement(#[from] crate::registry::UnknownSupplement),
    /// The operation or a parameter combination is not supported here.
    #[error("{0}")]
    NotSupported(String),
    /// The code system is not served.
    #[error("code system `{0}` is not served")]
    UnknownSystem(String),
    /// The code system is served, this version is not.
    ///
    /// The wording is the ecosystem's, which names the versions that would
    /// have worked (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>);
    /// an answer that says only which version is missing leaves the caller
    /// guessing.
    #[error(
        "A definition for CodeSystem '{url}' version '{version}' could not be found, so the value set cannot be expanded{}",
        valid_versions(available)
    )]
    UnknownVersion {
        /// The system.
        url: String,
        /// The version.
        version: String,
        /// The versions the server does hold for the system.
        available: Vec<String>,
    },
    /// The code is not in the code system.
    #[error("code `{code}` is not in code system `{system}` version `{version}`")]
    UnknownCode {
        /// The system.
        system: String,
        /// The version.
        version: String,
        /// The code.
        code: String,
    },
    /// The code is written in the code system's compositional grammar, which
    /// this server does not evaluate.
    #[error(
        "code `{code}` is an expression in the compositional grammar of code system `{system}`, which this server does not evaluate"
    )]
    UnsupportedGrammar {
        /// The system.
        system: String,
        /// The code as given.
        code: String,
    },
    /// The code is malformed for the code system's grammar.
    #[error("code `{code}` is invalid: {reason}")]
    InvalidCode {
        /// The code as given.
        code: String,
        /// Why it is not a code.
        reason: String,
    },
    /// The value set is not known (the ecosystem's wording, so a validator
    /// recognises it).
    #[error("A definition for the value Set '{0}' could not be found")]
    UnknownValueSet(String),
    /// A value set another value set imports is not known.
    #[error("A definition for the value Set '{0}' could not be found")]
    UnknownImport(String),
    /// The concept map is not known.
    #[error("concept map `{0}` is not known")]
    UnknownConceptMap(String),
    /// The value set cannot be expanded as defined.
    #[error("{message}")]
    ValueSetInvalid {
        /// What is wrong.
        message: String,
        /// The request value the refusal is about, when the operation knows
        /// which input carries it.
        refused: Option<Box<Refused>>,
    },
    /// The value set reaches itself through `compose.include.valueSet` or
    /// `compose.exclude.valueSet`.
    #[error("{0}")]
    ValueSetCyclic(String),
    /// The expansion is larger than the server returns without paging.
    #[error("{0}")]
    TooCostly(String),
    /// The system cannot decide the relationship asked of it.
    #[error("{0}")]
    CannotDetermine(String),
    /// The provider failed.
    #[error("the code system provider failed")]
    Provider(#[source] ProviderError),
}

/// The request value a value set refusal is about, and where a parser stopped
/// reading it.
///
/// The operation names the input; the wire layer knows where the request
/// wrote it and turns this into `issue.expression` and a column. No
/// specification governs this shape: our own design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// The input that carries the value.
    pub input: RefusedInput,
    /// The value as the operation read it.
    pub text: String,
    /// The byte offset into `text` where a parser stopped, when one refused it.
    pub position: Option<usize>,
}

/// Which input of a value set operation carries a refused value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusedInput {
    /// A filter value of the inline `valueSet`'s compose.
    Filter(crate::compose::FilterAt),
    /// The value set `url`.
    Url,
}

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
    fn pointing(self, refused: Refused) -> Self {
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
        inline: Option<&crate::compose::Compose>,
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

/// Resolves the code system an operation works on: the instance when invoked
/// on one, else the `system` (and `version`) the request names.
///
/// # Errors
///
/// Returns [`OperationError::Required`] when a type-level invocation names no
/// system, [`OperationError::Invalid`] when an instance-level invocation names
/// another system, and the unknown-system or unknown-version errors.
pub fn resolve(
    registry: &Registry,
    invocation: &Invocation,
    system: Option<&str>,
    version: Option<&str>,
) -> Result<Resolved, OperationError> {
    match invocation {
        Invocation::Instance(resolved) => {
            let identity = resolved.provider.identity();
            if let Some(system) = system
                && system != identity.url
            {
                return Err(OperationError::Invalid(format!(
                    "the request names code system `{system}`, the instance is `{}`",
                    identity.url
                )));
            }
            if let Some(version) = version
                && version != identity.version
            {
                return Err(OperationError::Invalid(format!(
                    "the request names version `{version}`, the instance is `{}`",
                    identity.version
                )));
            }
            Ok(resolved.clone())
        }
        Invocation::Type => {
            let system = system.ok_or_else(|| {
                OperationError::Required(String::from(
                    "a code system is required: the `system` parameter, a `coding.system`, or an instance-level invocation",
                ))
            })?;
            Ok(registry.resolve(system, version)?)
        }
    }
}

/// Locates `code` in `provider`, as the unknown-code error when absent.
///
/// A code the system's compositional grammar defines and this server does not
/// evaluate is refused for the grammar instead: the code system has the code,
/// the server has no answer for it, and the two are different failures
/// (<https://hl7.org/fhir/R4B/terminologycapabilities-definitions.html#TerminologyCapabilities.codeSystem.version.compositional>).
///
/// # Errors
///
/// Returns [`OperationError::UnsupportedGrammar`],
/// [`OperationError::UnknownCode`], or the provider's error.
pub fn locate(
    provider: &Arc<dyn CodeSystemProvider>,
    code: &str,
) -> Result<crate::provider::Located, OperationError> {
    provider.locate(code)?.ok_or_else(|| {
        let identity = provider.identity();
        if provider.declaration().compositional == crate::provider::Compositional::Defined
            && provider.is_expression(code)
        {
            return OperationError::UnsupportedGrammar {
                system: identity.url.clone(),
                code: code.to_owned(),
            };
        }
        // NOTE: a system whose grammar this server evaluates says why it
        // refused the code (the Compositional Grammar specification, §7.3
        // Validating), which "unknown code" alone does not.
        if let Some(reason) = provider.rejection(code) {
            return OperationError::InvalidCode {
                code: code.to_owned(),
                reason,
            };
        }
        OperationError::UnknownCode {
            system: identity.url.clone(),
            version: identity.version.clone(),
            code: code.to_owned(),
        }
    })
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

impl From<crate::valueset::negotiation::NegotiationError> for OperationError {
    fn from(error: crate::valueset::negotiation::NegotiationError) -> Self {
        match error {
            crate::valueset::negotiation::NegotiationError::SystemVersion { .. } => {
                Self::VersionCheck(error.to_string())
            }
            crate::valueset::negotiation::NegotiationError::ValueSetVersion { .. } => {
                Self::Invalid(error.to_string())
            }
        }
    }
}

/// Where an operation finds its code systems and value sets.
#[derive(Debug, Clone, Copy)]
pub struct Sources<'a> {
    /// The code systems.
    pub registry: &'a Registry,
    /// The value sets.
    pub value_sets: &'a crate::valueset::store::ValueSetStore,
    /// The concept maps.
    pub concept_maps: &'a crate::conceptmap::store::ConceptMapStore,
}

impl<'a> Sources<'a> {
    /// These sources with the dormant supplements `wanted` names layered over
    /// their systems, when any is named; the borrowed sources otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`OperationError::UnknownSupplement`] for a name no loaded
    /// supplement answers to.
    pub fn with_supplements(
        &self,
        wanted: &[String],
    ) -> Result<std::borrow::Cow<'a, Registry>, OperationError> {
        if wanted.is_empty() {
            return Ok(std::borrow::Cow::Borrowed(self.registry));
        }
        Ok(std::borrow::Cow::Owned(
            self.registry.with_supplements(wanted)?,
        ))
    }

    /// The value set an operation names: inline, stored by `url` (and
    /// `version`), or a provider's implicit form.
    ///
    /// # Errors
    ///
    /// Returns [`OperationError::Invalid`] for both an inline and a `url`,
    /// [`OperationError::Required`] for neither, and
    /// [`OperationError::UnknownValueSet`] when nothing answers the `url`.
    pub fn value_set(
        &self,
        inline: Option<
            Result<crate::valueset::model::ValueSetModel, crate::valueset::model::ModelError>,
        >,
        url: Option<&str>,
        version: Option<&str>,
    ) -> Result<Arc<crate::valueset::model::ValueSetModel>, OperationError> {
        match (inline, url) {
            (Some(_), Some(_)) => Err(OperationError::Invalid(String::from(
                "provide either `url` or an inline `valueSet`, not both",
            ))),
            (Some(model), None) => {
                Ok(Arc::new(model.map_err(|e| {
                    OperationError::value_set_invalid(e.to_string())
                })?))
            }
            (None, Some(url)) => {
                if let Some(model) = self.value_sets.resolve(url, version) {
                    return Ok(model);
                }
                match self.registry.implicit_value_set(url) {
                    Some(Ok(compose)) => {
                        let metadata = self.registry.implicit_metadata(url);
                        Ok(Arc::new(crate::valueset::model::ValueSetModel {
                            // NOTE: an implicit value set has no resource, and a
                            // logical id belongs to one
                            // (<https://hl7.org/fhir/R4B/resource.html#id>).
                            id: None,
                            expansion_parameters: Vec::new(),
                            language: None,
                            url: url.to_owned(),
                            version: metadata.version,
                            supplements: Vec::new(),
                            standards_status: None,
                            name: metadata.name,
                            title: metadata.title,
                            status: String::from("active"),
                            experimental: metadata.experimental,
                            date: metadata.date,
                            publisher: None,
                            description: metadata.description,
                            copyright: metadata.copyright,
                            immutable: None,
                            compose,
                            contained: std::collections::BTreeMap::new(),
                        }))
                    }
                    Some(Err(source)) => {
                        let refused = Refused {
                            input: RefusedInput::Url,
                            text: url.to_owned(),
                            position: source.position(),
                        };
                        Err(OperationError::from(source).pointing(refused))
                    }
                    None => Err(OperationError::UnknownValueSet(match version {
                        Some(version) => format!("{url}|{version}"),
                        None => url.to_owned(),
                    })),
                }
            }
            (None, None) => Err(OperationError::Required(String::from(
                "a value set is required: the `url` parameter or an inline `valueSet`",
            ))),
        }
    }
}

/// The extension that names an issue's message
/// (<https://hl7.org/fhir/extensions/StructureDefinition-operationoutcome-message-id.html>).
pub const MESSAGE_ID_URL: &str =
    "http://hl7.org/fhir/StructureDefinition/operationoutcome-message-id";

/// The message the terminology ecosystem attaches to an issue: the reference
/// server's message key, which its test cases fix (spec-silent, #189).
///
/// The wire key is [`MessageId::key`]; the wording of `issue.details.text`
/// never decides the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageId {
    /// An unknown code in a code system labeled as a fragment.
    UnknownCodeInFragment,
    /// An unknown code in a versioned code system.
    UnknownCodeInVersion,
    /// An unknown code in a code system without a version.
    UnknownCodeIn,
    /// No coding of a `CodeableConcept` is in the value set.
    TxGeneralCcErrorMessage,
    /// The code is not in the value set.
    NoneOfTheProvidedCodesAreInTheValueSetOne,
    /// No display for the requested language; the given display fits the default one.
    NoValidDisplayFoundNoneForLangOk,
    /// No display for the requested language and the given display fits none.
    NoValidDisplayFoundNoneForLangErr,
    /// The display differs in its whitespace only.
    DisplayNameWsForShouldBeOneOfInsteadOf,
    /// The display is wrong.
    DisplayNameForShouldBeOneOfInsteadOf,
    /// A supplement the value set requires is not loaded.
    ValueSetSupplementMissing,
    /// The value set cannot be resolved.
    UnableToResolveValueSet,
    /// A code system version an expansion needs is unknown.
    UnknownCodeSystemVersionExp,
    /// A code system version is unknown.
    UnknownCodeSystemVersion,
    /// A code system is unknown.
    UnknownCodeSystem,
    /// The versionless include's default version differs from the coding's.
    ValueSetValueMismatchDefault,
    /// The include's version, once resolved, differs from the coding's.
    ValueSetValueMismatchChanged,
    /// The include's version differs from the coding's.
    ValueSetValueMismatch,
    /// A `check-system-version` or `check-valueset-version` failed.
    ValueSetVersionCheck,
    /// An abstract code where `abstract = false`.
    AbstractCodeNotAllowed,
    /// The code differs from the system's spelling by case or form.
    CodeCaseDifference,
    /// The code is valid and inactive.
    StatusCodeWarningCode,
    /// The value set marks the concept deprecated.
    ConceptDeprecatedInValueSet,
    /// The code system marks the concept deprecated.
    DeprecatedConceptFound,
    /// The concept is inactive.
    InactiveConceptFound,
    /// The display matches a retired designation.
    InactiveDisplayFound,
    /// A reference to a draft resource.
    MsgDraft,
    /// A reference to a deprecated resource.
    MsgDeprecated,
    /// A reference to a withdrawn resource.
    MsgWithdrawn,
    /// A reference to an experimental resource.
    MsgExperimental,
    /// The code system cannot be inferred from the code.
    UnableToInferCodeSystem,
    /// More than one system of the value set holds the code, so no one system
    /// can be inferred from it.
    UnableToResolveSystemValueSetHasMultipleMatches,
    /// A supplement was named as the code system.
    CodeSystemCsNoSupplement,
    /// The coding names a value set as its system.
    TerminologyTxSystemValueSet2,
    /// The coding's system is a relative reference.
    TerminologyTxSystemRelative,
    /// The coding has no system.
    CodingHasNoSystemCannotValidate,
    /// The code system cannot be enumerated.
    CodeSystemNotEnumerable,
    /// The expansion is too costly.
    ValueSetTooCostly,
    /// Any other failure.
    TxGeneralError,
}

impl MessageId {
    /// The `operationoutcome-message-id` value on the wire.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::UnknownCodeInFragment => "UNKNOWN_CODE_IN_FRAGMENT",
            Self::UnknownCodeInVersion => "Unknown_Code_in_Version",
            Self::UnknownCodeIn => "Unknown_Code_in",
            Self::TxGeneralCcErrorMessage => "TX_GENERAL_CC_ERROR_MESSAGE",
            Self::NoneOfTheProvidedCodesAreInTheValueSetOne => {
                "None_of_the_provided_codes_are_in_the_value_set_one"
            }
            Self::NoValidDisplayFoundNoneForLangOk => "NO_VALID_DISPLAY_FOUND_NONE_FOR_LANG_OK",
            Self::NoValidDisplayFoundNoneForLangErr => "NO_VALID_DISPLAY_FOUND_NONE_FOR_LANG_ERR",
            Self::DisplayNameWsForShouldBeOneOfInsteadOf => {
                "Display_Name_WS_for__should_be_one_of__instead_of"
            }
            Self::DisplayNameForShouldBeOneOfInsteadOf => {
                "Display_Name_for__should_be_one_of__instead_of"
            }
            Self::ValueSetSupplementMissing => "VALUESET_SUPPLEMENT_MISSING",
            Self::UnableToResolveValueSet => "Unable_to_resolve_value_Set_",
            Self::UnknownCodeSystemVersionExp => "UNKNOWN_CODESYSTEM_VERSION_EXP",
            Self::UnknownCodeSystemVersion => "UNKNOWN_CODESYSTEM_VERSION",
            Self::UnknownCodeSystem => "UNKNOWN_CODESYSTEM",
            Self::ValueSetValueMismatchDefault => "VALUESET_VALUE_MISMATCH_DEFAULT",
            Self::ValueSetValueMismatchChanged => "VALUESET_VALUE_MISMATCH_CHANGED",
            Self::ValueSetValueMismatch => "VALUESET_VALUE_MISMATCH",
            Self::ValueSetVersionCheck => "VALUESET_VERSION_CHECK",
            Self::AbstractCodeNotAllowed => "ABSTRACT_CODE_NOT_ALLOWED",
            Self::CodeCaseDifference => "CODE_CASE_DIFFERENCE",
            Self::StatusCodeWarningCode => "STATUS_CODE_WARNING_CODE",
            Self::ConceptDeprecatedInValueSet => "CONCEPT_DEPRECATED_IN_VALUESET",
            Self::DeprecatedConceptFound => "DEPRECATED_CONCEPT_FOUND",
            Self::InactiveConceptFound => "INACTIVE_CONCEPT_FOUND",
            Self::InactiveDisplayFound => "INACTIVE_DISPLAY_FOUND",
            Self::MsgDraft => "MSG_DRAFT",
            Self::MsgDeprecated => "MSG_DEPRECATED",
            Self::MsgWithdrawn => "MSG_WITHDRAWN",
            Self::MsgExperimental => "MSG_EXPERIMENTAL",
            Self::UnableToInferCodeSystem => "UNABLE_TO_INFER_CODESYSTEM",
            Self::UnableToResolveSystemValueSetHasMultipleMatches => {
                "Unable_to_resolve_system__value_set_has_multiple_matches"
            }
            Self::CodeSystemCsNoSupplement => "CODESYSTEM_CS_NO_SUPPLEMENT",
            Self::TerminologyTxSystemValueSet2 => "Terminology_TX_System_ValueSet2",
            Self::TerminologyTxSystemRelative => "Terminology_TX_System_Relative",
            Self::CodingHasNoSystemCannotValidate => "Coding_has_no_system__cannot_validate",
            Self::CodeSystemNotEnumerable => "CODESYSTEM_NOT_ENUMERABLE",
            Self::ValueSetTooCostly => "VALUESET_TOO_COSTLY",
            Self::TxGeneralError => "TX_GENERAL_ERROR",
        }
    }
}

/// One `OperationOutcome.issue` of a validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// `issue.severity`: `error`, `warning`, or `information`.
    pub severity: &'static str,
    /// `issue.code` from the issue-type value set.
    pub code: &'static str,
    /// The `tx-issue-type` code in `issue.details.coding`.
    pub kind: &'static str,
    /// The ecosystem's message, decided where the issue is raised.
    pub message: MessageId,
    /// `issue.details.text`.
    pub text: String,
    /// `issue.expression`: the parameter at fault.
    pub expression: Option<String>,
}

impl Issue {
    /// The message id the terminology ecosystem attaches to this issue.
    #[must_use]
    pub const fn message_id(&self) -> &'static str {
        self.message.key()
    }
}

/// A system the server does not serve, as `x-caused-by-unknown-system` names it.
///
/// Returns the canonical (`url` or `url|version`) and the issue that says so;
/// the wording is the ecosystem's
/// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>, the
/// `$validate-code` return parameters).
#[must_use]
pub fn unknown_system(
    url: &str,
    version: Option<&str>,
    expression: Option<String>,
    valid: &[String],
) -> (String, Issue) {
    let (canonical, message, text) = match version {
        Some(version) => (
            format!("{url}|{version}"),
            MessageId::UnknownCodeSystemVersion,
            format!(
                "A definition for CodeSystem '{url}' version '{version}' could not be found, so the code cannot be validated{}",
                valid_versions(valid)
            ),
        ),
        None => (
            url.to_owned(),
            MessageId::UnknownCodeSystem,
            format!(
                "A definition for CodeSystem '{url}' could not be found, so the code cannot be validated"
            ),
        ),
    };
    (
        canonical,
        Issue {
            severity: "error",
            code: "not-found",
            kind: "not-found",
            message,
            text,
            expression,
        },
    )
}

/// The `issue.expression` for `leaf` of the element at `base`.
///
/// A coding inside a `CodeableConcept` is addressed by its index and the part
/// at fault (`CodeableConcept.coding[1].display`), the ecosystem's shape; a
/// bare parameter is named as itself.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the callers store the expression in the issue's optional field as is"
)]
#[must_use]
pub fn at(base: &str, leaf: &str) -> Option<String> {
    Some(match base {
        "code" => leaf.to_owned(),
        "coding" => format!("Coding.{leaf}"),
        _ if base.starts_with("CodeableConcept.coding[") => format!("{base}.{leaf}"),
        _ => base.to_owned(),
    })
}

/// The `code-comment` warning for an active concept that is deprecated, with
/// the `status` output it earns (the ecosystem's `DEPRECATED_CONCEPT_FOUND`);
/// `None` otherwise.
///
/// A provider states the deprecation through `Status::standards_status`,
/// from the standards-status extension or from the standard concept
/// properties (`status = deprecated`, a past `deprecated` or
/// `deprecationDate`).
#[must_use]
pub fn deprecated_note(
    code: &str,
    status: &crate::provider::Status,
    expression: Option<String>,
) -> Option<(Issue, String)> {
    if !status.active || status.standards_status.as_deref() != Some("deprecated") {
        return None;
    }
    Some((
        Issue {
            severity: "warning",
            code: "business-rule",
            kind: "code-comment",
            message: MessageId::DeprecatedConceptFound,
            text: format!("The concept '{code}' is deprecated and its use should be reviewed"),
            expression,
        },
        String::from("deprecated"),
    ))
}

/// The warning and the status output an inactive concept earns.
///
/// The ecosystem asks for a `code-comment` warning beside `inactive = true`,
/// and for `status` when the system states one
/// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>, "Inactive Codes").
#[must_use]
pub fn inactive_note(
    code: &str,
    status: &crate::provider::Status,
    expression: Option<String>,
) -> Option<(Issue, Option<String>)> {
    if status.active {
        return None;
    }
    let reason = status.inactive_reason.as_deref().unwrap_or("inactive");
    let (text, status_code) = if reason == "inactive" {
        (
            format!("The concept '{code}' has a status of inactive and its use should be reviewed"),
            None,
        )
    } else {
        (
            format!(
                "The concept '{code}' has a status of {reason} and inactive and its use should be reviewed"
            ),
            Some(reason.to_owned()),
        )
    };
    Some((
        Issue {
            severity: "warning",
            code: "business-rule",
            kind: "code-comment",
            message: MessageId::InactiveConceptFound,
            text,
            expression,
        },
        status_code,
    ))
}

/// The `issue.expression` of the whole input element (`Coding`, `code`, or the
/// indexed concept coding), for a note about the concept itself.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the callers store the expression in the issue's optional field as is"
)]
#[must_use]
pub fn whole(base: &str) -> Option<String> {
    Some(match base {
        "coding" => String::from("Coding"),
        other => other.to_owned(),
    })
}

/// The word for a standing that warrants a note, else `None`.
///
/// `structuredefinition-standards-status` outranks the resource's own `status`
/// and `experimental`
/// (<https://hl7.org/fhir/R5/extension-structuredefinition-standards-status.html>).
#[must_use]
pub fn standing_word(standing: &crate::provider::Standing) -> Option<&'static str> {
    match standing.standards_status.as_deref() {
        Some("deprecated") => Some("deprecated"),
        Some("withdrawn") => Some("withdrawn"),
        Some("draft") => Some("draft"),
        _ if standing.status == "draft" => Some("draft"),
        _ if standing.experimental => Some("experimental"),
        _ => None,
    }
}

/// The `status-check` notes the ecosystem asks for when a referenced resource
/// is draft, experimental, deprecated, or withdrawn (its test cases).
#[must_use]
pub fn standing_note(
    kind: &str,
    canonical: &str,
    standing: &crate::provider::Standing,
) -> Option<Issue> {
    let word = standing_word(standing)?;
    let message = match word {
        "deprecated" => MessageId::MsgDeprecated,
        "withdrawn" => MessageId::MsgWithdrawn,
        "experimental" => MessageId::MsgExperimental,
        _ => MessageId::MsgDraft,
    };
    Some(Issue {
        severity: "information",
        code: "business-rule",
        kind: "status-check",
        message,
        text: format!("Reference to {word} {kind} {canonical}"),
        expression: None,
    })
}

/// The `Valid versions: a or b` tail of a not-found text, from the served
/// versions of the system; empty when none is served.
#[must_use]
pub fn valid_versions(versions: &[String]) -> String {
    match versions {
        [] => String::new(),
        [one] => format!(". Valid versions: {one}"),
        // NOTE: the ecosystem writes two with `or` and more with `and`; both
        // spellings are in its own fixtures.
        [first, last] => format!(". Valid versions: {first} or {last}"),
        [head @ .., last] => format!(". Valid versions: {} and {last}", head.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::{Issue, MessageId, standing_word};
    use crate::provider::Standing;

    #[test]
    fn a_standards_status_outranks_the_resources_own_status() {
        // <https://hl7.org/fhir/R5/extension-structuredefinition-standards-status.html>
        let deprecated = Standing {
            status: String::from("draft"),
            experimental: true,
            standards_status: Some(String::from("deprecated")),
        };
        assert_eq!(
            standing_word(&deprecated),
            Some("deprecated"),
            "the standards status is the resource's standing"
        );
    }

    #[test]
    fn a_settled_resource_warrants_no_standing_note() {
        assert_eq!(
            standing_word(&Standing::default()),
            None,
            "an active, non-experimental resource has nothing to note"
        );
        let experimental = Standing {
            experimental: true,
            ..Standing::default()
        };
        assert_eq!(standing_word(&experimental), Some("experimental"));
        let draft = Standing {
            status: String::from("draft"),
            ..Standing::default()
        };
        assert_eq!(standing_word(&draft), Some("draft"));
    }

    #[test]
    fn the_message_id_is_the_issues_own_and_survives_a_reworded_text() {
        let issue = Issue {
            severity: "information",
            code: "business-rule",
            kind: "code-rule",
            message: MessageId::CodeCaseDifference,
            text: String::from("Any wording at all"),
            expression: None,
        };
        assert_eq!(issue.message_id(), "CODE_CASE_DIFFERENCE");
        assert_eq!(MessageId::TxGeneralError.key(), "TX_GENERAL_ERROR");
    }
}
