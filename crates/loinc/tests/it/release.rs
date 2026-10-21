// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading the synthetic LOINC-shaped release.

use ::loinc::release::Release;
use ::loinc::{answer, part, term, variant};
use ferroterm_testkit::loinc::{
    ANSWER_LIST, GLUCOSE, GLUCOSE_PART, OLD_GLUCOSE, SODIUM, SURVEY, code, write_release,
};

#[test]
fn every_file_of_the_release_reads_by_column_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_release(dir.path()).expect("writes");
    let release = Release::open(dir.path()).expect("opens");
    let terms = term::read(&release).expect("terms");
    assert_eq!(terms.rows.len(), 4);
    assert!(terms.columns.iter().any(|c| c == "LONG_COMMON_NAME"));
    let glucose = terms
        .rows
        .iter()
        .find(|t| t.code == code(GLUCOSE))
        .expect("glucose");
    assert_eq!(glucose.long_common_name, "Glucose [Mass/volume] in Blood");
    assert_eq!(
        glucose.fields.get("CLASS").map(String::as_str),
        Some("CHEM")
    );
    assert!(glucose.active());
    assert!(
        !terms
            .rows
            .iter()
            .find(|t| t.code == code(OLD_GLUCOSE))
            .expect("old")
            .active()
    );
    assert!(
        terms
            .rows
            .iter()
            .find(|t| t.code == code(SODIUM))
            .expect("sodium")
            .external_copyright
            .is_some()
    );
    let parts = part::read_parts(&release).expect("parts");
    assert_eq!(parts.len(), 3);
    let edges = part::read_hierarchy(&release).expect("hierarchy");
    assert_eq!(edges.len(), 7, "the class part and its term");
    let links = part::read_links(&release).expect("links");
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].axis, "COMPONENT");
    assert_eq!(links[0].part, code(GLUCOSE_PART));
    assert!(edges.iter().any(
        |e| e.code == code(GLUCOSE) && e.parent.as_deref() == Some(code(GLUCOSE_PART).as_str())
    ));
    assert!(
        edges.iter().any(|e| e.parent.is_none()),
        "a root has no parent"
    );
    let lists = answer::read(&release).expect("answers");
    let list = lists.get(&code(ANSWER_LIST)).expect("list");
    assert_eq!(list.answers.len(), 2);
    assert_eq!(list.terms, [code(SURVEY)]);
    let variants = variant::read(&release).expect("variants");
    assert_eq!(variants.len(), 1);
    assert_eq!(variants[0].language, "nl-NL");
    assert_eq!(
        variants[0]
            .terms
            .get(&code(GLUCOSE))
            .and_then(|t| t.long_common_name.as_deref()),
        Some("Glucose [massa/volume] in bloed")
    );
}

#[test]
fn a_release_without_the_term_table_or_with_a_bad_code_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(Release::open(dir.path()).is_err());
    write_release(dir.path()).expect("writes");
    let path = dir.path().join("LoincTable/Loinc.csv");
    let text = std::fs::read_to_string(&path)
        .expect("reads")
        .replace(&code(GLUCOSE), "9000X-0");
    std::fs::write(&path, text).expect("writes");
    let release = Release::open(dir.path()).expect("opens");
    assert!(matches!(
        term::read(&release),
        Err(::loinc::release::ReleaseError::Code { .. })
    ));
}

#[test]
fn the_term_table_nearest_the_root_wins_over_the_panels_copy() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_release(dir.path()).expect("writes");
    let panels = dir.path().join("AccessoryFiles/PanelsAndForms");
    std::fs::create_dir_all(&panels).expect("creates");
    std::fs::write(
        panels.join("Loinc.csv"),
        "LOINC_NUM,LONG_COMMON_NAME\n11491-6,not a term\n",
    )
    .expect("writes");
    let release = Release::open(dir.path()).expect("opens");
    assert_eq!(
        release.file("Loinc.csv").expect("term table"),
        dir.path().join("LoincTable/Loinc.csv")
    );
    assert_eq!(term::read(&release).expect("terms").rows.len(), 4);
}
