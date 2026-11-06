// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The failures a provider reports, with the detail an `OperationOutcome`
//! carries about a refused filter value or implicit value set.

/// A failure inside a provider.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// The system cannot decide the relationship asked of it.
    #[error("cannot determine: {0}")]
    CannotDetermine(String),
    /// The system cannot enumerate its concepts (a grammar-defined system).
    #[error("the code system cannot enumerate its concepts")]
    NotEnumerable,
    /// The system enumerates some selections of its codes, and these filters
    /// bound none of them.
    #[error("this filter cannot be expanded")]
    FilterNotEnumerable,
    /// The provider does not answer this filter.
    #[error("filter `{property}` with operator `{operator}` is not supported")]
    UnsupportedFilter {
        /// The filter property.
        property: String,
        /// The operator code.
        operator: String,
    },
    /// The filter value is not what the operator expects.
    #[error(transparent)]
    InvalidFilterValue(Box<InvalidValue>),
    /// A regular expression does not compile.
    #[error("invalid regular expression")]
    Regex(#[from] regex::Error),
    /// A filter names a code the system does not have.
    #[error("unknown code `{0}`")]
    UnknownCode(String),
    /// The code is malformed for the code system's grammar, and the system
    /// can say why (a postcoordination value on no axis of its stem).
    #[error("code `{code}` is invalid: {reason}")]
    InvalidCode {
        /// The code as given.
        code: String,
        /// Why it is not a code.
        reason: String,
    },
    /// The storage behind the provider failed; the cause is the substrate's error.
    #[error("the code system storage failed")]
    Storage(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
    /// The code system's content is not present or is an example, so a code
    /// cannot be validated or enumerated against it.
    #[error("code system `{system}` has content `{content}`; its codes cannot be checked")]
    IncompleteContent {
        /// The system.
        system: String,
        /// The `content` mode.
        content: &'static str,
    },
    /// An implicit value set URI of this system is malformed.
    #[error(transparent)]
    MalformedImplicitValueSet(Box<MalformedValueSet>),
    /// An implicit concept map URI of this system is malformed.
    #[error("implicit concept map `{url}` is malformed: {reason}")]
    MalformedImplicitConceptMap {
        /// The URI.
        url: String,
        /// Why.
        reason: String,
    },
    /// An implicit concept map URI names a map this system does not hold.
    #[error("implicit concept map `{url}` names no map this code system holds")]
    UnknownImplicitConceptMap {
        /// The URI.
        url: String,
    },
    /// An implicit URI of this system names a version of it that this provider
    /// is not, so another loaded version of the system may answer it.
    #[error("unknown version `{version}` of code system `{url}`")]
    UnservedImplicitVersion {
        /// The system.
        url: String,
        /// The version the implicit URI names.
        version: String,
    },
    /// An implicit concept map maps to a code system this server cannot name,
    /// so its group could state no target system.
    #[error("implicit concept map `{url}` maps to a code system this server cannot name: {reason}")]
    UnnamedConceptMapTarget {
        /// The URI.
        url: String,
        /// Which target the server has no code system URI for.
        reason: String,
    },
}

impl ProviderError {
    /// The refusal of `value` for the filter on `property`, for `reason`.
    #[must_use]
    pub fn invalid_filter_value(
        property: impl Into<String>,
        value: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::InvalidFilterValue(Box::new(InvalidValue {
            property: property.into(),
            value: value.into(),
            reason: reason.into(),
            position: None,
            source: None,
        }))
    }

    /// The refusal of the implicit value set `url`, for `reason`.
    #[must_use]
    pub fn malformed_implicit_value_set(url: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::MalformedImplicitValueSet(Box::new(MalformedValueSet {
            url: url.into(),
            reason: reason.into(),
            position: None,
            source: None,
        }))
    }

    /// The byte offset into the refused text where a parser stopped reading
    /// it, when a parser refused it: into the filter value of
    /// [`ProviderError::InvalidFilterValue`], into the URI of
    /// [`ProviderError::MalformedImplicitValueSet`].
    #[must_use]
    pub fn position(&self) -> Option<usize> {
        match self {
            Self::InvalidFilterValue(invalid) => invalid.position,
            Self::MalformedImplicitValueSet(malformed) => malformed.position,
            _ => None,
        }
    }
}

/// A filter value the operator refuses, and where a parser stopped reading it.
///
/// It sits boxed inside [`ProviderError`], so the parse error costs the other
/// variants no width (no specification governs this: our own design).
#[derive(Debug, thiserror::Error)]
#[error("filter `{property}` value `{value}` is invalid: {reason}")]
pub struct InvalidValue {
    /// The filter property.
    pub property: String,
    /// The offending value.
    pub value: String,
    /// Why.
    pub reason: String,
    /// The byte offset into `value` where a parser stopped, when one refused it.
    pub position: Option<usize>,
    /// The parser's error, when a parser refused the value.
    #[source]
    pub source: Option<sct_scg::front::ParseError>,
}

/// An implicit value set URI a system refuses, and where a parser stopped
/// reading it.
#[derive(Debug, thiserror::Error)]
#[error("implicit value set `{url}` is malformed: {reason}")]
pub struct MalformedValueSet {
    /// The URI.
    pub url: String,
    /// Why.
    pub reason: String,
    /// The byte offset into `url` where a parser stopped reading the part the
    /// system parses, when one refused it.
    pub position: Option<usize>,
    /// The parser's error, when a parser refused the URI.
    #[source]
    pub source: Option<sct_scg::front::ParseError>,
}
