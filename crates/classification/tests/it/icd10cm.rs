// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ICD-10-CM reader (the tabular XML and the CMS order-file columns).

use ::classification::icd10cm::{Icd10cmError, KINDS, SHORT, locate, read};
use ::classification::{Class, Classification, PREFERRED};
use ferroterm_testkit::classification::{
    CHOLERA, CLASSICAL, CM_BLOCK, CM_CHAPTER, CM_INJURY, CM_VAULT, CM_VAULT_INITIAL, CM_VERSION,
    HERPES, UNSPECIFIED, write_icd10cm,
};

fn class<'a>(classification: &'a Classification, code: &str) -> &'a Class {
    classification
        .classes
        .iter()
        .find(|c| c.code == code)
        .expect("a class with the code")
}

fn fixture() -> (tempfile::TempDir, Classification) {
    let dir = tempfile::tempdir().expect("tempdir");
    write_icd10cm(dir.path()).expect("writes");
    let files = locate(&[dir.path().to_path_buf()]).expect("locates");
    assert!(
        files.tabular.ends_with("icd10cm_tabular_2099.xml"),
        "the tabular file is located by its name"
    );
    let classification = read(&files).expect("reads");
    (dir, classification)
}

#[test]
fn chapters_sections_and_diags_form_the_tree() {
    let (_dir, classification) = fixture();
    assert_eq!(classification.version.as_deref(), Some(CM_VERSION));
    assert_eq!(classification.kinds, KINDS);
    let chapter = class(&classification, CM_CHAPTER);
    assert_eq!(chapter.kind, "chapter");
    assert_eq!(chapter.parent, None);
    assert_eq!(
        chapter.title("en"),
        Some("Certain infectious and parasitic diseases (A00-B99)")
    );
    assert!(chapter.rubrics.iter().any(|r| r.kind == "includes"));
    let block = class(&classification, CM_BLOCK);
    assert_eq!(block.kind, "block");
    assert_eq!(block.parent.as_deref(), Some(CM_CHAPTER));
    let cholera = class(&classification, CHOLERA);
    assert_eq!(cholera.kind, "category");
    assert_eq!(cholera.parent.as_deref(), Some(CM_BLOCK));
    assert_eq!(cholera.valid, Some(false), "a header in the order file");
    assert!(
        cholera
            .rubrics
            .iter()
            .any(|r| r.kind == "excludes1" && r.text == "cholera-like illness (A09)")
    );
    let classical = class(&classification, CLASSICAL);
    assert_eq!(classical.kind, "subcategory");
    assert_eq!(classical.parent.as_deref(), Some(CHOLERA));
    assert_eq!(classical.valid, Some(true));
    assert!(
        classical
            .rubrics
            .iter()
            .any(|r| r.kind == "inclusionTerm" && r.text == "Classical cholera")
    );
    assert!(
        classical
            .rubrics
            .iter()
            .any(|r| r.kind == SHORT && r.text.starts_with("Cholera due to"))
    );
    assert_eq!(class(&classification, UNSPECIFIED).valid, Some(false));
}

#[test]
fn a_one_category_section_folds_into_its_category() {
    let (_dir, classification) = fixture();
    let herpes = class(&classification, HERPES);
    assert_eq!(herpes.kind, "category");
    assert_eq!(
        herpes.parent.as_deref(),
        Some(CM_CHAPTER),
        "the section `B10` shares the code and is not a class of its own"
    );
    assert_eq!(
        classification
            .classes
            .iter()
            .filter(|c| c.code == HERPES)
            .count(),
        1
    );
    assert!(herpes.rubrics.iter().any(|r| r.kind == "excludes2"));
    assert!(herpes.rubrics.iter().any(|r| r.kind == "useAdditionalCode"));
}

#[test]
fn the_order_file_adds_the_seventh_character_codes_under_their_stem() {
    let (_dir, classification) = fixture();
    let vault = class(&classification, CM_VAULT);
    assert_eq!(vault.valid, Some(false));
    let initial = class(&classification, CM_VAULT_INITIAL);
    assert_eq!(initial.parent.as_deref(), Some(CM_VAULT));
    assert_eq!(initial.kind, "subcategory");
    assert_eq!(initial.valid, Some(true));
    assert_eq!(
        initial.title("en"),
        Some("Fracture of vault of skull, initial encounter for closed fracture")
    );
    assert!(
        initial
            .rubrics
            .iter()
            .any(|r| r.kind == SHORT && r.text == "Fracture of vault of skull, init for clos fx")
    );
    let skull = class(&classification, "S02");
    assert!(
        skull.rubrics.iter().any(
            |r| r.kind == "sevenChrDef" && r.text == "A: initial encounter for closed fracture"
        )
    );
    assert!(skull.rubrics.iter().any(|r| r.kind == "sevenChrNote"));
    assert!(skull.rubrics.iter().any(|r| r.kind == "codeAlso"));
    assert_eq!(class(&classification, CM_INJURY).kind, "chapter");
    assert_eq!(classification.classes.len(), 12);
    assert!(
        classification
            .classes
            .iter()
            .all(|c| c.rubrics.iter().any(|r| r.kind == PREFERRED))
    );
}

#[test]
fn a_missing_file_is_named() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_icd10cm(dir.path()).expect("writes");
    std::fs::remove_file(dir.path().join("icd10cm_order_2099.txt")).expect("removes");
    assert!(matches!(
        locate(&[dir.path().to_path_buf()]),
        Err(Icd10cmError::Missing {
            name: "icd10cm_order_"
        })
    ));
}
