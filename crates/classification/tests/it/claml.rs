// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ClaML` reader (the `ClaML` DTD; <https://hl7.org/fhir/R4B/icd.html> for
//! the period in the codes).

use ::classification::claml::{ClamlError, MODIFIER, read};
use ::classification::{Class, Classification, PREFERRED};
use ferroterm_testkit::classification::{
    BASE, BILE_DUCT, BLOCK, CHAPTER, CLAML_VERSION, DEFORMITY, LIVER, LIVER_CELL,
    MALIGNANT_MESOTHELIOMA, MESOTHELIOMAS, MORPHOLOGY, NECK_DEFORMITY, SKULL, VAULT, VAULT_CLOSED,
    VAULT_OPEN, claml,
};

fn class<'a>(classification: &'a Classification, code: &str) -> &'a Class {
    classification
        .classes
        .iter()
        .find(|c| c.code == code)
        .expect("a class with the code")
}

#[test]
fn the_title_kinds_classes_and_parents_are_read() {
    let classification = read(&claml()).expect("reads");
    assert_eq!(classification.name, "ICD-10-NL");
    assert_eq!(classification.version.as_deref(), Some(CLAML_VERSION));
    assert_eq!(classification.language, "nl");
    assert_eq!(
        classification.kinds,
        [
            "chapter", "block", "category", "mblock", "mdigit4", "mdigit6"
        ]
    );
    assert_eq!(
        classification.title,
        "ICD-10 Nederlandse vertaling (synthetisch)"
    );
    let chapter = class(&classification, CHAPTER);
    assert_eq!(chapter.kind, "chapter");
    assert_eq!(chapter.parent, None);
    assert_eq!(chapter.title("nl"), Some("Nieuwvormingen"));
    assert_eq!(chapter.title("en"), Some("Neoplasms"));
    let block = class(&classification, BLOCK);
    assert_eq!(block.parent.as_deref(), Some(CHAPTER));
    let liver = class(&classification, LIVER);
    assert_eq!(liver.parent.as_deref(), Some(BLOCK));
    let cell = class(&classification, LIVER_CELL);
    assert_eq!(cell.parent.as_deref(), Some(LIVER));
    assert_eq!(cell.title("en"), Some("Liver cell carcinoma"));
    let inclusion: Vec<&str> = cell
        .rubrics
        .iter()
        .filter(|r| r.kind == "inclusion")
        .map(|r| r.text.as_str())
        .collect();
    assert_eq!(inclusion, ["hepatocellulair carcinoom"]);
}

#[test]
fn codes_gain_the_period_and_usage_marks_are_kept() {
    let classification = read(&claml()).expect("reads");
    let bile = class(&classification, BILE_DUCT);
    assert_eq!(bile.code, "C22.1", "`C221` in the document");
    assert_eq!(bile.usage.as_deref(), Some("dagger"));
    assert_eq!(bile.parent.as_deref(), Some(LIVER));
}

#[test]
fn references_in_brackets_and_entities_flatten_into_the_label() {
    let classification = read(&claml()).expect("reads");
    let liver = class(&classification, LIVER);
    let exclusion = liver
        .rubrics
        .iter()
        .find(|r| r.kind == "exclusion")
        .expect("exclusion");
    assert_eq!(
        exclusion.text,
        "secundaire maligne nieuwvorming van lever (C78.7)"
    );
    let note = liver
        .rubrics
        .iter()
        .find(|r| r.kind == "note")
        .expect("note");
    assert_eq!(note.text, "Een & ander in fragmenten");
}

#[test]
fn modifiers_expand_onto_the_leaves_they_apply_to() {
    let classification = read(&claml()).expect("reads");
    let closed = class(&classification, VAULT_CLOSED);
    assert_eq!(closed.parent.as_deref(), Some(VAULT));
    assert_eq!(closed.kind, "category");
    assert_eq!(
        closed.title("nl"),
        Some("Fractuur van schedeldak, gesloten")
    );
    assert_eq!(
        closed.title("en"),
        Some("Fracture of vault of skull, closed")
    );
    assert!(
        closed
            .rubrics
            .iter()
            .any(|r| r.kind == MODIFIER && r.text == "0")
    );
    let open = class(&classification, VAULT_OPEN);
    assert_eq!(open.title("nl"), Some("Fractuur van schedeldak, open"));
    assert!(
        !classification.classes.iter().any(|c| c.code == "S02.10"),
        "S02.1 excludes the modifier"
    );
    assert!(
        !classification.classes.iter().any(|c| c.code == "S02.00.0"),
        "the modifier applies once"
    );
    let skull = class(&classification, SKULL);
    assert_eq!(
        skull.parent.as_deref(),
        Some("S00-S09"),
        "the block itself is not a leaf and is not modified"
    );
    assert_eq!(class(&classification, BASE).parent.as_deref(), Some(SKULL));
    assert_eq!(classification.classes.len(), 19);
    assert!(
        classification
            .classes
            .iter()
            .all(|c| c.rubrics.iter().any(|r| r.kind == PREFERRED))
    );
}

#[test]
fn a_document_that_is_not_claml_or_names_an_undefined_modifier_is_refused() {
    assert!(matches!(
        read("<root><Title name=\"x\"/></root>"),
        Err(ClamlError::NotClaml)
    ));
    assert!(matches!(
        read("<ClaML version=\"2.0.0\"><ClassKinds/></ClaML>"),
        Err(ClamlError::NoTitle)
    ));
    let undefined = claml().replace("code=\"S5\" all=\"true\"", "code=\"S9\" all=\"true\"");
    assert!(matches!(
        read(&undefined),
        Err(ClamlError::UnknownModifier { ref modifier, .. }) if modifier == "S9"
    ));
    assert!(matches!(
        read("<ClaML version=\"2.0.0\"><Title name=\"x\"/><Class kind=\"chapter\"/></ClaML>"),
        Err(ClamlError::Attribute {
            element: "Class",
            attribute: "code"
        })
    ));
    assert!(read("<ClaML><Title>").is_err(), "not well-formed");
}

#[test]
fn a_morphology_code_keeps_its_spelling_beside_the_subcategory_it_resembles() {
    let classification = read(&claml()).expect("reads");
    let neck = class(&classification, NECK_DEFORMITY);
    assert_eq!(neck.parent.as_deref(), Some(DEFORMITY));
    assert_eq!(neck.kind, "category");
    assert_eq!(neck.title("nl"), Some("Verworven misvorming van hals"));
    let mesotheliomas = class(&classification, MESOTHELIOMAS);
    assert_eq!(
        mesotheliomas.parent.as_deref(),
        Some(MORPHOLOGY),
        "`M953` hangs under `M`, not under `M95`, so it earns no period"
    );
    assert_eq!(mesotheliomas.kind, "mdigit4");
    assert_eq!(mesotheliomas.title("nl"), Some("Mesotheliomen"));
    let malignant = class(&classification, MALIGNANT_MESOTHELIOMA);
    assert_eq!(malignant.parent.as_deref(), Some(MESOTHELIOMAS));
    assert_eq!(
        classification
            .classes
            .iter()
            .filter(|c| c.code == NECK_DEFORMITY)
            .count(),
        1,
        "one class spells M95.3"
    );
}
