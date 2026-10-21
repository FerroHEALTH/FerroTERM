// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! From a generated `ValueSet` of any served FHIR version to the model.
//!
//! The four versions share the compose elements the model needs; one macro
//! produces a module per version so they cannot drift:
//! `convert::r4b::convert(&resource)`.

use super::model::ModelError;
use crate::filter::{Filter, FilterOperator};

fn text<T: AsRef<str>>(value: Option<T>) -> Option<String> {
    value.map(|v| v.as_ref().to_owned())
}

/// One `compose.include.filter`, located at `expression` in `system`.
fn filter(
    system: Option<&str>,
    expression: &str,
    property: Option<&str>,
    op: Option<&str>,
    value: Option<&str>,
) -> Result<Filter, ModelError> {
    let property = property.unwrap_or_default().to_owned();
    let op_code = op.unwrap_or_default();
    let op = FilterOperator::parse(op_code).ok_or_else(|| ModelError::FilterOperator {
        property: property.clone(),
        op: op_code.to_owned(),
    })?;
    let value = value.ok_or_else(|| ModelError::FilterValue {
        system: system.unwrap_or_default().to_owned(),
        property: property.clone(),
        op: op_code.to_owned(),
        expression: expression.to_owned(),
    })?;
    Ok(Filter {
        property,
        op,
        value: value.to_owned(),
    })
}

/// The cross-version extension an R4 resource carries an R5 filter operator in
/// (<https://hl7.org/fhir/versions.html#extensions>).
const FILTER_OP_EXTENSION: &str =
    "http://hl7.org/fhir/5.0/StructureDefinition/extension-ValueSet.compose.include.filter.op";

macro_rules! convert_value_set {
    (@value required, $f:ident) => {
        $f.value.value.as_deref()
    };
    (@value optional, $f:ident) => {
        $f.value.as_ref().and_then(|v| v.value.as_deref())
    };
    ($module:ident, $value:tt) => {
        /// The conversion for this FHIR version's `ValueSet`.
        pub mod $module {
            use fhir_types::$module::value_set::{ValueSet, ValueSetComposeInclude};

            use super::super::model::{ModelError, ValueSetModel};
            use super::text;
            use crate::compose::{Compose, ConceptRef, Include, SystemRef};

            fn include(source: &ValueSetComposeInclude, at: &str) -> Result<Include, ModelError> {
                let system = source.system.as_ref().and_then(|s| s.value.as_deref());
                let mut filters = Vec::with_capacity(source.filter.len());
                for (index, f) in source.filter.iter().enumerate() {
                    // NOTE: an R4 resource converted from R5 carries `child-of` and
                    // `descendent-leaf` in the cross-version extension on `op`
                    // (<https://hl7.org/fhir/versions.html#extensions>).
                    let extended = f.op.extension.iter().find(|x| x.url == super::FILTER_OP_EXTENSION).and_then(|x| match &x.value {
                        Some(fhir_types::$module::extension::ExtensionValue::Code(c)) => c.value.as_deref(),
                        _ => None,
                    });
                    filters.push(super::filter(
                        system,
                        &format!("{at}.filter[{index}]"),
                        f.property.value.as_deref(),
                        f.op.value.as_deref().or(extended),
                        convert_value_set!(@value $value, f),
                    )?);
                }
                Ok(Include {
                    system: text(system).map(
                        |url| SystemRef {
                            url,
                            version: text(source.version.as_ref().and_then(|v| v.value.as_deref())),
                        },
                    ),
                    concepts: source
                        .concept
                        .iter()
                        .map(|c| ConceptRef {
                            code: text(c.code.value.as_deref()).unwrap_or_default(),
                            display: text(c.display.as_ref().and_then(|d| d.value.as_deref())),
                            // NOTE: `valueset-deprecated` marks a concept the value set
                            // still lists but discourages
                            // (<https://hl7.org/fhir/R4B/extension-valueset-deprecated.html>).
                            deprecated: c.extension.iter().any(|x| {
                                x.url == "http://hl7.org/fhir/StructureDefinition/valueset-deprecated"
                                    && match &x.value {
                                        Some(fhir_types::$module::extension::ExtensionValue::Boolean(b)) => {
                                            b.value == Some(true)
                                        }
                                        // The ecosystem's fixtures spell the boolean as a code.
                                        Some(fhir_types::$module::extension::ExtensionValue::Code(c)) => {
                                            c.value.as_deref() == Some("true")
                                        }
                                        _ => false,
                                    }
                            }),
                        })
                        .collect(),
                    filters,
                    value_sets: source
                        .value_set
                        .iter()
                        .filter_map(|v| text(v.value.as_deref()))
                        .collect(),
                })
            }

            /// Reduces a `ValueSet` of this FHIR version to the model.
            ///
            /// # Errors
            ///
            /// Returns [`ModelError`] for a filter with an unknown operator or
            /// with no value.
            pub fn convert(resource: &ValueSet) -> Result<ValueSetModel, ModelError> {
                // NOTE: `ValueSet.url` is 0..1 (<https://hl7.org/fhir/R4B/valueset.html>);
                // an inline value set may have none, a stored one must (the loader checks).
                let url = text(resource.url.as_ref().and_then(|u| u.value.as_deref()))
                    .unwrap_or_default();
                let mut compose = Compose::default();
                if let Some(source) = &resource.compose {
                    compose.inactive = source.inactive.as_ref().and_then(|b| b.value);
                    for (index, i) in source.include.iter().enumerate() {
                        compose
                            .include
                            .push(include(i, &format!("ValueSet.compose.include[{index}]"))?);
                    }
                    for (index, e) in source.exclude.iter().enumerate() {
                        compose
                            .exclude
                            .push(include(e, &format!("ValueSet.compose.exclude[{index}]"))?);
                    }
                }
                let string = |value: &Option<fhir_types::$module::primitives::String>| {
                    text(value.as_ref().and_then(|v| v.value.as_deref()))
                };
                let flag = |value: &Option<fhir_types::$module::primitives::Boolean>| {
                    value.as_ref().and_then(|b| b.value)
                };
                // NOTE: `valueset-supplement` names the supplements a value set needs
                // (<https://hl7.org/fhir/R4B/extension-valueset-supplement.html>).
                let supplements = resource
                    .extension
                    .iter()
                    .filter(|x| x.url == "http://hl7.org/fhir/StructureDefinition/valueset-supplement")
                    .filter_map(|x| match &x.value {
                        Some(fhir_types::$module::extension::ExtensionValue::Canonical(c)) => c.value.clone(),
                        _ => None,
                    })
                    .collect();
                // NOTE: `valueset-expansion-parameter` on the compose carries the
                // value set's default expansion parameters
                // (<https://hl7.org/fhir/R4B/extension-valueset-expansion-parameter.html>).
                let expansion_parameters = resource
                    .compose
                    .iter()
                    .flat_map(|c| c.extension.iter())
                    .filter(|x| x.url == "http://hl7.org/fhir/StructureDefinition/valueset-expansion-parameter")
                    .filter_map(|x| {
                        let part = |name: &str| {
                            x.extension.iter().find(|p| p.url == name).and_then(|p| match &p.value {
                                Some(fhir_types::$module::extension::ExtensionValue::Code(c)) => c.value.clone(),
                                Some(fhir_types::$module::extension::ExtensionValue::String(s)) => s.value.clone(),
                                Some(fhir_types::$module::extension::ExtensionValue::Uri(u)) => u.value.clone(),
                                Some(fhir_types::$module::extension::ExtensionValue::Boolean(b)) => {
                                    b.value.map(|b| b.to_string())
                                }
                                Some(fhir_types::$module::extension::ExtensionValue::Integer(i)) => {
                                    i.value.map(|i| i.to_string())
                                }
                                _ => None,
                            })
                        };
                        Some(crate::valueset::model::ExpansionDefault {
                            name: part("name")?,
                            value: part("value")?,
                        })
                    })
                    .collect();
                // NOTE: a canonical reference to a contained resource is the
                // `#id` fragment of the container
                // (<https://hl7.org/fhir/R4B/references.html#canonical>).
                let mut contained = std::collections::BTreeMap::new();
                for entry in &resource.contained {
                    if let fhir_types::$module::resource::Resource::ValueSet(inner) = entry
                        && let Some(id) = inner.id.as_deref()
                    {
                        contained.insert(id.to_owned(), convert(inner)?);
                    }
                }
                let standards_status = resource
                    .extension
                    .iter()
                    .find(|x| x.url == "http://hl7.org/fhir/StructureDefinition/structuredefinition-standards-status")
                    .and_then(|x| match &x.value {
                        Some(fhir_types::$module::extension::ExtensionValue::Code(c)) => c.value.clone(),
                        _ => None,
                    });
                Ok(ValueSetModel {
                    id: resource.id.clone(),
                    url,
                    version: string(&resource.version),
                    language: text(resource.language.as_ref().and_then(|l| l.value.as_deref())),
                    supplements,
                    expansion_parameters,
                    standards_status,
                    name: string(&resource.name),
                    title: string(&resource.title),
                    status: text(resource.status.value.as_deref()).unwrap_or_default(),
                    experimental: flag(&resource.experimental),
                    date: text(resource.date.as_ref().and_then(|d| d.value.as_deref())),
                    publisher: string(&resource.publisher),
                    description: text(
                        resource
                            .description
                            .as_ref()
                            .and_then(|d| d.value.as_deref()),
                    ),
                    copyright: text(
                        resource.copyright.as_ref().and_then(|c| c.value.as_deref()),
                    ),
                    immutable: flag(&resource.immutable),
                    compose,
                    contained,
                })
            }
        }
    };
}

convert_value_set!(r4, required);
convert_value_set!(r4b, required);
convert_value_set!(r5, required);
convert_value_set!(r6, optional);
