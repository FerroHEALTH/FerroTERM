// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Labcodeset resources the build writes load as a value set, a LOINC
//! supplement, and ordinal value sets.

use ferroterm_testkit::labcodeset::{GLUCOSE, ORDINAL_OID, write_resources};
use fhir_terminology::fhir_codesystem::load::{FhirVersion, load_dir, package_version};
use fhir_terminology::valueset;

#[test]
fn the_built_resources_load_as_a_supplement_and_value_sets() {
    let dir = tempfile::tempdir().expect("tempdir");
    let resources = write_resources(dir.path()).expect("builds");
    assert_eq!(
        package_version(&resources).expect("reads"),
        Some(FhirVersion::R4B)
    );
    let systems = load_dir(&resources, FhirVersion::R4B).expect("loads");
    assert_eq!(systems.len(), 1);
    let supplement = &systems[0];
    assert_eq!(supplement.supplements.as_deref(), Some("http://loinc.org"));
    assert_eq!(supplement.concepts.len(), 3);
    assert!(
        supplement.concepts.iter().any(|c| c.code == GLUCOSE
            && c.designations
                .iter()
                .any(|d| d.value.starts_with("glucose"))),
        "the Dutch name is a designation"
    );
    let value_sets = valueset::load::load_dir(&resources, FhirVersion::R4B).expect("loads");
    assert_eq!(value_sets.len(), 2);
    let labcodeset = value_sets
        .iter()
        .find(|v| v.url == "https://ferroterm.eu/fhir/ValueSet/nl-labcodeset")
        .expect("the Labcodeset value set");
    assert_eq!(labcodeset.language.as_deref(), Some("nl-NL"));
    assert_eq!(labcodeset.compose.include[0].concepts.len(), 2);
    let ordinal = value_sets
        .iter()
        .find(|v| v.url == format!("urn:oid:{ORDINAL_OID}"))
        .expect("the ordinal value set");
    assert_eq!(
        ordinal.compose.include[0]
            .system
            .as_ref()
            .map(|s| s.url.as_str()),
        Some("http://snomed.info/sct")
    );
}
