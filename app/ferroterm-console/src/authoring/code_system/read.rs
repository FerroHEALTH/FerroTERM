// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The draft the code system editor opens on a `CodeSystem` the server
//! holds, read through the `fhir-types` model of the release that served it.

use super::CODING_SEPARATOR;
use super::{
    Concept, DEPRECATION_DATE, Declared, Designation, Draft, INACTIVE, Keys, Lifecycle,
    RETIREMENT_DATE, STATUS, Valued,
};
use crate::fhir::release::{Reads, document};
use crate::fhir::searchset::Found;

/// One stored concept as the editor opens it: its code, its display and
/// definition, its designations as `(language, use, value)`, and its
/// property values as `(code, text)`.
pub(super) struct Opened {
    /// `concept.code`.
    pub(super) code: String,
    /// `concept.display`.
    pub(super) display: String,
    /// `concept.definition`.
    pub(super) definition: String,
    /// Each `designation`.
    pub(super) designations: Vec<(String, String, String)>,
    /// Each `property`, its value as the text its control holds.
    pub(super) values: Vec<(String, String)>,
}

/// A coding as the one field the editor types it in, `system|code`.
pub(super) fn typed_coding(system: Option<&str>, code: Option<&str>) -> String {
    format!(
        "{}{CODING_SEPARATOR}{}",
        system.unwrap_or_default(),
        code.unwrap_or_default()
    )
}

/// Where one stored concept is in its life, from the property values it
/// carries.
///
/// The status code leads, because it is the property that names the state;
/// the dates fill in the one the state carries, and a concept with no status
/// but a date reads as the state that date implies.
fn lifecycle_of(values: &[(String, String)]) -> Lifecycle {
    let text_of = |wanted: &str| {
        values
            .iter()
            .find(|(code, _)| code == wanted)
            .map(|(_, text)| text.clone())
            .unwrap_or_default()
    };
    let status = text_of(STATUS);
    let retirement = text_of(RETIREMENT_DATE);
    let deprecation = text_of(DEPRECATION_DATE);
    if status.is_empty() {
        if !retirement.is_empty() {
            return Lifecycle::Retired { on: retirement };
        }
        if !deprecation.is_empty() {
            return Lifecycle::Deprecated { on: deprecation };
        }
        if text_of(INACTIVE) == "true" {
            return Lifecycle::Retired { on: String::new() };
        }
        return Lifecycle::Active;
    }
    let on = match status.as_str() {
        "retired" => retirement,
        "deprecated" => deprecation,
        _other => String::new(),
    };
    Lifecycle::of(&status, &on)
}

/// The rows a draft opens on: the declarations and the concepts the
/// lifecycle does not own, each with a key of its own.
pub(super) fn rows(
    declared: Vec<(String, String, String)>,
    concepts: Vec<Opened>,
) -> (Vec<Declared>, Vec<Concept>, Keys) {
    let mut keys = Keys::default();
    let properties = declared
        .into_iter()
        .filter(|(code, _, _)| !Lifecycle::owned().contains(&code.as_str()))
        .map(|(code, uri, kind)| Declared {
            key: keys.next(),
            code,
            uri,
            kind,
        })
        .collect();
    let concepts = concepts
        .into_iter()
        .map(|concept| Concept {
            key: keys.next(),
            lifecycle: lifecycle_of(&concept.values),
            code: concept.code,
            display: concept.display,
            definition: concept.definition,
            designations: concept
                .designations
                .into_iter()
                .map(|(language, usage, value)| Designation {
                    key: keys.next(),
                    language,
                    usage,
                    value,
                })
                .collect(),
            values: concept
                .values
                .into_iter()
                .filter(|(code, _)| !Lifecycle::owned().contains(&code.as_str()))
                .map(|(code, text)| Valued {
                    key: keys.next(),
                    code,
                    text,
                })
                .collect(),
        })
        .collect();
    (properties, concepts, keys)
}

/// Reads the `CodeSystem` of the `fhir-types` module `$module` into the
/// draft the editor opens on it.
macro_rules! reads_code_system {
    ($module:ident) => {
        mod $module {
            use fhir_types::$module::code_system::{
                CodeSystem, CodeSystemConcept, CodeSystemConceptPropertyValue,
            };

            use super::{Draft, Found, Opened, Reads, document, rows, typed_coding};

            /// The text of a primitive, empty where it states none.
            fn text(value: Option<&Option<String>>) -> String {
                value.cloned().flatten().unwrap_or_default()
            }

            /// A property value as the text its control holds.
            fn value_text(value: &CodeSystemConceptPropertyValue) -> String {
                match value {
                    CodeSystemConceptPropertyValue::Code(code) => text(Some(&code.value)),
                    CodeSystemConceptPropertyValue::Coding(coding) => typed_coding(
                        coding
                            .system
                            .as_ref()
                            .and_then(|system| system.value.as_deref()),
                        coding.code.as_ref().and_then(|code| code.value.as_deref()),
                    ),
                    CodeSystemConceptPropertyValue::String(string) => text(Some(&string.value)),
                    CodeSystemConceptPropertyValue::Integer(number) => number
                        .value
                        .map(|number| number.to_string())
                        .unwrap_or_default(),
                    CodeSystemConceptPropertyValue::Boolean(flag) => {
                        flag.value.map(|flag| flag.to_string()).unwrap_or_default()
                    }
                    CodeSystemConceptPropertyValue::DateTime(moment) => text(Some(&moment.value)),
                    CodeSystemConceptPropertyValue::Decimal(number) => text(Some(&number.value)),
                }
            }

            /// One stored concept, as the editor opens it.
            fn opened(concept: &CodeSystemConcept) -> Opened {
                Opened {
                    code: text(Some(&concept.code.value)),
                    display: text(concept.display.as_ref().map(|display| &display.value)),
                    definition: text(
                        concept
                            .definition
                            .as_ref()
                            .map(|definition| &definition.value),
                    ),
                    designations: concept
                        .designation
                        .iter()
                        .map(|designation| {
                            (
                                text(
                                    designation
                                        .language
                                        .as_ref()
                                        .map(|language| &language.value),
                                ),
                                designation
                                    .r#use
                                    .as_ref()
                                    .map(|usage| {
                                        typed_coding(
                                            usage
                                                .system
                                                .as_ref()
                                                .and_then(|system| system.value.as_deref()),
                                            usage
                                                .code
                                                .as_ref()
                                                .and_then(|code| code.value.as_deref()),
                                        )
                                    })
                                    .unwrap_or_default(),
                                text(Some(&designation.value.value)),
                            )
                        })
                        .collect(),
                    values: concept
                        .property
                        .iter()
                        .map(|value| (text(Some(&value.code.value)), value_text(&value.value)))
                        .collect(),
                }
            }

            impl Reads<Draft> for CodeSystem {
                fn read(&self) -> Draft {
                    let (properties, concepts, keys) = rows(
                        self.property
                            .iter()
                            .map(|property| {
                                (
                                    text(Some(&property.code.value)),
                                    text(property.uri.as_ref().map(|uri| &uri.value)),
                                    text(Some(&property.r#type.value)),
                                )
                            })
                            .collect(),
                        self.concept.iter().map(opened).collect(),
                    );
                    Draft {
                        id: self.id.clone().unwrap_or_default(),
                        version_id: text(
                            self.meta
                                .as_ref()
                                .and_then(|meta| meta.version_id.as_ref())
                                .map(|id| &id.value),
                        ),
                        url: text(self.url.as_ref().map(|url| &url.value)),
                        version: text(self.version.as_ref().map(|version| &version.value)),
                        title: text(self.title.as_ref().map(|title| &title.value)),
                        name: text(self.name.as_ref().map(|name| &name.value)),
                        status: text(Some(&self.status.value)),
                        content: text(Some(&self.content.value)),
                        case_sensitive: self
                            .case_sensitive
                            .as_ref()
                            .and_then(|flag| flag.value)
                            .unwrap_or(true),
                        properties,
                        concepts,
                        keys,
                        base: document(self),
                    }
                }
            }

            crate::fhir::searchset::carries!($module, CodeSystem, Draft);
        }
    };
}

reads_code_system!(r4);
reads_code_system!(r4b);
reads_code_system!(r5);
reads_code_system!(r6);
