// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The LOINC build over the testkit's release-shaped fixture.

use concept_graph::persist::Hierarchy;
use concept_store::record::PropertyValue;
use concept_store::store::{Store, Vocabulary};
use designation_index::index::Query;
use designation_index::persist::read_from;
use ferroterm_build::loinc::{self, ANSWER_LIST_KEY, COPYRIGHT_KEY, KIND_KEY, SYSTEM};
use ferroterm_testkit::loinc::{
    ANSWER_LIST, GLUCOSE, GLUCOSE_PART, OLD_GLUCOSE, ROOT_PART, SODIUM, SURVEY, VERSION, YES, code,
    write_release,
};

#[test]
fn the_loinc_release_builds_an_artifact_the_store_graph_and_text_open() {
    let release = tempfile::tempdir().expect("tempdir");
    write_release(release.path()).expect("writes");
    let out = tempfile::tempdir().expect("tempdir");
    let report = loinc::build(release.path(), None, out.path()).expect("builds");
    assert_eq!(
        report.version, VERSION,
        "the version comes from VersionLastChanged"
    );
    assert_eq!(report.terms, 4);
    assert_eq!(report.parts, 3);
    assert_eq!(report.answer_lists, 1);
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.path().join("manifest.json")).expect("manifest"),
    )
    .expect("json");
    assert_eq!(manifest["system"], SYSTEM);
    assert_eq!(manifest["version"], VERSION);
    assert_eq!(manifest["languages"], serde_json::json!(["en", "nl-NL"]));
    let store = Store::open(&report.store).expect("opens");
    let glucose = store
        .ordinal(&code(GLUCOSE).to_ascii_uppercase())
        .expect("read")
        .expect("glucose");
    let old = store
        .ordinal(&code(OLD_GLUCOSE))
        .expect("read")
        .expect("old glucose");
    assert!(
        store
            .concept(glucose)
            .expect("read")
            .expect("record")
            .active
    );
    assert!(!store.concept(old).expect("read").expect("record").active);
    let keys = |name: &str| {
        store
            .vocabulary_ordinal(Vocabulary::PropertyKeys, name)
            .expect("read")
            .expect("key")
    };
    let properties = store.properties(glucose).expect("read");
    let of = |name: &str| {
        properties
            .iter()
            .find(|(k, _)| *k == keys(name))
            .map(|(_, v)| v.clone())
    };
    assert_eq!(
        of("CLASS"),
        Some(vec![PropertyValue::String(String::from("CHEM"))])
    );
    assert_eq!(
        of(COPYRIGHT_KEY),
        Some(vec![PropertyValue::Code(String::from("LOINC"))])
    );
    assert_eq!(
        of(KIND_KEY),
        Some(vec![PropertyValue::Code(String::from("term"))])
    );
    let sodium = store.ordinal(&code(SODIUM)).expect("read").expect("sodium");
    let sodium_props = store.properties(sodium).expect("read");
    assert!(sodium_props.iter().any(|(k, v)| *k == keys(COPYRIGHT_KEY)
        && *v == vec![PropertyValue::Code(String::from("3rdParty"))]));
    let survey = store.ordinal(&code(SURVEY)).expect("read").expect("survey");
    let survey_props = store.properties(survey).expect("read");
    assert!(
        survey_props.iter().any(|(k, v)| *k == keys(ANSWER_LIST_KEY)
            && *v == vec![PropertyValue::Code(code(ANSWER_LIST))])
    );
    assert!(
        store.ordinal(&code(YES)).expect("read").is_some(),
        "answers are codes too"
    );
}

#[test]
fn the_loinc_hierarchy_and_text_index_open_beside_the_store() {
    let release = tempfile::tempdir().expect("tempdir");
    write_release(release.path()).expect("writes");
    let out = tempfile::tempdir().expect("tempdir");
    let report = loinc::build(release.path(), None, out.path()).expect("builds");
    let store = Store::open(&report.store).expect("opens");
    let glucose = store
        .ordinal(&code(GLUCOSE).to_ascii_uppercase())
        .expect("read")
        .expect("glucose");
    let graph = std::fs::read(out.path().join("hierarchy.bin")).expect("hierarchy");
    let hierarchy = Hierarchy::read_from(&mut graph.as_slice()).expect("reads");
    let root = store
        .ordinal(&code(ROOT_PART))
        .expect("read")
        .expect("root");
    let part = store
        .ordinal(&code(GLUCOSE_PART))
        .expect("read")
        .expect("part");
    assert!(hierarchy.closure.is_ancestor(part, glucose));
    assert!(hierarchy.closure.is_ancestor(root, glucose));
    let text = std::fs::read(out.path().join("text.bin")).expect("text");
    let index = read_from(&mut text.as_slice()).expect("index");
    let hits = index.search(
        &Query {
            text: String::from("massa"),
            language: Some(String::from("nl")),
            ..Query::default()
        },
        0,
        10,
    );
    assert_eq!(
        hits.total, 1,
        "the Dutch variant is indexed under its language"
    );
    assert_eq!(
        index.entry(hits.designations[0]).expect("entry").concept,
        glucose
    );
}

#[test]
fn the_version_comes_from_the_release_name_or_the_flag() {
    assert_eq!(
        loinc::version_from_name(std::path::Path::new("/x/Loinc_2.82.zip")).as_deref(),
        Some("2.82")
    );
    assert_eq!(
        loinc::version_from_name(std::path::Path::new("Loinc_2.82")).as_deref(),
        Some("2.82")
    );
    assert_eq!(
        loinc::version_from_name(std::path::Path::new("release.zip")),
        None
    );
    let release = tempfile::tempdir().expect("tempdir");
    write_release(release.path()).expect("writes");
    let out = tempfile::tempdir().expect("tempdir");
    let report = loinc::build(release.path(), Some("2.82"), out.path()).expect("builds");
    assert_eq!(report.version, "2.82");
}
