// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The version-neutral picture of a `CodeSystem` resource.

use std::collections::BTreeSet;

use crate::provider::{
    ContentMode, Designation, DesignationUse, FilterDefinition, HierarchyMeaning, Property,
    PropertyDefinition, PropertyKind,
};

/// One concept, flattened out of the resource's nesting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptEntry {
    /// The code.
    pub code: String,
    /// `concept.display`.
    pub display: Option<String>,
    /// `concept.definition`.
    pub definition: Option<String>,
    /// `concept.designation`.
    pub designations: Vec<Designation>,
    /// The concept's `structuredefinition-standards-status` code, when set.
    pub standards_status: Option<String>,
    /// `concept.property`, as declared.
    pub properties: Vec<Property>,
    /// The codes of the parents: the enclosing concept, plus every `parent`
    /// and `subsumedBy` property value.
    pub parents: Vec<String>,
}

/// A `CodeSystem` resource reduced to what the provider serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSystemModel {
    /// `CodeSystem.id`: the logical id the resource was authored with, when it
    /// carries one (<https://hl7.org/fhir/R4B/resource.html#id>). The server
    /// reads the resource at it.
    pub id: Option<String>,
    /// `CodeSystem.url`.
    pub url: String,
    /// `CodeSystem.version`, or an empty string when the resource has none.
    pub version: String,
    /// `CodeSystem.name`.
    pub name: Option<String>,
    /// `CodeSystem.title`, else `name`.
    pub title: Option<String>,
    /// `CodeSystem.language`: the language of the displays.
    pub language: Option<String>,
    /// `CodeSystem.status`.
    pub status: String,
    /// `CodeSystem.experimental`, when stated.
    pub experimental: Option<bool>,
    /// `CodeSystem.date`: when the resource last changed.
    pub date: Option<String>,
    /// `CodeSystem.publisher`.
    pub publisher: Option<String>,
    /// `CodeSystem.description`, as the Markdown it was written in.
    pub description: Option<String>,
    /// The `structuredefinition-standards-status` extension's code, when set.
    pub standards_status: Option<String>,
    /// `CodeSystem.content`.
    pub content: ContentMode,
    /// `CodeSystem.caseSensitive` (absent means case-sensitive comparison).
    pub case_sensitive: bool,
    /// `CodeSystem.hierarchyMeaning`.
    pub hierarchy_meaning: Option<HierarchyMeaning>,
    /// `CodeSystem.compositional`.
    pub compositional: bool,
    /// `CodeSystem.versionNeeded`.
    pub version_needed: bool,
    /// `CodeSystem.supplements`: the system this resource adds to, when it is one.
    pub supplements: Option<String>,
    /// `CodeSystem.property`.
    pub properties: Vec<PropertyDefinition>,
    /// `CodeSystem.filter`.
    pub filters: Vec<FilterDefinition>,
    /// Every concept, in document order.
    pub concepts: Vec<ConceptEntry>,
}

/// A failure to build a model.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    /// The resource has no `url`.
    #[error("the CodeSystem has no url")]
    NoUrl,
    /// A concept carries no `code` (`CodeSystem.concept.code` is required).
    #[error("a concept{} has no code", under.as_deref().map(|c| format!(" under `{c}`")).unwrap_or_default())]
    ConceptCode {
        /// The enclosing concept, when the concept is nested.
        under: Option<String>,
    },
    /// A concept property carries no `code` (`CodeSystem.concept.property.code` is required).
    #[error("concept `{concept}` has a property without a code")]
    PropertyCode {
        /// The concept.
        concept: String,
    },
    /// `content` is not one of the code system content modes.
    #[error("`{0}` is not a codesystem-content-mode")]
    Content(String),
    /// `hierarchyMeaning` is not one of the hierarchy meanings.
    #[error("`{0}` is not a codesystem-hierarchy-meaning")]
    HierarchyMeaning(String),
    /// A property type is not one of the concept property types.
    #[error("property `{code}` has type `{kind}`, not a concept-property-type")]
    PropertyType {
        /// The property.
        code: String,
        /// The declared type.
        kind: String,
    },
    /// A filter names an operator outside `filter-operator`.
    #[error("filter `{code}` names operator `{operator}`, not a filter-operator")]
    FilterOperator {
        /// The filter.
        code: String,
        /// The operator.
        operator: String,
    },
    /// Two concepts share a code.
    #[error("code `{0}` is defined twice")]
    DuplicateCode(String),
    /// A concept's `parent` or `subsumedBy` names a code the resource lacks.
    #[error("concept `{code}` names parent `{parent}`, which the CodeSystem does not define")]
    UnknownParent {
        /// The concept.
        code: String,
        /// The parent named.
        parent: String,
    },
}

impl ContentMode {
    /// The mode for a `codesystem-content-mode` code.
    #[must_use]
    pub fn parse(code: &str) -> Option<Self> {
        match code {
            "not-present" => Some(Self::NotPresent),
            "example" => Some(Self::Example),
            "fragment" => Some(Self::Fragment),
            "complete" => Some(Self::Complete),
            "supplement" => Some(Self::Supplement),
            _ => None,
        }
    }
}

impl HierarchyMeaning {
    /// The meaning for a `codesystem-hierarchy-meaning` code.
    #[must_use]
    pub fn parse(code: &str) -> Option<Self> {
        match code {
            "grouped-by" => Some(Self::GroupedBy),
            "is-a" => Some(Self::IsA),
            "part-of" => Some(Self::PartOf),
            "classified-with" => Some(Self::ClassifiedWith),
            _ => None,
        }
    }
}

impl PropertyKind {
    /// The kind for a `concept-property-type` code.
    #[must_use]
    pub fn parse(code: &str) -> Option<Self> {
        match code {
            "code" => Some(Self::Code),
            "Coding" => Some(Self::Coding),
            "string" => Some(Self::String),
            "integer" => Some(Self::Integer),
            "boolean" => Some(Self::Boolean),
            "dateTime" => Some(Self::DateTime),
            "decimal" => Some(Self::Decimal),
            _ => None,
        }
    }
}

impl CodeSystemModel {
    /// The `CodeSystem` a provider that holds no resource of its own serves:
    /// its identity, its declaration, and its standing, with no `concept`.
    ///
    /// A system the server holds behind an index declares
    /// `content = not-present`, which is exactly "none of the concepts are
    /// included in the resource"
    /// (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>); the concepts
    /// are reached through the operations.
    #[must_use]
    pub fn of_provider(provider: &dyn crate::provider::CodeSystemProvider) -> Self {
        let identity = provider.identity();
        let declaration = provider.declaration();
        let standing = provider.standing();
        Self {
            // NOTE: a logical id belongs to a resource
            // (<https://hl7.org/fhir/R4B/resource.html#id>), and this picture is
            // built from a provider, so the server names the instance itself.
            id: None,
            url: identity.url.clone(),
            version: identity.version.clone(),
            name: identity.name.clone(),
            title: identity.title.clone(),
            language: provider.language().map(str::to_owned),
            status: standing.status,
            experimental: Some(standing.experimental),
            date: None,
            publisher: None,
            description: None,
            standards_status: standing.standards_status,
            content: declaration.content,
            case_sensitive: declaration.case_sensitive,
            hierarchy_meaning: declaration.hierarchy_meaning,
            // `CodeSystem.compositional` is whether the code system defines a
            // grammar, which is `defined`, not whether this server evaluates it.
            compositional: declaration.compositional.defined(),
            version_needed: identity.version_needed,
            supplements: None,
            properties: declaration.properties.clone(),
            filters: declaration.filters.clone(),
            concepts: Vec::new(),
        }
    }

    /// Checks the model: distinct codes, known parents.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::DuplicateCode`] or [`ModelError::UnknownParent`].
    pub fn validate(&self) -> Result<(), ModelError> {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for concept in &self.concepts {
            if !seen.insert(concept.code.as_str()) {
                return Err(ModelError::DuplicateCode(concept.code.clone()));
            }
        }
        for concept in &self.concepts {
            for parent in &concept.parents {
                if !seen.contains(parent.as_str()) {
                    return Err(ModelError::UnknownParent {
                        code: concept.code.clone(),
                        parent: parent.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

/// The standard designation use of a `concept.designation.use` coding.
pub(crate) fn designation_use(
    system: Option<&str>,
    code: Option<&str>,
    display: Option<&str>,
) -> Option<DesignationUse> {
    Some(DesignationUse {
        system: system?.to_owned(),
        code: code?.to_owned(),
        display: display.map(str::to_owned),
    })
}

/// The standard concept property names the provider derives status from
/// (<https://hl7.org/fhir/R4B/codesystem-concept-properties.html>).
pub(crate) const INACTIVE: &str = "inactive";
pub(crate) const STATUS: &str = "status";
pub(crate) const RETIREMENT_DATE: &str = "retirementDate";
/// The R4 `deprecated` date and the R5 `deprecationDate`, and the `status`
/// value `deprecated`: a concept "deprecated but not inactive can still be
/// used, but their use is discouraged"
/// (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>).
pub(crate) const DEPRECATED: &str = "deprecated";
pub(crate) const DEPRECATION_DATE: &str = "deprecationDate";
pub(crate) const NOT_SELECTABLE: &str = "notSelectable";
pub(crate) const PARENT: &str = "parent";
pub(crate) const CHILD: &str = "child";
/// HL7 Terminology's v3 systems express the hierarchy as `subsumedBy`.
pub(crate) const SUBSUMED_BY: &str = "subsumedBy";
/// The `status` value that retires a concept, the one of the four typical
/// values that ends a concept's life
/// (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>).
pub(crate) const RETIRED: &str = "retired";

/// The `CodeSystem` resource a provider serves: the one it was built from when
/// it holds one, else the metadata picture of what it declares.
#[must_use]
pub fn described(
    provider: &dyn crate::provider::CodeSystemProvider,
) -> std::borrow::Cow<'_, CodeSystemModel> {
    provider.code_system().map_or_else(
        || std::borrow::Cow::Owned(CodeSystemModel::of_provider(provider)),
        std::borrow::Cow::Borrowed,
    )
}
