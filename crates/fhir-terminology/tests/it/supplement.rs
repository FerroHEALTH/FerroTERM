// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Supplements add designations and properties without touching the system.

use std::collections::BTreeMap;
use std::sync::Arc;

use fhir_terminology::filter::{Filter, FilterOperator};
use fhir_terminology::provider::{
    CodeSystemProvider, Designation, MapSelection, Property, PropertyValue, ProviderError,
};
use fhir_terminology::registries::bcp13::Bcp13Provider;
use fhir_terminology::snomed::{SYSTEM, SnomedProvider};
use fhir_terminology::supplement::{Additions, Supplement, Supplemented};

use ferroterm_testkit::snomed::{ANIMAL, CAT, FISH, ICD10_MAP_SCTID, PETS, item, sctid};

use crate::fixture::Fixture;

/// A supplement adding one Dutch designation to `code`.
fn dutch(url: &str, code: &str, term: &str) -> Supplement {
    Supplement {
        url: String::from(url),
        version: Some(String::from("1")),
        concepts: BTreeMap::from([(
            String::from(code),
            Additions {
                designations: vec![Designation {
                    standards_status: None,
                    language: Some(String::from("nl")),
                    use_: None,
                    value: String::from(term),
                }],
                properties: Vec::new(),
            },
        )]),
    }
}

fn filter(property: &str, op: FilterOperator, value: &str) -> Filter {
    Filter {
        property: property.to_owned(),
        op,
        value: value.to_owned(),
    }
}

#[test]
fn a_supplement_layers_designations_and_properties_over_the_system() {
    let inner: Arc<dyn CodeSystemProvider> = Arc::new(Fixture::hierarchical("2025"));
    let supplement = Supplement {
        url: String::from("http://example.org/fixture-de"),
        version: Some(String::from("1")),
        concepts: BTreeMap::from([(
            String::from("cat"),
            Additions {
                designations: vec![Designation {
                    standards_status: None,
                    language: Some(String::from("de")),
                    use_: None,
                    value: String::from("Katze"),
                }],
                properties: vec![Property {
                    code: String::from("colour"),
                    value: PropertyValue::String(String::from("any")),
                    ..Property::default()
                }],
            },
        )]),
    };
    let supplemented = Supplemented::new(inner, vec![supplement]);
    let cat = supplemented
        .locate("cat")
        .expect("reads")
        .expect("cat")
        .concept;
    let dog = supplemented
        .locate("dog")
        .expect("reads")
        .expect("dog")
        .concept;
    assert_eq!(
        supplemented
            .designations(cat, Some("de"))
            .expect("reads")
            .len(),
        1
    );
    assert_eq!(
        supplemented.designations(cat, None).expect("reads").len(),
        3
    );
    assert_eq!(
        supplemented.designations(dog, None).expect("reads").len(),
        2
    );
    assert_eq!(
        supplemented
            .display(cat, Some("de"))
            .expect("reads")
            .as_deref(),
        Some("Katze"),
        "a supplement designation in the requested language is the display \
         (<https://hl7.org/fhir/R4B/codesystem.html#supplements>, \
         <https://hl7.org/fhir/R4B/valueset-operation-expand.html> displayLanguage)"
    );
    assert_eq!(
        supplemented.display(cat, None).expect("reads").as_deref(),
        Some("Cat"),
        "without a language the system's display wins"
    );
    assert!(
        supplemented
            .properties(cat)
            .expect("reads")
            .iter()
            .any(|p| p.code == "colour")
    );
    assert!(
        !supplemented
            .properties(dog)
            .expect("reads")
            .iter()
            .any(|p| p.code == "colour")
    );
    assert_eq!(supplemented.declaration().languages, ["de", "en", "nl"]);
    assert_eq!(supplemented.identity().version, "2025");
    assert_eq!(supplemented.supplements().len(), 1);
}

#[test]
fn a_supplemented_system_answers_the_implicit_content_of_the_system_it_supplements() {
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::snomed::write(dir.path()).expect("writes the fixture");
    let inner: Arc<dyn CodeSystemProvider> =
        Arc::new(SnomedProvider::open(dir.path(), "en").expect("opens"));
    let supplemented = Supplemented::new(
        Arc::clone(&inner),
        vec![dutch(
            "http://example.org/fixture-nl",
            &sctid(item(CAT)),
            "Kat",
        )],
    );
    let cite = "naming a supplement expands `as if the supplements were included in the value set \
                definition` (<https://hl7.org/fhir/R5/valueset-operation-expand.html>, \
                `useSupplement`), so the implicit content is the system's own";
    for form in [
        String::from("fhir_vs"),
        format!("fhir_vs=isa/{}", sctid(item(ANIMAL))),
        String::from("fhir_vs=refset"),
    ] {
        let url = format!("{SYSTEM}?{form}");
        assert_eq!(
            supplemented
                .implicit_value_set(&url)
                .expect("the system claims the URI")
                .expect("a compose"),
            inner
                .implicit_value_set(&url)
                .expect("the system claims the URI")
                .expect("a compose"),
            "{cite}"
        );
        assert_eq!(
            supplemented.implicit_metadata(&url),
            inner.implicit_metadata(&url),
            "{cite}"
        );
    }
    let map = format!("{SYSTEM}?fhir_cm={ICD10_MAP_SCTID}");
    assert_eq!(
        supplemented
            .implicit_concept_map(&map, MapSelection::Whole)
            .expect("the system claims the URI")
            .expect("a map"),
        inner
            .implicit_concept_map(&map, MapSelection::Whole)
            .expect("the system claims the URI")
            .expect("a map"),
        "{cite}"
    );
    let fish = supplemented
        .locate(&sctid(item(FISH)))
        .expect("reads")
        .expect("fish")
        .concept;
    assert_eq!(
        supplemented.successors(fish).expect("reads"),
        inner.successors(fish).expect("reads"),
        "{cite}"
    );
    assert_eq!(
        supplemented.inactive().expect("reads"),
        inner.inactive().expect("reads"),
        "{cite}"
    );
    let cat = supplemented
        .locate(&sctid(item(CAT)))
        .expect("reads")
        .expect("cat")
        .concept;
    assert_eq!(
        supplemented
            .display(cat, Some("nl"))
            .expect("reads")
            .as_deref(),
        Some("Kat"),
        "the supplement still adds its designation"
    );
}

#[test]
fn a_supplemented_registry_still_expands_the_include_its_filters_bound() {
    let inner: Arc<dyn CodeSystemProvider> = Arc::new(Bcp13Provider::new());
    let supplemented = Supplemented::new(
        Arc::clone(&inner),
        vec![dutch(
            "http://example.org/bcp13-nl",
            "text/plain",
            "Platte tekst",
        )],
    );
    let bounded = [
        filter("registered", FilterOperator::Equal, "true"),
        filter("base", FilterOperator::Equal, "text/plain"),
    ];
    assert_eq!(
        supplemented.filter_all(&bounded).expect("enumerates").len(),
        inner.filter_all(&bounded).expect("enumerates").len(),
        "a supplement adds designations and properties \
         (<https://hl7.org/fhir/R4B/codesystem.html>), so the filters that bound the selection \
         bound it still"
    );
    assert!(
        matches!(
            supplemented.filter_all(&[filter("base", FilterOperator::Equal, "text")]),
            Err(ProviderError::NotEnumerable)
        ),
        "the refusal of an unbounded selection is the system's own too"
    );
    let plain = supplemented
        .locate("text/plain")
        .expect("reads")
        .expect("registered")
        .concept;
    assert!(
        supplemented
            .filter_matches(plain, &filter("base", FilterOperator::Equal, "text"))
            .expect("answers"),
        "the system decides what its filters match"
    );
    assert_eq!(
        supplemented
            .display(plain, Some("nl"))
            .expect("reads")
            .as_deref(),
        Some("Platte tekst"),
        "the supplement still adds its designation"
    );
}

#[test]
fn a_supplemented_system_declares_and_resolves_the_forms_of_the_system_it_supplements() {
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::snomed::write(dir.path()).expect("writes the fixture");
    let inner: Arc<dyn CodeSystemProvider> =
        Arc::new(SnomedProvider::open(dir.path(), "en").expect("opens"));
    let supplemented = Supplemented::new(
        Arc::clone(&inner),
        vec![dutch(
            "http://example.org/fixture-nl",
            &sctid(item(CAT)),
            "Kat",
        )],
    );
    assert_eq!(
        supplemented.declaration().implicit_forms,
        inner.declaration().implicit_forms
    );
    let animal = sctid(item(ANIMAL));
    let pets = sctid(item(PETS));
    let ecl = format!("%3C%3C%20{animal}");
    crate::implicit_forms::every_form_resolves(
        &supplemented,
        &[
            ("http://snomed.info/sct?fhir_vs", &[]),
            ("http://snomed.info/sct?fhir_vs=isa/[sctid]", &[&animal]),
            ("http://snomed.info/sct?fhir_vs=refset", &[]),
            ("http://snomed.info/sct?fhir_vs=refset/[sctid]", &[&pets]),
            ("http://snomed.info/sct?fhir_vs=ecl/[ecl]", &[&ecl]),
        ],
    );
}
