// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a provider declares about its code system.
//!
//! The `CodeSystem` metadata of the capability statement
//! (<https://hl7.org/fhir/R4B/codesystem.html>): its properties, filters,
//! capabilities, grammar, and implicit value set forms.

use std::collections::BTreeSet;

use super::filter::FilterOperator;

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
    /// [`super::CodeSystemProvider::implicit_value_set`] resolves, one per URL
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

#[cfg(test)]
mod tests {
    use super::{ContentMode, HierarchyMeaning, PropertyKind};

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
