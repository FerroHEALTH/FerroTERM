// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: Apache-2.0

//! The RF2 component rows: concepts, descriptions, relationships,
//! concrete-value relationships, and alternate identifiers.
//!
//! Each row type names its columns and parses one record; the reader
//! attributes a failure to the file, line, and column.

use std::io::Read;

use crate::constants;
use crate::id::{ConceptId, DescriptionId, ModuleId, RelationshipId, Sctid};
use crate::reader::{FieldError, Record, Rf2Error, Rf2Reader};
use crate::time::EffectiveTime;

/// A row type of a component file.
pub trait Component: Sized {
    /// The header the file must carry.
    const COLUMNS: &[&str];

    /// Parses one record.
    ///
    /// # Errors
    ///
    /// Returns [`Rf2Error::Field`] for a malformed value.
    fn from_record(record: &Record<'_>) -> Result<Self, Rf2Error>;
}

/// A streaming iterator over the rows of one component file.
#[derive(Debug)]
pub struct Rows<R: Read, T: Component> {
    reader: Rf2Reader<R>,
    marker: std::marker::PhantomData<T>,
}

impl<R: Read, T: Component> Rows<R, T> {
    /// Wraps a reader whose header was validated against `T::COLUMNS`.
    ///
    /// # Errors
    ///
    /// Returns [`Rf2Error`] when the header is missing or differs.
    pub fn new(path: &std::path::Path, read: R) -> Result<Self, Rf2Error> {
        Ok(Self {
            reader: Rf2Reader::new(path, read, T::COLUMNS)?,
            marker: std::marker::PhantomData,
        })
    }
}

impl<T: Component> Rows<std::io::BufReader<std::fs::File>, T> {
    /// Opens `path` and validates its header against `T::COLUMNS`.
    ///
    /// # Errors
    ///
    /// Returns [`Rf2Error`] when the file cannot be read or its header differs.
    pub fn open(path: &std::path::Path) -> Result<Self, Rf2Error> {
        Ok(Self {
            reader: Rf2Reader::open(path, T::COLUMNS)?,
            marker: std::marker::PhantomData,
        })
    }
}

impl<R: Read, T: Component> Iterator for Rows<R, T> {
    type Item = Result<T, Rf2Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.reader.next_record() {
            Ok(Some(record)) => Some(T::from_record(&record)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

/// A row carrying the RF2 `characteristicTypeId` column.
///
/// The relationship and concrete value files both carry it, and the
/// enumeration behind it says which rows define their source concept
/// (release file specification appendix E.5,
/// <https://docs.snomed.org/snomed-ct-specifications/snomed-ct-release-file-specification/appendices/appendix-e-concept-enumerations/e5-concept-enumerations-for-characteristictypeid.md>).
pub trait CharacteristicTyped {
    /// The characteristic type of this row.
    fn characteristic_type_id(&self) -> ConceptId;
}

impl CharacteristicTyped for Relationship {
    fn characteristic_type_id(&self) -> ConceptId {
        self.characteristic_type_id
    }
}

impl CharacteristicTyped for ConcreteRelationship {
    fn characteristic_type_id(&self) -> ConceptId {
        self.characteristic_type_id
    }
}

impl<R: Read, T: Component + CharacteristicTyped> Rows<R, T> {
    /// Narrows the file to the rows that define their source concept.
    ///
    /// A row is admitted only when it carries
    /// `900000000000011006 |Inferred relationship|`, so a hierarchy or an
    /// attribute value built from a relationship file reads the rows through
    /// this adapter. Every other row is left out and counted by
    /// [`InferredRows::skipped`].
    #[must_use]
    pub fn inferred(self) -> InferredRows<R, T> {
        InferredRows {
            rows: self,
            skipped: 0,
        }
    }
}

/// The rows of a relationship file whose characteristic type is
/// `900000000000011006 |Inferred relationship|`, as [`Rows::inferred`] selects
/// them.
#[derive(Debug)]
pub struct InferredRows<R: Read, T: Component + CharacteristicTyped> {
    rows: Rows<R, T>,
    skipped: u64,
}

impl<R: Read, T: Component + CharacteristicTyped> InferredRows<R, T> {
    /// How many rows the adapter has left out so far.
    ///
    /// A caller that reads a whole file reports this, so a release whose
    /// relationships sit outside the inferred view is visible rather than a
    /// hierarchy that came out empty for no stated reason.
    #[must_use]
    pub fn skipped(&self) -> u64 {
        self.skipped
    }
}

impl<R: Read, T: Component + CharacteristicTyped> Iterator for InferredRows<R, T> {
    type Item = Result<T, Rf2Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // NOTE: of the appendix E.5 enumeration only |Inferred relationship|
        // defines the concept, and E.5 records that no released relationship
        // carries the |Defining relationship| supertype, so that goes too.
        loop {
            match self.rows.next()? {
                Ok(row) if row.characteristic_type_id() != constants::INFERRED => {
                    self.skipped = self.skipped.saturating_add(1);
                }
                row => return Some(row),
            }
        }
    }
}

/// The columns every component row starts with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentBase {
    /// When this version of the component took effect.
    pub effective_time: EffectiveTime,
    /// Whether the component is active in this version.
    pub active: bool,
    /// The module that owns the component.
    pub module_id: ModuleId,
}

fn base(record: &Record<'_>) -> Result<ComponentBase, Rf2Error> {
    Ok(ComponentBase {
        effective_time: record.parse(1, EffectiveTime::parse)?,
        active: record.boolean(2)?,
        module_id: record.parse(3, ModuleId::parse)?,
    })
}

/// A row of `sct2_Concept`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Concept {
    /// The concept.
    pub id: ConceptId,
    /// The shared columns.
    pub base: ComponentBase,
    /// `Primitive` or `Defined`.
    pub definition_status_id: ConceptId,
}

impl Component for Concept {
    const COLUMNS: &[&str] = &[
        "id",
        "effectiveTime",
        "active",
        "moduleId",
        "definitionStatusId",
    ];

    fn from_record(record: &Record<'_>) -> Result<Self, Rf2Error> {
        Ok(Self {
            id: record.parse(0, ConceptId::parse)?,
            base: base(record)?,
            definition_status_id: record.parse(4, ConceptId::parse)?,
        })
    }
}

/// A row of `sct2_Description` or `sct2_TextDefinition` (the layouts coincide).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Description {
    /// The description.
    pub id: DescriptionId,
    /// The shared columns.
    pub base: ComponentBase,
    /// The concept described.
    pub concept_id: ConceptId,
    /// The ISO 639-1 language code.
    pub language_code: String,
    /// Fully specified name, synonym, or definition.
    pub type_id: ConceptId,
    /// The text.
    pub term: String,
    /// The case significance.
    pub case_significance_id: ConceptId,
}

impl Component for Description {
    const COLUMNS: &[&str] = &[
        "id",
        "effectiveTime",
        "active",
        "moduleId",
        "conceptId",
        "languageCode",
        "typeId",
        "term",
        "caseSignificanceId",
    ];

    fn from_record(record: &Record<'_>) -> Result<Self, Rf2Error> {
        Ok(Self {
            id: record.parse(0, DescriptionId::parse)?,
            base: base(record)?,
            concept_id: record.parse(4, ConceptId::parse)?,
            language_code: record.text(5)?.to_owned(),
            type_id: record.parse(6, ConceptId::parse)?,
            term: record.text(7)?.to_owned(),
            case_significance_id: record.parse(8, ConceptId::parse)?,
        })
    }
}

/// A row of `sct2_Relationship` or `sct2_StatedRelationship` (the layouts coincide).
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::struct_field_names,
    reason = "the fields are the RF2 columns, named as the release file specification names them"
)]
pub struct Relationship {
    /// The relationship.
    pub id: RelationshipId,
    /// The shared columns.
    pub base: ComponentBase,
    /// The source concept.
    pub source_id: ConceptId,
    /// The destination concept.
    pub destination_id: ConceptId,
    /// The relationship group; `0` is ungrouped.
    pub relationship_group: u32,
    /// The attribute, for example `Is a`.
    pub type_id: ConceptId,
    /// The characteristic type, from the appendix E.5 enumeration.
    pub characteristic_type_id: ConceptId,
    /// The modifier.
    pub modifier_id: ConceptId,
}

impl Component for Relationship {
    const COLUMNS: &[&str] = &[
        "id",
        "effectiveTime",
        "active",
        "moduleId",
        "sourceId",
        "destinationId",
        "relationshipGroup",
        "typeId",
        "characteristicTypeId",
        "modifierId",
    ];

    fn from_record(record: &Record<'_>) -> Result<Self, Rf2Error> {
        Ok(Self {
            id: record.parse(0, RelationshipId::parse)?,
            base: base(record)?,
            source_id: record.parse(4, ConceptId::parse)?,
            destination_id: record.parse(5, ConceptId::parse)?,
            relationship_group: group(record, 6)?,
            type_id: record.parse(7, ConceptId::parse)?,
            characteristic_type_id: record.parse(8, ConceptId::parse)?,
            modifier_id: record.parse(9, ConceptId::parse)?,
        })
    }
}

fn group(record: &Record<'_>, index: usize) -> Result<u32, Rf2Error> {
    let value = record.integer(index)?;
    // The field error names the text; a `TryFromIntError` adds nothing to it.
    let Ok(group) = u32::try_from(value) else {
        return Err(record.field_error(
            index,
            FieldError::Integer {
                text: value.to_string(),
            },
        ));
    };
    Ok(group)
}

/// A concrete value: a number (`#3`) or a string (`"text"`) with its RF2 marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConcreteValue {
    /// A number in its lexical form, after the `#`.
    Number(String),
    /// A string, after the quotes.
    String(String),
}

impl ConcreteValue {
    fn parse(text: &str) -> Result<Self, FieldError> {
        if let Some(number) = text.strip_prefix('#') {
            let valid = !number.is_empty()
                && number
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b == b'.' || b == b'-')
                && number.bytes().filter(|b| *b == b'.').count() <= 1;
            return if valid {
                Ok(Self::Number(number.to_owned()))
            } else {
                Err(FieldError::Invalid {
                    what: "concrete number",
                    text: text.to_owned(),
                })
            };
        }
        if let Some(inner) = text.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            return Ok(Self::String(inner.to_owned()));
        }
        Err(FieldError::Invalid {
            what: "concrete value",
            text: text.to_owned(),
        })
    }
}

/// A row of `sct2_RelationshipConcreteValues`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcreteRelationship {
    /// The relationship.
    pub id: RelationshipId,
    /// The shared columns.
    pub base: ComponentBase,
    /// The source concept.
    pub source_id: ConceptId,
    /// The concrete value.
    pub value: ConcreteValue,
    /// The relationship group.
    pub relationship_group: u32,
    /// The attribute.
    pub type_id: ConceptId,
    /// The characteristic type, from the appendix E.5 enumeration.
    pub characteristic_type_id: ConceptId,
    /// The modifier.
    pub modifier_id: ConceptId,
}

impl Component for ConcreteRelationship {
    const COLUMNS: &[&str] = &[
        "id",
        "effectiveTime",
        "active",
        "moduleId",
        "sourceId",
        "value",
        "relationshipGroup",
        "typeId",
        "characteristicTypeId",
        "modifierId",
    ];

    fn from_record(record: &Record<'_>) -> Result<Self, Rf2Error> {
        Ok(Self {
            id: record.parse(0, RelationshipId::parse)?,
            base: base(record)?,
            source_id: record.parse(4, ConceptId::parse)?,
            value: record.parse(5, ConcreteValue::parse)?,
            relationship_group: group(record, 6)?,
            type_id: record.parse(7, ConceptId::parse)?,
            characteristic_type_id: record.parse(8, ConceptId::parse)?,
            modifier_id: record.parse(9, ConceptId::parse)?,
        })
    }
}

/// A row of `sct2_Identifier`: an alternate identifier for a component.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::struct_field_names,
    reason = "the fields are the RF2 columns, named as the release file specification names them"
)]
pub struct AlternateIdentifier {
    /// The identifier in the other scheme.
    pub alternate_identifier: String,
    /// The shared columns.
    pub base: ComponentBase,
    /// The scheme the identifier belongs to.
    pub identifier_scheme_id: ConceptId,
    /// The component identified.
    pub referenced_component_id: Sctid,
}

impl Component for AlternateIdentifier {
    const COLUMNS: &[&str] = &[
        "alternateIdentifier",
        "effectiveTime",
        "active",
        "moduleId",
        "identifierSchemeId",
        "referencedComponentId",
    ];

    fn from_record(record: &Record<'_>) -> Result<Self, Rf2Error> {
        Ok(Self {
            alternate_identifier: record.text(0)?.to_owned(),
            base: base(record)?,
            identifier_scheme_id: record.parse(4, ConceptId::parse)?,
            referenced_component_id: record.parse(5, Sctid::parse)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ConcreteValue;

    #[test]
    fn concrete_values_keep_their_marker_semantics() {
        assert_eq!(
            ConcreteValue::parse("#3"),
            Ok(ConcreteValue::Number("3".to_owned()))
        );
        assert_eq!(
            ConcreteValue::parse("#2.5"),
            Ok(ConcreteValue::Number("2.5".to_owned()))
        );
        assert_eq!(
            ConcreteValue::parse("\"mg\""),
            Ok(ConcreteValue::String("mg".to_owned()))
        );
        assert!(ConcreteValue::parse("3").is_err());
        assert!(ConcreteValue::parse("#").is_err());
        assert!(ConcreteValue::parse("\"open").is_err());
    }
}
