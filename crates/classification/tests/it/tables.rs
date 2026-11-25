// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The publisher release readers over the testkit's synthetic releases in
//! each publisher's distribution format.

use ::classification::{
    Class, Classification, PREFERRED, dicom, edqm, emdn, icdo, isco, npu, orpha, sheet, spor,
};
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

#[test]
fn the_orphadata_files_read_every_language_and_the_inactive_flag() {
    let dir = releases();
    let en = dir.path().join("en_product1.xml");
    let fr = dir.path().join("fr_product1.xml");
    let orpha = orpha::read(&[&en, &fr], None).expect("reads");
    assert_eq!(
        orpha.version.as_deref(),
        Some("2099-01-31"),
        "the JDBOR date"
    );
    assert_eq!(orpha.language, "en");
    let disease = class(&orpha, tables::orpha::DISEASE);
    assert!(disease.active);
    assert_eq!(disease.title("en"), Some("Synthetic syndrome"));
    assert_eq!(disease.title("fr"), Some("Syndrome synthétique"));
    assert_eq!(rubric(disease, orpha::SYNONYM), ["Synthosis", "Synthose"]);
    assert_eq!(rubric(disease, orpha::TYPE), ["Disease"]);
    let obsolete = class(&orpha, tables::orpha::OBSOLETE);
    assert!(!obsolete.active);
    assert_eq!(
        rubric(obsolete, orpha::FLAG),
        ["Inactive", "Obsolete entity"]
    );
    let pinned = orpha::read(&[&en], Some("2099")).expect("reads");
    assert_eq!(pinned.version.as_deref(), Some("2099"));
}

#[test]
fn the_dcm_ontology_reads_its_edition_and_retired_codes_from_owl_or_zip() {
    let dir = releases();
    let owl = dir.path().join("dcm.owl");
    let zip_path = dir.path().join("dcm.owl.zip");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&zip_path).expect("creates"));
    writer
        .start_file("dcm.owl", zip::write::SimpleFileOptions::default())
        .expect("starts");
    std::io::Write::write_all(&mut writer, tables::dicom::owl().as_bytes()).expect("writes");
    writer.finish().expect("finishes");
    for path in [&owl, &zip_path] {
        let dcm = dicom::read(path, None).expect("reads");
        assert_eq!(dcm.version.as_deref(), Some(tables::dicom::EDITION));
        assert_eq!(dcm.classes.len(), 2, "the ontology itself is no code");
        let current = class(&dcm, tables::dicom::CURRENT);
        assert!(current.active);
        assert_eq!(current.title("en"), Some("Synthetic acquisition"));
        assert_eq!(rubric(current, "definition"), ["An invented acquisition."]);
        let retired = class(&dcm, tables::dicom::RETIRED);
        assert!(!retired.active);
        assert_eq!(
            rubric(retired, dicom::REPLACED_BY),
            [tables::dicom::CURRENT]
        );
        assert_eq!(rubric(retired, "change-note"), ["Retired"]);
    }
}

#[test]
fn the_edqm_response_reads_translations_links_and_status() {
    let dir = releases();
    let edqm = edqm::read(&[&dir.path().join("edqm.json")], None).expect("reads");
    assert_eq!(edqm.version.as_deref(), Some(tables::edqm::MODIFIED));
    assert_eq!(edqm.kinds, ["PDF", "ROA"]);
    let form = class(&edqm, tables::edqm::DOSE_FORM);
    assert!(form.active);
    assert_eq!(form.kind, "PDF");
    assert_eq!(form.title("fr"), Some("Comprimé synthétique"));
    assert_eq!(rubric(form, "link-bdf"), ["99992000"]);
    assert_eq!(rubric(form, edqm::DOMAIN), ["Human and Veterinary"]);
    assert!(
        !class(&edqm, tables::edqm::ROUTE).active,
        "a deprecated term"
    );
}

#[test]
fn the_spor_responses_read_each_kind_under_its_system() {
    let dir = releases();
    let path = |name: &str| dir.path().join(name);
    let rms = spor::read(&[&path("spor-rms.xml")], None).expect("reads");
    assert_eq!(rms.system, format!("{}{}", spor::LISTS, tables::spor::LIST));
    assert_eq!(rms.classification.title, "Synthetic list");
    assert_eq!(rms.classification.version.as_deref(), Some("2099-03-01"));
    let term = class(&rms.classification, tables::spor::TERM);
    assert!(term.active);
    assert_eq!(term.title("en"), Some("Synthetic use"));
    assert_eq!(term.title("de"), Some("Synthetische Anwendung"));
    assert_eq!(rubric(term, spor::SYNONYM), ["999999000011S"]);
    assert!(!class(&rms.classification, tables::spor::OLD_TERM).active);
    let oms = spor::read(&[&path("spor-oms.json")], None).expect("reads");
    assert_eq!(oms.system, spor::ORGANISATIONS);
    assert_eq!(
        class(&oms.classification, tables::spor::ORGANISATION).title("en"),
        Some("Synthetic Medicines Agency")
    );
    let sms = spor::read(&[&path("spor-sms.json")], None).expect("reads");
    assert_eq!(sms.system, spor::SUBSTANCES);
    let substance = class(&sms.classification, tables::spor::SUBSTANCE);
    assert!(substance.active);
    assert_eq!(substance.title("en"), Some("SYNTHETIC ACID"));
    assert_eq!(rubric(substance, spor::SYNONYM), ["Acidum syntheticum"]);
    let pms = spor::read(&[&path("spor-pms.json")], Some("2099-12")).expect("reads");
    assert_eq!(pms.system, spor::PRODUCTS);
    assert_eq!(pms.classification.version.as_deref(), Some("2099-12"));
    assert_eq!(
        class(&pms.classification, tables::spor::PRODUCT).title("en"),
        Some("Synthomab 10 mg tablets")
    );
    assert!(matches!(
        spor::read(&[&path("spor-oms.json"), &path("spor-sms.json")], None),
        Err(spor::SporError::Mixed { .. })
    ));
    assert!(matches!(
        spor::read(&[&path("edqm.json")], None),
        Err(spor::SporError::Unknown(_))
    ));
}
