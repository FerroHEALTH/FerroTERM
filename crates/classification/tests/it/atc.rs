// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ATC reader over the testkit's index and `BST801T` fixtures
//! (<https://atcddd.fhi.no/atc/structure_and_principles/>).

use ::classification::PREFERRED;
use ::classification::atc::{AtcError, DDD, INDICATOR, KINDS, SYSTEM, read};
use ferroterm_testkit::atc::{
    CHEMICAL, GROUP, OTHER_SUBSTANCE, SUBSTANCE, VERSION, write_bst801, write_index,
};

#[test]
fn the_index_csv_reads_into_the_five_level_tree_with_ddds() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("atc.csv");
    write_index(&path).expect("writes");
    let atc = read(&path, Some(VERSION)).expect("reads");
    assert_eq!(atc.version.as_deref(), Some(VERSION));
    assert_eq!(atc.kinds, KINDS);
    assert_eq!(
        atc.classes.len(),
        6,
        "a substance with two DDD rows is one class"
    );
    let substance = atc
        .classes
        .iter()
        .find(|c| c.code == SUBSTANCE)
        .expect("substance");
    assert_eq!(substance.kind, "chemical-substance");
    assert_eq!(substance.parent.as_deref(), Some(CHEMICAL));
    assert_eq!(substance.title("en"), Some("metforminoid"));
    let ddds: Vec<&str> = substance
        .rubrics
        .iter()
        .filter(|r| r.kind == DDD)
        .map(|r| r.text.as_str())
        .collect();
    assert_eq!(ddds, ["2 g O", "1 g P; parenteral form"]);
    let group = atc.classes.iter().find(|c| c.code == GROUP).expect("group");
    assert_eq!(group.kind, "anatomical-main-group");
    assert_eq!(group.parent, None);
    assert!(
        atc.classes
            .iter()
            .find(|c| c.code == OTHER_SUBSTANCE)
            .expect("other")
            .rubrics
            .iter()
            .all(|r| r.kind == PREFERRED)
    );
    assert_eq!(SYSTEM, "http://www.whocc.no/atc");
}

#[test]
fn bst801_reads_both_languages_and_skips_removed_records() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("BST801T");
    write_bst801(&path).expect("writes");
    let atc = read(&path, Some(VERSION)).expect("reads");
    assert_eq!(atc.language, "nl");
    assert_eq!(atc.classes.len(), 6, "the removed record is skipped");
    let chemical = atc
        .classes
        .iter()
        .find(|c| c.code == CHEMICAL)
        .expect("chemical");
    assert_eq!(chemical.title("nl"), Some("Biguanoïden"), "Latin-1 decoded");
    assert_eq!(chemical.title("en"), Some("Biguanoids"));
    let substance = atc
        .classes
        .iter()
        .find(|c| c.code == SUBSTANCE)
        .expect("substance");
    assert!(
        substance
            .rubrics
            .iter()
            .any(|r| r.kind == INDICATOR && r.text == "1")
    );
}

#[test]
fn a_bad_code_a_missing_parent_and_a_short_record_are_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bad = dir.path().join("bad.csv");
    std::fs::write(&bad, "ATC code,ATC level name\nA10BA0,six\n").expect("writes");
    assert!(matches!(
        read(&bad, None),
        Err(AtcError::Code { ref code, .. }) if code == "A10BA0"
    ));
    let orphan = dir.path().join("orphan.csv");
    std::fs::write(&orphan, "ATC code,ATC level name\nA10BA02,metformin\n").expect("writes");
    assert!(matches!(
        read(&orphan, None),
        Err(AtcError::MissingParent { ref parent, .. }) if parent == "A10BA"
    ));
    let no_column = dir.path().join("nocol.csv");
    std::fs::write(&no_column, "Code;Something\nA;x\n").expect("writes");
    assert!(matches!(
        read(&no_column, None),
        Err(AtcError::Column {
            column: "ATC level name",
            ..
        })
    ));
    let short = dir.path().join("BST801T");
    std::fs::write(&short, "08011A       too short\n").expect("writes");
    assert!(matches!(
        read(&short, None),
        Err(AtcError::Short { line: 1, .. })
    ));
}
