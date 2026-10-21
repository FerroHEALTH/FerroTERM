// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The version-neutral `ConceptMap` (<https://hl7.org/fhir/R4B/conceptmap.html>,
//! <https://hl7.org/fhir/R5/conceptmap.html>).

use crate::versioned::Versioned;

/// How a target relates to its source concept.
///
/// The variants are R4's `equivalence` codes; R5's `relationship` codes map
/// onto them (`source-is-narrower-than-target` is R4's `wider`,
/// `source-is-broader-than-target` is `narrower`, `not-related-to` is
/// `disjoint`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relationship {
    /// `relatedto` / `related-to`.
    RelatedTo,
    /// `equivalent`.
    Equivalent,
    /// `equal`.
    Equal,
    /// `wider` / `source-is-narrower-than-target`.
    Wider,
    /// `subsumes`.
    Subsumes,
    /// `narrower` / `source-is-broader-than-target`.
    Narrower,
    /// `specializes`.
    Specializes,
    /// `inexact`.
    Inexact,
    /// `unmatched`.
    Unmatched,
    /// `disjoint` / `not-related-to`.
    Disjoint,
}

impl Relationship {
    /// From an R4 `equivalence` code
    /// (<https://hl7.org/fhir/R4B/valueset-concept-map-equivalence.html>).
    #[must_use]
    pub fn from_equivalence(code: &str) -> Option<Self> {
        Some(match code {
            "relatedto" => Self::RelatedTo,
            "equivalent" => Self::Equivalent,
            "equal" => Self::Equal,
            "wider" => Self::Wider,
            "subsumes" => Self::Subsumes,
            "narrower" => Self::Narrower,
            "specializes" => Self::Specializes,
            "inexact" => Self::Inexact,
            "unmatched" => Self::Unmatched,
            "disjoint" => Self::Disjoint,
            _ => return None,
        })
    }

    /// From an R5 `relationship` code
    /// (<https://hl7.org/fhir/R5/valueset-concept-map-relationship.html>).
    #[must_use]
    pub fn from_relationship(code: &str) -> Option<Self> {
        Some(match code {
            "related-to" => Self::RelatedTo,
            "equivalent" => Self::Equivalent,
            "source-is-narrower-than-target" => Self::Wider,
            "source-is-broader-than-target" => Self::Narrower,
            "not-related-to" => Self::Disjoint,
            _ => return None,
        })
    }

    /// The R4 `equivalence` code.
    #[must_use]
    pub const fn equivalence(self) -> &'static str {
        match self {
            Self::RelatedTo => "relatedto",
            Self::Equivalent => "equivalent",
            Self::Equal => "equal",
            Self::Wider => "wider",
            Self::Subsumes => "subsumes",
            Self::Narrower => "narrower",
            Self::Specializes => "specializes",
            Self::Inexact => "inexact",
            Self::Unmatched => "unmatched",
            Self::Disjoint => "disjoint",
        }
    }

    /// The R5 `relationship` code.
    #[must_use]
    pub const fn relationship(self) -> &'static str {
        match self {
            Self::RelatedTo | Self::Inexact => "related-to",
            Self::Equivalent | Self::Equal => "equivalent",
            Self::Wider | Self::Subsumes => "source-is-narrower-than-target",
            Self::Narrower | Self::Specializes => "source-is-broader-than-target",
            Self::Unmatched | Self::Disjoint => "not-related-to",
        }
    }

    /// Whether a match with this relationship translates the source
    /// (`unmatched` and `disjoint` do not).
    #[must_use]
    pub const fn translates(self) -> bool {
        !matches!(self, Self::Unmatched | Self::Disjoint)
    }

    /// The relationship read the other way round, for `reverse`.
    #[must_use]
    pub const fn inverse(self) -> Self {
        match self {
            Self::Wider => Self::Narrower,
            Self::Narrower => Self::Wider,
            Self::Subsumes => Self::Specializes,
            Self::Specializes => Self::Subsumes,
            other => other,
        }
    }
}

/// `group.element.target.dependsOn` and `.product`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependsOn {
    /// `property` (R4) or `attribute` (R5).
    pub attribute: String,
    /// `system`, when the value is a code of one.
    pub system: Option<String>,
    /// `value` as text.
    pub value: String,
    /// `display`.
    pub display: Option<String>,
}

/// One `group.element.target`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// `code`.
    pub code: Option<String>,
    /// `display`.
    pub display: Option<String>,
    /// `equivalence` / `relationship`.
    pub relationship: Relationship,
    /// `comment`.
    pub comment: Option<String>,
    /// `dependsOn`.
    pub depends_on: Vec<DependsOn>,
    /// `product`.
    pub product: Vec<DependsOn>,
}

/// One `group.element`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    /// `code`.
    pub code: Option<String>,
    /// `display`.
    pub display: Option<String>,
    /// `noMap` (R5), or an R4 element without targets.
    pub no_map: bool,
    /// A comment on the element (R6's `comment`, carried as an extension in R5).
    pub comment: Option<String>,
    /// `target`.
    pub targets: Vec<Target>,
}

/// `group.unmapped.mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnmappedMode {
    /// `use-source-code` (R5) / `provided` (R4): the source code itself.
    Provided,
    /// `fixed`: one code for every unmapped source.
    Fixed,
    /// `other-map`: consult another map.
    OtherMap,
}

impl UnmappedMode {
    /// From an R4 or R5 mode code.
    #[must_use]
    pub fn parse(code: &str) -> Option<Self> {
        Some(match code {
            "provided" | "use-source-code" => Self::Provided,
            "fixed" => Self::Fixed,
            "other-map" => Self::OtherMap,
            _ => return None,
        })
    }
}

/// `group.unmapped`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unmapped {
    /// `mode`.
    pub mode: UnmappedMode,
    /// `code`, for `fixed`.
    pub code: Option<String>,
    /// `display`, for `fixed`.
    pub display: Option<String>,
    /// `relationship` (R5), for `fixed` and `provided`.
    pub relationship: Option<Relationship>,
    /// `url` (R4) / `otherMap` (R5), for `other-map`.
    pub other_map: Option<String>,
}

/// One `group`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// `source`, the source code system.
    pub source: Option<String>,
    /// `sourceVersion`, or the version of an R5 `source` canonical.
    pub source_version: Option<String>,
    /// `target`, the target code system.
    pub target: Option<String>,
    /// `targetVersion`, or the version of an R5 `target` canonical.
    pub target_version: Option<String>,
    /// `element`.
    pub elements: Vec<Element>,
    /// `unmapped`.
    pub unmapped: Option<Unmapped>,
}

/// What `$translate` needs of a `ConceptMap`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptMapModel {
    /// `id`: the logical id the resource was authored with, when it carries
    /// one (<https://hl7.org/fhir/R4B/resource.html#id>). The server reads the
    /// resource at it.
    pub id: Option<String>,
    /// `url`.
    pub url: String,
    /// `version`.
    pub version: Option<String>,
    /// `name`.
    pub name: Option<String>,
    /// `title`.
    pub title: Option<String>,
    /// `status`.
    pub status: String,
    /// `experimental`, when stated.
    pub experimental: Option<bool>,
    /// `date`: when the resource last changed.
    pub date: Option<String>,
    /// `publisher`.
    pub publisher: Option<String>,
    /// `description`, as the Markdown it was written in.
    pub description: Option<String>,
    /// `source[x]` (R4) / `sourceScope[x]` (R5): the source value set.
    pub source_scope: Option<String>,
    /// `target[x]` (R4) / `targetScope[x]` (R5): the target value set.
    pub target_scope: Option<String>,
    /// `group`.
    pub groups: Vec<Group>,
}

impl Versioned for ConceptMapModel {
    fn url(&self) -> &str {
        &self.url
    }

    fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
}

/// A `ConceptMap` the model cannot represent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    /// The resource has no `url`.
    #[error("the ConceptMap has no url")]
    NoUrl,
    /// A target names a relationship the specification does not define.
    #[error("target `{target}` of `{element}` has unknown relationship `{code}`")]
    Relationship {
        /// The element code.
        element: String,
        /// The target code.
        target: String,
        /// The code given.
        code: String,
    },
    /// `unmapped.mode` is not defined.
    #[error("unknown unmapped mode `{0}`")]
    UnmappedMode(String),
}
