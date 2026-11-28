// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The edition map `[edition]?fhir_cm=edition/[module]`: an extension's
//! concepts translated into the edition it depends on, as the tx-ecosystem IG
//! requires ("Servers SHALL correctly convert edition-specific concepts to the
//! International Edition", requirements, SNOMED CT).

use std::sync::Arc;

use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::operations::sources::Sources;
use fhir_terminology::operations::translate::{TranslateInput, translate};
use fhir_terminology::provider::concept_map::Relationship;
use fhir_terminology::registry::Registry;
use fhir_terminology::snomed::{SYSTEM, SnomedProvider};
use fhir_terminology::valueset::store::ValueSetStore;

use ferroterm_testkit::snomed;
use ferroterm_testkit::snomed::{
    ANIMAL, BIRD, CAT, COVERING, DEPENDED_DATE, DOG, FISH, FUR, LEGS, MODULE_CONCEPT, item, sctid,
};

fn code(ordinal: u32) -> String {
    sctid(item(ordinal))
}

/// The depended-on edition, as a translation names its target.
fn target() -> String {
    format!("{SYSTEM}|http://snomed.info/sct/{}", code(MODULE_CONCEPT))
}

/// The targets of `concept` in the depended-on edition, with their versions.
fn translated(concept: u32) -> Vec<(String, Relationship, Option<String>)> {
    let dir = tempfile::tempdir().expect("tempdir");
    snomed::write_with_extension(dir.path()).expect("writes the fixture");
    let provider = SnomedProvider::open(dir.path(), "en").expect("opens");
    let mut registry = Registry::default();
    registry.register(Arc::new(provider)).expect("registers");
    let value_sets = ValueSetStore::default();
    let concept_maps = ConceptMapStore::default();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let input = TranslateInput {
        system: Some(SYSTEM.to_owned()),
        code: Some(code(concept)),
        target_system: Some(target()),
        ..TranslateInput::default()
    };
    translate(&sources, &input)
        .expect("translates")
        .matches
        .into_iter()
        .filter_map(|found| {
            let concept = found.concept?;
            Some((concept.code?, found.relationship, concept.version))
        })
        .collect()
}

#[test]
fn a_concept_of_the_depended_edition_translates_to_itself_in_its_version() {
    let version = format!(
        "http://snomed.info/sct/{}/version/{DEPENDED_DATE}",
        code(MODULE_CONCEPT)
    );
    assert_eq!(
        translated(CAT),
        [(code(CAT), Relationship::Equivalent, Some(version))]
    );
}

#[test]
fn a_defined_extension_concept_translates_to_an_expression_over_the_depended_edition() {
    let found = translated(DOG);
    let [(expression, relationship, _)] = found.as_slice() else {
        panic!("one translation, not {found:?}");
    };
    assert_eq!(*relationship, Relationship::Equivalent);
    // The animal is the dog's proximal primitive supertype, and the dog's own
    // role groups refine it.
    assert_eq!(
        *expression,
        format!(
            "{}:{{{}={}}},{{{}=#4}}",
            code(ANIMAL),
            code(COVERING),
            code(FUR),
            code(LEGS)
        )
    );
}

#[test]
fn a_primitive_concept_under_a_top_level_concept_has_no_translation() {
    assert_eq!(translated(BIRD), []);
}

#[test]
fn an_inactive_concept_translates_through_its_historical_associations() {
    let found = translated(FISH);
    assert!(
        found
            .iter()
            .any(|(target, relationship, _)| *target == code(CAT)
                && *relationship == Relationship::Equivalent),
        "{found:?}"
    );
}
