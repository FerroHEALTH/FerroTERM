// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The LOINC Extension map `[edition]?fhir_cm=11010000107`, both ways, and
//! the implicit maps a `$translate` finds by its target system.

use std::sync::Arc;

use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::operations::sources::Sources;
use fhir_terminology::operations::translate::{TranslateInput, translate};
use fhir_terminology::provider::concept_map::Relationship;
use fhir_terminology::registry::Registry;
use fhir_terminology::snomed::{SYSTEM, SnomedProvider};
use fhir_terminology::valueset::store::ValueSetStore;

use ferroterm_testkit::snomed;
use ferroterm_testkit::snomed::{CAT, CAT_LOINC, DOG, VERSION, item, sctid};

const LOINC: &str = "http://loinc.org";

fn registry() -> (tempfile::TempDir, Registry) {
    let dir = tempfile::tempdir().expect("tempdir");
    snomed::write(dir.path()).expect("writes the fixture");
    let provider = SnomedProvider::open(dir.path(), "en").expect("opens");
    let mut registry = Registry::default();
    registry.register(Arc::new(provider)).expect("registers");
    (dir, registry)
}

/// The target codes and relationships of a translation.
fn translated(input: &TranslateInput) -> Vec<(String, Relationship)> {
    let (_dir, registry) = registry();
    let value_sets = ValueSetStore::default();
    let concept_maps = ConceptMapStore::default();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    translate(&sources, input)
        .expect("translates")
        .matches
        .into_iter()
        .filter_map(|found| Some((found.concept?.code?, found.relationship)))
        .collect()
}

fn map_url() -> String {
    format!("{VERSION}?fhir_cm=11010000107")
}

#[test]
fn the_map_translates_a_concept_to_its_loinc_code_and_back() {
    let to_loinc = TranslateInput {
        url: Some(map_url()),
        system: Some(SYSTEM.to_owned()),
        code: Some(sctid(item(CAT))),
        ..TranslateInput::default()
    };
    assert_eq!(
        translated(&to_loinc),
        [(CAT_LOINC.to_owned(), Relationship::Equivalent)]
    );
    let to_snomed = TranslateInput {
        url: Some(map_url()),
        system: Some(LOINC.to_owned()),
        code: Some(CAT_LOINC.to_owned()),
        ..TranslateInput::default()
    };
    assert_eq!(
        translated(&to_snomed),
        [(sctid(item(CAT)), Relationship::Equivalent)]
    );
    let unmapped = TranslateInput {
        url: Some(map_url()),
        system: Some(SYSTEM.to_owned()),
        code: Some(sctid(item(DOG))),
        ..TranslateInput::default()
    };
    assert_eq!(translated(&unmapped), []);
}

#[test]
fn a_translation_that_names_only_the_target_system_finds_the_map() {
    // "Where the request names no map, the server SHALL find the ConceptMaps
    // [...] that fit the request" (tx-ecosystem IG, requirements, $translate).
    let to_loinc = TranslateInput {
        system: Some(SYSTEM.to_owned()),
        code: Some(sctid(item(CAT))),
        target_system: Some(LOINC.to_owned()),
        ..TranslateInput::default()
    };
    assert_eq!(
        translated(&to_loinc),
        [(CAT_LOINC.to_owned(), Relationship::Equivalent)]
    );
    let to_snomed = TranslateInput {
        system: Some(LOINC.to_owned()),
        code: Some(CAT_LOINC.to_owned()),
        target_system: Some(format!("{SYSTEM}|{VERSION}")),
        ..TranslateInput::default()
    };
    assert_eq!(
        translated(&to_snomed),
        [(sctid(item(CAT)), Relationship::Equivalent)]
    );
    let other_version = TranslateInput {
        system: Some(LOINC.to_owned()),
        code: Some(CAT_LOINC.to_owned()),
        target_system: Some(format!(
            "{SYSTEM}|http://snomed.info/sct/1/version/20000101"
        )),
        ..TranslateInput::default()
    };
    assert_eq!(translated(&other_version), []);
}
