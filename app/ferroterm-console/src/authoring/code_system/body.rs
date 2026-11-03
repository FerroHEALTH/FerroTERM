// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `CodeSystem` a save of the code system editor writes, through the
//! `fhir-types` model of the release that serves it.

use super::CODING_SEPARATOR;
use super::{
    Concept, Draft, Lifecycle, TYPE_BOOLEAN, TYPE_CODE, TYPE_CODING, TYPE_DATE_TIME, TYPE_DECIMAL,
    TYPE_INTEGER,
};
use crate::fhir::release::Rewrites;

/// One property value as the editor writes it: the `value[x]` type its
/// declaration names, and the value.
///
/// `property.value[x]` is a choice of seven types
/// (<https://hl7.org/fhir/R4B/codesystem-definitions.html#CodeSystem.concept.property.value_x_>).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Typed {
    /// `valueCode`.
    Code(String),
    /// `valueCoding`, its system and its code.
    Coding(Option<String>, Option<String>),
    /// `valueString`.
    String(String),
    /// `valueInteger`.
    Integer(i32),
    /// `valueBoolean`.
    Boolean(bool),
    /// `valueDateTime`.
    DateTime(String),
    /// `valueDecimal`, in the lexical form the reader typed.
    Decimal(String),
}

/// A `Coding` written `system|code`, as its system and its code, or `None`
/// when nothing was typed.
///
/// FHIR search spells a coding that way
/// (<https://hl7.org/fhir/R4B/search.html#token>), so one field carries both
/// halves. Text with no separator is the code alone, which is how a reader
/// types a coding whose system the resource states elsewhere.
pub(super) fn coding_of(typed: &str) -> Option<(Option<String>, Option<String>)> {
    if typed.is_empty() {
        return None;
    }
    let (system, code) = typed
        .split_once(CODING_SEPARATOR)
        .map_or(("", typed), |(system, code)| (system.trim(), code.trim()));
    let stated = |text: &str| (!text.is_empty()).then(|| text.to_owned());
    match (stated(system), stated(code)) {
        (None, None) => None,
        coding => Some(coding),
    }
}

/// One property value, under the type `kind` names.
///
/// Text that is not of the declared type is written as a string, so the
/// server refuses it in its own words rather than the viewer silently
/// dropping the value.
pub(super) fn typed_value(kind: &str, text: &str) -> Typed {
    let typed = match kind {
        TYPE_CODE => Some(Typed::Code(text.to_owned())),
        TYPE_CODING => coding_of(text).map(|(system, code)| Typed::Coding(system, code)),
        TYPE_BOOLEAN => text.parse::<bool>().ok().map(Typed::Boolean),
        TYPE_INTEGER => text.parse::<i32>().ok().map(Typed::Integer),
        TYPE_DECIMAL => text
            .parse::<serde_json::Number>()
            .ok()
            .map(|_number| Typed::Decimal(text.to_owned())),
        TYPE_DATE_TIME => Some(Typed::DateTime(text.to_owned())),
        _string => None,
    };
    typed.unwrap_or_else(|| Typed::String(text.to_owned()))
}

impl Draft {
    /// The property values one concept writes: its lifecycle's, then the
    /// values typed into its rows under the types they are declared with.
    fn values_of(&self, concept: &Concept) -> Vec<(String, Typed)> {
        let mut values: Vec<(String, Typed)> = concept
            .lifecycle
            .properties()
            .into_iter()
            .map(|(code, value)| (code.to_owned(), value))
            .collect();
        for valued in &concept.values {
            let code = valued.code.trim();
            if code.is_empty() {
                continue;
            }
            values.push((
                code.to_owned(),
                typed_value(self.kind_of(code), valued.text.trim()),
            ));
        }
        values
    }
}

/// Writes a draft over the `CodeSystem` of the `fhir-types` module
/// `$module`.
macro_rules! rewrites_code_system {
    ($module:ident) => {
        mod $module {
            use fhir_types::$module::code_system::{
                CodeSystem, CodeSystemConcept, CodeSystemConceptDesignation,
                CodeSystemConceptProperty, CodeSystemConceptPropertyValue, CodeSystemProperty,
            };
            use fhir_types::$module::coding::Coding;
            use fhir_types::$module::primitives;

            use super::{Draft, Lifecycle, Rewrites, Typed, coding_of};

            /// `text`, or nothing when the reader left it empty.
            ///
            /// An element the reader emptied is removed rather than sent as an
            /// empty string, which is not a value any of these elements admits.
            fn stated<T: From<String>>(text: &str) -> Option<T> {
                (!text.is_empty()).then(|| T::from(text.to_owned()))
            }

            /// A coding of `system` and `code`.
            fn coding((system, code): (Option<String>, Option<String>)) -> Coding {
                Coding {
                    system: system.map(primitives::Uri::from),
                    code: code.map(primitives::Code::from),
                    ..Coding::default()
                }
            }

            /// One property value, under its `value[x]` type.
            fn value(typed: Typed) -> CodeSystemConceptPropertyValue {
                match typed {
                    Typed::Code(code) => CodeSystemConceptPropertyValue::Code(code.into()),
                    Typed::Coding(system, code) => {
                        CodeSystemConceptPropertyValue::Coding(Box::new(coding((system, code))))
                    }
                    Typed::String(text) => CodeSystemConceptPropertyValue::String(text.into()),
                    Typed::Integer(number) => {
                        CodeSystemConceptPropertyValue::Integer(primitives::Integer {
                            value: Some(number),
                            ..primitives::Integer::default()
                        })
                    }
                    Typed::Boolean(flag) => {
                        CodeSystemConceptPropertyValue::Boolean(primitives::Boolean {
                            value: Some(flag),
                            ..primitives::Boolean::default()
                        })
                    }
                    Typed::DateTime(moment) => {
                        CodeSystemConceptPropertyValue::DateTime(moment.into())
                    }
                    Typed::Decimal(number) => {
                        CodeSystemConceptPropertyValue::Decimal(number.into())
                    }
                }
            }

            /// The row of `stored` the code `code` names, as the server sent it.
            ///
            /// A concept and a property declaration are both identified by their
            /// code (<https://hl7.org/fhir/R4B/codesystem.html>), so that is what
            /// an authored row is matched to the stored one by.
            fn held<T: Clone + Default>(
                stored: &[T],
                code: &str,
                code_of: fn(&T) -> &primitives::Code,
            ) -> T {
                stored
                    .iter()
                    .find(|row| code_of(row).value.as_deref() == Some(code))
                    .cloned()
                    .unwrap_or_default()
            }

            impl Rewrites<Draft> for CodeSystem {
                fn rewritten(mut self, draft: &Draft) -> Self {
                    self.id = (!draft.id.is_empty()).then(|| draft.id.clone());
                    self.url = Some(draft.url.trim().into());
                    self.version = stated(draft.version.trim());
                    self.title = stated(draft.title.trim());
                    self.name = stated(draft.name.trim());
                    self.status = draft.status.as_str().into();
                    self.content = draft.content.as_str().into();
                    self.case_sensitive = Some(primitives::Boolean {
                        value: Some(draft.case_sensitive),
                        ..primitives::Boolean::default()
                    });

                    let declared = std::mem::take(&mut self.property);
                    let mut properties: Vec<CodeSystemProperty> = Lifecycle::declarations()
                        .into_iter()
                        .map(|(code, uri, kind)| CodeSystemProperty {
                            code: code.into(),
                            uri: Some(uri.into()),
                            r#type: kind.into(),
                            ..CodeSystemProperty::default()
                        })
                        .collect();
                    for row in &draft.properties {
                        let code = row.code.trim();
                        if code.is_empty() {
                            continue;
                        }
                        let mut written = held(&declared, code, |held| &held.code);
                        written.code = code.into();
                        written.uri = stated(row.uri.trim());
                        written.r#type = row.kind.trim().into();
                        properties.push(written);
                    }
                    self.property = properties;

                    let stored = std::mem::take(&mut self.concept);
                    self.concept = draft
                        .concepts
                        .iter()
                        .filter(|concept| !concept.code.trim().is_empty())
                        .map(|concept| {
                            // The stored concept of the same code is what this is
                            // written over, so its own child concepts survive.
                            let code = concept.code.trim();
                            let mut written: CodeSystemConcept =
                                held(&stored, code, |held| &held.code);
                            written.code = code.into();
                            written.display = stated(concept.display.trim());
                            written.definition = stated(concept.definition.trim());
                            written.designation = concept
                                .designations
                                .iter()
                                .filter(|designation| !designation.value.trim().is_empty())
                                .map(|designation| CodeSystemConceptDesignation {
                                    language: stated(designation.language.trim()),
                                    r#use: coding_of(designation.usage.trim()).map(coding),
                                    value: designation.value.trim().into(),
                                    ..CodeSystemConceptDesignation::default()
                                })
                                .collect();
                            written.property = draft
                                .values_of(concept)
                                .into_iter()
                                .map(|(code, typed)| CodeSystemConceptProperty {
                                    id: None,
                                    extension: Vec::new(),
                                    modifier_extension: Vec::new(),
                                    code: code.into(),
                                    value: value(typed),
                                })
                                .collect();
                            written
                        })
                        .collect();
                    self
                }
            }
        }
    };
}

rewrites_code_system!(r4);
rewrites_code_system!(r4b);
rewrites_code_system!(r5);
rewrites_code_system!(r6);
