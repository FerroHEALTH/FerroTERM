// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The NPU, ICD-O-3, EMDN, and ISCO-08 readers over the testkit's tables in
//! each publisher's distribution format.

use ::classification::{Class, Classification, PREFERRED, emdn, icdo, isco, npu, sheet};
use ferroterm_testkit::tables::{self, VERSION};

fn class<'a>(classification: &'a Classification, code: &str) -> &'a Class {
    classification
        .classes
        .iter()
        .find(|c| c.code == code)
        .expect("the code is read")
}

fn rubric<'a>(class: &'a Class, kind: &str) -> Vec<&'a str> {
    class
        .rubrics
        .iter()
        .filter(|r| r.kind == kind)
        .map(|r| r.text.as_str())
        .collect()
}

fn releases() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    tables::write_releases(dir.path()).expect("writes");
    dir
}

#[test]
fn the_npu_code_file_reads_flat_with_retired_codes_inactive() {
    let dir = releases();
    let npu = npu::read(&dir.path().join("npu-codes.csv"), VERSION).expect("reads");
    assert_eq!(npu.version.as_deref(), Some(VERSION));
    assert_eq!(npu.hierarchy, None, "NPU is a flat list");
    let active = class(&npu, tables::npu::ACTIVE);
    assert!(active.active);
    assert_eq!(active.title("en"), Some(tables::npu::SHORT));
    assert_eq!(
        rubric(active, npu::FULL),
        ["Plasma\u{2014}Synthion; substance concentration = ? millimole per litre"]
    );
    assert_eq!(rubric(active, "unit"), ["millimole per litre"]);
    assert_eq!(rubric(active, "replaces"), [tables::npu::RETIRED]);
    assert!(
        rubric(active, "effective_to").is_empty(),
        "the zero date means open-ended"
    );
    let retired = class(&npu, tables::npu::RETIRED);
    assert!(!retired.active);
    assert_eq!(rubric(retired, "replaced_by"), [tables::npu::ACTIVE]);
    assert_eq!(npu::SYSTEM, "http://npu-terminology.org");
}

#[test]
fn the_icdo_tables_read_topography_in_a_tree_and_morphology_with_its_terms() {
    let dir = releases();
    let topography = dir.path().join("Topoenglish.txt");
    let morphology = dir.path().join("icdo-morphology.xlsx");
    let icdo = icdo::read(&[&topography, &morphology], VERSION).expect("reads");
    let sub = class(&icdo, tables::icdo::SUBCATEGORY);
    assert_eq!(sub.parent.as_deref(), Some(tables::icdo::CATEGORY));
    assert_eq!(sub.kind, icdo::SUBCATEGORY);
    assert_eq!(sub.title("en"), Some("Upper synthetic organ"));
    assert_eq!(rubric(sub, icdo::SYNONYM), ["Synthetic apex"]);
    assert_eq!(class(&icdo, tables::icdo::CATEGORY).parent, None);
    let morph = class(&icdo, tables::icdo::MORPHOLOGY);
    assert_eq!(morph.kind, icdo::MORPHOLOGY);
    assert_eq!(morph.parent, None);
    assert_eq!(morph.title("en"), Some("Synthetic carcinoma, NOS"));
    assert_eq!(rubric(morph, icdo::SYNONYM), ["Synthoma"]);
    assert_eq!(rubric(morph, icdo::RELATED), ["Invented neoplasm"]);
    assert_eq!(
        icdo::SYSTEM,
        "http://terminology.hl7.org/CodeSystem/icd-o-3"
    );
}

#[test]
fn the_emdn_workbook_reads_its_release_and_nests_codes_by_prefix() {
    let dir = releases();
    let emdn = emdn::read(&dir.path().join("emdn.xlsx"), None).expect("reads");
    assert_eq!(
        emdn.version.as_deref(),
        Some(VERSION),
        "from the sheet name"
    );
    let leaf = class(&emdn, tables::emdn::TYPE);
    assert_eq!(leaf.parent.as_deref(), Some(tables::emdn::GROUP));
    assert_eq!(leaf.title("en"), Some("SYNTHETIC PROBES, STERILE"));
    assert_eq!(rubric(leaf, emdn::TERMINAL), ["YES"]);
    assert_eq!(rubric(leaf, emdn::LEVEL), ["3"]);
    assert_eq!(
        class(&emdn, tables::emdn::GROUP).parent.as_deref(),
        Some(tables::emdn::CATEGORY)
    );
    assert_eq!(class(&emdn, tables::emdn::CATEGORY).kind, emdn::CATEGORY);
    let pinned = emdn::read(&dir.path().join("emdn.xlsx"), Some("2100")).expect("reads");
    assert_eq!(pinned.version.as_deref(), Some("2100"));
}

#[test]
fn the_isco_workbook_reads_four_levels_with_their_notes() {
    let dir = releases();
    let isco = isco::read(&dir.path().join("isco08.xlsx"), None).expect("reads");
    assert_eq!(isco.version.as_deref(), Some(isco::VERSION));
    assert_eq!(isco.classes.len(), 4);
    let unit = class(&isco, tables::isco::UNIT);
    assert_eq!(unit.kind, "unit-group");
    assert_eq!(unit.parent.as_deref(), Some("999"));
    assert_eq!(unit.title("en"), Some("Synthesis Helpers"));
    assert_eq!(rubric(unit, "definition"), ["Helpers assist synthesis."]);
    assert_eq!(rubric(unit, "included-occupations"), ["Stirrer"]);
    assert_eq!(class(&isco, tables::isco::MAJOR).parent, None);
    assert_eq!(
        isco.classes
            .iter()
            .filter(|c| c.rubrics.iter().any(|r| r.kind == PREFERRED))
            .count(),
        4
    );
}

#[test]
fn a_table_without_its_code_column_is_refused_by_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("npu.csv");
    std::fs::write(&path, "id,short_definition\n1,x\n").expect("writes");
    assert!(matches!(
        npu::read(&path, VERSION),
        Err(sheet::SheetError::Column {
            column: "npu_code",
            ..
        })
    ));
    let book = dir.path().join("empty.xlsx");
    std::fs::write(&book, b"not a zip").expect("writes");
    assert!(matches!(
        isco::read(&book, None),
        Err(sheet::SheetError::Zip { .. })
    ));
}
