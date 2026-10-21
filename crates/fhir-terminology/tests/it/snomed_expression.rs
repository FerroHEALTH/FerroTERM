// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Post-coordinated SNOMED CT expressions through the operations, over the
//! synthetic edition.
//!
//! An expression in Compositional Grammar is a valid `code` for
//! `http://snomed.info/sct` and is "subject to the same rules as
//! precoordinated concepts" (<https://hl7.org/fhir/R4B/snomedct.html>,
//! "Code"). The identifiers are the fixture's own: a test ships no SNOMED CT
//! content.

use std::sync::Arc;

use concept_graph::subsumption::Outcome;
use fhir_terminology::compose::{Compose, ConceptRef, Expander, Include, Options, SystemRef};
use fhir_terminology::conceptmap::model::Relationship;
use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::fhir_codesystem::load::FhirVersion;
use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;
use fhir_terminology::operations::{
    Invocation, OperationError, Sources, closure, lookup, subsumes, validate_code,
    value_set_validate_code,
};
use fhir_terminology::registry::Registry;
use fhir_terminology::snomed::{SYSTEM, SnomedProvider};
use fhir_terminology::valueset;
use fhir_terminology::valueset::store::ValueSetStore;

use ferroterm_testkit::snomed;
use ferroterm_testkit::snomed::{ANIMAL, CAT, COVERING, DOG, FISH, FUR, LEGS, VERSION, code};

fn registry() -> (tempfile::TempDir, Registry) {
    let dir = tempfile::tempdir().expect("tempdir");
    snomed::write(dir.path()).expect("writes the fixture");
    let provider = SnomedProvider::open(dir.path(), "en").expect("opens");
    let mut registry = Registry::new();
    registry.register(Arc::new(provider)).expect("registers");
    (dir, registry)
}

fn looked_up(registry: &Registry, code: &str) -> lookup::LookupOutcome {
    let input = lookup::LookupInput {
        system: Some(SYSTEM.to_owned()),
        code: Some(code.to_owned()),
        properties: vec![String::from("*")],
        ..lookup::LookupInput::default()
    };
    lookup::lookup(registry, &Invocation::Type, &input).expect("looks up")
}

fn validated(registry: &Registry, code: &str) -> validate_code::ValidationOutcome {
    let request = validate_code::ValidateCodeInput {
        url: Some(SYSTEM.to_owned()),
        code: Some(code.to_owned()),
        ..validate_code::ValidateCodeInput::default()
    };
    validate_code::validate_code(registry, &Invocation::Type, &request).expect("validates")
}

fn outcome(registry: &Registry, a: &str, b: &str) -> Outcome {
    let input = subsumes::SubsumesInput {
        system: Some(SYSTEM.to_owned()),
        code_a: Some(a.to_owned()),
        code_b: Some(b.to_owned()),
        ..subsumes::SubsumesInput::default()
    };
    subsumes::subsumes(registry, &Invocation::Type, &input).expect("subsumes")
}

/// `=== <cat> : <covering> = <fur>`, the shape of a refined focus concept.
fn refined(focus: u32, attribute: u32, value: u32) -> String {
    format!("{} : {} = {}", code(focus), code(attribute), code(value))
}

#[test]
fn a_well_formed_expression_over_defined_concepts_validates() {
    let (_dir, registry) = registry();
    // The Compositional Grammar specification, §7.3 Validating: the syntax and
    // that "All concept references included in the expression must be valid".
    let outcome = validated(&registry, &refined(CAT, COVERING, FUR));
    assert!(outcome.result, "{:?}", outcome.message);
    assert_eq!(outcome.system.as_deref(), Some(SYSTEM));
    assert_eq!(outcome.version.as_deref(), Some(VERSION));
    assert_eq!(
        outcome.normalized_code.as_deref(),
        Some(format!("=== {} : {} = {}", code(CAT), code(COVERING), code(FUR)).as_str()),
        "the code the system spells is the expression in one canonical order"
    );
}

#[test]
fn a_syntax_error_answers_false_with_the_position_the_parser_stopped_at() {
    let (_dir, registry) = registry();
    let malformed = format!("{} : {} =", code(CAT), code(COVERING));
    let outcome = validated(&registry, &malformed);
    assert!(!outcome.result);
    let message = outcome.message.expect("a message");
    assert!(
        message.contains("not valid compositional grammar"),
        "{message}"
    );
    assert!(
        message.contains(&format!("at byte {}", malformed.len())),
        "the message names the byte the grammar stopped at: {message}"
    );
}

#[test]
fn an_expression_naming_a_concept_the_version_lacks_answers_false_and_names_it() {
    let (_dir, registry) = registry();
    let absent = snomed::sctid(snomed::Item::raw(4242));
    let expression = format!("{} : {} = {absent}", code(CAT), code(COVERING));
    let outcome = validated(&registry, &expression);
    assert!(!outcome.result);
    let message = outcome.message.expect("a message");
    assert!(
        message.contains(&absent) && message.contains("does not define"),
        "the message names the concept the version lacks: {message}"
    );
}

#[test]
fn lookup_answers_the_canonical_expression_and_a_display_carrying_the_terms() {
    let (_dir, registry) = registry();
    let outcome = looked_up(&registry, &refined(CAT, COVERING, FUR));
    assert_eq!(
        outcome.code,
        format!("=== {} : {} = {}", code(CAT), code(COVERING), code(FUR))
    );
    // "If no term or description template has been published, the full
    // expression with terms embedded may be used"
    // (<https://hl7.org/fhir/R4B/snomedct.html>, "Display").
    assert_eq!(
        outcome.display,
        format!(
            "=== {} |Cat| : {} |Has covering| = {} |Fur|",
            code(CAT),
            code(COVERING),
            code(FUR)
        )
    );
    assert!(
        outcome
            .designations
            .iter()
            .any(|d| d.value == outcome.display),
        "the generated display is the expression's one designation"
    );
}

#[test]
fn lookup_answers_the_display_in_the_language_the_request_asks_for() {
    let (_dir, registry) = registry();
    let input = lookup::LookupInput {
        system: Some(SYSTEM.to_owned()),
        code: Some(code(CAT)),
        display_language: Some(String::from("nl")),
        ..lookup::LookupInput::default()
    };
    let concept = lookup::lookup(&registry, &Invocation::Type, &input).expect("looks up");
    assert_eq!(concept.display, "Kat");
    let input = lookup::LookupInput {
        system: Some(SYSTEM.to_owned()),
        code: Some(refined(CAT, COVERING, FUR)),
        display_language: Some(String::from("nl")),
        ..lookup::LookupInput::default()
    };
    let expression = lookup::lookup(&registry, &Invocation::Type, &input).expect("looks up");
    assert!(
        expression.display.contains("|Kat|"),
        "the terms are the edition's own in the requested language: {}",
        expression.display
    );
}

#[test]
fn lookup_generates_the_normal_form_of_an_expression() {
    let (_dir, registry) = registry();
    let outcome = looked_up(&registry, &refined(ANIMAL, COVERING, FUR));
    let terse = outcome
        .properties
        .iter()
        .find(|p| p.code == "normalFormTerse")
        .expect("normalFormTerse");
    // The focus concept keeps its own definition's attributes beside the
    // expression's refinement; the animal has none of its own.
    assert_eq!(
        format!("{:?}", terse.value),
        format!(
            "String(\"=== {} : {} = {}\")",
            code(ANIMAL),
            code(COVERING),
            code(FUR)
        )
    );
    let with_terms = outcome
        .properties
        .iter()
        .find(|p| p.code == "normalForm")
        .expect("normalForm");
    assert!(
        format!("{:?}", with_terms.value).contains("|Animal|"),
        "{:?}",
        with_terms.value
    );
}

#[test]
fn an_expression_states_its_own_definition_status_as_a_property() {
    let (_dir, registry) = registry();
    // "'equivalent to' is the assumed definition status when it is not
    // explicitly stated" (the Compositional Grammar specification, §6.7).
    let assumed = looked_up(&registry, &refined(CAT, COVERING, FUR));
    let sufficiently_defined = |outcome: &lookup::LookupOutcome| {
        format!(
            "{:?}",
            outcome
                .properties
                .iter()
                .find(|p| p.code == "sufficientlyDefined")
                .expect("sufficientlyDefined")
                .value
        )
    };
    assert_eq!(sufficiently_defined(&assumed), "Boolean(true)");
    let primitive = looked_up(&registry, &format!("<<< {}", refined(CAT, COVERING, FUR)));
    assert_eq!(sufficiently_defined(&primitive), "Boolean(false)");
}

#[test]
fn an_expression_with_one_focus_concept_and_no_refinement_is_that_concept() {
    let (_dir, registry) = registry();
    // `=== X` states conditions that are necessary and sufficient, so it means
    // exactly `X` (the Compositional Grammar specification, §6.7).
    assert_eq!(
        outcome(&registry, &format!("=== {}", code(CAT)), &code(CAT)),
        Outcome::Equivalent
    );
    // `<<< X` states conditions that are necessary only, so it is a subtype.
    assert_eq!(
        outcome(&registry, &format!("<<< {}", code(CAT)), &code(CAT)),
        Outcome::SubsumedBy
    );
    assert_eq!(
        outcome(&registry, &code(CAT), &format!("<<< {}", code(CAT))),
        Outcome::Subsumes
    );
}

#[test]
fn a_focus_concept_alone_decides_subsumption_by_the_hierarchy() {
    let (_dir, registry) = registry();
    assert_eq!(
        outcome(&registry, &format!("=== {}", code(ANIMAL)), &code(CAT)),
        Outcome::Subsumes
    );
    assert_eq!(
        outcome(&registry, &format!("=== {}", code(CAT)), &code(ANIMAL)),
        Outcome::SubsumedBy
    );
    assert_eq!(
        outcome(&registry, &format!("=== {}", code(CAT)), &code(FISH)),
        Outcome::NotSubsumed,
        "the fish is under no concept the cat is under"
    );
}

#[test]
fn a_refinement_narrows_the_focus_concept() {
    let (_dir, registry) = registry();
    let broad = format!("=== {}", code(ANIMAL));
    let narrow = format!("=== {}", refined(ANIMAL, COVERING, FUR));
    assert_eq!(outcome(&registry, &broad, &narrow), Outcome::Subsumes);
    assert_eq!(outcome(&registry, &narrow, &broad), Outcome::SubsumedBy);
    // The cat's own definition carries the covering, so the refined animal
    // subsumes it.
    assert_eq!(outcome(&registry, &narrow, &code(CAT)), Outcome::Subsumes);
}

#[test]
fn a_grouped_attribute_is_more_specific_than_the_same_attribute_ungrouped() {
    let (_dir, registry) = registry();
    // "Relationship groups refine inheritance, i.e., a grouped set of
    // attributes is more specific than the same attributes that are not
    // grouped" (the SNOMED CT Editorial Guide, Relationship Group).
    let ungrouped = format!("=== {}", refined(ANIMAL, COVERING, FUR));
    let grouped = format!(
        "=== {} : {{ {} = {} }}",
        code(ANIMAL),
        code(COVERING),
        code(FUR)
    );
    assert_eq!(
        outcome(&registry, &grouped, &ungrouped),
        Outcome::SubsumedBy
    );
    assert_eq!(outcome(&registry, &ungrouped, &grouped), Outcome::Subsumes);
}

#[test]
fn a_group_is_matched_by_one_group_of_the_subsumee() {
    let (_dir, registry) = registry();
    // The cat carries both attributes in one group; the dog carries them in
    // two, so only the cat answers a two-attribute group.
    let group = format!(
        "=== {} : {{ {} = {}, {} = #4 }}",
        code(ANIMAL),
        code(COVERING),
        code(FUR),
        code(LEGS)
    );
    assert_eq!(
        outcome(&registry, &group, &code(CAT)),
        Outcome::Equivalent,
        "the group is the cat's own definition"
    );
    assert_eq!(
        outcome(&registry, &group, &code(DOG)),
        Outcome::SubsumedBy,
        "the dog states the two attributes in two groups, which one group satisfies"
    );
}

#[test]
fn a_concrete_value_is_compared_for_equality_only() {
    let (_dir, registry) = registry();
    let four = format!("=== {} : {} = #4", code(ANIMAL), code(LEGS));
    let three = format!("=== {} : {} = #3", code(ANIMAL), code(LEGS));
    assert_eq!(outcome(&registry, &four, &code(CAT)), Outcome::Subsumes);
    assert_eq!(
        outcome(&registry, &three, &code(CAT)),
        Outcome::NotSubsumed,
        "a concrete value the concept does not carry is not matched"
    );
}

#[test]
fn an_expression_is_compared_with_another_expression_both_ways() {
    let (_dir, registry) = registry();
    let broad = format!("=== {}", refined(ANIMAL, COVERING, FUR));
    let narrow = format!(
        "=== {} : {} = {}, {} = #4",
        code(ANIMAL),
        code(COVERING),
        code(FUR),
        code(LEGS)
    );
    assert_eq!(outcome(&registry, &broad, &narrow), Outcome::Subsumes);
    assert_eq!(outcome(&registry, &narrow, &broad), Outcome::SubsumedBy);
    assert_eq!(outcome(&registry, &narrow, &narrow), Outcome::Equivalent);
    let unrelated = format!("=== {}", code(FISH));
    assert_eq!(
        outcome(&registry, &narrow, &unrelated),
        Outcome::NotSubsumed
    );
}

#[test]
fn closure_relates_an_expression_to_the_concepts_a_table_holds() {
    let (_dir, registry) = registry();
    let member = |code: String| closure::Member {
        system: SYSTEM.to_owned(),
        version: None,
        code,
    };
    let expression = format!("=== {}", refined(ANIMAL, COVERING, FUR));
    let edges = closure::relate(
        &registry,
        &[member(code(CAT)), member(code(FISH))],
        &[member(expression.clone())],
    )
    .expect("relates");
    // The relationship is read from target to source
    // (<https://hl7.org/fhir/R4B/terminology-service.html>, "Maintaining a
    // Closure Table"), so the expression subsumes the cat.
    let cat = edges
        .iter()
        .find(|edge| edge.target.code == code(CAT))
        .expect("an edge to the cat");
    assert_eq!(cat.source.code, expression);
    assert_eq!(cat.relationship, Relationship::Specializes);
    assert!(
        edges.iter().all(|edge| edge.target.code != code(FISH)),
        "two concepts that subsume neither way get no entry"
    );
}

/// The value set an `expressions` filter builds, with `value` when one is
/// given and the concept filter alone otherwise.
fn value_set(expressions: Option<&str>) -> serde_json::Value {
    let mut filters = vec![serde_json::json!({
        "property": "concept",
        "op": "is-a",
        "value": code(ANIMAL),
    })];
    if let Some(value) = expressions {
        filters.push(serde_json::json!({
            "property": "expressions",
            "op": "=",
            "value": value,
        }));
    }
    serde_json::json!({
        "resourceType": "ValueSet",
        "url": "http://example.org/vs/animals",
        "status": "active",
        "compose": { "include": [{ "system": SYSTEM, "filter": filters }] },
    })
}

fn in_value_set(expressions: Option<&str>, code: &str) -> bool {
    let (_dir, registry) = registry();
    let sets = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        sets.path().join("valueset-animals.json"),
        serde_json::to_string(&value_set(expressions)).expect("serialises"),
    )
    .expect("writes");
    let mut value_sets = ValueSetStore::new();
    for model in valueset::load::load_dir(sets.path(), FhirVersion::R4B).expect("loads") {
        value_sets.insert(model).expect("stores");
    }
    let concept_maps = ConceptMapStore::new();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let request = ValueSetValidateInput {
        url: Some(String::from("http://example.org/vs/animals")),
        system: Some(SYSTEM.to_owned()),
        code: Some(code.to_owned()),
        ..ValueSetValidateInput::default()
    };
    value_set_validate_code::validate_code(&sources, &request)
        .expect("validates")
        .result
}

#[test]
fn the_expressions_filter_decides_whether_a_value_set_admits_an_expression() {
    // "Specify whether post-coordination is allowed or not"
    // (<https://hl7.org/fhir/R4B/snomedct.html>, "Filter Properties").
    let expression = format!("=== {}", refined(CAT, COVERING, FUR));
    assert!(
        in_value_set(Some("true"), &expression),
        "`expressions = true` admits an expression whose focus concepts the value set holds"
    );
    assert!(
        !in_value_set(Some("false"), &expression),
        "`expressions = false` refuses it"
    );
    assert!(
        !in_value_set(None, &expression),
        "a value set that states no `expressions` filter refuses it"
    );
    // The precoordinated concept is a member either way.
    for filter in [Some("true"), Some("false"), None] {
        assert!(in_value_set(filter, &code(CAT)), "{filter:?}");
    }
}

#[test]
fn an_expression_outside_the_value_set_is_refused_even_where_expressions_are_allowed() {
    // The fish sits under no concept, so it is outside `is-a <animal>`.
    let outside = format!("=== {}", code(FISH));
    assert!(!in_value_set(Some("true"), &outside));
}

#[test]
fn an_expression_the_grammar_refuses_is_never_a_member() {
    let malformed = format!("{} : {} =", code(CAT), code(COVERING));
    assert!(!in_value_set(Some("true"), &malformed));
}

#[test]
fn a_nested_sub_expression_validates_and_renders() {
    let (_dir, registry) = registry();
    // `expressionValue = conceptReference / "(" ws subExpression ws ")"` (the
    // Compositional Grammar specification, §5).
    let nested = format!(
        "{} : {} = ({} : {} = #4)",
        code(ANIMAL),
        code(COVERING),
        code(FUR),
        code(LEGS)
    );
    let outcome = validated(&registry, &nested);
    assert!(outcome.result, "{:?}", outcome.message);
    assert_eq!(
        outcome.normalized_code.as_deref(),
        Some(
            format!(
                "=== {} : {} = ({} : {} = #4)",
                code(ANIMAL),
                code(COVERING),
                code(FUR),
                code(LEGS)
            )
            .as_str()
        )
    );
}

#[test]
fn two_spellings_of_one_expression_answer_the_same_code() {
    let (_dir, registry) = registry();
    // Whitespace is not significant outside a term or a quoted value (the
    // Compositional Grammar specification, §5), and the focus concepts of a
    // conjunction have no stated order.
    let spaced = format!("===   {}   +   {}   ", code(CAT), code(DOG));
    let reversed = format!("==={}+{}", code(DOG), code(CAT));
    assert_eq!(
        looked_up(&registry, &spaced).code,
        looked_up(&registry, &reversed).code
    );
}

#[test]
fn an_expression_over_an_inactive_concept_validates_and_is_marked_inactive() {
    let (_dir, registry) = registry();
    // The concept references "must refer to active concepts in the given
    // version and edition of SNOMED CT" (the Compositional Grammar
    // specification, §7.3 Validating), and inactive is not invalid on this
    // server (<https://hl7.org/fhir/R4B/snomedct.html>, "Inactive").
    let expression = format!("=== {}", code(FISH));
    let outcome = validated(&registry, &expression);
    assert!(outcome.result, "{:?}", outcome.message);
    assert_eq!(outcome.inactive, Some(true));
    let looked = looked_up(&registry, &expression);
    let inactive = looked
        .properties
        .iter()
        .find(|p| p.code == "inactive")
        .expect("inactive");
    assert_eq!(format!("{:?}", inactive.value), "Boolean(true)");
    // An expression over active concepts alone stays active.
    assert_eq!(
        validated(&registry, &refined(CAT, COVERING, FUR)).inactive,
        None
    );
}

#[test]
fn an_expression_answers_its_own_refinement_as_concept_model_properties() {
    let (_dir, registry) = registry();
    // The relationship-type properties are "the value of the … attribute in
    // the definition of the given code or expression"
    // (<https://hl7.org/fhir/R4B/snomedct.html>, "SNOMED CT Properties").
    let outcome = looked_up(&registry, &refined(ANIMAL, COVERING, FUR));
    let covering = outcome
        .properties
        .iter()
        .find(|p| p.code == code(COVERING))
        .expect("the covering attribute");
    assert_eq!(
        format!("{:?}", covering.value),
        format!("Code({:?})", code(FUR))
    );
}

#[test]
fn a_value_set_that_enumerates_an_expression_contains_it() {
    let (_dir, registry) = registry();
    // `ValueSet.compose.include.concept` "specifies a concept to be included
    // in the value set"
    // (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.compose.include.concept>),
    // and vsd-3 forbids a filter beside it, so no `expressions` filter can sit
    // there to admit the expression the value set already names.
    let written = refined(CAT, COVERING, FUR);
    let enumerated = serde_json::json!({
        "resourceType": "ValueSet",
        "url": "http://example.org/vs/enumerated",
        "status": "active",
        "compose": { "include": [{
            "system": SYSTEM,
            "concept": [{ "code": written }],
        }] },
    });
    let sets = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        sets.path().join("valueset-enumerated.json"),
        serde_json::to_string(&enumerated).expect("serialises"),
    )
    .expect("writes");
    let mut value_sets = ValueSetStore::new();
    for model in valueset::load::load_dir(sets.path(), FhirVersion::R4B).expect("loads") {
        value_sets.insert(model).expect("stores");
    }
    let concept_maps = ConceptMapStore::new();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let check = |code: &str| {
        value_set_validate_code::validate_code(
            &sources,
            &ValueSetValidateInput {
                url: Some(String::from("http://example.org/vs/enumerated")),
                system: Some(SYSTEM.to_owned()),
                code: Some(code.to_owned()),
                ..ValueSetValidateInput::default()
            },
        )
        .expect("validates")
    };
    let outcome = check(&written);
    assert!(outcome.result, "{:?}", outcome.message);
    // The same expression written another way is the same member.
    let spaced = format!("===  {}  :  {} = {}", code(CAT), code(COVERING), code(FUR));
    assert!(check(&spaced).result);
    // Another expression the value set never names is not.
    assert!(!check(&format!("=== {}", code(DOG))).result);
}

#[test]
fn exclude_post_coordinated_drops_an_enumerated_expression_from_an_expansion() {
    let (_dir, registry) = registry();
    // "Controls whether or not the value set expansion includes post
    // coordinated codes"
    // (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>,
    // `excludePostCoordinated`).
    let compose = Compose {
        include: vec![Include {
            system: Some(SystemRef {
                url: SYSTEM.to_owned(),
                version: None,
            }),
            concepts: vec![
                ConceptRef {
                    code: refined(CAT, COVERING, FUR),
                    display: None,
                    deprecated: false,
                },
                ConceptRef {
                    code: code(DOG),
                    display: None,
                    deprecated: false,
                },
            ],
            ..Include::default()
        }],
        ..Compose::default()
    };
    let expanded = |exclude: bool| {
        let options = Options {
            exclude_post_coordinated: exclude,
            ..Options::default()
        };
        Expander::new(&registry)
            .expand(&compose, &options)
            .expect("expands")
            .items
            .into_iter()
            .map(|item| item.code)
            .collect::<Vec<String>>()
    };
    assert_eq!(
        expanded(false),
        vec![refined(CAT, COVERING, FUR), code(DOG)],
        "an entry keeps the spelling the value set used"
    );
    assert_eq!(expanded(true), vec![code(DOG)]);
}

#[test]
fn an_expression_is_refused_by_a_system_that_is_not_snomed() {
    let (_dir, registry) = registry();
    let input = lookup::LookupInput {
        system: Some(String::from("http://example.org/other")),
        code: Some(refined(CAT, COVERING, FUR)),
        ..lookup::LookupInput::default()
    };
    let error = lookup::lookup(&registry, &Invocation::Type, &input).expect_err("refuses");
    assert!(
        matches!(error, OperationError::UnknownSystem(_)),
        "{error:?}"
    );
}
