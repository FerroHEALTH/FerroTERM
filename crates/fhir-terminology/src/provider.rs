// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The code system provider seam.
//!
//! Every code system reaches the operations through [`CodeSystemProvider`]:
//! an opaque concept handle, the FHIR `CodeSystem` metadata as its capability
//! declaration (<https://hl7.org/fhir/R4B/codesystem.html>), and the reads the
//! operations need. Storage stays behind the provider; the compose layer,
//! paging, and dedup live once above it.

use std::collections::BTreeSet;
use std::fmt;

use roaring::RoaringBitmap;

use concept_graph::subsumption::Outcome;

use crate::compose::Compose;
use crate::filter::{Filter, FilterOperator};

/// An opaque handle to one concept of one code system version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Concept(u32);

impl Concept {
    /// A handle over a dense index.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The dense index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// A set of concept handles, in sorted order.
pub type ConceptSet = RoaringBitmap;

/// Every ordinal below `concepts`, the selection a system's whole content is.
///
/// A range fills roaring's containers whole
/// (<https://docs.rs/roaring/0.11.5/roaring/struct.RoaringBitmap.html#method.insert_range>),
/// which an insert per ordinal does not.
#[must_use]
pub fn every(concepts: u32) -> ConceptSet {
    let mut set = ConceptSet::new();
    set.insert_range(0..concepts);
    set
}

/// Who a code system version is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The canonical system URI (`CodeSystem.url`).
    pub url: String,
    /// The version string (`CodeSystem.version`).
    pub version: String,
    /// The computer-friendly name (`CodeSystem.name`).
    pub name: Option<String>,
    /// A human title (`CodeSystem.title`).
    pub title: Option<String>,
    /// Whether a version is needed to interpret a code (`CodeSystem.versionNeeded`).
    pub version_needed: bool,
}

/// `CodeSystem.content` (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentMode {
    /// `not-present`: the server holds the content and serves it.
    NotPresent,
    /// `example`.
    Example,
    /// `fragment`.
    Fragment,
    /// `complete`.
    Complete,
    /// `supplement`.
    Supplement,
}

impl ContentMode {
    /// The FHIR code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotPresent => "not-present",
            Self::Example => "example",
            Self::Fragment => "fragment",
            Self::Complete => "complete",
            Self::Supplement => "supplement",
        }
    }
}

/// `CodeSystem.hierarchyMeaning` (<https://hl7.org/fhir/R4B/codesystem-hierarchy-meaning.html>).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HierarchyMeaning {
    /// `grouped-by`.
    GroupedBy,
    /// `is-a`: subsumption.
    IsA,
    /// `part-of`.
    PartOf,
    /// `classified-with`.
    ClassifiedWith,
}

impl HierarchyMeaning {
    /// The FHIR code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::GroupedBy => "grouped-by",
            Self::IsA => "is-a",
            Self::PartOf => "part-of",
            Self::ClassifiedWith => "classified-with",
        }
    }
}

/// `CodeSystem.property.type` (<https://hl7.org/fhir/R4B/concept-property-type.html>).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    /// `code`.
    Code,
    /// `Coding`.
    Coding,
    /// `string`.
    String,
    /// `integer`.
    Integer,
    /// `boolean`.
    Boolean,
    /// `dateTime`.
    DateTime,
    /// `decimal`.
    Decimal,
}

impl PropertyKind {
    /// The FHIR code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Coding => "Coding",
            Self::String => "string",
            Self::Integer => "integer",
            Self::Boolean => "boolean",
            Self::DateTime => "dateTime",
            Self::Decimal => "decimal",
        }
    }
}

/// One declared property (`CodeSystem.property`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyDefinition {
    /// The property code.
    pub code: String,
    /// The formal identity of the property, if any.
    pub uri: Option<String>,
    /// A description.
    pub description: Option<String>,
    /// The value type.
    pub kind: PropertyKind,
}

/// One declared filter (`CodeSystem.filter`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterDefinition {
    /// The filter code (the property a filter names).
    pub code: String,
    /// A description.
    pub description: Option<String>,
    /// The operators the filter accepts.
    pub operators: Vec<FilterOperator>,
    /// What the filter value is (`CodeSystem.filter.value`).
    pub value: String,
}

/// An optional capability a provider declares rather than every system assumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    /// Subsumption and the hierarchy filters; `hierarchy()` returns `Some`.
    Subsumption,
    /// Enumeration of every concept, for expansion without a filter.
    Enumeration,
    /// Implicit concept maps parsed from the system URI.
    ImplicitConceptMaps,
    /// Alternate or normalized codes resolve through `locate`.
    NormalizedCodes,
}

/// What a code system's compositional grammar is, and whether this server
/// evaluates it.
///
/// FHIR states the grammar twice, with two different definitions, so one flag
/// cannot answer both. `CodeSystem.compositional` is "The code system defines
/// a compositional (post-coordination) grammar"
/// (<https://hl7.org/fhir/R4B/codesystem-definitions.html#CodeSystem.compositional>),
/// a property of the code system.
/// `TerminologyCapabilities.codeSystem.version.compositional` is "If the
/// compositional grammar defined by the code system is supported"
/// (<https://hl7.org/fhir/R4B/terminologycapabilities-definitions.html#TerminologyCapabilities.codeSystem.version.compositional>),
/// a property of this server. R4, R5, and R6 define both elements in the same
/// words. Every state is one of three, so a server that supports a grammar the
/// code system does not define cannot be spelled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Compositional {
    /// The code system defines no compositional grammar.
    #[default]
    None,
    /// The code system defines a grammar, and this server does not evaluate
    /// it: a code written in the grammar is refused.
    Defined,
    /// The code system defines a grammar, and this server evaluates it: a code
    /// written in the grammar locates.
    Supported,
}

impl Compositional {
    /// `CodeSystem.compositional`: whether the code system defines a grammar.
    #[must_use]
    pub const fn defined(self) -> bool {
        !matches!(self, Self::None)
    }

    /// `TerminologyCapabilities.codeSystem.version.compositional`: whether this
    /// server supports the grammar the code system defines.
    #[must_use]
    pub const fn supported(self) -> bool {
        matches!(self, Self::Supported)
    }
}

/// What a provider declares about its code system (the `TerminologyCapabilities` input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// `CodeSystem.content`.
    pub content: ContentMode,
    /// `CodeSystem.caseSensitive`.
    pub case_sensitive: bool,
    /// `CodeSystem.hierarchyMeaning`, when the system has a hierarchy.
    pub hierarchy_meaning: Option<HierarchyMeaning>,
    /// The code system's compositional grammar, and whether this server
    /// evaluates it: `CodeSystem.compositional` and
    /// `TerminologyCapabilities.codeSystem.version.compositional` read
    /// different halves of it.
    pub compositional: Compositional,
    /// The designation languages the system carries (BCP 47).
    pub languages: Vec<String>,
    /// The declared properties.
    pub properties: Vec<PropertyDefinition>,
    /// The declared filters, beyond the generic ones every provider answers.
    pub filters: Vec<FilterDefinition>,
    /// The optional capabilities.
    pub capabilities: BTreeSet<Capability>,
    /// The implicit value set forms
    /// [`CodeSystemProvider::implicit_value_set`] resolves, one per URL
    /// template the system's page defines; empty when it resolves none.
    pub implicit_forms: Vec<ImplicitForm>,
}

/// One implicit value set form a provider resolves: a URL template and what
/// its placeholders take.
///
/// No FHIR/SNOMED spec governs this declaration: our own design. The pattern
/// is spelled as the system's page spells it, with each placeholder in square
/// brackets (`http://snomed.info/sct?fhir_vs=isa/[sctid]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplicitForm {
    /// The URL template.
    pub pattern: String,
    /// What the bracketed placeholders take.
    pub argument: ImplicitArgument,
}

impl ImplicitForm {
    /// Creates a form from its template and argument kind.
    #[must_use]
    pub fn new(pattern: &str, argument: ImplicitArgument) -> Self {
        Self {
            pattern: pattern.to_owned(),
            argument,
        }
    }
}

/// What the placeholders of an implicit value set template take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImplicitArgument {
    /// The template has no placeholder and is the value set URL as is.
    None,
    /// The one placeholder takes a code of the system.
    Code,
    /// The placeholders take free text: an expression, an identifier, or a
    /// URI the system defines.
    Expression,
}

impl ImplicitArgument {
    /// The code the capability statement carries (`none`, `code`, `expression`).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Code => "code",
            Self::Expression => "expression",
        }
    }
}

/// A located code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    /// The concept.
    pub concept: Concept,
    /// The code as the system spells it, which differs from the input for a
    /// case-insensitive or normalized match.
    pub code: String,
}

/// The standing of a code system as a resource: its publication status and
/// the terminology ecosystem's notes on referencing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    /// `CodeSystem.status` (`draft`, `active`, `retired`, `unknown`).
    pub status: String,
    /// `CodeSystem.experimental`.
    pub experimental: bool,
    /// The `structuredefinition-standards-status` extension's code, when set.
    pub standards_status: Option<String>,
}

impl Default for Standing {
    fn default() -> Self {
        Self {
            status: String::from("active"),
            experimental: false,
            standards_status: None,
        }
    }
}

/// The metadata an implicit value set carries beside its compose.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImplicitMetadata {
    /// `ValueSet.version`.
    pub version: Option<String>,
    /// `ValueSet.name`.
    pub name: Option<String>,
    /// `ValueSet.title`.
    pub title: Option<String>,
    /// `ValueSet.experimental`.
    pub experimental: Option<bool>,
    /// `ValueSet.date`.
    pub date: Option<String>,
    /// `ValueSet.description`.
    pub description: Option<String>,
    /// `ValueSet.copyright`: the licence notice the code system attaches to
    /// content drawn from it.
    pub copyright: Option<String>,
}

/// The status of a concept, for `inactive` and `abstract` on the wire.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_field_names,
    reason = "`standards_status` is the name of the FHIR extension it carries"
)]
pub struct Status {
    /// The concept's `structuredefinition-standards-status` code, when set
    /// (`deprecated`, `withdrawn`, ...): a note beside an active concept.
    pub standards_status: Option<String>,
    /// Whether the concept is active.
    pub active: bool,
    /// Why it is inactive, as the system states it.
    pub inactive_reason: Option<String>,
    /// Whether the concept is abstract (`notSelectable`).
    pub abstract_concept: bool,
    /// Whether the concept has no code of its own and is addressed by its URI
    /// (an ICD-11 grouper); `$lookup` answers it `notSelectable` as a property
    /// and no `abstract`, expansions and validation keep `abstract_concept`.
    pub codeless: bool,
}

/// The `use` of a designation (`ValueSet.expansion.contains.designation.use`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignationUse {
    /// The system of the use code.
    pub system: String,
    /// The use code.
    pub code: String,
    /// The display of the use code.
    pub display: Option<String>,
}

/// One designation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Designation {
    /// The BCP 47 language.
    pub language: Option<String>,
    /// The use.
    pub use_: Option<DesignationUse>,
    /// The text.
    pub value: String,
    /// The designation's `structuredefinition-standards-status` code, when
    /// set: a `withdrawn` or `deprecated` designation is no longer a correct
    /// display.
    pub standards_status: Option<String>,
}

/// A typed property value (`Parameters.parameter.part.value[x]` of `$lookup`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyValue {
    /// A `code`.
    Code(String),
    /// A `uri`.
    Uri(String),
    /// A `Coding`.
    Coding {
        /// The system.
        system: String,
        /// The code.
        code: String,
        /// The display.
        display: Option<String>,
    },
    /// A `string`.
    String(String),
    /// An `integer`.
    Integer(i64),
    /// A `boolean`.
    Boolean(bool),
    /// A `dateTime`, in its lexical form.
    DateTime(String),
    /// A `decimal`, in its lexical form.
    Decimal(String),
}

impl PropertyValue {
    /// The value as text, the form filters compare against.
    #[must_use]
    pub fn as_text(&self) -> String {
        match self {
            Self::Code(text)
            | Self::Uri(text)
            | Self::String(text)
            | Self::DateTime(text)
            | Self::Decimal(text) => text.clone(),
            Self::Coding { code, .. } => code.clone(),
            Self::Integer(value) => value.to_string(),
            Self::Boolean(value) => value.to_string(),
        }
    }
}

/// One property of a concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    /// The property code.
    pub code: String,
    /// The value.
    pub value: PropertyValue,
    /// A human description of the value (`$lookup` `property.description`).
    pub description: Option<String>,
    /// The parts of a structured value (`$lookup` `property.subproperty`).
    pub subproperties: Vec<Subproperty>,
}

impl Default for Property {
    fn default() -> Self {
        Self {
            code: String::new(),
            value: PropertyValue::String(String::new()),
            description: None,
            subproperties: Vec::new(),
        }
    }
}

/// The concept property codes the FHIR specification itself defines, in the
/// `http://hl7.org/fhir/concept-properties` code system
/// (<https://hl7.org/fhir/R5/codesystem.html#defined-props>; R6 adds
/// `alternateCode`, `order`, `definition`, and the `lastVersion*` and
/// `replacedBy*` pair).
const SPECIFICATION_CONCEPT_PROPERTIES: [&str; 20] = [
    "status",
    "inactive",
    "effectiveDate",
    "deprecated",
    "deprecationDate",
    "retirementDate",
    "notSelectable",
    "parent",
    "child",
    "partOf",
    "synonym",
    "alternateCode",
    "comment",
    "itemWeight",
    "order",
    "definition",
    "lastVersionActive",
    "lastVersionPresent",
    "replacedByCode",
    "replacedByCoding",
];

/// The formal URI of `code` when the FHIR specification defines it as a
/// concept property, else `None`.
///
/// A code system that declares the property itself states its own URI; this
/// answers for the ones every system shares
/// (<https://hl7.org/fhir/R5/codesystem.html#defined-props>).
#[must_use]
pub fn defined_property_uri(code: &str) -> Option<String> {
    SPECIFICATION_CONCEPT_PROPERTIES
        .contains(&code)
        .then(|| format!("http://hl7.org/fhir/concept-properties#{code}"))
}

/// One part of a structured property value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subproperty {
    /// The part's code.
    pub code: String,
    /// The part's value.
    pub value: PropertyValue,
    /// A human description of the part.
    pub description: Option<String>,
}

/// One concept a system names in place of an inactive one.
///
/// SNOMED CT states these in its historical association reference sets, so a
/// translation of a retired concept answers with its successor
/// (<https://hl7.org/fhir/R4B/snomedct.html>).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Successor {
    /// The code that stands in for the inactive one.
    pub code: String,
    /// Its display.
    pub display: Option<String>,
    /// The relationship the association asserts.
    pub relationship: crate::conceptmap::model::Relationship,
    /// The canonical of the implicit concept map the association forms.
    pub map: String,
}

/// Which of an implicit concept map's mappings a caller needs.
///
/// Reading the `ConceptMap` resource needs every one; `$translate` needs the
/// mappings from one code, and a reverse translation the mappings to one
/// (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>). An
/// implicit map is built on demand out of a system's own content, so the
/// selection is what keeps a translation costing the size of its answer
/// rather than the size of the map. No specification governs it: our own
/// design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapSelection<'a> {
    /// Every mapping the map states.
    Whole,
    /// The mappings from this code, as the elements that carry them.
    Source(&'a str),
    /// The mappings to this code, as the elements that carry them.
    ///
    /// An element that also targets other codes keeps only the targets that
    /// carry this one, which is what a reverse translation reads.
    Target(&'a str),
}

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
    pub source: Option<sct_ecl::ParseError>,
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
    pub source: Option<sct_ecl::ParseError>,
}

/// The hierarchy of a system whose `hierarchyMeaning` is `is-a`.
pub trait Hierarchy: fmt::Debug {
    /// The direct parents.
    fn parents(&self, concept: Concept) -> ConceptSet;
    /// Whether any direct parent of `concept` lies in `set`.
    ///
    /// This is the root test of a nested expansion, asked once per member of
    /// the selection, so the default's set per call is worth overriding wherever
    /// the adjacency is already a list.
    fn any_parent_in(&self, concept: Concept, set: &ConceptSet) -> bool {
        self.parents(concept)
            .iter()
            .any(|parent| set.contains(parent))
    }
    /// The direct children.
    fn children(&self, concept: Concept) -> ConceptSet;
    /// Every ancestor, excluding the concept.
    fn ancestors(&self, concept: Concept) -> ConceptSet;
    /// Every descendant, excluding the concept.
    fn descendants(&self, concept: Concept) -> ConceptSet;
    /// Whether `a` subsumes `b`, in the FHIR `$subsumes` vocabulary.
    fn subsumes(&self, a: Concept, b: Concept) -> Outcome {
        if a == b {
            Outcome::Equivalent
        } else if self.descendants(a).contains(b.index()) {
            Outcome::Subsumes
        } else if self.descendants(b).contains(a.index()) {
            Outcome::SubsumedBy
        } else {
            Outcome::NotSubsumed
        }
    }
}

/// One code system version behind the seam.
pub trait CodeSystemProvider: fmt::Debug + Send + Sync {
    /// Who this version is.
    fn identity(&self) -> &Identity;

    /// What it declares.
    fn declaration(&self) -> &Declaration;

    /// The `CodeSystem` resource this provider was built from, when it was
    /// built from one; `None` for a system the server holds behind an index.
    ///
    /// A read of the instance renders this when it is `Some`, so a system
    /// whose `content` says its concepts are in the resource answers with
    /// them (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>).
    fn code_system(&self) -> Option<&crate::fhir_codesystem::model::CodeSystemModel> {
        None
    }

    /// Finds a code; `Ok(None)` when the system has no such code.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails; a code that
    /// is absent is never an error.
    fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError>;

    /// The code of a concept.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError>;

    /// The display in `language`, or the system's default display.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn display(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError>;

    /// The formal definition, if the system has one.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn definition(&self, _concept: Concept) -> Result<Option<String>, ProviderError> {
        Ok(None)
    }

    /// Whether `code` is written in the code system's compositional grammar.
    ///
    /// A provider that declares [`Compositional::Defined`] recognises the shape
    /// of an expression it does not evaluate, so an operation refuses the code
    /// as a grammar this server does not serve rather than as a concept the
    /// code system does not have. Every other provider answers `false`: one
    /// that declares [`Compositional::Supported`] locates such a code instead.
    fn is_expression(&self, _code: &str) -> bool {
        false
    }

    /// Why `code` is not a code of this system, when the system can say more
    /// than "absent".
    ///
    /// A system whose compositional grammar this server evaluates knows
    /// whether a code it refused was malformed and where, or well formed and
    /// naming a concept the version does not define. The `$validate-code`
    /// message carries the answer instead of "unknown code" alone.
    fn rejection(&self, _code: &str) -> Option<String> {
        None
    }

    /// Whether `filter` is the one that admits post-coordinated expressions
    /// into a selection.
    ///
    /// A value set states post-coordination with a filter its code system
    /// defines, so the system names its own; the compose layer only asks
    /// whether one of an include's filters says yes.
    fn admits_post_coordination(&self, _filter: &Filter) -> bool {
        false
    }

    /// The language of the system's own displays (`CodeSystem.language`),
    /// when the system states one.
    fn language(&self) -> Option<&str> {
        None
    }

    /// The built index this version was read from, when the server read one.
    ///
    /// A system posted as a `CodeSystem` resource and a registry the binary
    /// carries answer `None`, so the capability statement declares nothing for
    /// them rather than an invented artifact.
    fn artifact(&self) -> Option<&crate::artifact::Source> {
        None
    }

    /// The system's standing as a resource; active and unremarkable unless the
    /// system states otherwise.
    fn standing(&self) -> Standing {
        Standing::default()
    }

    /// Active or inactive, abstract or not.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn status(&self, concept: Concept) -> Result<Status, ProviderError>;

    /// Every inactive concept, for `activeOnly` and `compose.inactive = false`
    /// over a large selection.
    ///
    /// The default scans every concept's status; a provider with a cheaper
    /// answer overrides.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::NotEnumerable`] when the system cannot
    /// enumerate, and [`ProviderError::Storage`] when the substrate fails.
    fn inactive(&self) -> Result<ConceptSet, ProviderError> {
        let mut inactive = ConceptSet::new();
        for index in self.all()? {
            if !self.status(Concept::new(index))?.active {
                inactive.insert(index);
            }
        }
        Ok(inactive)
    }

    /// The concepts an expansion for use outside a user interface leaves out:
    /// the abstract (`notSelectable`) groupers, for `excludeNotForUI`
    /// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>).
    ///
    /// The default scans every concept's status; a provider with a cheaper
    /// answer overrides.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::NotEnumerable`] when the system cannot
    /// enumerate, and [`ProviderError::Storage`] when the substrate fails.
    fn not_for_ui(&self) -> Result<ConceptSet, ProviderError> {
        let mut abstract_concepts = ConceptSet::new();
        for index in self.all()? {
            if self.status(Concept::new(index))?.abstract_concept {
                abstract_concepts.insert(index);
            }
        }
        Ok(abstract_concepts)
    }

    /// Whether `concept` is a post-coordinated expression the system composed
    /// on request, for `excludePostCoordinated`; a system without a grammar
    /// has none.
    fn is_postcoordinated(&self, _concept: Concept) -> bool {
        false
    }

    /// Every designation, optionally only those in `language`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn designations(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Vec<Designation>, ProviderError>;

    /// Every property.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn properties(&self, concept: Concept) -> Result<Vec<Property>, ProviderError>;

    /// The hierarchy, for a system that declares subsumption.
    fn hierarchy(&self) -> Option<&dyn Hierarchy> {
        None
    }

    /// Every concept, for a system that can enumerate.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::NotEnumerable`] for a grammar-defined system.
    fn all(&self) -> Result<ConceptSet, ProviderError>;

    /// The concepts with a designation whose words start with the words of `text`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn search(&self, text: &str, language: Option<&str>) -> Result<ConceptSet, ProviderError>;

    /// The compose an implicit value set URI of this system denotes, when the
    /// system defines implicit value sets and `url` is one of them.
    ///
    /// `None` when the URI is not an implicit value set of this system; the
    /// error when it is malformed. The default declares none.
    fn implicit_value_set(&self, _url: &str) -> Option<Result<Compose, ProviderError>> {
        None
    }

    /// The metadata of the implicit value set `url` denotes: the fields the
    /// returned `ValueSet` carries beside its compose. The default carries
    /// none; a system whose implicit sets have a version or a name of their
    /// own says so.
    fn implicit_metadata(&self, _url: &str) -> ImplicitMetadata {
        ImplicitMetadata::default()
    }

    /// Whether this version answers to `version` beside the one
    /// `identity().version` states: an alias the code system's own version
    /// scheme defines. The default declares none, so only the stated version
    /// and the ecosystem's `x` patterns resolve.
    fn answers_version(&self, _version: &str) -> bool {
        false
    }

    /// The property codes this provider generates on request.
    ///
    /// Each is answered by [`CodeSystemProvider::generated_property`] and only
    /// where the request named it. The default generates none.
    fn generated_properties(&self) -> &[&'static str] {
        &[]
    }

    /// The property `name` of `concept`, generated rather than read.
    ///
    /// A property a request has to ask for by name, because generating it
    /// costs more than reading one. `$lookup` names what it wants, and "if no
    /// properties are specified, the server chooses what to return"
    /// (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>, the
    /// `property` parameter), so a request that names none is answered without
    /// paying for these. The default generates none.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn generated_property(
        &self,
        _concept: Concept,
        _name: &str,
    ) -> Result<Option<Property>, ProviderError> {
        Ok(None)
    }

    /// The property codes this code system's specification defines that this
    /// server does not compute.
    ///
    /// `$lookup` refuses a `property` naming one of these with
    /// `not-supported`, so a client can tell a property this server will not
    /// compute from a concept that has nothing to say
    /// (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>, the
    /// `property` parameter). The default declares none.
    fn unserved_properties(&self) -> &[&'static str] {
        &[]
    }

    /// The concepts this system names in place of an inactive one, from its
    /// own historical associations. The default declares none.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Storage`] when the substrate fails.
    fn successors(&self, _concept: Concept) -> Result<Vec<Successor>, ProviderError> {
        Ok(Vec::new())
    }

    /// The `ConceptMap` an implicit concept map URI of this system denotes,
    /// carrying the elements `selection` asks for.
    ///
    /// `None` when the URI is not an implicit concept map of this system; the
    /// error when it is malformed or names a map the system does not hold.
    /// A narrowed `selection` returns the whole map with the mappings it
    /// excludes left out and every other field unchanged, so a caller reads
    /// the same answer either way. The default declares none.
    fn implicit_concept_map(
        &self,
        _url: &str,
        _selection: MapSelection<'_>,
    ) -> Option<Result<crate::conceptmap::model::ConceptMapModel, ProviderError>> {
        None
    }

    /// The concepts a filter selects.
    ///
    /// The default answers the generic operators on `concept` and `code`, the
    /// hierarchy operators when the system declares subsumption, and `=`,
    /// `in`, `not-in`, `regex`, `exists` over declared properties by scanning
    /// every concept; a provider overrides for its own filters and indexes.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::UnsupportedFilter`] for an operator or property
    /// the system does not answer, and the value errors of the operators.
    fn filter(&self, filter: &Filter) -> Result<ConceptSet, ProviderError> {
        crate::filter::evaluate(self, filter)
    }

    /// The concepts every filter of one include selects, all of them.
    ///
    /// The default intersects [`CodeSystemProvider::filter`] per filter; a
    /// system whose filters only make sense together (a registry flag that
    /// bounds an otherwise unbounded grammar) overrides.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`CodeSystemProvider::filter`], and
    /// [`ProviderError::NotEnumerable`] when the filters do not bound the set.
    fn filter_all(&self, filters: &[Filter]) -> Result<ConceptSet, ProviderError> {
        let Some((first, rest)) = filters.split_first() else {
            return self.all();
        };
        let mut set = self.filter(first)?;
        for filter in rest {
            set &= self.filter(filter)?;
        }
        Ok(set)
    }

    /// Whether the codes `filters` admit go beyond the concepts
    /// [`CodeSystemProvider::filter_all`] returns for them.
    ///
    /// A selection over a system that builds unboundedly many codes on a finite
    /// base enumerates the base and not the forms built on it, so its expansion
    /// is incomplete and `$expand` marks it: "if the value set itself is
    /// unbounded due to the inclusion of post-coordinated value sets (e.g.
    /// SNOMED CT, UCUM), then the extension
    /// `http://hl7.org/fhir/StructureDefinition/valueset-unclosed` can be used
    /// to indicate that the expansion is incomplete"
    /// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>, Notes). The
    /// default is a complete selection, and each provider decides for itself;
    /// an include that names its concepts never asks, because the codes it
    /// lists are the members.
    fn unclosed(&self, _filters: &[Filter]) -> bool {
        false
    }

    /// Why a selection this system leaves unclosed is unclosed, for
    /// `valueset-unclosed-reason` beside the mark.
    ///
    /// The default states no reason, and `$expand` then carries the mark
    /// alone.
    fn unclosed_reason(&self) -> Option<String> {
        None
    }

    /// The order this system states for what `filters` select.
    ///
    /// `None` leaves the selection in concept order, which is what a system
    /// whose ordinals already sort by code wants. A system that interns a code
    /// as it meets it has no such order, so it states one here and the
    /// expansion pages it that way.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`CodeSystemProvider::filter_all`].
    fn filter_ordered(&self, _filters: &[Filter]) -> Result<Option<Vec<u32>>, ProviderError> {
        Ok(None)
    }

    /// Whether `concept` satisfies `filter`, without enumerating the set.
    ///
    /// The default evaluates the filter and tests membership; a grammar system
    /// answers from the concept alone.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`CodeSystemProvider::filter`].
    fn filter_matches(&self, concept: Concept, filter: &Filter) -> Result<bool, ProviderError> {
        Ok(self.filter(filter)?.contains(concept.index()))
    }

    /// The subsumption of `a` over `b` when the system decides it without a
    /// materialized hierarchy (a grammar whose parameters narrow a code).
    ///
    /// `None` leaves the answer to [`CodeSystemProvider::hierarchy`].
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::CannotDetermine`] when the system cannot say.
    fn subsumes(&self, _a: Concept, _b: Concept) -> Result<Option<Outcome>, ProviderError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::{ContentMode, HierarchyMeaning, PropertyKind, defined_property_uri};

    #[test]
    fn a_specification_defined_property_carries_the_specifications_uri() {
        // <https://hl7.org/fhir/R5/codesystem.html#defined-props>
        assert_eq!(
            defined_property_uri("status").as_deref(),
            Some("http://hl7.org/fhir/concept-properties#status"),
            "status is defined in the FHIR concept properties"
        );
        // <https://hl7.org/fhir/6.0.0-ballot5/codesystem.html#defined-props>
        assert_eq!(
            defined_property_uri("definition").as_deref(),
            Some("http://hl7.org/fhir/concept-properties#definition"),
            "definition is defined in the FHIR concept properties from R6"
        );
    }

    #[test]
    fn a_code_systems_own_property_takes_no_specification_uri() {
        assert_eq!(
            defined_property_uri("prop"),
            None,
            "a property the specification does not define states its own URI"
        );
    }

    // The three enums below cross the wire: `content` and `hierarchyMeaning`
    // on a CodeSystem, and `type` on a property definition. Each variant's code
    // is fixed by the specification, so each is pinned rather than left to
    // whichever variant a provider happens to use.

    #[test]
    fn a_content_mode_carries_the_code_the_specification_defines() {
        // <https://hl7.org/fhir/R4B/valueset-codesystem-content-mode.html>
        assert_eq!(ContentMode::NotPresent.code(), "not-present");
        assert_eq!(ContentMode::Example.code(), "example");
        assert_eq!(ContentMode::Fragment.code(), "fragment");
        assert_eq!(ContentMode::Complete.code(), "complete");
        assert_eq!(ContentMode::Supplement.code(), "supplement");
    }

    #[test]
    fn a_hierarchy_meaning_carries_the_code_the_specification_defines() {
        // <https://hl7.org/fhir/R4B/valueset-codesystem-hierarchy-meaning.html>
        assert_eq!(HierarchyMeaning::GroupedBy.code(), "grouped-by");
        assert_eq!(HierarchyMeaning::IsA.code(), "is-a");
        assert_eq!(HierarchyMeaning::PartOf.code(), "part-of");
        assert_eq!(HierarchyMeaning::ClassifiedWith.code(), "classified-with");
    }

    #[test]
    fn a_property_kind_carries_the_code_the_specification_defines() {
        // <https://hl7.org/fhir/R4B/valueset-concept-property-type.html>;
        // `Coding` is the one code of the set that is not lower case.
        assert_eq!(PropertyKind::Code.code(), "code");
        assert_eq!(PropertyKind::Coding.code(), "Coding");
        assert_eq!(PropertyKind::String.code(), "string");
        assert_eq!(PropertyKind::Integer.code(), "integer");
        assert_eq!(PropertyKind::Boolean.code(), "boolean");
        assert_eq!(PropertyKind::DateTime.code(), "dateTime");
        assert_eq!(PropertyKind::Decimal.code(), "decimal");
    }
}
