// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! From a generated `CodeSystem` of any served FHIR version to the model.
//!
//! The four versions share the element set the model needs; R5 and R6 add
//! `designation.additionalUse`, carried as extra designations of the same
//! value. One macro produces a module per version so the four conversions
//! cannot drift: `convert::r4b::convert(&resource)`.

use super::model::{ModelError, PARENT, SUBSUMED_BY};
use crate::filter::FilterOperator;
use crate::provider::{
    ContentMode, FilterDefinition, HierarchyMeaning, PropertyDefinition, PropertyKind,
};

fn text<T: AsRef<str>>(value: Option<T>) -> Option<String> {
    value.map(|v| v.as_ref().to_owned())
}

/// `CodeSystem.content` as a mode.
fn content(code: &str) -> Result<ContentMode, ModelError> {
    ContentMode::parse(code).ok_or_else(|| ModelError::Content(code.to_owned()))
}

/// `CodeSystem.hierarchyMeaning` as a meaning.
fn hierarchy_meaning(code: Option<&str>) -> Result<Option<HierarchyMeaning>, ModelError> {
    code.map(|code| {
        HierarchyMeaning::parse(code).ok_or_else(|| ModelError::HierarchyMeaning(code.to_owned()))
    })
    .transpose()
}

/// One `CodeSystem.property` declaration.
fn property_definition(
    code: Option<&str>,
    uri: Option<&str>,
    description: Option<&str>,
    kind: Option<&str>,
) -> Result<PropertyDefinition, ModelError> {
    let code = code.unwrap_or_default().to_owned();
    let kind_code = kind.unwrap_or_default();
    let kind = PropertyKind::parse(kind_code).ok_or_else(|| ModelError::PropertyType {
        code: code.clone(),
        kind: kind_code.to_owned(),
    })?;
    Ok(PropertyDefinition {
        code,
        uri: text(uri),
        description: text(description),
        kind,
    })
}

/// One `CodeSystem.filter` declaration.
fn filter_definition(
    code: Option<&str>,
    description: Option<&str>,
    operators: &[Option<&str>],
    value: Option<&str>,
) -> Result<FilterDefinition, ModelError> {
    let code = code.unwrap_or_default().to_owned();
    let mut parsed = Vec::with_capacity(operators.len());
    for operator in operators {
        let op = operator.unwrap_or_default();
        parsed.push(
            FilterOperator::parse(op).ok_or_else(|| ModelError::FilterOperator {
                code: code.clone(),
                operator: op.to_owned(),
            })?,
        );
    }
    Ok(FilterDefinition {
        code,
        description: text(description),
        operators: parsed,
        value: value.unwrap_or_default().to_owned(),
    })
}

/// Adds `parent` to `parents` once.
fn note_parent(parents: &mut Vec<String>, parent: String) {
    if !parents.contains(&parent) {
        parents.push(parent);
    }
}

macro_rules! convert_code_system {
    ($module:ident, $additional_use:tt) => {
        /// The conversion for this FHIR version's `CodeSystem`.
        pub mod $module {
            use fhir_types::$module::code_system::{
                CodeSystem, CodeSystemConcept, CodeSystemConceptPropertyValue,
            };

            use super::super::model::{CodeSystemModel, ConceptEntry, ModelError, designation_use};
            use super::{PARENT, SUBSUMED_BY, note_parent, text};
            use crate::provider::{Designation, Property, PropertyValue};

            // NOTE: a primitive whose `value` is absent is legal FHIR
            // (<https://hl7.org/fhir/R5/json.html#primitive>), so the property is one
            // the resource does not state, never an empty string, a zero, or `false`.
            fn property_value(value: &CodeSystemConceptPropertyValue) -> Option<PropertyValue> {
                Some(match value {
                    CodeSystemConceptPropertyValue::Code(c) => {
                        PropertyValue::Code(text(c.value.as_deref())?)
                    }
                    CodeSystemConceptPropertyValue::Coding(c) => PropertyValue::Coding {
                        system: text(c.system.as_ref().and_then(|s| s.value.as_deref()))?,
                        code: text(c.code.as_ref().and_then(|s| s.value.as_deref()))?,
                        display: text(c.display.as_ref().and_then(|s| s.value.as_deref())),
                    },
                    CodeSystemConceptPropertyValue::String(s) => {
                        PropertyValue::String(text(s.value.as_deref())?)
                    }
                    CodeSystemConceptPropertyValue::Integer(i) => {
                        PropertyValue::Integer(i64::from(i.value?))
                    }
                    CodeSystemConceptPropertyValue::Boolean(b) => {
                        PropertyValue::Boolean(b.value?)
                    }
                    CodeSystemConceptPropertyValue::DateTime(d) => {
                        PropertyValue::DateTime(text(d.value.as_deref())?)
                    }
                    CodeSystemConceptPropertyValue::Decimal(d) => {
                        PropertyValue::Decimal(text(d.value.as_deref())?)
                    }
                })
            }

            // NOTE: `structuredefinition-standards-status` on a concept or a designation
            // marks it deprecated or withdrawn while it stays defined
            // (<https://hl7.org/fhir/R4B/extension-structuredefinition-standards-status.html>).
            fn standards_status(
                extensions: &[fhir_types::$module::extension::Extension],
            ) -> Option<String> {
                extensions
                    .iter()
                    .find(|x| {
                        x.url == "http://hl7.org/fhir/StructureDefinition/structuredefinition-standards-status"
                    })
                    .and_then(|x| match &x.value {
                        Some(fhir_types::$module::extension::ExtensionValue::Code(c)) => {
                            text(c.value.as_deref())
                        }
                        _ => None,
                    })
            }

            fn designations(concept: &CodeSystemConcept) -> Vec<Designation> {
                let mut out = Vec::new();
                for designation in &concept.designation {
                    let language =
                        text(designation.language.as_ref().and_then(|l| l.value.as_deref()));
                    let value = text(designation.value.value.as_deref()).unwrap_or_default();
                    let standing = standards_status(&designation.extension);
                    out.push(Designation {
                        language: language.clone(),
                        use_: designation.r#use.as_ref().and_then(|u| {
                            designation_use(
                                u.system.as_ref().and_then(|s| s.value.as_deref()),
                                u.code.as_ref().and_then(|s| s.value.as_deref()),
                                u.display.as_ref().and_then(|s| s.value.as_deref()),
                            )
                        }),
                        value: value.clone(),
                        standards_status: standing.clone(),
                    });
                    convert_code_system!(@additional $additional_use, designation, language, value, standing, out);
                }
                out
            }

            fn flatten(
                concepts: &[CodeSystemConcept],
                enclosing: Option<&str>,
                out: &mut Vec<ConceptEntry>,
            ) -> Result<(), ModelError> {
                for concept in concepts {
                    let code = text(concept.code.value.as_deref()).ok_or_else(|| {
                        ModelError::ConceptCode {
                            under: enclosing.map(str::to_owned),
                        }
                    })?;
                    let mut properties = Vec::new();
                    let mut parents: Vec<String> =
                        enclosing.map(str::to_owned).into_iter().collect();
                    for property in &concept.property {
                        let property_code = text(property.code.value.as_deref()).ok_or_else(
                            || ModelError::PropertyCode {
                                concept: code.clone(),
                            },
                        )?;
                        let Some(value) = property_value(&property.value) else {
                            continue;
                        };
                        if property_code == PARENT || property_code == SUBSUMED_BY {
                            note_parent(&mut parents, value.as_text());
                        }
                        properties.push(Property {
                            code: property_code,
                            value,
                            ..Property::default()
                        });
                    }
                    out.push(ConceptEntry {
                        code: code.clone(),
                        display: text(concept.display.as_ref().and_then(|d| d.value.as_deref())),
                        definition: text(
                            concept.definition.as_ref().and_then(|d| d.value.as_deref()),
                        ),
                        designations: designations(concept),
                        standards_status: standards_status(&concept.extension),
                        properties,
                        parents,
                    });
                    flatten(&concept.concept, Some(&code), out)?;
                }
                Ok(())
            }

            /// Reduces a `CodeSystem` of this FHIR version to the model.
            ///
            /// # Errors
            ///
            /// Returns [`ModelError`] for a resource without a `url`, a concept or a
            /// property without a code, an unknown
            /// `content`, `hierarchyMeaning`, property type, or filter operator,
            /// a duplicated code, or a parent the resource does not define.
            pub fn convert(resource: &CodeSystem) -> Result<CodeSystemModel, ModelError> {
                let url = text(resource.url.as_ref().and_then(|u| u.value.as_deref()))
                    .ok_or(ModelError::NoUrl)?;
                let content = super::content(resource.content.value.as_deref().unwrap_or(""))?;
                let hierarchy_meaning = super::hierarchy_meaning(
                    resource.hierarchy_meaning.as_ref().and_then(|c| c.value.as_deref()),
                )?;
                let mut properties = Vec::new();
                for property in &resource.property {
                    properties.push(super::property_definition(
                        property.code.value.as_deref(),
                        property.uri.as_ref().and_then(|u| u.value.as_deref()),
                        property.description.as_ref().and_then(|d| d.value.as_deref()),
                        property.r#type.value.as_deref(),
                    )?);
                }
                let mut filters = Vec::new();
                for filter in &resource.filter {
                    let operators: Vec<Option<&str>> =
                        filter.operator.iter().map(|o| o.value.as_deref()).collect();
                    filters.push(super::filter_definition(
                        filter.code.value.as_deref(),
                        filter.description.as_ref().and_then(|d| d.value.as_deref()),
                        &operators,
                        filter.value.value.as_deref(),
                    )?);
                }
                let mut concepts = Vec::new();
                flatten(&resource.concept, None, &mut concepts)?;
                let flag = |value: &Option<fhir_types::$module::primitives::Boolean>, default: bool| {
                    value.as_ref().and_then(|b| b.value).unwrap_or(default)
                };
                let model = CodeSystemModel {
                    id: resource.id.clone(),
                    url,
                    version: text(resource.version.as_ref().and_then(|v| v.value.as_deref()))
                        .unwrap_or_default(),
                    name: text(resource.name.as_ref().and_then(|n| n.value.as_deref())),
                    title: text(resource.title.as_ref().and_then(|t| t.value.as_deref()))
                        .or_else(|| text(resource.name.as_ref().and_then(|n| n.value.as_deref()))),
                    language: text(resource.language.as_ref().and_then(|l| l.value.as_deref())),
                    status: text(resource.status.value.as_deref()).unwrap_or_default(),
                    experimental: resource.experimental.as_ref().and_then(|b| b.value),
                    standards_status: resource
                        .extension
                        .iter()
                        .find(|x| x.url == "http://hl7.org/fhir/StructureDefinition/structuredefinition-standards-status")
                        .and_then(|x| match &x.value {
                            Some(fhir_types::$module::extension::ExtensionValue::Code(c)) => c.value.clone(),
                            _ => None,
                        }),
                    content,
                    case_sensitive: flag(&resource.case_sensitive, true),
                    hierarchy_meaning,
                    compositional: flag(&resource.compositional, false),
                    version_needed: flag(&resource.version_needed, false),
                    supplements: text(
                        resource.supplements.as_ref().and_then(|s| s.value.as_deref()),
                    ),
                    properties,
                    filters,
                    concepts,
                };
                model.validate()?;
                Ok(model)
            }
        }
    };
    (@additional true, $designation:ident, $language:ident, $value:ident, $standing:ident, $out:ident) => {
        for extra in &$designation.additional_use {
            $out.push(Designation {
                language: $language.clone(),
                use_: designation_use(
                    extra.system.as_ref().and_then(|s| s.value.as_deref()),
                    extra.code.as_ref().and_then(|s| s.value.as_deref()),
                    extra.display.as_ref().and_then(|s| s.value.as_deref()),
                ),
                value: $value.clone(),
                standards_status: $standing.clone(),
            });
        }
    };
    (@additional false, $designation:ident, $language:ident, $value:ident, $standing:ident, $out:ident) => {};
}

convert_code_system!(r4, false);
convert_code_system!(r4b, false);
convert_code_system!(r5, true);
convert_code_system!(r6, true);
