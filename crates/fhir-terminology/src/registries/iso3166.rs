// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! ISO 3166-1 country codes (`urn:iso:std:iso:3166`) from the Unicode CLDR data.
//!
//! All three code forms of each territory are codes of this system: the
//! alpha-2, the alpha-3, and the numeric. FHIR says so by selecting them from
//! it: `ValueSet/iso3166-1-2`, `-1-3`, and `-1-N` each include this system
//! under a `code` regex of `[A-Z]{2}`, `[A-Z]{3}`, and `[0-9]{3}`
//! (<https://hl7.org/fhir/R5/valueset-iso3166-1-3.html>). Serving only the
//! alpha-2 form left the other two value sets empty and refused an alpha-3
//! country code the specification binds.
//!
//! Upper case, compared without case, with the English name; every concept
//! carries all three forms as properties
//! (<https://terminology.hl7.org/ISO3166.html>).

use crate::fhir_codesystem::model::{CodeSystemModel, ConceptEntry};
use crate::fhir_codesystem::provider::{BuildError, FhirCodeSystem};
use crate::filter::FilterOperator;
use crate::provider::{
    ContentMode, Designation, FilterDefinition, Property, PropertyDefinition, PropertyKind,
    PropertyValue,
};

/// The system URI.
pub const URL: &str = "urn:iso:std:iso:3166";

const CODE_MAPPINGS: &str = include_str!("../../data/cldr/codeMappings.json");
const TERRITORIES: &str = include_str!("../../data/cldr/territories.json");

/// The vendored CLDR data does not parse.
#[derive(Debug, thiserror::Error)]
pub enum DataError {
    /// A JSON file does not parse or lacks the expected shape.
    #[error("the vendored CLDR data does not parse")]
    Json(#[from] serde_json::Error),
    /// The table does not build.
    #[error(transparent)]
    Build(#[from] BuildError),
}

/// Whether an alpha-2 code lies in the user-assigned ranges of ISO 3166-1
/// (`AA`, `QM` to `QZ`, `XA` to `XZ`, `ZZ`).
#[must_use]
pub fn is_user_assigned(code: &str) -> bool {
    let bytes = code.as_bytes();
    match bytes {
        [b'A', b'A'] | [b'Z', b'Z'] => true,
        [b'Q', second] => (b'M'..=b'Z').contains(second),
        [b'X', second] => second.is_ascii_uppercase(),
        _ => false,
    }
}

/// The ISO 3166-1 code system as a model, from the vendored CLDR data.
///
/// The officially assigned codes (a numeric code below 900) and the
/// user-assigned ranges are served; CLDR's own reservations (`EU`, `QO`) are
/// not. A user-assigned code without a CLDR name displays as `User-assigned`.
///
/// # Errors
///
/// Returns [`DataError::Json`] when the vendored data does not parse.
pub fn code_system() -> Result<CodeSystemModel, DataError> {
    let mappings: serde_json::Value = serde_json::from_str(CODE_MAPPINGS)?;
    let territories: serde_json::Value = serde_json::from_str(TERRITORIES)?;
    let version = at(&mappings, &["supplemental", "version", "_cldrVersion"])
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let names = at(
        &territories,
        &["main", "en", "localeDisplayNames", "territories"],
    );
    let mut concepts = Vec::new();
    let entries = at(&mappings, &["supplemental", "codeMappings"])
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flatten();
    for (code, mapping) in entries {
        if code.len() != 2 || !code.bytes().all(|b| b.is_ascii_uppercase()) {
            continue;
        }
        let Some(numeric) = mapping.get("_numeric").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let official = numeric.parse::<u16>().is_ok_and(|n| n < 900);
        if !official && !is_user_assigned(code) {
            continue;
        }
        let display = names
            .and_then(|n| n.get(code))
            .and_then(serde_json::Value::as_str)
            .map_or_else(|| String::from("User-assigned"), str::to_owned);
        let alpha3 = mapping.get("_alpha3").and_then(serde_json::Value::as_str);
        let forms = [Some(code.as_str()), alpha3, Some(numeric)];
        let mut properties = vec![code_value("alpha2", code), code_value("numeric", numeric)];
        if let Some(alpha3) = alpha3 {
            properties.push(code_value("alpha3", alpha3));
        }
        if !official {
            properties.push(Property {
                code: String::from("userAssigned"),
                value: PropertyValue::Boolean(true),
                ..Property::default()
            });
        }
        // One concept per form, each carrying the same display and the same
        // three properties, so a client that holds any one of them can read
        // the others without a second request.
        for form in forms.into_iter().flatten() {
            concepts.push(ConceptEntry {
                standards_status: None,
                code: form.to_owned(),
                display: Some(display.clone()),
                definition: None,
                designations: vec![Designation {
                    standards_status: None,
                    language: Some(String::from("en")),
                    use_: None,
                    value: display.clone(),
                }],
                properties: properties.clone(),
                parents: Vec::new(),
            });
        }
    }
    Ok(CodeSystemModel {
        // NOTE: the vendored table is no resource, and a logical id belongs to
        // one (<https://hl7.org/fhir/R4B/resource.html#id>).
        id: None,
        url: URL.to_owned(),
        version,
        name: None,
        title: Some(String::from("ISO 3166-1 country codes")),
        language: Some(String::from("en")),
        status: String::from("active"),
        experimental: None,
        date: None,
        publisher: None,
        description: None,
        standards_status: None,
        content: ContentMode::Complete,
        case_sensitive: false,
        hierarchy_meaning: None,
        compositional: false,
        version_needed: false,
        supplements: None,
        properties: property_definitions(),
        filters: vec![FilterDefinition {
            code: String::from("code"),
            description: Some(String::from("Codes matching a regular expression")),
            operators: vec![
                FilterOperator::Regex,
                FilterOperator::Equal,
                FilterOperator::In,
            ],
            value: String::from("a regular expression"),
        }],
        concepts,
    })
}

/// One property as a code value.
fn code_value(code: &str, value: &str) -> Property {
    Property {
        code: code.to_owned(),
        value: PropertyValue::Code(value.to_owned()),
        ..Property::default()
    }
}

/// The properties every code carries: the three forms, and `userAssigned`.
fn property_definitions() -> Vec<PropertyDefinition> {
    let code_property = |code: &str, description: &str, kind: PropertyKind| PropertyDefinition {
        code: code.to_owned(),
        uri: None,
        description: Some(description.to_owned()),
        kind,
    };
    vec![
        code_property("alpha2", "The ISO 3166-1 alpha-2 code", PropertyKind::Code),
        code_property("alpha3", "The ISO 3166-1 alpha-3 code", PropertyKind::Code),
        code_property("numeric", "The ISO 3166-1 numeric code", PropertyKind::Code),
        code_property(
            "userAssigned",
            "The code is in a user-assigned range",
            PropertyKind::Boolean,
        ),
    ]
}

/// The ISO 3166-1 provider.
///
/// # Errors
///
/// Returns [`DataError`] when the vendored data does not parse or build.
pub fn provider() -> Result<FhirCodeSystem, DataError> {
    Ok(FhirCodeSystem::new(code_system()?)?)
}

/// The value at `path` in `value`, when every step is an object with that key.
fn at<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a serde_json::Value> {
    path.iter().try_fold(value, |v, key| v.get(key))
}
