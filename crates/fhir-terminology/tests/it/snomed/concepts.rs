// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SNOMED CT provider's concepts: identity, codes, displays, designations, properties, the hierarchy and the normal forms.

use concept_graph::subsumption::Outcome;
use ferroterm_testkit::snomed;
use ferroterm_testkit::snomed::{
    ALTERNATIVE, ANIMAL, CAT, CODES_MAP, COVERING, DOG, EDITION, FISH, FUR, GB_LANGUAGE_REFSET,
    ICD10_MAP, Item, LEGS, MODULE_CONCEPT, MODULE_DEPENDENCY, NL_LANGUAGE_REFSET, PETS,
    POSSIBLY_EQUIVALENT_TO, REPLACED_BY, SAME_AS, SCHEME, TOP, VERSION, item, sctid,
};
use fhir_terminology::capabilities::Summary;
use fhir_terminology::operations::{Invocation, lookup, validate_code};
use fhir_terminology::provider::declaration::Capability;
use fhir_terminology::provider::filter::{Filter, FilterOperator};
use fhir_terminology::provider::{CodeSystemProvider, Concept, PropertyValue};
use fhir_terminology::registry::Registry;
use fhir_terminology::snomed::{OpenError, SYSTEM, SnomedProvider};
use fhir_terminology::stored;
use std::sync::Arc;

use super::provider;

#[test]
fn locate_accepts_valid_sctids_only() {
    let (_dir, p) = provider();
    let cat = p.locate(&sctid(item(CAT))).expect("reads").expect("cat");
    assert_eq!(cat.concept, Concept::new(CAT));
    assert_eq!(
        p.code(cat.concept).expect("reads").as_deref(),
        Some(sctid(item(CAT)).as_str())
    );
    // A well-formed SCTID the edition lacks, a wrong check digit, and text: absent, never an error.
    assert!(p.locate(&sctid(Item::raw(4242))).expect("reads").is_none());
    let mut wrong = sctid(item(CAT));
    wrong.pop();
    wrong.push('0');
    assert!(p.locate(&wrong).expect("reads").is_none());
    assert!(p.locate("cat").expect("reads").is_none());
}

#[test]
fn display_is_the_preferred_term_of_the_language_with_a_stated_fallback() {
    let (_dir, p) = provider();
    let cat = Concept::new(CAT);
    assert_eq!(p.display(cat, None).expect("reads").as_deref(), Some("Cat"));
    assert_eq!(
        p.display(cat, Some("en-GB")).expect("reads").as_deref(),
        Some("Cat")
    );
    assert_eq!(
        p.display(cat, Some("nl")).expect("reads").as_deref(),
        Some("Kat")
    );
    // No Dutch term: fall back to the default language's preferred term.
    assert_eq!(
        p.display(Concept::new(TOP), Some("nl"))
            .expect("reads")
            .as_deref(),
        Some("Living thing")
    );
    // Fish has an FSN in a refset and an unreferenced synonym: the active synonym in the language wins over the FSN.
    assert_eq!(
        p.display(Concept::new(FISH), None)
            .expect("reads")
            .as_deref(),
        Some("Fish")
    );
    // An unknown language: the default.
    assert_eq!(
        p.display(cat, Some("fr")).expect("reads").as_deref(),
        Some("Cat")
    );
}

#[test]
fn designations_carry_the_snomed_use_codings_and_filter_by_language() {
    let (_dir, p) = provider();
    let all = p.designations(Concept::new(CAT), None).expect("reads");
    assert_eq!(all.len(), 5);
    let fsn = all
        .iter()
        .find(|d| d.value == "Cat (synthetic)")
        .expect("fsn");
    assert_eq!(
        fsn.use_.as_ref().map(|u| u.code.as_str()),
        Some("900000000000003001")
    );
    assert_eq!(fsn.use_.as_ref().map(|u| u.system.as_str()), Some(SYSTEM));
    let dutch = p
        .designations(Concept::new(CAT), Some("nl-NL"))
        .expect("reads");
    let terms: Vec<&str> = dutch.iter().map(|d| d.value.as_str()).collect();
    assert_eq!(terms, ["Kat", "Poes"]);
    assert!(dutch.iter().all(|d| d.language.as_deref() == Some("nl")));
    assert!(p.definition(Concept::new(CAT)).expect("reads").is_none());
}

#[test]
fn properties_follow_the_snomed_on_fhir_list() {
    let (_dir, p) = provider();
    let props = p.properties(Concept::new(CAT)).expect("reads");
    let find = |code: &str| -> Vec<&PropertyValue> {
        props
            .iter()
            .filter(|p| p.code == code)
            .map(|p| &p.value)
            .collect()
    };
    assert_eq!(find("inactive"), [&PropertyValue::Boolean(false)]);
    assert_eq!(find("sufficientlyDefined"), [&PropertyValue::Boolean(true)]);
    assert_eq!(
        find("moduleId"),
        [&PropertyValue::Code(sctid(Item::raw(99)))]
    );
    assert_eq!(
        find("effectiveTime"),
        [&PropertyValue::String(String::from("20260101"))]
    );
    assert_eq!(find("parent"), [&PropertyValue::Code(sctid(item(ANIMAL)))]);
    assert_eq!(find("child"), Vec::<&PropertyValue>::new());
    assert_eq!(
        find(&sctid(item(COVERING))),
        [&PropertyValue::Code(sctid(item(FUR)))]
    );
    assert_eq!(
        find(&sctid(item(LEGS))),
        [&PropertyValue::Decimal(String::from("4"))]
    );
    let animal = p.properties(Concept::new(ANIMAL)).expect("reads");
    let children: Vec<&PropertyValue> = animal
        .iter()
        .filter(|p| p.code == "child")
        .map(|p| &p.value)
        .collect();
    assert_eq!(
        children,
        [
            &PropertyValue::Code(sctid(item(CAT))),
            &PropertyValue::Code(sctid(item(DOG)))
        ]
    );
    let fish = p.properties(Concept::new(FISH)).expect("reads");
    assert!(
        fish.iter()
            .any(|p| p.code == "inactive" && p.value == PropertyValue::Boolean(true))
    );
    assert!(
        fish.iter()
            .any(|p| p.code == "sufficientlyDefined" && p.value == PropertyValue::Boolean(false))
    );
    assert!(!p.status(Concept::new(FISH)).expect("reads").active);
    assert!(p.status(Concept::new(CAT)).expect("reads").active);
}

#[test]
fn the_hierarchy_answers_subsumption_and_the_filters_from_the_closure() {
    let (_dir, p) = provider();
    let hierarchy = p.hierarchy().expect("snomed has a hierarchy");
    assert_eq!(
        hierarchy.subsumes(Concept::new(ANIMAL), Concept::new(CAT)),
        Outcome::Subsumes
    );
    assert_eq!(
        hierarchy.subsumes(Concept::new(CAT), Concept::new(TOP)),
        Outcome::SubsumedBy
    );
    assert_eq!(
        hierarchy.subsumes(Concept::new(CAT), Concept::new(DOG)),
        Outcome::NotSubsumed
    );
    assert_eq!(
        hierarchy.subsumes(Concept::new(CAT), Concept::new(CAT)),
        Outcome::Equivalent
    );
    let descendants = p
        .filter(&Filter {
            property: String::from("concept"),
            op: FilterOperator::DescendentOf,
            value: sctid(item(ANIMAL)),
        })
        .expect("filters");
    assert_eq!(descendants.iter().collect::<Vec<_>>(), [CAT, DOG]);
    let leaves = p
        .filter(&Filter {
            property: String::from("concept"),
            op: FilterOperator::DescendentLeaf,
            value: sctid(item(TOP)),
        })
        .expect("filters");
    assert_eq!(
        leaves.iter().collect::<Vec<_>>(),
        [
            CAT,
            DOG,
            FUR,
            COVERING,
            LEGS,
            PETS,
            CODES_MAP,
            SAME_AS,
            SCHEME,
            REPLACED_BY,
            POSSIBLY_EQUIVALENT_TO,
            ALTERNATIVE,
            MODULE_DEPENDENCY,
            MODULE_CONCEPT,
            ICD10_MAP,
            GB_LANGUAGE_REFSET,
            NL_LANGUAGE_REFSET
        ]
    );
}

#[test]
fn search_reads_the_designation_index() {
    let (_dir, p) = provider();
    let ka = p.search("ka", Some("nl")).expect("searches");
    assert_eq!(ka.iter().collect::<Vec<_>>(), [CAT]);
    let synth = p.search("synth", None).expect("searches");
    assert_eq!(
        synth.len(),
        7,
        "every FSN except the two attribute FSNs carries the tag"
    );
    let none = p.search("zebra", None).expect("searches");
    assert!(none.is_empty());
}

#[test]
fn a_registry_with_the_provider_renders_terminology_capabilities() {
    let (_dir, p) = provider();
    let mut registry = Registry::default();
    registry.register(Arc::new(p)).expect("registers");
    let summary = Summary::of(&registry);
    let system = summary
        .systems
        .iter()
        .find(|s| s.url == SYSTEM)
        .expect("snomed");
    assert!(system.subsumption);
    assert_eq!(system.versions[0].code, VERSION);
    assert!(system.versions[0].is_default);
    // NOTE: the element is "If the compositional grammar defined by the code
    // system is supported"
    // (<https://hl7.org/fhir/R4B/terminologycapabilities-definitions.html#TerminologyCapabilities.codeSystem.version.compositional>).
    assert!(system.versions[0].compositional);
    assert_eq!(system.versions[0].languages, ["en", "nl"]);
    assert!(
        system.versions[0]
            .properties
            .contains(&String::from("inactive"))
    );
    let r4b = summary.to_r4b("2026-09-02T00:00:00Z");
    assert_eq!(r4b.code_system.len(), 1);
}

#[test]
fn a_foreign_or_broken_artifact_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(matches!(
        SnomedProvider::open(dir.path(), "en"),
        Err(OpenError::Artifact(stored::OpenError::Io { .. }))
    ));
    snomed::write(dir.path()).expect("writes the fixture");
    std::fs::write(
        dir.path().join("manifest.json"),
        r#"{"manifest":2,"system":"http://loinc.org","edition":"x","version":"2.80","store":"store.redb","hierarchy":"hierarchy.bin","text":"text.bin"}"#,
    )
    .expect("writes");
    assert!(
        matches!(SnomedProvider::open(dir.path(), "en"), Err(OpenError::NotSnomed(s)) if s == "http://loinc.org")
    );
    std::fs::write(
        dir.path().join("manifest.json"),
        r#"{"manifest":1,"system":"http://snomed.info/sct","edition":"x","version":"2.80","store":"store.redb","hierarchy":"hierarchy.bin","text":"text.bin"}"#,
    )
    .expect("writes");
    assert!(matches!(
        SnomedProvider::open(dir.path(), "en"),
        Err(OpenError::Artifact(stored::OpenError::ManifestVersion(1)))
    ));
}

/// `property` is "A property that the client wishes to be returned in the
/// output", and the codes the specification or the `CodeSystem` define "are
/// allowed, and these are returned in the out parameter `property`"
/// (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>). A code
/// neither defines names nothing to return, so it is left out with no issue.
#[test]
fn a_property_the_code_system_does_not_define_is_left_out() {
    let (_dir, p) = provider();
    let mut registry = Registry::default();
    registry.register(Arc::new(p)).expect("registers");
    let ask = |property: &str| {
        lookup::lookup(
            &registry,
            &Invocation::Type,
            &lookup::LookupInput {
                code: Some(sctid(item(CAT))),
                system: Some(String::from(SYSTEM)),
                properties: vec![String::from(property)],
                ..lookup::LookupInput::default()
            },
        )
    };
    let answered = ask("nonesuch").expect("a property nothing defines is no request error");
    assert!(
        answered
            .properties
            .iter()
            .all(|property| property.code != "nonesuch"),
        "nothing is answered for it"
    );
    for property in ["normalForm", "normalFormTerse"] {
        assert!(
            ask(property).is_ok(),
            "`{property}` is defined by the FHIR SNOMED CT page and generated here"
        );
    }
    // The page's own properties, the standard ones, and a concept model
    // attribute by concept id all answer.
    for property in ["inactive", "moduleId", "parent", "designation"] {
        assert!(ask(property).is_ok(), "`{property}` is defined");
    }
    assert!(ask(&sctid(item(COVERING))).is_ok());
}
// NOTE: `displayLanguage` "Specifies the language to be used for description
// when validating the display property"
// (<https://hl7.org/fhir/R4B/codesystem-operation-validate-code.html>).
#[test]
fn a_display_in_the_editions_language_does_not_satisfy_a_request_for_another_one() {
    let (_dir, p) = provider();
    assert_eq!(p.language(), Some("en"));
    let mut registry = Registry::default();
    registry.register(Arc::new(p)).expect("registers");
    let ask = |code: String, display: &str| {
        validate_code::validate_code(
            &registry,
            &Invocation::Type,
            &validate_code::ValidateCodeInput {
                url: Some(SYSTEM.to_owned()),
                code: Some(code),
                display: Some(display.to_owned()),
                display_language: Some(String::from("nl")),
                ..validate_code::ValidateCodeInput::default()
            },
        )
        .expect("validates")
    };

    // The cat carries a Dutch preferred synonym, so the English one is not a
    // display in Dutch and the Dutch one is named.
    let wrong = ask(sctid(item(CAT)), "Cat");
    assert!(!wrong.result, "{wrong:?}");
    assert_eq!(wrong.display.as_deref(), Some("Kat"));
    let issue = wrong.issues.first().expect("an issue");
    assert_eq!(issue.severity, "error");
    assert_eq!(issue.kind, "invalid-display");
    assert_eq!(
        issue.message_id(),
        "Display_Name_for__should_be_one_of__instead_of"
    );
    assert!(issue.text.contains("Kat"), "{}", issue.text);
    assert_eq!(wrong.message.as_deref(), Some(issue.text.as_str()));

    // The root carries English alone: the English display stands, and the
    // answer says the requested language has none.
    let none = ask(sctid(item(TOP)), "Living thing");
    assert!(none.result, "{none:?}");
    let issue = none.issues.first().expect("an issue");
    assert_eq!(issue.severity, "information");
    assert_eq!(issue.kind, "invalid-display");
    assert_eq!(
        issue.message_id(),
        "NO_VALID_DISPLAY_FOUND_NONE_FOR_LANG_OK"
    );
    assert!(none.message.is_some(), "{none:?}");
}
/// The Necessary Normal Form is the concept's inferred view rendered in
/// SNOMED CT Compositional Grammar.
///
/// The FHIR SNOMED CT page defines `normalForm` and `normalFormTerse` as that
/// expression with terms and with concept ids only
/// (<https://hl7.org/fhir/R4B/snomedct.html>, "SNOMED CT Properties"). The
/// fixture's cat is sufficiently defined, is under the animal, and carries one
/// role group holding a covering and a leg count, so the expression states the
/// prefix, the focus concept, and the group.
#[test]
fn the_normal_form_renders_the_inferred_view_of_a_concept() {
    let (_dir, p) = provider();
    let ask = |code: u32, property: &str| -> Option<String> {
        let concept = p
            .locate(&sctid(item(code)))
            .expect("locates")
            .expect("a concept")
            .concept;
        match p.generated_property(concept, property).expect("generates") {
            Some(property) => match property.value {
                PropertyValue::String(text) => Some(text),
                other => panic!("a string, not {other:?}"),
            },
            None => None,
        }
    };

    let terse = ask(CAT, "normalFormTerse").expect("the cat has a normal form");
    assert_eq!(
        terse,
        format!(
            "=== {animal} : {{ {covering} = {fur}, {legs} = #4 }}",
            animal = sctid(item(ANIMAL)),
            covering = sctid(item(COVERING)),
            fur = sctid(item(FUR)),
            legs = sctid(item(LEGS)),
        ),
        "the terse form carries concept ids only"
    );

    let with_terms = ask(CAT, "normalForm").expect("the cat has a normal form");
    assert!(
        with_terms.contains(&format!("{} |", sctid(item(ANIMAL)))),
        "the form with terms writes each reference as `id |term|`: {with_terms}"
    );
    assert!(
        with_terms.replace(char::is_whitespace, "").len()
            > terse.replace(char::is_whitespace, "").len(),
        "the form with terms is the longer of the two"
    );

    // The dog states the same two attributes in two groups where the cat
    // states them in one, so its expression carries two.
    let dog = ask(DOG, "normalFormTerse").expect("the dog has a normal form");
    assert_eq!(
        dog.matches('{').count(),
        2,
        "each role group is rendered as its own set: {dog}"
    );
}

/// The root has no inferred parent, and the grammar makes a focus concept
/// mandatory, so there is no expression to render for it.
#[test]
fn the_root_of_the_hierarchy_has_no_normal_form() {
    let (_dir, p) = provider();
    let concept = p
        .locate(&sctid(item(TOP)))
        .expect("locates")
        .expect("the root")
        .concept;
    assert_eq!(
        p.generated_property(concept, "normalForm")
            .expect("generates"),
        None,
        "inventing a focus concept for the root would say something the release does not"
    );
}

/// A request that names no property is answered without the generated ones,
/// which is what "if no properties are specified, the server chooses what to
/// return" admits
/// (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>).
#[test]
fn the_normal_form_is_answered_only_where_a_request_asks_for_it() {
    let (_dir, p) = provider();
    let mut registry = Registry::default();
    registry.register(Arc::new(p)).expect("registers");
    let ask = |properties: Vec<String>| {
        lookup::lookup(
            &registry,
            &Invocation::Type,
            &lookup::LookupInput {
                code: Some(sctid(item(CAT))),
                system: Some(String::from(SYSTEM)),
                properties,
                ..lookup::LookupInput::default()
            },
        )
        .expect("looks up")
        .properties
        .iter()
        .any(|property| property.code == "normalForm")
    };
    assert!(!ask(Vec::new()), "a request naming none pays for none");
    assert!(
        ask(vec![String::from("normalForm")]),
        "naming it answers it"
    );
    assert!(
        ask(vec![String::from("*")]),
        "`*` asks for everything there is"
    );
}

#[test]
fn identity_and_declaration_follow_the_manifest() {
    let (_dir, p) = provider();
    assert_eq!(p.identity().url, SYSTEM);
    assert_eq!(p.identity().version, VERSION);
    assert_eq!(p.edition_uri(), EDITION);
    let declaration = p.declaration();
    assert_eq!(declaration.languages, ["en", "nl"]);
    assert!(declaration.capabilities.contains(&Capability::Subsumption));
    assert!(declaration.capabilities.contains(&Capability::Enumeration));
    let codes: Vec<&str> = declaration
        .properties
        .iter()
        .map(|d| d.code.as_str())
        .collect();
    assert_eq!(
        &codes[..6],
        [
            "inactive",
            "sufficientlyDefined",
            "moduleId",
            "effectiveTime",
            "parent",
            "child"
        ]
    );
    assert!(codes.contains(&sctid(item(COVERING)).as_str()));
    assert!(codes.contains(&sctid(item(LEGS)).as_str()));
    assert_eq!(p.language_refsets(), [snomed::GB_REFSET, snomed::NL_REFSET]);
    assert_eq!(p.all().expect("all").len(), 21);
}
