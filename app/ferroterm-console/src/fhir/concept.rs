// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reads the concept browser makes, and the answers it draws.
//!
//! One concept is read with `CodeSystem/$lookup`
//! (<https://hl7.org/fhir/R4B/codesystem-operation-lookup.html>). Concepts are
//! selected with `ValueSet/$expand` over a value set the browser sends inline,
//! which the operation declares as the `valueSet` input: "the value set is
//! provided directly as part of the request"
//! (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>). Sending the
//! selection inline is what lets the screen browse any served system, because
//! an implicit value set canonical is a form each code system defines for
//! itself and naming one here would make the screen know a system.
//!
//! Reading lives here, outside every component, so the screen renders values
//! plain unit tests can pin.

use serde::{Deserialize, Serialize};

use ferrobridge_term::concept::{Designation, Property, PropertyValue};
use ferrobridge_term::outcome::{BatchOutcome, LookupOutcome};
use ferrobridge_term::request::{
    Expansion, Filter, Include, InlineValueSet, Request, ValueSetSource,
};

use crate::fhir::error::FhirError;
use crate::fhir::expansion::DesignationRow;
use crate::fhir::operation::{coding_of, mismatched, primitive_text};
use crate::fhir::terminology::SystemCard;
use crate::fhir::terminology::VersionRow;
use crate::fhir::version::FhirVersion;

/// The concept property naming a direct parent of a concept.
///
/// `parent` and `child` are the standard concept properties any code system
/// may declare (<http://hl7.org/fhir/concept-properties>), so reading them is
/// reading the specification rather than knowing a system.
pub(crate) const PARENT_PROPERTY: &str = "parent";

/// The concept property naming a direct child of a concept.
pub(crate) const CHILD_PROPERTY: &str = "child";

/// The filter operator that selects the direct children of one code.
///
/// "child-of: Includes all concept ids that have a direct parent that matches"
/// (<https://hl7.org/fhir/R5/codesystem-filter-operator.html>). It is the one
/// operator that answers a single level of a hierarchy, which is what a tree
/// walks.
pub(crate) const CHILD_OF_OPERATOR: &str = "child-of";

/// The filter operators whose meaning is the code system's own hierarchy.
///
/// The other operators in `filter-operator` select on the value of a property.
/// These six are defined against a parent-and-child relationship, so a version
/// declaring one of them declares that it has a hierarchy.
const HIERARCHY_OPERATORS: [&str; 6] = [
    CHILD_OF_OPERATOR,
    "descendent-leaf",
    "descendent-of",
    "generalizes",
    "is-a",
    "is-not-a",
];

/// What one served version declares about walking its hierarchy.
///
/// The browser draws a tree for a version that declares an operator selecting
/// direct children, and states what it found for one that does not. Nothing
/// here asks which code system it is looking at.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum Hierarchy {
    /// The version declares no hierarchy filter operator, so its concepts are
    /// a flat list.
    #[default]
    Absent,
    /// It declares hierarchy operators, but none that selects direct children.
    WithoutChildren {
        /// The hierarchy operators it does declare, in order.
        operators: Vec<String>,
    },
    /// It declares an operator that selects the direct children of a code.
    Walkable {
        /// The filter property the operator is declared on.
        property: String,
    },
}

impl Hierarchy {
    /// What `row` declares about its hierarchy.
    ///
    /// The first filter declaring the direct-child operator carries the walk,
    /// because the property it is declared on is the property the request has
    /// to name.
    pub(crate) fn of(row: &VersionRow) -> Self {
        let mut operators: Vec<String> = Vec::new();
        let mut walk: Option<&str> = None;
        for filter in &row.filters {
            for operator in &filter.operators {
                if !HIERARCHY_OPERATORS.contains(&operator.as_str()) {
                    continue;
                }
                if !operators.contains(operator) {
                    operators.push(operator.clone());
                }
                if operator == CHILD_OF_OPERATOR && walk.is_none() && !filter.code.is_empty() {
                    walk = Some(filter.code.as_str());
                }
            }
        }
        match walk {
            Some(property) => Self::Walkable {
                property: property.to_owned(),
            },
            None if operators.is_empty() => Self::Absent,
            None => {
                operators.sort();
                Self::WithoutChildren { operators }
            }
        }
    }

    /// The filter property a child walk names, when the version declares one.
    pub(crate) fn walk(&self) -> Option<&str> {
        match self {
            Self::Walkable { property } => Some(property.as_str()),
            Self::Absent | Self::WithoutChildren { .. } => None,
        }
    }
}

/// The version of `card` the browser reads.
///
/// An address naming a version reads that version and no other, so a link a
/// reader shared shows what they were looking at. An address naming none takes
/// the version an unversioned request resolves to, and falls back to the first
/// declared version where the server marks no default.
pub(crate) fn chosen_version(card: &SystemCard, wanted: Option<&str>) -> Option<VersionRow> {
    match wanted.map(str::trim).filter(|wanted| !wanted.is_empty()) {
        Some(wanted) => card
            .versions
            .iter()
            .find(|row| row.code.as_deref() == Some(wanted))
            .cloned(),
        None => card
            .versions
            .iter()
            .find(|row| row.is_default)
            .or_else(|| card.versions.first())
            .cloned(),
    }
}

/// The parameters one `CodeSystem/$lookup` sends.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct LookupRequest {
    /// The canonical of the code system the code belongs to.
    pub(crate) system: String,
    /// The code system version, when the address names one.
    pub(crate) system_version: Option<String>,
    /// The code to read.
    pub(crate) code: String,
    /// The BCP 47 tag displays are wanted in.
    pub(crate) display_language: Option<String>,
}

impl LookupRequest {
    /// The operation this read asks for.
    ///
    /// No `property` is sent, so the server answers the properties it holds
    /// rather than the ones this screen thought to ask for.
    pub(crate) fn request(&self) -> Request {
        Request::Lookup {
            system: self.system.clone(),
            code: self.code.clone(),
            version: self.system_version.clone(),
            date: None,
            display_language: self.display_language.clone(),
        }
    }
}

/// The selection one inline `ValueSet/$expand` asks for.
///
/// The same shape carries both selections the browser makes: a text search
/// over the whole system, and one level of children below a code.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ConceptQuery {
    /// The canonical of the code system to select from.
    pub(crate) system: String,
    /// The code system version, when the address names one.
    pub(crate) system_version: Option<String>,
    /// The text filter, when the reader typed one.
    pub(crate) filter: Option<String>,
    /// The direct-child filter, when the selection is one level of a tree.
    pub(crate) child_of: Option<ChildOf>,
    /// The BCP 47 tag displays are wanted in.
    pub(crate) display_language: Option<String>,
    /// How many concepts one answer may hold.
    pub(crate) count: u32,
}

/// The direct children of one code, as a `compose.include.filter`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ChildOf {
    /// The filter property the version declares the operator on.
    pub(crate) property: String,
    /// The code whose children are wanted.
    pub(crate) code: String,
}

impl ConceptQuery {
    /// The operation this selection asks for: an expansion of the value set
    /// it describes, sent inline as one include over one system.
    ///
    /// The value set travels as the `valueSet` input, a resource, so the
    /// request is a `POST` (<https://hl7.org/fhir/R4B/operations.html#request>)
    /// and no code system's own implicit form is named. `excludeNested` asks
    /// for a flat answer
    /// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>): the
    /// browser asks for one level at a time and reads the answer as a list,
    /// so a nested answer would hide a concept behind a parent the reader
    /// never opened.
    pub(crate) fn request(&self) -> Request {
        let include = Include {
            system: self.system.clone(),
            version: self.system_version.clone(),
            filters: self
                .child_of
                .iter()
                .map(|child| Filter {
                    property: child.property.clone(),
                    op: CHILD_OF_OPERATOR.to_owned(),
                    value: child.code.clone(),
                })
                .collect(),
        };
        Request::Expand(Expansion {
            filter: self.filter.clone(),
            count: Some(self.count),
            exclude_nested: Some(true),
            display_language: self.display_language.clone(),
            ..Expansion::of(ValueSetSource::Inline(InlineValueSet {
                include: vec![include],
            }))
        })
    }
}

/// One concept, as the browser draws it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Concept {
    /// The code system's name, as the server answered it.
    pub(crate) name: Option<String>,
    /// The code system version the answer came from.
    pub(crate) version: Option<String>,
    /// The display the server chose, in the language that was asked for.
    pub(crate) display: Option<String>,
    /// Every designation the server holds for the concept.
    pub(crate) designations: Vec<DesignationRow>,
    /// Every property the server answers for the concept, in its own order.
    pub(crate) properties: Vec<PropertyRow>,
}

/// One answered property of a concept.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PropertyRow {
    /// The property code, empty when the server named none.
    pub(crate) code: String,
    /// The value, whichever `value[x]` carried it.
    pub(crate) value: Option<String>,
    /// What the property means, when the server said.
    pub(crate) description: Option<String>,
}

impl Concept {
    /// The concept `outcome` carries, the reading of the read sent to `url`
    /// at `version`.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError::Decode`] for an answer that is not a `$lookup`
    /// answer.
    pub(crate) fn read(
        outcome: BatchOutcome,
        url: &str,
        version: FhirVersion,
    ) -> Result<Self, FhirError> {
        match outcome {
            BatchOutcome::Lookup(LookupOutcome::Found {
                name,
                version: system_version,
                display,
                designations,
                properties,
                ..
            }) => Ok(Self {
                name: Some(name),
                version: system_version,
                display: Some(display),
                designations: designations.into_iter().map(designation).collect(),
                properties: properties.into_iter().map(property).collect(),
            }),
            _ => Err(mismatched(url, version)),
        }
    }

    /// Every value the server answered for the property `code`.
    pub(crate) fn property_values(&self, code: &str) -> Vec<String> {
        self.properties
            .iter()
            .filter(|property| property.code == code)
            .filter_map(|property| property.value.clone())
            .collect()
    }
}

/// A property value as one line: a coding reads as its display, or as the
/// bare code where it has none.
fn value(value: &PropertyValue) -> Option<String> {
    primitive_text(value)
        .or_else(|| coding_of(value).and_then(|coding| coding.display.or(coding.code)))
}

/// A designation as a line; a use reads as its display, or as its code where
/// it has none.
pub(crate) fn designation(designation: Designation) -> DesignationRow {
    DesignationRow {
        language: designation.language,
        usage: designation
            .usage
            .and_then(|usage| usage.display.or(usage.code)),
        value: designation.value,
    }
}

/// A property as a line.
fn property(property: Property) -> PropertyRow {
    PropertyRow {
        code: property.code,
        value: property.value.as_ref().and_then(value),
        description: property.description,
    }
}

#[cfg(test)]
mod tests;
