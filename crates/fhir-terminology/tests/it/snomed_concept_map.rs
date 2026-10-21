// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SNOMED CT implicit concept maps `?fhir_cm=[sctid]` and the historical
//! associations `$translate` falls back to
//! (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Concept Maps").

use std::sync::Arc;

use fhir_terminology::conceptmap::model::{ConceptMapModel, Relationship};
use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::operations::translate::{Match, TranslateInput, Translation, translate};
use fhir_terminology::operations::{OperationError, Sources};
use fhir_terminology::provider::{Capability, MapSelection, ProviderError};
use fhir_terminology::registry::Registry;
use fhir_terminology::snomed::{SYSTEM, SnomedProvider};
use fhir_terminology::valueset::store::ValueSetStore;

use ferroterm_testkit::scaled;
use ferroterm_testkit::snomed;
use ferroterm_testkit::snomed::{
    ALTERNATIVE_SCTID, CAT, CODES_MAP, DOG, FISH, ICD10_MAP_SCTID, ICD10_SYSTEM,
    POSSIBLY_EQUIVALENT_TO_SCTID, REPLACED_BY_SCTID, SAME_AS_SCTID, VERSION, item, sctid,
};

/// The edition, and a registry holding it.
struct World {
    _dir: tempfile::TempDir,
    registry: Registry,
    value_sets: ValueSetStore,
    concept_maps: ConceptMapStore,
}

impl World {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        snomed::write(dir.path()).expect("writes the fixture");
        let provider = SnomedProvider::open(dir.path(), "en").expect("opens");
        let mut registry = Registry::new();
        registry.register(Arc::new(provider)).expect("registers");
        Self {
            _dir: dir,
            registry,
            value_sets: ValueSetStore::new(),
            concept_maps: ConceptMapStore::new(),
        }
    }

    fn sources(&self) -> Sources<'_> {
        Sources {
            registry: &self.registry,
            value_sets: &self.value_sets,
            concept_maps: &self.concept_maps,
        }
    }

    /// `$translate` of `code` through the map `url` names, or through none.
    fn translate(&self, url: Option<&str>, code: &str) -> Vec<Match> {
        let input = TranslateInput {
            url: url.map(str::to_owned),
            code: Some(code.to_owned()),
            system: Some(String::from(SYSTEM)),
            ..TranslateInput::default()
        };
        translate(&self.sources(), &input)
            .expect("translates")
            .matches
    }
}

/// The code of the fixture concept at `ordinal`.
fn code(ordinal: u32) -> String {
    sctid(item(ordinal))
}

/// The implicit concept map URI of the reference set `refset`.
fn map_url(refset: &str) -> String {
    format!("{SYSTEM}?fhir_cm={refset}")
}

/// The target code and relationship of the one match.
fn only(matches: &[Match]) -> (&str, Relationship) {
    let found = match matches {
        [found] => found,
        other => panic!("one match, not {}", other.len()),
    };
    (
        found
            .concept
            .as_ref()
            .and_then(|concept| concept.code.as_deref())
            .expect("a target code"),
        found.relationship,
    )
}

#[test]
fn the_edition_declares_that_it_defines_implicit_concept_maps() {
    let world = World::new();
    let resolved = world.registry.resolve(SYSTEM, None).expect("snomed");
    assert!(
        resolved
            .provider
            .declaration()
            .capabilities
            .contains(&Capability::ImplicitConceptMaps)
    );
}

#[test]
fn each_association_reference_set_translates_with_the_relationship_the_page_gives_it() {
    let world = World::new();
    // The four rows of the FHIR SNOMED CT page's table, each on its own; the fish
    // is inactive and every association points from it
    // (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Concept Maps").
    for (refset, target, relationship) in [
        (SAME_AS_SCTID, CAT, Relationship::Equal),
        (REPLACED_BY_SCTID, DOG, Relationship::Equivalent),
        (POSSIBLY_EQUIVALENT_TO_SCTID, DOG, Relationship::Inexact),
        (ALTERNATIVE_SCTID, CAT, Relationship::Inexact),
    ] {
        let matches = world.translate(Some(&map_url(refset)), &code(FISH));
        assert_eq!(
            only(&matches),
            (code(target).as_str(), relationship),
            "{refset} translates the fish"
        );
    }
}

#[test]
fn an_association_map_carries_the_page_s_template() {
    let world = World::new();
    let resolved = world.registry.resolve(SYSTEM, None).expect("snomed");
    let url = map_url(SAME_AS_SCTID);
    let map = resolved
        .provider
        .implicit_concept_map(&url, MapSelection::Whole)
        .expect("an implicit map")
        .expect("it builds");
    assert_eq!(map.url, url);
    assert_eq!(map.version.as_deref(), Some(VERSION));
    assert_eq!(map.status, "active");
    assert_eq!(
        map.source_scope.as_deref(),
        Some(format!("{VERSION}?fhir_vs").as_str()),
        "the template scopes the map to the edition's own implicit value set"
    );
    assert_eq!(map.source_scope, map.target_scope);
    assert!(
        map.name
            .as_deref()
            .is_some_and(|name| name.starts_with("SNOMED CT ")),
        "the template names the map after the reference set: {:?}",
        map.name
    );
    let group = map.groups.first().expect("one group");
    assert_eq!(group.source.as_deref(), Some(SYSTEM));
    assert_eq!(
        group.target.as_deref(),
        Some(SYSTEM),
        "an association maps SNOMED to SNOMED"
    );
    assert_eq!(group.source_version.as_deref(), Some(VERSION));
    assert_eq!(group.target_version.as_deref(), Some(VERSION));
}

#[test]
fn a_map_reference_set_translates_to_its_target_code_with_the_rf2_columns() {
    let world = World::new();
    let matches = world.translate(Some(&map_url(ICD10_MAP_SCTID)), &code(CAT));
    let found = match matches.as_slice() {
        [found] => found,
        other => panic!("one match, not {}", other.len()),
    };
    let concept = found.concept.as_ref().expect("a target concept");
    assert_eq!(concept.code.as_deref(), Some("C01"));
    // The group states the target system, so the match carries it
    // (<https://hl7.org/fhir/R4B/conceptmap-definitions.html#ConceptMap.group.target>).
    assert_eq!(concept.system.as_deref(), Some(ICD10_SYSTEM));
    assert_eq!(
        concept.version, None,
        "RF2 records no version of the scheme a mapTarget code comes from"
    );
    let parts: Vec<&str> = found
        .products
        .iter()
        .map(|product| product.element.as_str())
        .collect();
    assert_eq!(
        parts,
        [
            "mapGroup",
            "mapPriority",
            "mapRule",
            "mapAdvice",
            "correlationId"
        ],
        "every RF2 map column the row carries travels as a product part"
    );
    let advice = found
        .products
        .iter()
        .find(|product| product.element == "mapAdvice")
        .and_then(|product| product.concept.code.as_deref());
    assert_eq!(advice, Some("ALWAYS C01"));
}

#[test]
fn a_map_reference_set_names_the_code_system_its_targets_belong_to() {
    let world = World::new();
    let resolved = world.registry.resolve(SYSTEM, None).expect("snomed");
    let map = resolved
        .provider
        .implicit_concept_map(&map_url(ICD10_MAP_SCTID), MapSelection::Whole)
        .expect("an implicit map")
        .expect("it builds");
    let group = map.groups.first().expect("one group");
    assert_eq!(group.source.as_deref(), Some(SYSTEM));
    assert_eq!(group.source_version.as_deref(), Some(VERSION));
    // R4B: `group.target` is the absolute URI of the target system, needed unless
    // the target value set names one system or every target is `unmatched`, and an
    // implicit map states neither
    // (<https://hl7.org/fhir/R4B/conceptmap-definitions.html#ConceptMap.group.target>).
    assert_eq!(
        group.target.as_deref(),
        Some(ICD10_SYSTEM),
        "the reference set says which scheme its mapTarget codes come from"
    );
    assert_eq!(
        group.target_version, None,
        "RF2 records no version of that scheme"
    );
}

#[test]
fn a_map_reference_set_of_an_unnamed_scheme_is_refused() {
    let world = World::new();
    let resolved = world.registry.resolve(SYSTEM, None).expect("snomed");
    let url = map_url(&code(CODES_MAP));
    // A map reference set the server has no code system URI for cannot state
    // `group.target`, so it is refused rather than answered with a target Coding
    // that carries a code and no system
    // (<https://hl7.org/fhir/R4B/conceptmap-definitions.html#ConceptMap.group.target>).
    assert!(
        matches!(
            resolved
                .provider
                .implicit_concept_map(&url, MapSelection::Whole),
            Some(Err(ProviderError::UnnamedConceptMapTarget { .. }))
        ),
        "an unrecognised map reference set names no target system"
    );
    let input = TranslateInput {
        url: Some(url),
        code: Some(code(CAT)),
        system: Some(String::from(SYSTEM)),
        ..TranslateInput::default()
    };
    assert!(matches!(
        translate(&world.sources(), &input),
        Err(OperationError::NotSupported(_))
    ));
}

#[test]
fn translating_an_inactive_concept_without_a_map_names_its_successors() {
    let world = World::new();
    let matches = world.translate(None, &code(FISH));
    let mut found: Vec<(&str, Relationship)> = matches
        .iter()
        .map(|held| {
            (
                held.concept
                    .as_ref()
                    .and_then(|concept| concept.code.as_deref())
                    .expect("a target code"),
                held.relationship,
            )
        })
        .collect();
    found.sort_unstable_by(|left, right| left.0.cmp(right.0));
    let (cat, dog) = (code(CAT), code(DOG));
    let mut wanted = vec![
        (cat.as_str(), Relationship::Equal),
        (dog.as_str(), Relationship::Equivalent),
    ];
    wanted.sort_unstable_by(|left, right| left.0.cmp(right.0));
    assert_eq!(found, wanted, "SAME AS and REPLACED BY answer for the fish");
    assert!(
        matches.iter().all(|held| held
            .source
            .as_deref()
            .is_some_and(|url| url.contains("fhir_cm"))),
        "each match names the implicit map it came from"
    );
}

#[test]
fn translating_an_active_concept_without_a_map_names_no_successor() {
    let world = World::new();
    assert!(
        world.translate(None, &code(CAT)).is_empty(),
        "only an inactive concept has successors"
    );
}

#[test]
fn an_unknown_reference_set_is_not_found_and_a_malformed_one_is_invalid() {
    let world = World::new();
    let input = |url: &str| TranslateInput {
        url: Some(url.to_owned()),
        code: Some(code(FISH)),
        system: Some(String::from(SYSTEM)),
        ..TranslateInput::default()
    };
    // NOTE: no specification fixes the outcome for a `?fhir_cm=` the server cannot
    // answer; `not-found` is what tx.fhir.org and Snowstorm both return.
    assert!(matches!(
        translate(&world.sources(), &input(&map_url(&code(CAT)))),
        Err(OperationError::UnknownConceptMap(_))
    ));
    assert!(matches!(
        translate(&world.sources(), &input(&map_url("not-an-sctid"))),
        Err(OperationError::Invalid(_))
    ));
    // The base may be any edition version (<https://hl7.org/fhir/R4B/snomedct.html>,
    // "Implicit Concept Maps"), so one the server does not hold is that version
    // missing, not a malformed URI.
    assert!(
        matches!(
            translate(
                &world.sources(),
                &input("http://snomed.info/sct/999?fhir_cm=900000000000527005")
            ),
            Err(OperationError::UnknownVersion { ref url, ref version, .. })
                if url == SYSTEM && version == "http://snomed.info/sct/999"
        ),
        "an edition the server does not serve is not found"
    );
}

#[test]
fn a_reference_set_that_maps_nothing_is_not_a_concept_map() {
    let world = World::new();
    let resolved = world.registry.resolve(SYSTEM, None).expect("snomed");
    let pets = code(snomed::PETS);
    assert!(
        matches!(
            resolved
                .provider
                .implicit_concept_map(&map_url(&pets), MapSelection::Whole),
            Some(Err(ProviderError::UnknownImplicitConceptMap { .. }))
        ),
        "a plain reference set has neither a target component nor a map target"
    );
}

#[test]
fn a_value_set_uri_is_not_a_concept_map_uri() {
    let world = World::new();
    let resolved = world.registry.resolve(SYSTEM, None).expect("snomed");
    assert!(
        resolved
            .provider
            .implicit_concept_map(&format!("{SYSTEM}?fhir_vs"), MapSelection::Whole)
            .is_none()
    );
    assert!(
        resolved
            .provider
            .implicit_value_set(&map_url(SAME_AS_SCTID))
            .is_none()
    );
}
/// A generated edition and the implicit concept map of its ICD-10 extended map
/// reference set, for the narrowing a translation asks for.
///
/// The map has one row per concept, so it grows with the edition the way a
/// release's own map reference set does, and a translation that reads it whole
/// costs the edition rather than the answer.
struct MappedWorld {
    _dir: tempfile::TempDir,
    registry: Registry,
    value_sets: ValueSetStore,
    concept_maps: ConceptMapStore,
    url: String,
}

impl MappedWorld {
    fn new(concepts: u32) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        scaled::write(dir.path(), concepts).expect("writes the edition");
        let provider = SnomedProvider::open(dir.path(), "en").expect("opens");
        let mut registry = Registry::new();
        registry.register(Arc::new(provider)).expect("registers");
        Self {
            _dir: dir,
            registry,
            value_sets: ValueSetStore::new(),
            concept_maps: ConceptMapStore::new(),
            url: map_url(ICD10_MAP_SCTID),
        }
    }

    fn sources(&self) -> Sources<'_> {
        Sources {
            registry: &self.registry,
            value_sets: &self.value_sets,
            concept_maps: &self.concept_maps,
        }
    }

    /// The map the provider builds for `selection`.
    fn map(&self, selection: MapSelection<'_>) -> ConceptMapModel {
        self.registry
            .resolve(SYSTEM, None)
            .expect("snomed")
            .provider
            .implicit_concept_map(&self.url, selection)
            .expect("an implicit map")
            .expect("it builds")
    }

    /// The number of elements the map `selection` asks for carries.
    fn elements(&self, selection: MapSelection<'_>) -> usize {
        self.map(selection)
            .groups
            .first()
            .expect("one group")
            .elements
            .len()
    }

    /// Stores the whole map, so `$translate` reads it as a stored resource and
    /// never narrows it.
    fn store_the_whole_map(mut self) -> Self {
        self.concept_maps.replace(self.map(MapSelection::Whole));
        self
    }

    /// `$translate` of `code` of `system` through the map `url` names.
    fn translate(&self, system: &str, code: &str, reverse: bool) -> Translation {
        let input = TranslateInput {
            url: Some(self.url.clone()),
            code: Some(code.to_owned()),
            system: Some(system.to_owned()),
            reverse: reverse.then_some(true),
            ..TranslateInput::default()
        };
        translate(&self.sources(), &input).expect("translates")
    }
}

/// The map `whole` states with only the elements whose own code is `code`.
fn from_code(whole: &ConceptMapModel, code: &str) -> ConceptMapModel {
    let mut restricted = whole.clone();
    for group in &mut restricted.groups {
        group
            .elements
            .retain(|element| element.code.as_deref() == Some(code));
    }
    restricted
}

/// The map `whole` states with only the mappings that target `code`.
fn to_code(whole: &ConceptMapModel, code: &str) -> ConceptMapModel {
    let mut restricted = whole.clone();
    for group in &mut restricted.groups {
        for element in &mut group.elements {
            element
                .targets
                .retain(|target| target.code.as_deref() == Some(code));
        }
        group.elements.retain(|element| !element.targets.is_empty());
    }
    restricted
}

#[test]
fn the_map_narrowed_to_a_source_code_is_the_whole_map_restricted_to_it() {
    let world = MappedWorld::new(300);
    let whole = world.map(MapSelection::Whole);
    assert_eq!(
        world.elements(MapSelection::Whole),
        300,
        "the whole map states one element per mapped concept"
    );
    // The narrowed map differs from the whole one in the elements it carries and
    // in nothing else, so a translation reads the same map either way.
    for ordinal in [0_u32, 1, 7, 42, 299] {
        let code = scaled::code(ordinal);
        assert_eq!(
            world.map(MapSelection::Source(&code)),
            from_code(&whole, &code),
            "the map narrowed to {code} is the whole map restricted to it"
        );
    }
    let unmapped = scaled::code(4_000);
    assert_eq!(
        world.map(MapSelection::Source(&unmapped)),
        from_code(&whole, &unmapped),
        "a code the edition does not hold narrows to a map with no element"
    );
}

#[test]
fn the_map_narrowed_to_a_target_code_is_the_whole_map_restricted_to_it() {
    let world = MappedWorld::new(300);
    let whole = world.map(MapSelection::Whole);
    for ordinal in [0_u32, 1, 7, 42, 299] {
        let target = scaled::map_target(ordinal);
        assert_eq!(
            world.map(MapSelection::Target(&target)),
            to_code(&whole, &target),
            "the map narrowed to the target {target} is the whole map restricted to it"
        );
    }
    let absent = String::from("X99999");
    assert_eq!(
        world.map(MapSelection::Target(&absent)),
        to_code(&whole, &absent),
        "a target no row carries narrows to a map with no element"
    );
}

#[test]
fn translating_reads_the_narrowed_map_and_answers_what_the_whole_map_answers() {
    let narrowed = MappedWorld::new(300);
    let stored = MappedWorld::new(300).store_the_whole_map();
    // A code the map states a target for, one it states nothing for, and one the
    // edition does not hold: the narrowed map answers each the way the whole one
    // does, absence included.
    let codes = [0_u32, 1, 7, 42, 299]
        .map(scaled::code)
        .into_iter()
        .chain([scaled::code(4_000), String::from("not-an-sctid")]);
    for code in codes {
        assert_eq!(
            narrowed.translate(SYSTEM, &code, false),
            stored.translate(SYSTEM, &code, false),
            "translating {code} answers the same over the narrowed and the whole map"
        );
    }
}

#[test]
fn a_reverse_translation_reads_the_narrowed_map_and_answers_the_same() {
    let narrowed = MappedWorld::new(300);
    let stored = MappedWorld::new(300).store_the_whole_map();
    for ordinal in [0_u32, 1, 7, 42, 299] {
        let target = scaled::map_target(ordinal);
        assert_eq!(
            narrowed.translate(ICD10_SYSTEM, &target, true),
            stored.translate(ICD10_SYSTEM, &target, true),
            "translating {target} back answers the same over the narrowed and the whole map"
        );
    }
    assert_eq!(
        narrowed.translate(ICD10_SYSTEM, "X99999", true),
        stored.translate(ICD10_SYSTEM, "X99999", true),
        "a target no row carries answers the same either way"
    );
}

#[test]
fn a_translation_reads_the_same_map_however_large_the_reference_set_is() {
    // The elements a translation reads are the answer's, so the map it builds is
    // the same size over an edition seven times as large: the cost follows the
    // answer rather than the release.
    let small = MappedWorld::new(300);
    let large = MappedWorld::new(2_100);
    let code = scaled::code(42);
    assert_eq!(
        small.elements(MapSelection::Source(&code)),
        1,
        "one code translates through one element"
    );
    assert_eq!(
        large.elements(MapSelection::Source(&code)),
        small.elements(MapSelection::Source(&code)),
        "the narrowed map holds the answer, never the reference set"
    );
    assert_eq!(
        large.elements(MapSelection::Whole),
        2_100,
        "the whole map still states every mapping the reference set holds"
    );
    assert_eq!(
        small.translate(SYSTEM, &code, false).matches,
        large.translate(SYSTEM, &code, false).matches,
        "and both editions translate the code the same way"
    );
}
