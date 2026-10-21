// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The generic `CodeSystem` provider over the synthetic R5 resources.

use std::sync::Arc;

use concept_graph::subsumption::Outcome;
use ferroterm_testkit::fhir::{ANIMALS, ANIMALS_NL, COLOURS, SKETCH, write_code_systems};
use fhir_terminology::fhir_codesystem::load::{FhirVersion, load_dir, package_version};
use fhir_terminology::fhir_codesystem::provider::FhirCodeSystem;
use fhir_terminology::filter::{Filter, FilterOperator};
use fhir_terminology::operations::{Invocation, OperationError, lookup, validate_code};
use fhir_terminology::provider::{
    Capability, CodeSystemProvider, ContentMode, PropertyValue, ProviderError,
};
use fhir_terminology::registry::Registry;
use fhir_terminology::supplement::{Additions, Supplement, Supplemented};

fn load_all() -> (tempfile::TempDir, Vec<FhirCodeSystem>) {
    let dir = tempfile::tempdir().expect("tempdir");
    write_code_systems(dir.path()).expect("writes");
    assert_eq!(
        package_version(dir.path()).expect("reads"),
        Some(FhirVersion::R5),
        "the package names its FHIR version"
    );
    let models = load_dir(dir.path(), FhirVersion::R5).expect("loads");
    let providers = models
        .into_iter()
        .map(|m| FhirCodeSystem::new(m).expect("builds"))
        .collect();
    (dir, providers)
}

fn find<'a>(providers: &'a [FhirCodeSystem], url: &str) -> &'a FhirCodeSystem {
    providers
        .iter()
        .find(|p| p.identity().url == url)
        .expect("system")
}

fn codes(p: &dyn CodeSystemProvider, set: &roaring::RoaringBitmap) -> Vec<String> {
    let mut out: Vec<String> = set
        .iter()
        .filter_map(|i| {
            p.code(fhir_terminology::provider::Concept::new(i))
                .expect("reads")
        })
        .collect();
    out.sort();
    out
}

#[test]
fn the_directory_loads_every_code_system_and_skips_other_resources() {
    let (_dir, providers) = load_all();
    let mut urls: Vec<&str> = providers
        .iter()
        .map(|p| p.identity().url.as_str())
        .collect();
    urls.sort_unstable();
    assert_eq!(urls, [ANIMALS, ANIMALS_NL, COLOURS, SKETCH]);
    let animals = find(&providers, ANIMALS);
    assert_eq!(animals.identity().version, "2.0");
    assert_eq!(
        animals.identity().title.as_deref(),
        Some("Animals (synthetic)")
    );
    let d = animals.declaration();
    assert_eq!(d.content, ContentMode::Complete);
    assert!(d.capabilities.contains(&Capability::Subsumption));
    assert!(d.capabilities.contains(&Capability::Enumeration));
    assert_eq!(d.languages, ["de", "en"]);
    assert_eq!(d.filters[0].code, "legs");
    assert_eq!(
        d.filters[0].operators,
        [FilterOperator::Equal, FilterOperator::In]
    );
    let names: Vec<&str> = d.properties.iter().map(|p| p.code.as_str()).collect();
    assert!(names.contains(&"legs") && names.contains(&"parent") && names.contains(&"child"));
}

#[test]
fn locate_display_designations_and_status_follow_the_resource() {
    let (_dir, providers) = load_all();
    let animals = find(&providers, ANIMALS);
    let cat = animals.locate("cat").expect("reads").expect("cat").concept;
    assert!(
        animals.locate("CAT").expect("reads").is_none(),
        "case-sensitive system"
    );
    assert_eq!(
        animals.display(cat, None).expect("reads").as_deref(),
        Some("Cat")
    );
    assert_eq!(
        animals
            .display(cat, Some("de-DE"))
            .expect("reads")
            .as_deref(),
        Some("Katze")
    );
    assert_eq!(
        animals.display(cat, Some("fr")).expect("reads").as_deref(),
        Some("Cat")
    );
    assert_eq!(
        animals.designations(cat, Some("en")).expect("reads")[0].value,
        "Domestic cat"
    );
    assert_eq!(
        animals
            .definition(
                animals
                    .locate("animal")
                    .expect("reads")
                    .expect("animal")
                    .concept
            )
            .expect("reads")
            .as_deref(),
        Some("A living thing that is not a plant.")
    );
    let fish = animals
        .locate("fish")
        .expect("reads")
        .expect("fish")
        .concept;
    let dodo = animals
        .locate("dodo")
        .expect("reads")
        .expect("dodo")
        .concept;
    let pet = animals.locate("pet").expect("reads").expect("pet").concept;
    assert!(
        !animals.status(fish).expect("reads").active,
        "status retired is inactive"
    );
    assert!(
        !animals.status(dodo).expect("reads").active,
        "inactive = true"
    );
    assert!(
        animals.status(pet).expect("reads").abstract_concept,
        "notSelectable is abstract"
    );
    assert!(animals.status(cat).expect("reads").active);

    let colours = find(&providers, COLOURS);
    let red = colours
        .locate("red")
        .expect("reads")
        .expect("case-insensitive match");
    assert_eq!(red.code, "RED", "the system's own spelling is returned");
    assert!(colours.locate("BLUE").expect("reads").is_some());
}

#[test]
fn properties_and_hierarchy_come_from_nesting_and_subsumed_by() {
    let (_dir, providers) = load_all();
    let animals = find(&providers, ANIMALS);
    let kitten = animals
        .locate("kitten")
        .expect("reads")
        .expect("kitten")
        .concept;
    let props = animals.properties(kitten).expect("reads");
    let parents: Vec<&PropertyValue> = props
        .iter()
        .filter(|p| p.code == "parent")
        .map(|p| &p.value)
        .collect();
    assert_eq!(
        parents,
        [
            &PropertyValue::Code(String::from("cat")),
            &PropertyValue::Code(String::from("pet"))
        ]
    );
    assert!(
        props
            .iter()
            .any(|p| p.code == "legs" && p.value == PropertyValue::Integer(4))
    );
    assert!(
        props
            .iter()
            .any(|p| p.code == "inactive" && p.value == PropertyValue::Boolean(false))
    );
    let hierarchy = animals.hierarchy().expect("is-a hierarchy");
    let living = animals
        .locate("living")
        .expect("reads")
        .expect("living")
        .concept;
    let cat = animals.locate("cat").expect("reads").expect("cat").concept;
    assert_eq!(hierarchy.subsumes(living, kitten), Outcome::Subsumes);
    assert_eq!(hierarchy.subsumes(kitten, cat), Outcome::SubsumedBy);
    let leaves = animals
        .filter(&Filter {
            property: String::from("concept"),
            op: FilterOperator::DescendentLeaf,
            value: String::from("animal"),
        })
        .expect("filters");
    assert_eq!(codes(animals, &leaves), ["dog", "fish", "kitten"]);
    let four_legs = animals
        .filter(&Filter {
            property: String::from("legs"),
            op: FilterOperator::Equal,
            value: String::from("4"),
        })
        .expect("filters");
    assert_eq!(codes(animals, &four_legs), ["cat", "dog", "kitten"]);
    let children_of_living = animals
        .properties(living)
        .expect("reads")
        .iter()
        .filter(|p| p.code == "child")
        .count();
    assert_eq!(children_of_living, 2);
    let colours = find(&providers, COLOURS);
    assert!(
        colours.hierarchy().is_none(),
        "no hierarchyMeaning, no subsumption"
    );
    assert!(matches!(
        colours.filter(&Filter {
            property: String::from("concept"),
            op: FilterOperator::IsA,
            value: String::from("RED")
        }),
        Err(ProviderError::UnsupportedFilter { .. })
    ));
}

#[test]
fn search_folds_case_and_diacritics_and_respects_language() {
    let (_dir, providers) = load_all();
    let animals = find(&providers, ANIMALS);
    assert_eq!(
        codes(
            animals,
            &animals.search("kat", Some("de")).expect("searches")
        ),
        ["cat"]
    );
    assert_eq!(
        codes(animals, &animals.search("dom cat", None).expect("searches")),
        ["cat"]
    );
    assert_eq!(
        codes(animals, &animals.search("PLA", None).expect("searches")),
        ["plant"]
    );
    assert!(animals.search("zebra", None).expect("searches").is_empty());
}

#[test]
fn example_content_refuses_validation_and_enumeration() {
    let (_dir, providers) = load_all();
    let sketch = find(&providers, SKETCH);
    assert!(matches!(
        sketch.locate("a"),
        Err(ProviderError::IncompleteContent { .. })
    ));
    assert!(matches!(sketch.all(), Err(ProviderError::NotEnumerable)));
    let mut registry = Registry::new();
    let (_dir2, providers2) = load_all();
    for p in providers2 {
        registry.register(Arc::new(p)).expect("registers");
    }
    let request = validate_code::ValidateCodeInput {
        url: Some(SKETCH.to_owned()),
        code: Some(String::from("a")),
        ..validate_code::ValidateCodeInput::default()
    };
    let error =
        validate_code::validate_code(&registry, &Invocation::Type, &request).expect_err("refused");
    assert!(matches!(error, OperationError::NotSupported(_)));
    assert_eq!(error.issue_code(), "not-supported");
}

/// A one-code `CodeSystem` at `url` whose `content` is the mode named, written
/// to a file so the generic provider loads it the way a package's is loaded.
fn content_mode(url: &str, content: &str) -> (tempfile::TempDir, Registry) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("CodeSystem-mode.json");
    let resource = serde_json::json!({
        "resourceType": "CodeSystem",
        "url": url,
        "version": "0.1.0",
        "status": "active",
        "content": content,
        "caseSensitive": true,
        "concept": [{"code": "code1", "display": "Display 1"}]
    });
    std::fs::write(&path, resource.to_string()).expect("writes");
    let model = fhir_terminology::fhir_codesystem::load::load_file(&path, FhirVersion::R5)
        .expect("loads the resource");
    let mut registry = Registry::new();
    registry
        .register(Arc::new(FhirCodeSystem::new(model).expect("builds")))
        .expect("registers");
    (dir, registry)
}

/// The `CodeSystem/$validate-code` answer for `code` against `url`.
fn validated(registry: &Registry, url: &str, code: &str) -> validate_code::ValidationOutcome {
    let request = validate_code::ValidateCodeInput {
        url: Some(url.to_owned()),
        code: Some(code.to_owned()),
        ..validate_code::ValidateCodeInput::default()
    };
    validate_code::validate_code(registry, &Invocation::Type, &request).expect("validates")
}

#[test]
fn a_fragment_holds_a_code_it_does_not_carry_and_a_complete_system_does_not() {
    // A `fragment` resource carries "a subset of the code system concepts" and
    // a `complete` one "all the concepts defined by the code system"
    // (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>), so absence
    // from the first is not a verdict and absence from the second is.
    const MODE: &str = "http://example.org/fhir/CodeSystem/mode";
    let (_fragment_dir, fragment) = content_mode(MODE, "fragment");
    let answer = validated(&fragment, MODE, "code1x");
    assert!(answer.result, "{answer:?}");
    assert_eq!(answer.message, None, "a result of true says nothing");
    assert_eq!(answer.issues.len(), 1, "{:?}", answer.issues);
    assert_eq!(answer.issues[0].severity, "warning");
    assert_eq!(answer.issues[0].kind, "invalid-code");
    assert!(
        answer.issues[0].text.contains("labeled as a fragment"),
        "the answer reflects the fragment: {}",
        answer.issues[0].text
    );
    assert!(validated(&fragment, MODE, "code1").result, "a carried code");

    let (_complete_dir, complete) = content_mode(MODE, "complete");
    let answer = validated(&complete, MODE, "code1x");
    assert!(!answer.result, "{answer:?}");
    assert_eq!(answer.issues[0].severity, "error");

    // A `not-present` system served from an index answers for itself, so its
    // absent code stays a code the system does not have.
    let registry = crate::fixture::registry();
    let answer = validated(&registry, crate::fixture::URL, "zebra");
    assert!(!answer.result, "{answer:?}");
    assert_eq!(answer.issues[0].severity, "error");
}

#[test]
fn a_supplement_layers_designations_and_properties_over_the_system() {
    let (_dir, providers) = load_all();
    let supplement_model = find(&providers, ANIMALS_NL).model().clone();
    assert_eq!(supplement_model.content, ContentMode::Supplement);
    assert_eq!(supplement_model.supplements.as_deref(), Some(ANIMALS));
    let mut concepts = std::collections::BTreeMap::new();
    for entry in &supplement_model.concepts {
        concepts.insert(
            entry.code.clone(),
            Additions {
                designations: entry.designations.clone(),
                properties: entry.properties.clone(),
            },
        );
    }
    let supplement = Supplement {
        url: supplement_model.url.clone(),
        version: Some(supplement_model.version.clone()),
        concepts,
    };
    let (_dir3, mut base) = load_all();
    let animals = base.remove(
        base.iter()
            .position(|p| p.identity().url == ANIMALS)
            .expect("animals"),
    );
    let supplemented = Supplemented::new(Arc::new(animals), vec![supplement]);
    let cat = supplemented
        .locate("cat")
        .expect("reads")
        .expect("cat")
        .concept;
    assert_eq!(
        supplemented
            .display(cat, Some("nl"))
            .expect("reads")
            .as_deref(),
        Some("Kat")
    );
    assert!(
        supplemented
            .properties(cat)
            .expect("reads")
            .iter()
            .any(|p| p.code == "colour")
    );
    assert_eq!(supplemented.declaration().languages, ["de", "en", "nl"]);

    let mut registry = Registry::new();
    registry
        .register(Arc::new(supplemented))
        .expect("registers");
    let input = lookup::LookupInput {
        system: Some(ANIMALS.to_owned()),
        code: Some(String::from("cat")),
        display_language: Some(String::from("nl")),
        ..lookup::LookupInput::default()
    };
    let outcome = lookup::lookup(&registry, &Invocation::Type, &input).expect("looks up");
    assert_eq!(outcome.display, "Kat");
    assert_eq!(outcome.version.as_deref(), Some("2.0"));
}

fn take(providers: Vec<FhirCodeSystem>, url: &str) -> FhirCodeSystem {
    providers
        .into_iter()
        .find(|p| p.identity().url == url)
        .expect("system")
}

fn registry_of(provider: FhirCodeSystem) -> Registry {
    let mut registry = Registry::new();
    registry.register(Arc::new(provider)).expect("registers");
    registry
}

fn lookup_cat(registry: &Registry, properties: &[&str]) -> lookup::LookupOutcome {
    let input = lookup::LookupInput {
        system: Some(ANIMALS.to_owned()),
        code: Some(String::from("cat")),
        properties: properties.iter().map(|p| (*p).to_owned()).collect(),
        ..lookup::LookupInput::default()
    };
    lookup::lookup(registry, &Invocation::Type, &input).expect("looks up")
}

#[test]
fn lookup_property_star_answers_parent_child_inactive_and_the_declared_properties() {
    let (_dir, providers) = load_all();
    let registry = registry_of(take(providers, ANIMALS));
    let outcome = lookup_cat(&registry, &["*"]);
    let properties: Vec<(&str, String)> = outcome
        .properties
        .iter()
        .map(|p| (p.code.as_str(), p.value.as_text()))
        .collect();
    assert_eq!(
        properties,
        [
            ("inactive", String::from("false")),
            ("legs", String::from("4")),
            ("parent", String::from("animal")),
            ("child", String::from("kitten")),
        ]
    );
    let named = lookup_cat(&registry, &["parent", "inactive"]);
    let codes: Vec<&str> = named.properties.iter().map(|p| p.code.as_str()).collect();
    assert_eq!(codes, ["inactive", "parent"]);
    assert!(
        named.designations.is_empty(),
        "designations were not asked for"
    );
}

#[test]
fn lookup_answers_the_display_as_a_designation_in_the_system_language() {
    let (_dir, providers) = load_all();
    let registry = registry_of(take(providers, ANIMALS));
    let outcome = lookup_cat(&registry, &[]);
    assert_eq!(outcome.display, "Cat");
    let display = outcome
        .designations
        .iter()
        .find(|d| d.value == "Cat")
        .expect("the display is a designation");
    assert_eq!(display.language.as_deref(), Some("en"));
    let use_ = display.use_.as_ref().expect("has a use");
    assert_eq!(
        (use_.system.as_str(), use_.code.as_str()),
        (
            "http://terminology.hl7.org/CodeSystem/hl7TermMaintInfra",
            "preferredForLanguage"
        )
    );
    let values: Vec<&str> = outcome
        .designations
        .iter()
        .map(|d| d.value.as_str())
        .collect();
    assert_eq!(values, ["Domestic cat", "Katze", "Cat"]);
    let german = lookup_cat(&registry, &["lang.de"]);
    let values: Vec<&str> = german
        .designations
        .iter()
        .map(|d| d.value.as_str())
        .collect();
    assert_eq!(
        values,
        ["Katze"],
        "lang.de keeps the German designations only"
    );
}

#[test]
fn lookup_answers_the_code_the_system_and_abstract() {
    let (_dir, providers) = load_all();
    let registry = registry_of(take(providers, ANIMALS));
    let cat = lookup_cat(&registry, &[]);
    assert_eq!((cat.code.as_str(), cat.system.as_str()), ("cat", ANIMALS));
    assert!(!cat.abstract_concept);
    let input = lookup::LookupInput {
        system: Some(ANIMALS.to_owned()),
        code: Some(String::from("living")),
        ..lookup::LookupInput::default()
    };
    let living = lookup::lookup(&registry, &Invocation::Type, &input).expect("looks up");
    assert!(living.abstract_concept, "notSelectable = true is abstract");
}

#[test]
fn lookup_name_is_the_code_system_name_then_the_title_then_the_url() {
    let (_dir, providers) = load_all();
    let animals = take(providers, ANIMALS);
    let mut model = animals.model().clone();
    assert_eq!(lookup_cat(&registry_of(animals), &[]).name, "Animals");
    model.name = None;
    let titled = FhirCodeSystem::new(model.clone()).expect("builds");
    assert_eq!(
        lookup_cat(&registry_of(titled), &[]).name,
        "Animals (synthetic)"
    );
    model.title = None;
    let bare = FhirCodeSystem::new(model).expect("builds");
    assert_eq!(lookup_cat(&registry_of(bare), &[]).name, ANIMALS);
}

// NOTE: R5 `$lookup` declares `definition` as its own output
// (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>).
#[test]
fn lookup_carries_the_definition_when_the_system_states_one() {
    let mut registry = Registry::new();
    let (_dir, providers) = load_all();
    for p in providers {
        registry.register(Arc::new(p)).expect("registers");
    }
    let animal = lookup::lookup(
        &registry,
        &Invocation::Type,
        &lookup::LookupInput {
            system: Some(ANIMALS.to_owned()),
            code: Some(String::from("animal")),
            ..lookup::LookupInput::default()
        },
    )
    .expect("looks up");
    assert_eq!(
        animal.definition.as_deref(),
        Some("A living thing that is not a plant.")
    );
    let cat = lookup::lookup(
        &registry,
        &Invocation::Type,
        &lookup::LookupInput {
            system: Some(ANIMALS.to_owned()),
            code: Some(String::from("cat")),
            ..lookup::LookupInput::default()
        },
    )
    .expect("looks up");
    assert_eq!(cat.definition, None);
}

#[test]
fn standards_status_marks_deprecated_concepts_and_withdrawn_designations() {
    let (_dir, providers) = load_all();
    let animals = find(&providers, ANIMALS);
    let plant = animals.locate("plant").expect("locates").expect("plant");
    let status = animals.status(plant.concept).expect("status");
    assert!(
        status.active,
        "a deprecated standards status keeps the concept active"
    );
    assert_eq!(status.standards_status.as_deref(), Some("deprecated"));
    let dog = animals.locate("dog").expect("locates").expect("dog");
    let designations = animals
        .designations(dog.concept, None)
        .expect("designations");
    let hound = designations
        .iter()
        .find(|d| d.value == "Hound")
        .expect("the withdrawn designation is still listed");
    assert_eq!(hound.standards_status.as_deref(), Some("withdrawn"));
    assert_eq!(
        animals
            .status(dog.concept)
            .expect("status")
            .standards_status,
        None
    );
}

#[test]
fn not_selectable_is_known_by_its_standard_uri_and_a_declared_false_stays_a_property() {
    use fhir_types::codec::{Json, Path};
    let build = |property_code: &str| {
        let object = serde_json::json!({
            "resourceType": "CodeSystem", "url": "http://example.org/fhir/CodeSystem/selectable",
            "version": "1", "status": "active", "content": "complete", "caseSensitive": true,
            "property": [{"code": property_code, "uri": "http://hl7.org/fhir/concept-properties#notSelectable", "type": "boolean"}],
            "concept": [
                {"code": "codeU", "display": "Unknown"},
                {"code": "codeS", "display": "Selectable", "property": [{"code": property_code, "valueBoolean": false}]},
                {"code": "codeNS", "display": "Not selectable", "property": [{"code": property_code, "valueBoolean": true}]}
            ]
        });
        let document: fhir_types::codec::Value =
            serde_json::from_str(&object.to_string()).expect("the document parses");
        let resource = fhir_types::r5::code_system::CodeSystem::from_json(
            document.as_object().expect("object"),
            &mut Path::root("CodeSystem"),
        )
        .expect("decodes");
        let model =
            fhir_terminology::fhir_codesystem::convert::r5::convert(&resource).expect("converts");
        FhirCodeSystem::new(model).expect("builds")
    };
    for property_code in ["notSelectable", "not-selectable"] {
        let provider = build(property_code);
        let ns = provider.locate("codeNS").expect("locates").expect("codeNS");
        assert!(
            provider
                .status(ns.concept)
                .expect("status")
                .abstract_concept,
            "{property_code}: the standard URI marks the concept abstract"
        );
        let s = provider.locate("codeS").expect("locates").expect("codeS");
        assert!(!provider.status(s.concept).expect("status").abstract_concept);
        let declared_false = provider
            .properties(s.concept)
            .expect("properties")
            .into_iter()
            .any(|p| p.code == property_code && p.value == PropertyValue::Boolean(false));
        assert!(
            declared_false,
            "{property_code}: an explicit false is still listed"
        );
        let selectable = provider
            .filter(&Filter {
                property: property_code.to_owned(),
                op: FilterOperator::Equal,
                value: String::from("false"),
            })
            .expect("filters");
        assert_eq!(codes(&provider, &selectable), ["codeS"], "{property_code}");
    }
}

/// A `fragment` resource carries "a subset of the code system", so codes the
/// system defines outside it are valid and unlisted, which is what
/// `valueset-unclosed` marks
/// (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>,
/// <https://hl7.org/fhir/R4B/valueset-operation-expand.html>, Notes); a
/// `complete` resource carries every code the system defines.
#[test]
fn an_expansion_over_a_fragment_is_unclosed_and_one_over_a_complete_system_is_not() {
    let (_dir, providers) = load_all();
    let complete = find(&providers, COLOURS);
    assert_eq!(complete.declaration().content, ContentMode::Complete);
    assert!(!complete.unclosed(&[]));
    let mut model = complete.model().clone();
    model.content = ContentMode::Fragment;
    let fragment = FhirCodeSystem::new(model).expect("builds");
    assert!(fragment.unclosed(&[]));
}

/// The mark is joined by the reason and by the fragment the expansion drew on,
/// beside the `used-*` family the terminology ecosystem requires: "all versions
/// of code systems used (in 'used-*')"
/// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
// NOTE: no published `StructureDefinition` defines `valueset-unclosed-reason` and no
// `OperationDefinition` declares `used-fragment`, so the ecosystem suite's
// `fragment/fragment-expansion` case is the source (<https://github.com/HL7/fhir-tx-ecosystem-ig>).
#[test]
fn a_fragment_expansion_states_its_reason_and_names_its_used_fragment() {
    let (_dir, providers) = load_all();
    let mut model = find(&providers, COLOURS).model().clone();
    model.content = ContentMode::Fragment;
    let mut registry = Registry::new();
    registry
        .register(Arc::new(FhirCodeSystem::new(model).expect("builds")))
        .expect("registers");
    let value_sets = fhir_terminology::valueset::store::ValueSetStore::new();
    let concept_maps = fhir_terminology::conceptmap::store::ConceptMapStore::new();
    let sources = fhir_terminology::operations::Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let inline = fhir_types::r4b::value_set::ValueSet {
        url: Some("http://example.org/fhir/ValueSet/fragment-all".into()),
        status: "active".into(),
        compose: Some(fhir_types::r4b::value_set::ValueSetCompose {
            include: vec![fhir_types::r4b::value_set::ValueSetComposeInclude {
                system: Some(COLOURS.into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let request = fhir_terminology::operations::expand::ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline)),
        ..Default::default()
    };
    let outcome =
        fhir_terminology::operations::expand::expand(&sources, &request).expect("expands");
    assert!(outcome.unclosed);
    assert_eq!(
        outcome.unclosed_reasons,
        [format!(
            "This extension is based on a fragment of the code system {COLOURS}"
        )]
    );
    let used: Vec<&fhir_terminology::operations::expand::ExpansionParameter> = outcome
        .parameters
        .iter()
        .filter(|p| p.name == "used-codesystem" || p.name == "used-fragment")
        .collect();
    let canonical =
        fhir_terminology::operations::expand::ParameterValue::Uri(format!("{COLOURS}|1"));
    assert_eq!(
        used.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["used-codesystem", "used-fragment"],
        "{:?}",
        outcome.parameters
    );
    assert!(used.iter().all(|p| p.value == canonical), "{used:?}");
    let rendered = fhir_terminology::valueset::render::r4b::expansion(&outcome);
    assert_eq!(
        rendered
            .expansion
            .expect("an expansion")
            .extension
            .iter()
            .map(|e| (e.url.as_str(), e.value.clone()))
            .collect::<Vec<_>>(),
        [
            (
                "http://hl7.org/fhir/StructureDefinition/valueset-unclosed",
                Some(fhir_types::r4b::extension::ExtensionValue::Boolean(
                    true.into()
                ))
            ),
            (
                "http://hl7.org/fhir/StructureDefinition/valueset-unclosed-reason",
                Some(fhir_types::r4b::extension::ExtensionValue::String(
                    format!("This extension is based on a fragment of the code system {COLOURS}")
                        .as_str()
                        .into()
                ))
            ),
        ]
    );
}

#[test]
fn a_code_system_resource_declares_no_implicit_value_set_form() {
    let (_dir, providers) = load_all();
    for provider in &providers {
        crate::implicit_forms::declares_none(provider, &provider.identity().url);
    }
}
