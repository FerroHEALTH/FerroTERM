// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

use serde_json::Value;

use super::*;

/// The `ConceptMap` a save of `draft` writes at `version`.
fn saved(draft: &MapDraft, version: FhirVersion) -> String {
    draft
        .body(version)
        .expect("the draft's stored map is a ConceptMap at its release")
}

/// `held`, a `ConceptMap` the server holds, opened at `version`.
fn opened(version: FhirVersion, held: &Value) -> MapDraft {
    version
        .decode(&held.to_string())
        .expect("the server's own answer decodes at its release")
}

/// A draft with one group and one mapped code.
fn one_mapping(relationship: &str) -> MapDraft {
    let mut draft = MapDraft::new();
    let mut keys = Keys::default();
    let group = keys.next();
    let element = keys.next();
    let target = keys.next();
    draft.url = String::from("https://terminology.example/colour-map");
    draft.source_scope = String::from("https://terminology.example/colours-vs");
    draft.target_scope = String::from("https://terminology.example/paints-vs");
    draft.groups = vec![MapGroup {
        key: group,
        source: String::from("https://terminology.example/colours"),
        source_version: String::from("1.0.0"),
        target: String::from("https://terminology.example/paints"),
        target_version: String::new(),
        elements: vec![MapElement {
            key: element,
            code: String::from("red"),
            display: String::from("Red"),
            no_map: false,
            targets: vec![MapTarget {
                key: target,
                code: String::from("vermilion"),
                display: String::from("Vermilion"),
                relationship: relationship.to_owned(),
                comment: String::new(),
            }],
        }],
    }];
    draft.keys = keys;
    draft
}

#[test]
fn a_target_without_a_relationship_is_refused() {
    let draft = one_mapping("");
    assert_eq!(
        draft.problems(FhirVersion::R4B),
        vec![Problem::RelationshipMissing {
            element: String::from("red"),
            code: String::from("vermilion"),
        }],
        "every version makes the element 1..1, so a target without one is refused"
    );
    assert_ne!(draft.problems(FhirVersion::R4B), Vec::<Problem>::new());
    assert!(
        !draft.problems(FhirVersion::R5).is_empty(),
        "R5 makes `relationship` mandatory the same way R4B makes `equivalence` mandatory"
    );
}

#[test]
fn a_target_that_states_its_relationship_is_savable() {
    let draft = one_mapping("equivalent");
    for version in [
        FhirVersion::R4,
        FhirVersion::R4B,
        FhirVersion::R5,
        FhirVersion::R6,
    ] {
        assert!(
            draft.problems(version).is_empty(),
            "{version:?} takes a target that states its relationship: {:?}",
            draft.problems(version)
        );
    }
}

#[test]
fn a_draft_with_no_canonical_or_status_says_both() {
    let mut draft = MapDraft::default();
    assert_eq!(
        draft.problems(FhirVersion::R5),
        vec![Problem::NoCanonical, Problem::NoStatus]
    );
    draft.url = String::from("https://terminology.example/m");
    draft.status = String::from("draft");
    assert_eq!(draft.problems(FhirVersion::R5), Vec::<Problem>::new());
}

#[test]
fn a_group_that_names_only_one_system_is_refused() {
    let mut draft = one_mapping("equivalent");
    draft
        .groups
        .first_mut()
        .expect("the draft carries one group")
        .target = String::new();
    assert!(
        draft
            .problems(FhirVersion::R4B)
            .contains(&Problem::GrouplessSystems),
        "a group maps between two systems, so one of them is not enough"
    );
}

#[test]
fn no_map_and_a_target_are_exclusive_on_a_version_that_defines_no_map() {
    let mut draft = one_mapping("equivalent");
    draft
        .groups
        .first_mut()
        .expect("the draft carries one group")
        .elements
        .first_mut()
        .expect("the group carries one element")
        .no_map = true;
    assert_eq!(
        draft.problems(FhirVersion::R5),
        vec![Problem::MappedAndUnmapped {
            element: String::from("red"),
        }],
        "a code cannot both map to nothing and map to something"
    );
    assert!(
        draft.problems(FhirVersion::R4B).is_empty(),
        "R4B defines no `noMap`, so the flag is not a fact it can state"
    );
}

#[test]
fn the_version_decides_which_element_a_relationship_is_written_under() {
    let draft = one_mapping("equivalent");
    let r4b = saved(&draft, FhirVersion::R4B);
    assert!(
        r4b.contains(r#""equivalence":"equivalent""#),
        "R4B states a target's relation in `equivalence`: {r4b}"
    );
    assert!(
        !r4b.contains(r#""relationship""#),
        "and never in R5's element: {r4b}"
    );
    let r5 = saved(&draft, FhirVersion::R5);
    assert!(
        r5.contains(r#""relationship":"equivalent""#),
        "R5 states it in `relationship`: {r5}"
    );
    assert!(
        !r5.contains(r#""equivalence""#),
        "and never in R4B's element: {r5}"
    );
}

#[test]
fn the_version_decides_which_element_scopes_the_map() {
    let draft = one_mapping("equivalent");
    let r4b = saved(&draft, FhirVersion::R4B);
    assert!(
        r4b.contains(r#""sourceCanonical":"https://terminology.example/colours-vs""#),
        "R4B scopes a map with `source[x]`: {r4b}"
    );
    assert!(!r4b.contains("sourceScope"), "{r4b}");
    let r6 = saved(&draft, FhirVersion::R6);
    assert!(
        r6.contains(r#""sourceScopeCanonical":"https://terminology.example/colours-vs""#),
        "R6 scopes it with `sourceScope[x]`: {r6}"
    );
    assert!(!r6.contains(r#""sourceCanonical""#), "{r6}");
}

#[test]
fn a_group_s_system_version_is_an_element_on_r4b_and_a_canonical_on_r5() {
    let draft = one_mapping("equivalent");
    let r4b = saved(&draft, FhirVersion::R4B);
    assert!(
        r4b.contains(r#""source":"https://terminology.example/colours""#)
            && r4b.contains(r#""sourceVersion":"1.0.0""#),
        "R4B states the system as a uri with its version beside it: {r4b}"
    );
    let r5 = saved(&draft, FhirVersion::R5);
    assert!(
        r5.contains(r#""source":"https://terminology.example/colours|1.0.0""#),
        "R5 states it as a canonical carrying its own version: {r5}"
    );
    assert!(!r5.contains("sourceVersion"), "{r5}");
}

#[test]
fn no_map_is_written_only_on_a_version_that_defines_it() {
    let mut draft = one_mapping("equivalent");
    let element = draft
        .groups
        .first_mut()
        .expect("the draft carries one group")
        .elements
        .first_mut()
        .expect("the group carries one element");
    element.no_map = true;
    element.targets.clear();
    let r5 = saved(&draft, FhirVersion::R5);
    assert!(r5.contains(r#""noMap":true"#), "{r5}");
    let r4b = saved(&draft, FhirVersion::R4B);
    assert!(
        !r4b.contains("noMap"),
        "R4B defines no such element, so writing one would be refused: {r4b}"
    );
}

#[test]
fn an_element_the_form_never_drew_survives_a_save() {
    let held: Value = serde_json::from_str(
        r#"{"resourceType":"ConceptMap","id":"m","meta":{"versionId":"3"},
            "url":"https://terminology.example/colour-map","status":"active",
            "name":"ColourMap","publisher":"Someone",
            "sourceUri":"https://terminology.example/colours-vs",
            "group":[{"source":"https://terminology.example/colours",
              "target":"https://terminology.example/paints",
              "unmapped":{"mode":"fixed","code":"unknown"},
              "element":[{"code":"red","target":[
                {"code":"vermilion","equivalence":"equivalent","comment":"kept"}]}]}]}"#,
    )
    .expect("the server's own answer parses");
    let draft = opened(FhirVersion::R4B, &held);
    assert_eq!(draft.version_id, "3");
    assert_eq!(draft.source_scope, "https://terminology.example/colours-vs");
    assert!(draft.managed());
    let saved = saved(&draft, FhirVersion::R4B);
    assert!(saved.contains(r#""publisher":"Someone""#), "{saved}");
    assert!(saved.contains(r#""name":"ColourMap""#), "{saved}");
    assert!(saved.contains(r#""unmapped""#), "{saved}");
    assert!(saved.contains(r#""comment":"kept""#), "{saved}");
}

#[test]
fn a_map_read_on_one_root_reaches_no_other_root_with_its_elements() {
    let held: Value = serde_json::from_str(
        r#"{"resourceType":"ConceptMap","url":"https://terminology.example/m",
            "status":"active","sourceUri":"https://terminology.example/vs",
            "group":[{"source":"https://terminology.example/a",
              "target":"https://terminology.example/b",
              "element":[{"code":"x","target":[
                {"code":"y","equivalence":"wider"}]}]}]}"#,
    )
    .expect("the server's own answer parses");
    let draft = opened(FhirVersion::R4B, &held);
    let refused = draft
        .body(FhirVersion::R5)
        .expect_err("R5 defines no `sourceUri`, so the R4B map is no R5 map");
    assert!(
        refused.to_string().contains("sourceUri"),
        "no element of the root it was read from survives into the one it is sent to: {refused}"
    );
}

#[test]
fn an_unmanaged_resource_states_no_version_to_update_from() {
    let held: Value = serde_json::from_str(
        r#"{"resourceType":"ConceptMap","url":"https://terminology.example/built","status":"active"}"#,
    )
    .expect("the server's own answer parses");
    assert!(
        !opened(FhirVersion::R4B, &held).managed(),
        "a resource the REST API never stamped has no version an update could state"
    );
}

#[test]
fn a_canonical_splits_into_the_system_and_the_version_it_carries() {
    assert_eq!(
        split_canonical("https://x.example/s|2.0"),
        (String::from("https://x.example/s"), String::from("2.0"))
    );
    assert_eq!(
        split_canonical("https://x.example/s"),
        (String::from("https://x.example/s"), String::new())
    );
    assert_eq!(canonical("https://x.example/s", ""), "https://x.example/s");
    assert_eq!(
        canonical("https://x.example/s", "2.0"),
        "https://x.example/s|2.0"
    );
}

/// `cmd-1` is an error on R4, R4B and R5 and a warning on the R6 ballot,
/// and each version names its own codes
/// (<https://hl7.org/fhir/R4B/conceptmap.html>,
/// <https://hl7.org/fhir/R5/conceptmap.html>).
#[test]
fn a_relationship_the_version_requires_a_comment_on_is_refused_uncommented() {
    let mut draft = one_mapping("narrower");
    draft.status = String::from("active");
    assert_eq!(
        draft.problems(FhirVersion::R4B),
        vec![Problem::CommentRequired {
            element: String::from("red"),
            code: String::from("vermilion"),
            relationship: String::from("narrower"),
        }]
    );
    let mut broader = one_mapping("source-is-broader-than-target");
    broader.status = String::from("active");
    assert!(
        broader
            .problems(FhirVersion::R5)
            .iter()
            .any(|problem| matches!(problem, Problem::CommentRequired { .. })),
        "R5 names its own two codes"
    );
    assert!(
        broader.problems(FhirVersion::R6).is_empty(),
        "the R6 ballot demoted cmd-1 to a warning, so the editor refuses nothing for it"
    );
}

#[test]
fn a_draft_map_is_excused_the_comment_on_the_versions_that_excuse_it() {
    let broader = one_mapping("source-is-broader-than-target");
    assert_eq!(broader.status, "draft");
    assert!(
        broader.problems(FhirVersion::R5).is_empty(),
        "R5 cmd-1 carries a `status = draft` arm"
    );
    let narrower = one_mapping("narrower");
    assert!(
        !narrower.problems(FhirVersion::R4B).is_empty(),
        "R4B cmd-1 carries no such arm, so a draft is refused too"
    );
}

#[test]
fn a_target_that_carries_a_comment_satisfies_the_invariant() {
    let mut draft = one_mapping("narrower");
    draft.status = String::from("active");
    draft
        .groups
        .first_mut()
        .expect("the draft carries one group")
        .elements
        .first_mut()
        .expect("the group carries one element")
        .targets
        .first_mut()
        .expect("the element carries one target")
        .comment = String::from("Vermilion is one shade of red.");
    assert_eq!(draft.problems(FhirVersion::R4B), Vec::<Problem>::new());
}

#[test]
fn a_group_with_no_code_is_refused() {
    let mut draft = one_mapping("equivalent");
    draft
        .groups
        .first_mut()
        .expect("the draft carries one group")
        .elements
        .clear();
    assert!(
        draft
            .problems(FhirVersion::R5)
            .contains(&Problem::GroupWithoutCodes),
        "`ConceptMap.group.element` is 1..* in every served version"
    );
}

#[test]
fn every_problem_reads_as_a_sentence() {
    for problem in [
        Problem::NoCanonical,
        Problem::NoStatus,
        Problem::GrouplessSystems,
        Problem::GroupWithoutCodes,
        Problem::CommentRequired {
            element: String::from("red"),
            code: String::from("vermilion"),
            relationship: String::from("narrower"),
        },
        Problem::RelationshipMissing {
            element: String::from("red"),
            code: String::from("vermilion"),
        },
        Problem::MappedAndUnmapped {
            element: String::from("red"),
        },
    ] {
        assert!(
            !problem
                .text(&ferrohealth_ui::i18n::Locale::default())
                .is_empty(),
            "{problem:?} renders as nothing"
        );
    }
}

#[test]
fn each_version_offers_the_value_set_its_own_element_is_bound_to() {
    for version in [FhirVersion::R4, FhirVersion::R4B] {
        assert_eq!(
            dialect(version).relationship_value_set,
            EQUIVALENCE_VALUE_SET
        );
    }
    for version in [FhirVersion::R5, FhirVersion::R6] {
        assert_eq!(
            dialect(version).relationship_value_set,
            RELATIONSHIP_VALUE_SET
        );
    }
}

#[test]
fn a_scope_that_names_a_mapped_code_system_is_refused() {
    let mut draft = one_mapping("equivalent");
    draft.source_scope = String::from("https://terminology.example/colours|1.0.0");
    draft.target_scope = String::from("https://terminology.example/paints");
    for version in [
        FhirVersion::R4,
        FhirVersion::R4B,
        FhirVersion::R5,
        FhirVersion::R6,
    ] {
        assert_eq!(
            draft.problems(version),
            vec![
                Problem::ScopeIsCodeSystem {
                    side: Side::Source,
                    scope: String::from("https://terminology.example/colours|1.0.0"),
                },
                Problem::ScopeIsCodeSystem {
                    side: Side::Target,
                    scope: String::from("https://terminology.example/paints"),
                },
            ],
            "every version scopes a map with a value set, never a code system it maps"
        );
    }
}

#[test]
fn a_scope_that_names_a_value_set_or_nothing_is_savable() {
    let mut draft = one_mapping("equivalent");
    assert_eq!(draft.problems(FhirVersion::R4), Vec::<Problem>::new());
    draft.source_scope = String::new();
    draft.target_scope = String::new();
    assert_eq!(draft.problems(FhirVersion::R5), Vec::<Problem>::new());
}

#[test]
fn the_scope_refusal_names_the_scope_and_the_definition() {
    let locale = ferrohealth_ui::i18n::Locale::default();
    let text = Problem::ScopeIsCodeSystem {
        side: Side::Source,
        scope: String::from("https://terminology.example/colours"),
    }
    .text(&locale);
    assert!(
        text.contains("https://terminology.example/colours"),
        "{text}"
    );
    assert!(text.contains("ConceptMap.source[x]"), "{text}");
}

/// A `searchset` carrying an `OperationOutcome` and the concept map `m`.
const SEARCHED: &str = r#"{"resourceType":"Bundle","type":"searchset","entry":[
    {"resource":{"resourceType":"OperationOutcome",
      "issue":[{"severity":"information","code":"informational"}]}},
    {"resource":{"resourceType":"ConceptMap","id":"m","status":"active"}}]}"#;

#[test]
fn a_searchset_answers_the_first_resource_of_the_type_that_was_searched_for() {
    let maps: crate::fhir::searchset::Found<MapDraft> = FhirVersion::R4B
        .decode(SEARCHED)
        .expect("the server's own answer decodes at R4B");
    assert_eq!(maps.found().first().map(|map| map.id.as_str()), Some("m"));
    let systems: crate::fhir::searchset::Found<crate::authoring::code_system::Draft> =
        FhirVersion::R4B
            .decode(SEARCHED)
            .expect("the server's own answer decodes at R4B");
    assert_eq!(
        systems.found(),
        Vec::<&crate::authoring::code_system::Draft>::new(),
        "a search for one type does not answer another"
    );
}

/// A map's scope is `source[x]` on R4 and R4B and `sourceScope[x]` on R5
/// and R6 (<https://hl7.org/fhir/R4B/conceptmap.html>,
/// <https://hl7.org/fhir/R5/conceptmap.html>), so each release's map is read
/// in its own elements.
#[test]
fn a_stored_concept_map_reads_the_scope_under_each_release_s_spelling() {
    let r4b = opened(
        FhirVersion::R4B,
        &serde_json::from_str(
            r#"{"resourceType":"ConceptMap","id":"m","meta":{"versionId":"2"},
                "url":"https://terminology.example/m","version":"1","status":"active",
                "sourceUri":"https://terminology.example/vs-a",
                "targetCanonical":"https://terminology.example/vs-b",
                "group":[{"source":"https://terminology.example/a","sourceVersion":"1.0",
                  "target":"https://terminology.example/b",
                  "element":[{"code":"x","display":"Ex","target":[
                    {"code":"y","display":"Why","equivalence":"wider","comment":"c"}]}]}]}"#,
        )
        .expect("the fixture is JSON"),
    );
    assert_eq!(r4b.version_id, "2");
    assert_eq!(r4b.source_scope, "https://terminology.example/vs-a");
    assert_eq!(r4b.target_scope, "https://terminology.example/vs-b");
    let group = r4b.groups.first().expect("the map carries one group");
    assert_eq!(group.source_version, "1.0");
    let element = group.elements.first().expect("the group carries one code");
    assert!(!element.no_map, "R4B defines no `noMap`");
    let target = element
        .targets
        .first()
        .expect("the code carries one target");
    assert_eq!(target.relationship, "wider");

    let r5 = opened(
        FhirVersion::R5,
        &serde_json::from_str(
            r#"{"resourceType":"ConceptMap","url":"https://terminology.example/m","status":"draft",
                "sourceScopeCanonical":"https://terminology.example/vs-a",
                "group":[{"source":"https://terminology.example/a|1.0",
                  "target":"https://terminology.example/b",
                  "element":[{"code":"x","noMap":true}]}]}"#,
        )
        .expect("the fixture is JSON"),
    );
    assert_eq!(r5.source_scope, "https://terminology.example/vs-a");
    assert_eq!(r5.target_scope, "");
    let group = r5.groups.first().expect("the map carries one group");
    assert_eq!(
        group.source_version, "1.0",
        "the version inside the canonical"
    );
    assert!(group.elements.first().is_some_and(|element| element.no_map));
}
