// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The composer's tests.

use super::*;
use crate::fhir::compose::stored::StoredValueSet;
use crate::fhir::version::FhirVersion;

/// The `ValueSet` a save of `draft` sends at R4B, as a JSON value.
fn resource(draft: &Draft) -> Value {
    serde_json::from_str(
        &draft
            .body(FhirVersion::R4B)
            .expect("the draft's stored resource is a ValueSet at R4B"),
    )
    .expect("a save writes JSON")
}

/// The `Parameters` a preview of `page` of `draft` sends at R4B, written
/// with its members in order.
fn preview(draft: &Draft, page: &Preview) -> String {
    serde_json::from_str::<Value>(
        &draft
            .preview_body(FhirVersion::R4B, page)
            .expect("the draft's stored resource is a ValueSet at R4B"),
    )
    .expect("a preview writes JSON")
    .to_string()
}

/// A draft with one include over `system`, and the key of that include.
fn over(system: &str) -> (Draft, u32) {
    let mut draft = Draft::new();
    draft.url = String::from("https://terminology.example/vs/local");
    let key = draft
        .clause_keys(true)
        .first()
        .copied()
        .expect("a new draft opens with one include");
    if let Some(clause) = draft.clause_mut(key) {
        clause.system = system.to_owned();
    }
    (draft, key)
}

#[test]
fn a_clause_naming_nothing_breaks_the_first_invariant() {
    let draft = Draft::new();
    let broken = draft.broken();
    assert_eq!(
        broken.first().map(|row| row.defect),
        Some(Defect::NoSystemOrValueSet),
        "an empty clause selects nothing, which vsd-1 refuses"
    );
    assert_eq!(
        broken.first().map(|row| row.defect.constraint()),
        Some("vsd-1")
    );
    assert!(
        !draft.savable(),
        "a broken draft is never offered for saving"
    );
}

#[test]
fn codes_without_a_system_break_the_second_invariant() {
    let mut draft = Draft::new();
    draft.url = String::from("https://terminology.example/vs/local");
    let key = draft
        .clause_keys(true)
        .first()
        .copied()
        .expect("one include");
    draft.add_value_set(key, "https://terminology.example/vs/national");
    draft.add_concept(key, "A", "Alpha");
    assert_eq!(
        draft.broken().first().map(|row| row.defect),
        Some(Defect::CriteriaWithoutSystem),
        "vsd-2: concepts name the system they are read from"
    );
}

#[test]
fn codes_and_filters_in_one_clause_break_the_third_invariant() {
    let (mut draft, key) = over("https://terminology.example/x");
    draft.add_concept(key, "A", "Alpha");
    draft.add_filter(key, "concept", "is-a");
    let broken = draft.broken();
    assert_eq!(
        broken.first().map(|row| row.defect),
        Some(Defect::ConceptsAndFilters),
        "vsd-3: a filter beside enumerated codes is refused"
    );
    assert_eq!(
        broken.first().map(|row| row.defect.constraint()),
        Some("vsd-3")
    );
    assert_eq!(broken.first().map(|row| row.key), Some(key));
}

#[test]
fn a_value_set_beside_enumerated_codes_is_allowed_and_intersects() {
    let (mut draft, key) = over("https://terminology.example/x");
    draft.add_value_set(key, "https://terminology.example/vs/national");
    draft.add_concept(key, "A", "Alpha");
    assert!(
        draft.broken().is_empty(),
        "vsd-3 forbids concepts beside filters, not beside a value set"
    );
    let rule = draft
        .clauses
        .iter()
        .find(|clause| clause.key == key)
        .map(|clause| clause.rule(&ferrohealth_ui::i18n::Locale::default()))
        .unwrap_or_default();
    assert!(
        rule.contains("is in the value set named here")
            && rule.contains("comes from the code system named here")
            && rule.contains("is one of the 1 codes listed"),
        "the criteria inside one include intersect: `{rule}`"
    );
    assert!(rule.starts_with("This clause draws in"), "{rule}");
}

#[test]
fn several_value_sets_in_one_clause_are_stated_as_intersecting() {
    let (mut draft, key) = over("");
    draft.add_value_set(key, "https://terminology.example/vs/a");
    draft.add_value_set(key, "https://terminology.example/vs/b");
    let rule = draft
        .clauses
        .iter()
        .find(|clause| clause.key == key)
        .map(|clause| clause.rule(&ferrohealth_ui::i18n::Locale::default()))
        .unwrap_or_default();
    assert!(
        rule.contains("is in every one of the 2 value sets named here"),
        "several valueSet references in one include intersect: `{rule}`"
    );
}

#[test]
fn an_exclude_says_it_takes_codes_back_out() {
    let mut draft = Draft::new();
    let key = draft.add_clause(false);
    draft.add_value_set(key, "https://terminology.example/vs/a");
    let rule = draft
        .clauses
        .iter()
        .find(|clause| clause.key == key)
        .map(|clause| clause.rule(&ferrohealth_ui::i18n::Locale::default()))
        .unwrap_or_default();
    assert!(
        rule.starts_with("This clause takes back out"),
        "exclude subtracts from the union of the includes: `{rule}`"
    );
}

#[test]
fn the_resource_is_the_compose_the_server_reads() {
    let (mut draft, key) = over("https://terminology.example/x");
    draft.name = String::from("LocalSet");
    draft.status = String::from("active");
    draft.inactive = Some(false);
    draft.add_concept(key, "A", "Alpha");
    if let Some(row) = draft
        .clause_mut(key)
        .and_then(|clause| clause.concepts.last_mut())
    {
        row.display = String::from("The author's own wording");
    }
    let national = draft.add_clause(true);
    draft.add_value_set(national, "https://terminology.example/vs/national");
    let excluded = draft.add_clause(false);
    if let Some(clause) = draft.clause_mut(excluded) {
        clause.system = String::from("https://terminology.example/x");
    }
    draft.add_filter(excluded, "concept", "is-a");
    if let Some(row) = draft
        .clause_mut(excluded)
        .and_then(|clause| clause.filters.last_mut())
    {
        row.value = String::from("root");
    }
    assert_eq!(
        resource(&draft).to_string(),
        concat!(
            r#"{"compose":{"exclude":[{"filter":[{"op":"is-a","property":"concept","value":"root"}],"#,
            r#""system":"https://terminology.example/x"}],"inactive":false,"#,
            r#""include":[{"concept":[{"code":"A","display":"The author's own wording"}],"#,
            r#""system":"https://terminology.example/x"},"#,
            r#"{"valueSet":["https://terminology.example/vs/national"]}]},"#,
            r#""name":"LocalSet","resourceType":"ValueSet","status":"active",
"url":"https://terminology.example/vs/local"}"#
        )
        .replace('\n', ""),
        "the display the author wrote overrides the code system's own"
    );
    assert!(draft.savable());
}

#[test]
fn an_exclude_carries_no_display_because_the_server_ignores_one() {
    let mut draft = Draft::new();
    draft.url = String::from("https://terminology.example/vs/local");
    let excluded = draft.add_clause(false);
    if let Some(clause) = draft.clause_mut(excluded) {
        clause.system = String::from("https://terminology.example/x");
    }
    draft.add_concept(excluded, "A", "Alpha");
    if let Some(row) = draft
        .clause_mut(excluded)
        .and_then(|clause| clause.concepts.last_mut())
    {
        row.display = String::from("Something else");
    }
    let written = resource(&draft).to_string();
    assert!(
        written.contains(r#""exclude":[{"concept":[{"code":"A"}]"#),
        "`Any display names specified for the codes are ignored` in an exclude: {written}"
    );
}

#[test]
fn an_update_carries_back_every_element_the_form_does_not_draw() {
    // An update replaces the whole resource
    // (<https://hl7.org/fhir/R4B/http.html#update>), so what the composer
    // does not model has to travel back out of the document it read.
    let document: Value = serde_json::from_str(
        r#"{"resourceType":"ValueSet","id":"local",
            "meta":{"versionId":"3","source":"https://terminology.example/source"},
            "language":"nl","text":{"status":"generated","div":"<div xmlns=\"http://www.w3.org/1999/xhtml\"><p/></div>"},
            "url":"https://terminology.example/vs/local","status":"active",
            "description":"What this value set is for","publisher":"A publisher",
            "experimental":true,"jurisdiction":[{"text":"NL"}],
            "compose":{"lockedDate":"2026-01-01",
              "include":[{"system":"https://terminology.example/x",
                "concept":[{"code":"A","designation":[{"language":"nl","value":"Alfa"}]}]}]}}"#,
    )
    .expect("the server's own answer parses");
    let stored: StoredValueSet = crate::fhir::version::FhirVersion::R4B
        .decode(&document.to_string())
        .expect("the elements the composer reads parse");
    let written = resource(&stored.draft(&document));
    for (element, kept) in [
        ("description", "What this value set is for"),
        ("publisher", "A publisher"),
        ("language", "nl"),
    ] {
        assert_eq!(
            written.get(element).and_then(Value::as_str),
            Some(kept),
            "`{element}` survives a save that never drew it"
        );
    }
    assert!(written.get("text").is_some(), "the narrative survives");
    assert!(
        written.get("jurisdiction").is_some(),
        "so does the jurisdiction"
    );
    assert_eq!(
        written
            .pointer("/compose/lockedDate")
            .and_then(Value::as_str),
        Some("2026-01-01"),
        "and a compose element the composer does not draw"
    );
    assert!(
        written
            .pointer("/compose/include/0/concept/0/designation")
            .is_some(),
        "and a designation on a code the composer only names: {written}"
    );
    assert_eq!(
        written.pointer("/meta/source").and_then(Value::as_str),
        Some("https://terminology.example/source"),
        "the meta the server sent is not replaced by the version alone"
    );
}

#[test]
fn a_draft_composing_nothing_is_not_savable_and_sends_no_compose() {
    // `compose.include` is 1..* and `compose` itself is 0..1
    // (<https://hl7.org/fhir/R4B/valueset.html>).
    let mut draft = Draft::new();
    draft.url = String::from("https://terminology.example/vs/local");
    let key = draft.clause_keys(true)[0];
    draft.clauses.retain(|clause| clause.key != key);
    assert!(
        !draft.savable(),
        "a value set with no include is not offered for saving"
    );
    let written = resource(&draft);
    assert!(
        written.get("compose").is_none(),
        "and sends no compose element at all: {written}"
    );
}

#[test]
fn an_exclude_alone_is_not_savable() {
    let mut draft = Draft::new();
    draft.url = String::from("https://terminology.example/vs/local");
    let key = draft.clause_keys(true)[0];
    draft.clauses.retain(|clause| clause.key != key);
    let excluded = draft.add_clause(false);
    if let Some(clause) = draft.clause_mut(excluded) {
        clause.system = String::from("https://terminology.example/x");
    }
    assert!(
        !draft.savable(),
        "an exclude subtracts from a union that has to exist"
    );
}

#[test]
fn a_preview_sends_the_unsaved_compose_inline_and_its_page() {
    let (mut draft, key) = over("https://terminology.example/x");
    draft.add_concept(key, "A", "Alpha");
    let body = preview(&draft, &Preview {
        filter: Some(String::from("alp")),
        count: 20,
        offset: 40,
    });
    assert!(
        body.starts_with(r#"{"parameter":[{"name":"valueSet","resource":{"compose":"#),
        "the compose travels as a resource rather than as a canonical: {body}"
    );
    assert!(
        body.contains(r#"{"name":"filter","valueString":"alp"}"#),
        "{body}"
    );
    assert!(
        body.contains(r#"{"name":"count","valueInteger":20}"#),
        "{body}"
    );
    assert!(
        body.contains(r#"{"name":"offset","valueInteger":40}"#),
        "{body}"
    );
}

#[test]
fn the_first_page_sends_no_offset() {
    let (draft, _key) = over("https://terminology.example/x");
    let body = preview(&draft, &Preview {
        filter: None,
        count: 20,
        offset: 0,
    });
    assert!(
        !body.contains("offset"),
        "an unmoved page states no offset: {body}"
    );
    assert!(
        !body.contains("filter"),
        "an empty eye sends no filter: {body}"
    );
}

#[test]
fn a_resource_read_back_round_trips_through_the_draft() {
    let document: Value = serde_json::from_str(
        r#"{"resourceType":"ValueSet","id":"local","meta":{"versionId":"3"},
            "url":"https://terminology.example/vs/local","status":"active","name":"LocalSet",
            "compose":{"inactive":true,
              "include":[{"valueSet":["https://terminology.example/vs/national"]},
                         {"system":"https://terminology.example/x","version":"2.0",
                          "concept":[{"code":"A","display":"Alpha, as we say it"}]}],
              "exclude":[{"system":"https://terminology.example/x",
                          "filter":[{"property":"concept","op":"is-a","value":"root"}]}]}}"#,
    )
    .expect("the server's own answer parses");
    let stored: StoredValueSet = crate::fhir::version::FhirVersion::R4B
        .decode(&document.to_string())
        .expect("the composer reads it");
    assert!(
        stored.writable(),
        "the server stamped a version, so it holds the record"
    );
    let draft = stored.draft(&document);
    assert_eq!(draft.id, "local");
    assert_eq!(draft.version_id, "3");
    assert_eq!(draft.inactive, Some(true));
    assert_eq!(draft.clauses.len(), 3);
    let keys: Vec<u32> = draft.clauses.iter().map(|clause| clause.key).collect();
    assert_eq!(
        keys.iter().collect::<std::collections::BTreeSet<_>>().len(),
        keys.len(),
        "no two clauses share a key: {keys:?}"
    );
    assert_eq!(
        draft.clause_keys(false).len(),
        1,
        "one exclude was read back"
    );
    let second = draft.clause_keys(true)[1];
    let coded = draft
        .clauses
        .iter()
        .find(|clause| clause.key == second)
        .expect("the second include");
    assert_eq!(coded.system_version, "2.0");
    assert_eq!(
        coded.concepts.first().map(|row| row.display.clone()),
        Some(String::from("Alpha, as we say it"))
    );
    assert!(!draft.clauses[2].included, "the exclude stays an exclude");
    assert_eq!(
        resource(&draft),
        resource(&stored.draft(&document)),
        "reading and writing back changes nothing"
    );
}

#[test]
fn a_resource_the_server_stamped_no_version_on_is_read_only() {
    let document: Value = serde_json::from_str(
        r#"{"resourceType":"ValueSet","id":"national","url":"https://terminology.example/vs/n",
            "status":"active","compose":{"include":[{"system":"https://terminology.example/x"}]}}"#,
    )
    .expect("the server's own answer parses");
    let stored: StoredValueSet = crate::fhir::version::FhirVersion::R4B
        .decode(&document.to_string())
        .expect("the composer reads it");
    assert!(
        !stored.writable(),
        "content the server serves out of its loaded indexes has no write path"
    );
    assert!(
        !stored.draft(&document).id.is_empty(),
        "it still opens, to be read"
    );
}

#[test]
fn a_resource_carrying_no_compose_opens_on_an_empty_clause() {
    let document: Value =
        serde_json::from_str(r#"{"resourceType":"ValueSet","id":"x","status":"draft"}"#)
            .expect("the server's own answer parses");
    let stored: StoredValueSet = crate::fhir::version::FhirVersion::R4B
        .decode(&document.to_string())
        .expect("the composer reads it");
    assert_eq!(
        stored.draft(&document).clauses.len(),
        1,
        "an expansion-only value set opens on something to compose into"
    );
}

#[test]
fn a_code_is_never_listed_twice_in_one_clause() {
    let (mut draft, key) = over("https://terminology.example/x");
    draft.add_concept(key, "A", "Alpha");
    draft.add_concept(key, "A", "Alpha again");
    assert_eq!(
        draft
            .clauses
            .iter()
            .find(|clause| clause.key == key)
            .map(|clause| clause.concepts.len()),
        Some(1),
        "picking the same code twice adds one row"
    );
}

#[test]
fn removing_a_clause_leaves_every_other_key_where_it_was() {
    let mut draft = Draft::new();
    let first = draft
        .clause_keys(true)
        .first()
        .copied()
        .expect("one include");
    let second = draft.add_clause(true);
    let third = draft.add_clause(false);
    draft.clauses.retain(|clause| clause.key != second);
    assert_eq!(draft.clause_keys(true), vec![first]);
    assert_eq!(draft.clause_keys(false), vec![third]);
    let fourth = draft.add_clause(true);
    assert!(
        ![first, second, third].contains(&fourth),
        "a key is never reused, so a row cannot inherit another row's state"
    );
}

#[test]
fn the_statuses_offered_are_the_publication_status_codes() {
    assert_eq!(STATUSES, ["draft", "active", "retired", "unknown"]);
    assert!(STATUSES.contains(&NEW_STATUS));
}
