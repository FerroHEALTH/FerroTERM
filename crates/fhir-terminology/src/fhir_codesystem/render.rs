// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A model back to a generated `CodeSystem`, for a read of a served instance.
//!
//! One macro produces a module per served version so they cannot drift:
//! `render::r4b::code_system(&model)`. The `CodeSystem` elements this server
//! fills are the same in R4, R4B, R5, and the R6 ballot
//! (<https://hl7.org/fhir/R4B/codesystem.html>,
//! <https://hl7.org/fhir/R5/codesystem.html>), so one arm serves all four.
//!
//! A system the server holds behind an index declares `content = not-present`
//! and carries no `concept`; a system loaded as a `CodeSystem` resource
//! carries the concepts it declared, flattened, with its parents as `parent`
//! properties (<https://hl7.org/fhir/R4B/codesystem-concept-properties.html>).

/// The `structuredefinition-standards-status` extension, which marks a
/// resource, a concept, or a designation deprecated or withdrawn while it
/// stays defined
/// (<https://hl7.org/fhir/R4B/extension-structuredefinition-standards-status.html>).
const STANDARDS_STATUS: &str =
    "http://hl7.org/fhir/StructureDefinition/structuredefinition-standards-status";

macro_rules! render_code_system {
    ($module:ident) => {
        /// The `CodeSystem` renders of one FHIR version.
        pub mod $module {
            use fhir_types::$module::code_system::{
                CodeSystem, CodeSystemConcept, CodeSystemConceptDesignation,
                CodeSystemConceptProperty, CodeSystemConceptPropertyValue, CodeSystemFilter,
                CodeSystemProperty,
            };
            use fhir_types::$module::coding::Coding;
            use fhir_types::$module::extension::{Extension, ExtensionValue};

            use crate::fhir_codesystem::model::{CodeSystemModel, ConceptEntry};
            use crate::fhir_codesystem::render::STANDARDS_STATUS;
            use crate::provider::{Designation, PropertyValue};

            /// The `CodeSystem` of `model`.
            #[must_use]
            pub fn code_system(model: &CodeSystemModel) -> CodeSystem {
                CodeSystem {
                    url: Some(model.url.as_str().into()),
                    version: (!model.version.is_empty()).then(|| model.version.as_str().into()),
                    name: model.name.as_deref().map(Into::into),
                    title: model.title.as_deref().map(Into::into),
                    language: model.language.as_deref().map(Into::into),
                    status: model.status.as_str().into(),
                    experimental: model.experimental.map(Into::into),
                    extension: standards_status(model.standards_status.as_deref()),
                    case_sensitive: Some(model.case_sensitive.into()),
                    hierarchy_meaning: model.hierarchy_meaning.map(|meaning| meaning.code().into()),
                    compositional: model.compositional.then(|| true.into()),
                    version_needed: model.version_needed.then(|| true.into()),
                    content: model.content.code().into(),
                    supplements: model.supplements.as_deref().map(Into::into),
                    filter: model.filters.iter().map(filter).collect(),
                    property: model.properties.iter().map(property).collect(),
                    concept: model.concepts.iter().map(concept).collect(),
                    ..Default::default()
                }
            }

            /// The standards-status extension of `code`, when there is one.
            fn standards_status(code: Option<&str>) -> Vec<Extension> {
                code.map(|code| Extension {
                    url: STANDARDS_STATUS.to_owned(),
                    value: Some(ExtensionValue::Code(code.into())),
                    ..Default::default()
                })
                .into_iter()
                .collect()
            }

            /// One `CodeSystem.filter`.
            fn filter(declared: &crate::provider::FilterDefinition) -> CodeSystemFilter {
                CodeSystemFilter {
                    code: declared.code.as_str().into(),
                    description: declared.description.as_deref().map(Into::into),
                    operator: declared
                        .operators
                        .iter()
                        .map(|operator| operator.code().into())
                        .collect(),
                    value: declared.value.as_str().into(),
                    ..Default::default()
                }
            }

            /// One `CodeSystem.property`.
            fn property(declared: &crate::provider::PropertyDefinition) -> CodeSystemProperty {
                CodeSystemProperty {
                    code: declared.code.as_str().into(),
                    uri: declared.uri.as_deref().map(Into::into),
                    description: declared.description.as_deref().map(Into::into),
                    r#type: declared.kind.code().into(),
                    ..Default::default()
                }
            }

            /// One `CodeSystem.concept`, flat: the model holds the hierarchy as
            /// `parent` codes, which the `parent` property carries back
            /// (<https://hl7.org/fhir/R4B/codesystem-concept-properties.html>).
            fn concept(entry: &ConceptEntry) -> CodeSystemConcept {
                let mut property: Vec<CodeSystemConceptProperty> = entry
                    .parents
                    .iter()
                    .map(|parent| CodeSystemConceptProperty {
                        id: None,
                        extension: Vec::new(),
                        modifier_extension: Vec::new(),
                        code: "parent".into(),
                        value: CodeSystemConceptPropertyValue::Code(parent.as_str().into()),
                    })
                    .collect();
                property.extend(
                    entry
                        .properties
                        .iter()
                        .map(|held| CodeSystemConceptProperty {
                            id: None,
                            extension: Vec::new(),
                            modifier_extension: Vec::new(),
                            code: held.code.as_str().into(),
                            value: value(&held.value),
                        }),
                );
                CodeSystemConcept {
                    code: entry.code.as_str().into(),
                    display: entry.display.as_deref().map(Into::into),
                    definition: entry.definition.as_deref().map(Into::into),
                    extension: standards_status(entry.standards_status.as_deref()),
                    designation: entry.designations.iter().map(designation).collect(),
                    property,
                    ..Default::default()
                }
            }

            /// One `CodeSystem.concept.designation`.
            fn designation(held: &Designation) -> CodeSystemConceptDesignation {
                CodeSystemConceptDesignation {
                    language: held.language.as_deref().map(Into::into),
                    r#use: held.use_.as_ref().map(|use_| Coding {
                        system: Some(use_.system.as_str().into()),
                        code: Some(use_.code.as_str().into()),
                        display: use_.display.as_deref().map(Into::into),
                        ..Default::default()
                    }),
                    value: held.value.as_str().into(),
                    extension: standards_status(held.standards_status.as_deref()),
                    ..Default::default()
                }
            }

            /// A property value as the version's `concept.property.value[x]`.
            fn value(held: &PropertyValue) -> CodeSystemConceptPropertyValue {
                match held {
                    PropertyValue::Code(code) => {
                        CodeSystemConceptPropertyValue::Code(code.as_str().into())
                    }
                    PropertyValue::Uri(uri) => {
                        CodeSystemConceptPropertyValue::String(uri.as_str().into())
                    }
                    PropertyValue::String(text) => {
                        CodeSystemConceptPropertyValue::String(text.as_str().into())
                    }
                    PropertyValue::Coding {
                        system,
                        code,
                        display,
                    } => CodeSystemConceptPropertyValue::Coding(Box::new(Coding {
                        system: Some(system.as_str().into()),
                        code: Some(code.as_str().into()),
                        display: display.as_deref().map(Into::into),
                        ..Default::default()
                    })),
                    PropertyValue::Integer(number) => match i32::try_from(*number) {
                        Ok(number) => CodeSystemConceptPropertyValue::Integer(number.into()),
                        Err(_too_wide) => CodeSystemConceptPropertyValue::String(
                            number.to_string().as_str().into(),
                        ),
                    },
                    PropertyValue::Boolean(flag) => {
                        CodeSystemConceptPropertyValue::Boolean((*flag).into())
                    }
                    PropertyValue::DateTime(instant) => {
                        CodeSystemConceptPropertyValue::DateTime(instant.as_str().into())
                    }
                    PropertyValue::Decimal(number) => {
                        CodeSystemConceptPropertyValue::Decimal(number.as_str().into())
                    }
                }
            }
        }
    };
}

render_code_system!(r4);
render_code_system!(r4b);
render_code_system!(r5);
render_code_system!(r6);
