// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one correction this service's FHIR resource files need.
//!
//! FHIR declares `experimental` as a `boolean`, and a boolean is written in
//! JSON as `true` or `false` with no quotes
//! (<https://hl7.org/fhir/R4/datatypes.html#boolean>,
//! <https://hl7.org/fhir/R4/json.html>). One of the service's maps publishes
//! it as the string `"true"`, which a strict reader refuses, so the add-on
//! normalizes it before the file is handed on and records that it did.
//!
//! Every correction is reported. A file that needs none is passed through as
//! the bytes that arrived, so a run record can tell a corrected file from an
//! untouched one by the list alone.

use core::fmt;

/// What one correction changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixupKind {
    /// An `experimental` element arrived as a JSON string.
    ExperimentalString {
        /// The string the file carried.
        was: String,
        /// The boolean it now carries.
        now: bool,
    },
}

impl fmt::Display for FixupKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExperimentalString { was, now } => {
                write!(f, "experimental was the string \"{was}\" and is now {now}")
            }
        }
    }
}

/// One correction a file needed, and where it was applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixup {
    /// The JSON Pointer of the corrected element (RFC 6901,
    /// <https://www.rfc-editor.org/rfc/rfc6901>).
    pub pointer: String,
    /// What the correction changed.
    pub kind: FixupKind,
}

impl fmt::Display for Fixup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.pointer, self.kind)
    }
}

/// A file that could not be corrected.
#[derive(Debug, thiserror::Error)]
pub enum FixupError {
    /// The file is not JSON, so it carries no FHIR resource to correct.
    #[error("the file is not JSON")]
    NotJson {
        /// Why the bytes could not be read as JSON.
        #[source]
        source: serde_json::Error,
    },
    /// An `experimental` element is a string that is neither boolean value.
    #[error("{pointer} is the string \"{value}\", which is no boolean")]
    Experimental {
        /// The JSON Pointer of the element.
        pointer: String,
        /// The string the file carried.
        value: String,
    },
    /// The corrected resource could not be written back to JSON.
    #[error("the corrected resource could not be serialized")]
    Serialize {
        /// Why serialization failed.
        #[source]
        source: serde_json::Error,
    },
}

/// A file after the corrections, and the corrections that were applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedUp {
    /// The bytes to hand on.
    pub bytes: Vec<u8>,
    /// The corrections that were applied, in document order.
    pub fixups: Vec<Fixup>,
}

/// Escapes one JSON Pointer reference token (RFC 6901 §3).
fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// Corrects one FHIR resource file, reporting what it changed.
///
/// A file that needs no correction comes back byte-identical, because the
/// bytes that arrived are returned unread. A corrected file is written back
/// from the parsed document, so its member order is the one `serde_json` writes rather than
/// the order it arrived in; JSON object members are unordered
/// (<https://www.rfc-editor.org/rfc/rfc8259#section-4>).
///
/// # Errors
///
/// Returns [`FixupError::NotJson`] when the bytes are not a JSON document,
/// [`FixupError::Experimental`] when an `experimental` element is a string
/// that is neither `"true"` nor `"false"`, and [`FixupError::Serialize`] when
/// the corrected document cannot be written back.
pub fn normalize(bytes: &[u8]) -> Result<FixedUp, FixupError> {
    let mut document: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|source| FixupError::NotJson { source })?;
    let mut fixups = Vec::new();
    correct(&mut document, "", &mut fixups)?;
    if fixups.is_empty() {
        return Ok(FixedUp {
            bytes: bytes.to_vec(),
            fixups,
        });
    }
    let corrected =
        serde_json::to_vec(&document).map_err(|source| FixupError::Serialize { source })?;
    Ok(FixedUp {
        bytes: corrected,
        fixups,
    })
}

/// Walks one JSON value, correcting every `experimental` string under it.
fn correct(
    value: &mut serde_json::Value,
    pointer: &str,
    fixups: &mut Vec<Fixup>,
) -> Result<(), FixupError> {
    match value {
        serde_json::Value::Object(members) => {
            for (name, member) in members.iter_mut() {
                let child = format!("{pointer}/{}", escape(name));
                if name == "experimental"
                    && let serde_json::Value::String(text) = member
                {
                    let now = match text.as_str() {
                        "true" => true,
                        "false" => false,
                        other => {
                            return Err(FixupError::Experimental {
                                pointer: child,
                                value: other.to_owned(),
                            });
                        }
                    };
                    fixups.push(Fixup {
                        pointer: child,
                        kind: FixupKind::ExperimentalString {
                            was: text.clone(),
                            now,
                        },
                    });
                    *member = serde_json::Value::Bool(now);
                    continue;
                }
                correct(member, &child, fixups)?;
            }
        }
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                correct(item, &format!("{pointer}/{index}"), fixups)?;
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Fixup, FixupError, FixupKind, normalize};

    #[test]
    fn a_string_experimental_becomes_the_boolean_and_is_recorded() {
        let file = br#"{"resourceType":"ConceptMap","experimental":"true"}"#;
        let fixed = normalize(file).expect("the resource reads");
        assert_eq!(
            fixed.fixups,
            vec![Fixup {
                pointer: String::from("/experimental"),
                kind: FixupKind::ExperimentalString {
                    was: String::from("true"),
                    now: true,
                },
            }],
            "the correction is reported with its pointer"
        );
        let document: serde_json::Value =
            serde_json::from_slice(&fixed.bytes).expect("the corrected resource reads");
        assert_eq!(
            document.get("experimental"),
            Some(&serde_json::Value::Bool(true)),
            "experimental is the boolean FHIR declares"
        );
    }

    #[test]
    fn a_resource_that_needs_nothing_is_byte_identical() {
        let file = br#"{"resourceType":"CodeSystem","experimental":false,"name":"Example"}"#;
        let fixed = normalize(file).expect("the resource reads");
        assert!(fixed.fixups.is_empty(), "nothing needed correcting");
        assert_eq!(
            fixed.bytes,
            file.to_vec(),
            "an untouched file keeps its bytes"
        );
    }

    #[test]
    fn a_nested_resource_is_corrected_too() {
        let file = br#"{"resourceType":"Bundle","entry":[{"resource":{"experimental":"false"}}]}"#;
        let fixed = normalize(file).expect("the bundle reads");
        assert_eq!(
            fixed.fixups.first().map(|fixup| fixup.pointer.as_str()),
            Some("/entry/0/resource/experimental"),
            "the pointer names the element inside the bundle"
        );
    }

    #[test]
    fn an_experimental_string_that_is_no_boolean_is_refused() {
        let file = br#"{"resourceType":"CodeSystem","experimental":"yes"}"#;
        let error = normalize(file).expect_err("the value cannot be normalized");
        assert!(
            matches!(error, FixupError::Experimental { ref value, .. } if value == "yes"),
            "the refusal names the value: {error}"
        );
    }

    #[test]
    fn an_archive_is_not_a_resource_file() {
        let error = normalize(b"PK\x03\x04").expect_err("a zip is not JSON");
        assert!(
            matches!(error, FixupError::NotJson { .. }),
            "the refusal says the file is not JSON: {error}"
        );
    }
}
