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
const MAPPINGS_FILE: &str = "codeMappings.json";
const TERRITORIES_FILE: &str = "territories.json";

/// The vendored CLDR data does not parse.
#[derive(Debug, thiserror::Error)]
pub enum DataError {
    /// A JSON file does not parse or lacks the expected shape.
    #[error("the vendored CLDR data does not parse")]
    Json(#[from] serde_json::Error),
    /// A JSON file parses and lacks a member the table is read from, or holds
    /// it as another JSON type.
    #[error("the vendored CLDR file {file} has no {member}")]
    Missing {
        /// The vendored file.
        file: &'static str,
        /// The dotted path of the missing member.
        member: String,
    },
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
/// Returns [`DataError::Json`] when the vendored data does not parse, and
/// [`DataError::Missing`] when it lacks the version, the names or the codes.
pub fn code_system() -> Result<CodeSystemModel, DataError> {
    let mappings: serde_json::Value = serde_json::from_str(CODE_MAPPINGS)?;
    let territories: serde_json::Value = serde_json::from_str(TERRITORIES)?;
    from_json(&mappings, &territories)
}

/// The code system from the parsed `codeMappings.json` and `territories.json`.
fn from_json(
    mappings: &serde_json::Value,
    territories: &serde_json::Value,
) -> Result<CodeSystemModel, DataError> {
    const VERSION: &[&str] = &["supplemental", "version", "_cldrVersion"];
    const NAMES: &[&str] = &["main", "en", "localeDisplayNames", "territories"];
    const MAPPINGS: &[&str] = &["supplemental", "codeMappings"];
    let version = required(mappings, VERSION, MAPPINGS_FILE, serde_json::Value::as_str)?.to_owned();
    let names = required(
        territories,
        NAMES,
        TERRITORIES_FILE,
        serde_json::Value::as_object,
    )?;
    let entries = required(
        mappings,
        MAPPINGS,
        MAPPINGS_FILE,
        serde_json::Value::as_object,
    )?;
    let mut concepts = Vec::new();
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
            .get(code)
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

/// The value at `path` in `value` in the form `read` takes, or the error
/// naming the member of `file`.
fn required<'a, T: ?Sized>(
    value: &'a serde_json::Value,
    path: &[&str],
    file: &'static str,
    read: impl FnOnce(&'a serde_json::Value) -> Option<&'a T>,
) -> Result<&'a T, DataError> {
    path.iter()
        .try_fold(value, |v, key| v.get(key))
        .and_then(read)
        .ok_or_else(|| DataError::Missing {
            file,
            member: path.join("."),
        })
}

#[cfg(test)]
mod tests {
    use super::{DataError, from_json};

    const MAPPINGS: &str = r#"{"supplemental": {"version": {"_cldrVersion": "49"},
        "codeMappings": {"NL": {"_numeric": "528", "_alpha3": "NLD"}}}}"#;
    const NAMES: &str = r#"{"main": {"en": {"localeDisplayNames":
        {"territories": {"NL": "Netherlands"}}}}}"#;

    fn read(mappings: &str, names: &str) -> Result<super::CodeSystemModel, DataError> {
        let mappings = serde_json::from_str(mappings).expect("the mappings parse");
        let names = serde_json::from_str(names).expect("the names parse");
        from_json(&mappings, &names)
    }

    fn missing(mappings: &str, names: &str) -> (&'static str, String) {
        match read(mappings, names) {
            Err(DataError::Missing { file, member }) => (file, member),
            other => panic!("expected a missing member, got {other:?}"),
        }
    }

    #[test]
    fn a_whole_file_pair_reads_its_version_and_codes() {
        let model = read(MAPPINGS, NAMES).expect("the synthetic data reads");
        assert_eq!(model.version, "49");
        let codes: Vec<&str> = model.concepts.iter().map(|c| c.code.as_str()).collect();
        assert_eq!(codes, ["NL", "NLD", "528"]);
    }

    #[test]
    fn a_missing_or_mistyped_version_is_an_error_never_unknown() {
        let without = MAPPINGS.replace(r#""_cldrVersion": "49""#, r#""other": "49""#);
        assert_eq!(
            missing(&without, NAMES),
            (
                "codeMappings.json",
                String::from("supplemental.version._cldrVersion")
            )
        );
        let number = MAPPINGS.replace(r#""_cldrVersion": "49""#, r#""_cldrVersion": 49"#);
        assert_eq!(
            missing(&number, NAMES).1,
            "supplemental.version._cldrVersion"
        );
    }

    #[test]
    fn missing_names_or_code_mappings_are_errors_never_an_empty_table() {
        let no_names = r#"{"main": {"en": {"localeDisplayNames": {}}}}"#;
        assert_eq!(
            missing(MAPPINGS, no_names),
            (
                "territories.json",
                String::from("main.en.localeDisplayNames.territories")
            )
        );
        let no_codes = r#"{"supplemental": {"version": {"_cldrVersion": "49"}}}"#;
        assert_eq!(
            missing(no_codes, NAMES),
            (
                "codeMappings.json",
                String::from("supplemental.codeMappings")
            )
        );
    }
}
