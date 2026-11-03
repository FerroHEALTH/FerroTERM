// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR JSON a save of the code system editor writes, one element at a
//! time.

use serde_json::Map;
use serde_json::Value;
use serde_json::json;

use super::Designation;
use super::TYPE_BOOLEAN;
use super::TYPE_CODE;
use super::TYPE_CODING;
use super::TYPE_DATE_TIME;
use super::TYPE_DECIMAL;
use super::TYPE_INTEGER;
use crate::fhir::authoring::CODING_SEPARATOR;

/// The string element `name` of `resource`, empty when it states none.
pub(super) fn text_of(resource: &Value, name: &str) -> String {
    resource
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The object `resource` carries, or an empty one when it is not an object.
pub(super) fn object(resource: &Value) -> Map<String, Value> {
    resource.as_object().cloned().unwrap_or_default()
}

/// Writes `value` under `name`, or removes the element when nothing was typed.
///
/// An element the reader emptied is removed rather than sent as an empty
/// string, which is not a value any of these elements admits.
pub(super) fn set(written: &mut Map<String, Value>, name: &str, value: &str) {
    if value.is_empty() {
        written.remove(name);
    } else {
        written.insert(name.to_owned(), json!(value));
    }
}

/// One designation, as the resource carries it.
pub(super) fn designation_body(designation: &Designation) -> Value {
    let mut written = Map::new();
    if !designation.language.trim().is_empty() {
        written.insert("language".to_owned(), json!(designation.language.trim()));
    }
    if let Some(usage) = coding_body(designation.usage.trim()) {
        written.insert("use".to_owned(), usage);
    }
    written.insert("value".to_owned(), json!(designation.value.trim()));
    Value::Object(written)
}

/// A `Coding` written `system|code`, or `None` when nothing was typed.
///
/// FHIR search spells a coding that way
/// (<https://hl7.org/fhir/R4B/search.html#token>), so one field carries both
/// halves. Text with no separator is the code alone, which is how a reader
/// types a coding whose system the resource states elsewhere.
pub(super) fn coding_body(typed: &str) -> Option<Value> {
    if typed.is_empty() {
        return None;
    }
    let (system, code) = typed
        .split_once(CODING_SEPARATOR)
        .map_or(("", typed), |(system, code)| (system.trim(), code.trim()));
    let mut coding = Map::new();
    if !system.is_empty() {
        coding.insert("system".to_owned(), json!(system));
    }
    if !code.is_empty() {
        coding.insert("code".to_owned(), json!(code));
    }
    if coding.is_empty() {
        None
    } else {
        Some(Value::Object(coding))
    }
}

/// One property value, under the element its declared type names.
///
/// `property.value[x]` is a choice of seven types
/// (<https://hl7.org/fhir/R4B/codesystem-definitions.html#CodeSystem.concept.property.value_x_>),
/// and the declared type decides which one a value is written as. Text that is
/// not of the declared type is written as a string, so the server refuses it
/// in its own words rather than the viewer silently dropping the value.
pub(super) fn value_body(code: &str, kind: &str, text: &str) -> Value {
    let typed = match kind {
        TYPE_CODE => Some(json!({"code": code, "valueCode": text})),
        TYPE_CODING => coding_body(text).map(|coding| json!({"code": code, "valueCoding": coding})),
        TYPE_BOOLEAN => text
            .parse::<bool>()
            .ok()
            .map(|flag| json!({"code": code, "valueBoolean": flag})),
        TYPE_INTEGER => text
            .parse::<i32>()
            .ok()
            .map(|number| json!({"code": code, "valueInteger": number})),
        TYPE_DECIMAL => text
            .parse::<serde_json::Number>()
            .ok()
            .map(|number| json!({"code": code, "valueDecimal": number})),
        TYPE_DATE_TIME => Some(json!({"code": code, "valueDateTime": text})),
        _string => None,
    };
    typed.unwrap_or_else(|| json!({"code": code, "valueString": text}))
}
