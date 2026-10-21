// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An openEHR archetype's local terminology, as the FHIR resources a producer
//! derives from it.
//!
//! An archetype constrains most of its coded fields with an archetype-local
//! `at`-code list, which no terminology server can address because nothing
//! publishes those lists as FHIR resources. A producer turns them into
//! ordinary resources: the `at`-codes of one archetype become a `CodeSystem`
//! whose concepts carry the rubric as the display and the description as the
//! definition, per language; an `ac`-code or an inline code list becomes a
//! `ValueSet` over it; and the archetype's `term_bindings` become a
//! `ConceptMap` onto whatever external system each binds to.
//!
//! **No openEHR specification defines a canonical URI for an archetype's local
//! terminology.** The archetype id is globally unique (the Archetype Object
//! Model 2 specification, §3.2 Archetype Identification), so a producer mints
//! the URL from it under a domain the producer owns. The fixture below mints
//! them under `example.org`, and nothing in this server reads the URL as
//! anything but an opaque canonical.
//!
//! The shape here is an archetype's, and the content is this project's own.

use std::path::Path;

/// The archetype whose local terminology this fixture carries.
///
/// The form is the one AOM 2 §3.2 fixes:
/// `<rm_originator>-<rm_name>-<rm_entity>.<concept>.v<version>`.
pub const ARCHETYPE: &str = "openEHR-EHR-OBSERVATION.ft_example.v1";

/// The `CodeSystem` of that archetype's `at`-codes.
pub const AT_CODES: &str =
    "http://example.org/fhir/CodeSystem/openEHR-EHR-OBSERVATION.ft_example.v1";

/// The `ValueSet` one `ac`-code of it constrains.
pub const AC_VALUE_SET: &str =
    "http://example.org/fhir/ValueSet/openEHR-EHR-OBSERVATION.ft_example.v1-ac0001";

/// The `ConceptMap` of that archetype's `term_bindings`.
pub const TERM_BINDINGS: &str =
    "http://example.org/fhir/ConceptMap/openEHR-EHR-OBSERVATION.ft_example.v1";

/// The external system one `at`-code binds to.
pub const BOUND_SYSTEM: &str = "http://example.org/fhir/CodeSystem/external";

/// The same archetype's codes, minted by a second deployer under its own
/// domain.
pub const AT_CODES_ELSEWHERE: &str =
    "https://other.example/terminology/CodeSystem/openEHR-EHR-OBSERVATION.ft_example.v1";

/// The archetype's `at`-codes as a `CodeSystem`.
///
/// The rubric of each node is the display and the description is the
/// definition, and the second language of the archetype is a designation, which
/// is how an archetype carries both.
fn at_codes() -> serde_json::Value {
    serde_json::json!({
      "resourceType": "CodeSystem", "language": "en",
      "url": AT_CODES, "version": "1", "name": "FtExampleLocalCodes",
      "title": "Local codes of openEHR-EHR-OBSERVATION.ft_example.v1",
      "status": "active", "content": "complete", "caseSensitive": true,
      "identifier": [{"system": "http://example.org/openehr/archetype", "value": ARCHETYPE}],
      "concept": [
        {"code": "at0000", "display": "Example observation",
         "definition": "The root node of the archetype."},
        {"code": "at0004", "display": "Any event",
         "definition": "An unspecified event."},
        {"code": "at0005", "display": "Steady",
         "definition": "The measurement was taken at rest.",
         "designation": [{"language": "nl", "value": "Rustig"}]},
        {"code": "at0006", "display": "Exertion",
         "definition": "The measurement was taken during exertion.",
         "designation": [{"language": "nl", "value": "Inspanning"}]}
      ]
    })
}

/// The `ac`-code as a `ValueSet` over two of those codes.
fn ac_value_set() -> serde_json::Value {
    serde_json::json!({
      "resourceType": "ValueSet",
      "url": AC_VALUE_SET, "version": "1", "name": "FtExampleExertionState",
      "title": "Exertion state of openEHR-EHR-OBSERVATION.ft_example.v1",
      "status": "active",
      "compose": {"include": [{"system": AT_CODES, "concept": [
        {"code": "at0005"}, {"code": "at0006"}
      ]}]}
    })
}

/// The external system one `at`-code binds to, so the map has a target.
fn bound_system() -> serde_json::Value {
    serde_json::json!({
      "resourceType": "CodeSystem", "language": "en",
      "url": BOUND_SYSTEM, "version": "1", "name": "FtExternal",
      "status": "active", "content": "complete", "caseSensitive": true,
      "concept": [{"code": "resting", "display": "At rest"}]
    })
}

/// The archetype's `term_bindings` as a `ConceptMap`.
fn term_bindings() -> serde_json::Value {
    serde_json::json!({
      "resourceType": "ConceptMap",
      "url": TERM_BINDINGS, "version": "1", "name": "FtExampleBindings",
      "status": "active",
      "sourceScopeUri": AT_CODES, "targetScopeUri": BOUND_SYSTEM,
      "group": [{
        "source": AT_CODES, "target": BOUND_SYSTEM,
        "element": [{"code": "at0005", "display": "Steady", "target": [
          {"code": "resting", "display": "At rest", "relationship": "equivalent"}
        ]}]
      }]
    })
}

/// The same archetype's `at`-codes as a second deployer would mint them.
///
/// The archetype is the same one; the domain is not. Two producers of the same
/// archetype mint two canonicals, and a server can hold both because a
/// canonical is what identifies a code system.
fn at_codes_elsewhere() -> serde_json::Value {
    serde_json::json!({
      "resourceType": "CodeSystem", "language": "en",
      "url": AT_CODES_ELSEWHERE, "version": "1", "name": "FtExampleLocalCodesElsewhere",
      "title": "Local codes of openEHR-EHR-OBSERVATION.ft_example.v1, as another deployer minted them",
      "status": "active", "content": "complete", "caseSensitive": true,
      "identifier": [{"system": "https://other.example/openehr/archetype", "value": ARCHETYPE}],
      "concept": [
        {"code": "at0005", "display": "Another deployer's rubric for the same node"}
      ]
    })
}

/// Writes the second deployer's `CodeSystem` into `dir`.
///
/// # Errors
///
/// Returns the I\/O error when a file cannot be written.
pub fn write_second_deployer(dir: &Path) -> std::io::Result<()> {
    std::fs::write(
        dir.join("CodeSystem-ft-example-elsewhere.json"),
        serde_json::to_string_pretty(&at_codes_elsewhere())?,
    )?;
    std::fs::write(
        dir.join("package.json"),
        serde_json::to_string_pretty(&serde_json::json!({
          "name": "other.openehr.ft-example",
          "version": "1.0.0",
          "fhirVersions": ["5.0.0"]
        }))?,
    )
}

/// Writes the four resources into `dir`, the way a producer publishes them.
///
/// # Errors
///
/// Returns the I/O error when a file cannot be written.
pub fn write_archetype_terminology(dir: &Path) -> std::io::Result<()> {
    for (name, value) in [
        ("CodeSystem-ft-example-at-codes.json", at_codes()),
        ("CodeSystem-ft-external.json", bound_system()),
        ("ValueSet-ft-example-ac0001.json", ac_value_set()),
        ("ConceptMap-ft-example-bindings.json", term_bindings()),
        // A producer publishes a FHIR package, and its manifest is what says
        // which release the resources are written in.
        (
            "package.json",
            serde_json::json!({
              "name": "example.openehr.ft-example",
              "version": "1.0.0",
              "fhirVersions": ["5.0.0"]
            }),
        ),
    ] {
        std::fs::write(dir.join(name), serde_json::to_string_pretty(&value)?)?;
    }
    Ok(())
}
