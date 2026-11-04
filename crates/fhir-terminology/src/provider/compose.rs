// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ValueSet.compose` as the seam hands it across.
//!
//! An include selects and an exclude takes away
//! (<https://hl7.org/fhir/R4B/valueset.html#compositions>); the compose layer
//! evaluates it above every provider.

use super::filter::Filter;

/// A system and optional version an include names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemRef {
    /// The system URI.
    pub url: String,
    /// The version, or the registry default.
    pub version: Option<String>,
}

/// One enumerated concept of an include.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConceptRef {
    /// The code.
    pub code: String,
    /// A display to use instead of the system's.
    pub display: Option<String>,
    /// Whether the value set marks the concept deprecated
    /// (`valueset-deprecated`).
    pub deprecated: bool,
    /// The extensions the value set states on the concept that `$expand`
    /// echoes on its entry.
    pub echoes: Echoes,
}

/// The extensions on a concept that `$expand` echoes on its entry.
///
/// "Echo in value set if defined in code system or value set"
/// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>, Extensions).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Echoes {
    /// `valueset-label` (or `codesystem-label`): the label shown before the
    /// display.
    pub label: Option<String>,
    /// `valueset-conceptOrder` (or `codesystem-conceptOrder`).
    pub order: Option<i32>,
    /// `rendering-style` on the concept.
    pub style: Option<String>,
    /// `rendering-xhtml` on the concept's display.
    pub xhtml: Option<String>,
    /// `valueset-concept-definition`: the value set's own definition of the
    /// concept, echoed when the request asks for definitions.
    pub definition: Option<String>,
}

/// One extension as [`Echoes::read`] takes it: its URL and its value when that
/// is a string or an integer.
#[derive(Debug, Clone, Copy)]
pub struct Stated<'a> {
    /// `Extension.url`.
    pub url: &'a str,
    /// `valueString`.
    pub text: Option<&'a str>,
    /// `valueInteger`.
    pub integer: Option<i32>,
}

/// A generated `Extension` of the FHIR version `$module` as a [`Stated`].
macro_rules! stated {
    ($module:ident, $extension:expr) => {{
        let extension: &fhir_types::$module::extension::Extension = $extension;
        $crate::provider::compose::Stated {
            url: extension.url.as_str(),
            text: match &extension.value {
                Some(fhir_types::$module::extension::ExtensionValue::String(s)) => {
                    s.value.as_deref()
                }
                _ => None,
            },
            integer: match &extension.value {
                Some(fhir_types::$module::extension::ExtensionValue::Integer(i)) => i.value,
                _ => None,
            },
        }
    }};
}
pub(crate) use stated;

/// The resource a concept is stated in, which names the label and order
/// extensions it uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// `CodeSystem.concept`.
    CodeSystem,
    /// `ValueSet.compose.include.concept`.
    ValueSet,
}

const RENDERING_STYLE: &str = "http://hl7.org/fhir/StructureDefinition/rendering-style";
const RENDERING_XHTML: &str = "http://hl7.org/fhir/StructureDefinition/rendering-xhtml";
const CONCEPT_DEFINITION: &str =
    "http://hl7.org/fhir/StructureDefinition/valueset-concept-definition";

impl Echoes {
    /// The echoes stated on a concept (`on_concept`) and on its display
    /// (`on_display`). Each is read only where its context admits it (FHIR
    /// Extensions Pack 5.4.0): `rendering-xhtml` on a `string`, the label and the
    /// order on the concept of their own resource.
    pub fn read<'a>(
        origin: Origin,
        on_concept: impl IntoIterator<Item = Stated<'a>>,
        on_display: impl IntoIterator<Item = Stated<'a>>,
    ) -> Self {
        let (label, order) = match origin {
            Origin::CodeSystem => (
                "http://hl7.org/fhir/StructureDefinition/codesystem-label",
                "http://hl7.org/fhir/StructureDefinition/codesystem-conceptOrder",
            ),
            Origin::ValueSet => (
                "http://hl7.org/fhir/StructureDefinition/valueset-label",
                "http://hl7.org/fhir/StructureDefinition/valueset-conceptOrder",
            ),
        };
        let mut echoes = Self::default();
        for stated in on_concept {
            let text = stated.text.map(str::to_owned);
            match stated.url {
                url if url == label => echoes.label = echoes.label.or(text),
                url if url == order => echoes.order = echoes.order.or(stated.integer),
                RENDERING_STYLE => echoes.style = echoes.style.or(text),
                CONCEPT_DEFINITION if origin == Origin::ValueSet => {
                    echoes.definition = echoes.definition.or(text);
                }
                _ => {}
            }
        }
        for stated in on_display {
            if stated.url == RENDERING_XHTML {
                echoes.xhtml = echoes.xhtml.or(stated.text.map(str::to_owned));
            }
        }
        echoes
    }

    /// Whether nothing is stated.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// These echoes with each one unstated taken from `fallback`: a value set's
    /// own statement wins over its code system's.
    #[must_use]
    pub fn or(self, fallback: Self) -> Self {
        Self {
            label: self.label.or(fallback.label),
            order: self.order.or(fallback.order),
            style: self.style.or(fallback.style),
            xhtml: self.xhtml.or(fallback.xhtml),
            definition: self.definition.or(fallback.definition),
        }
    }
}

/// `ValueSet.compose.include` (and `exclude`, which has the same shape).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Include {
    /// The system, when codes are selected from one.
    pub system: Option<SystemRef>,
    /// Enumerated concepts; empty means every code the filters admit.
    pub concepts: Vec<ConceptRef>,
    /// Filters, all of which apply.
    pub filters: Vec<Filter>,
    /// Value sets whose expansions the selection must be in, all of them.
    pub value_sets: Vec<String>,
}

/// `ValueSet.compose`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Compose {
    /// The includes; their union.
    pub include: Vec<Include>,
    /// The excludes; subtracted.
    pub exclude: Vec<Include>,
    /// `compose.inactive`: `Some(false)` keeps inactive codes out of every expansion.
    pub inactive: Option<bool>,
    /// `compose.property` (R5): the properties an expansion returns when the
    /// client asks for none, `*` meaning every one the server knows
    /// (<https://hl7.org/fhir/R5/valueset-definitions.html#ValueSet.compose.property>).
    pub property: Vec<String>,
}

impl Compose {
    /// Where the first filter on `property` whose value is `value` sits,
    /// searching the includes before the excludes.
    #[must_use]
    pub fn filter_at(&self, property: &str, value: &str) -> Option<FilterAt> {
        let find = |side: Side, rules: &[Include]| {
            rules.iter().enumerate().find_map(|(rule, include)| {
                include
                    .filters
                    .iter()
                    .position(|filter| filter.property == property && filter.value == value)
                    .map(|filter| FilterAt { side, rule, filter })
            })
        };
        find(Side::Include, &self.include).or_else(|| find(Side::Exclude, &self.exclude))
    }
}

/// Which list of `ValueSet.compose` a rule sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// `compose.include`.
    Include,
    /// `compose.exclude`.
    Exclude,
}

/// Where a filter sits in a compose: `compose.include[rule].filter[filter]`,
/// or the same under `compose.exclude`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterAt {
    /// The list the rule is in.
    pub side: Side,
    /// The index of the rule in that list.
    pub rule: usize,
    /// The index of the filter in the rule.
    pub filter: usize,
}
