// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reader over the published record layouts.

use ::classification::{Class, PREFERRED};
use ::gstandaard::{ATC, GPK, GStandaardError, HPK, PRK, read};
use ferroterm_testkit::gstandaard::{
    ARTICLE, ENDED_ARTICLE, REMOVED_ARTICLE, VERSION, write_release,
};

fn only<'a>(classes: &'a [Class], code: &str) -> &'a Class {
    classes.iter().find(|c| c.code == code).expect("class")
}

fn texts<'a>(class: &'a Class, kind: &str) -> Vec<&'a str> {
    class
        .rubrics
        .iter()
        .filter(|r| r.kind == kind)
        .map(|r| r.text.as_str())
        .collect()
}

#[test]
fn the_four_rungs_read_with_names_thesauri_and_links() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_release(dir.path()).expect("writes");
    let ladder = read(dir.path(), VERSION).expect("reads");
    assert_eq!(ladder.gpk.version.as_deref(), Some(VERSION));
    assert_eq!(ladder.gpk.hierarchy, None);
    assert_eq!(ladder.gpk.kinds, ["gpk"]);
    let gpk = only(&ladder.gpk.classes, ferroterm_testkit::gstandaard::GPK);
    assert_eq!(texts(gpk, PREFERRED), ["METFORMINOIDE TABLET 500MG"]);
    assert_eq!(texts(gpk, "short"), ["metforminoide tablet 500mg"]);
    assert_eq!(texts(gpk, "label"), ["METFORMINOIDE 500"]);
    assert_eq!(texts(gpk, "substance"), ["METFORMINOIDE"]);
    assert_eq!(texts(gpk, "strength"), ["500MG"]);
    assert_eq!(texts(gpk, "form"), ["tablet"], "resolved through BST902T");
    assert_eq!(texts(gpk, "route"), ["oraal"]);
    assert_eq!(texts(gpk, ATC), [ferroterm_testkit::gstandaard::ATC]);

    let prk = only(&ladder.prk.classes, ferroterm_testkit::gstandaard::PRK);
    assert_eq!(texts(prk, GPK), [ferroterm_testkit::gstandaard::GPK]);
    assert_eq!(texts(prk, "unit"), ["stuk"]);
    let hpk = only(&ladder.hpk.classes, ferroterm_testkit::gstandaard::HPK);
    assert_eq!(texts(hpk, PRK), [ferroterm_testkit::gstandaard::PRK]);
    assert_eq!(texts(hpk, GPK), [ferroterm_testkit::gstandaard::GPK]);
    assert_eq!(texts(hpk, "brand"), ["SYNTHOMET"]);
    assert_eq!(texts(hpk, "firm"), ["SYNTHETICA BV"]);

    assert_eq!(
        ladder.article.classes.len(),
        2,
        "the removed record is skipped"
    );
    let article = only(&ladder.article.classes, ARTICLE);
    assert!(article.active);
    assert_eq!(texts(article, HPK), [ferroterm_testkit::gstandaard::HPK]);
    assert_eq!(texts(article, PRK), [ferroterm_testkit::gstandaard::PRK]);
    assert_eq!(texts(article, GPK), [ferroterm_testkit::gstandaard::GPK]);
    assert_eq!(
        texts(article, PREFERRED),
        ["SYNTHOMET TABLET 500MG 30 STUKS"]
    );
    let ended = only(&ladder.article.classes, ENDED_ARTICLE);
    assert!(!ended.active);
    assert_eq!(texts(ended, "removed"), ["20980601"]);
    assert!(
        ladder
            .article
            .classes
            .iter()
            .all(|c| c.code != REMOVED_ARTICLE),
        "mutation code 9"
    );
}

#[test]
fn a_missing_file_and_a_short_record_are_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_release(dir.path()).expect("writes");
    std::fs::remove_file(dir.path().join("BST052T")).expect("removes");
    assert!(matches!(
        read(dir.path(), VERSION),
        Err(GStandaardError::Missing { file: "BST052", .. })
    ));
    std::fs::write(dir.path().join("BST052T"), "0052 short\n").expect("writes");
    assert!(matches!(
        read(dir.path(), VERSION),
        Err(GStandaardError::Short {
            line: 1,
            length: 128,
            ..
        })
    ));
    std::fs::remove_file(dir.path().join("BST902T")).expect("removes");
    std::fs::remove_file(dir.path().join("BST052T")).expect("removes the short file");
    std::fs::write(
        dir.path().join("bst052t.txt"),
        ferroterm_testkit::gstandaard::prk(),
    )
    .expect("writes");
    let ladder = read(dir.path(), VERSION).expect("reads without the thesauri");
    let gpk = only(&ladder.gpk.classes, ferroterm_testkit::gstandaard::GPK);
    assert_eq!(texts(gpk, "form"), ["12"], "the item code when unresolved");
}
