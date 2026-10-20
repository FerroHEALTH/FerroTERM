//! The classification build over the testkit's `ClaML` and ICD-10-CM fixtures.

use ::classification::claml::read;
use concept_graph::persist::Hierarchy;
use concept_store::record::PropertyValue;
use concept_store::store::{Store, Vocabulary};
use designation_index::index::Query;
use designation_index::persist::read_from;
use ferroterm_build::classification::{
    self, HIERARCHY_MEANING, ICD10CM_SYSTEM, KIND, KIND_KEY, USAGE_KEY, VALID_KEY,
};
use ferroterm_testkit::classification::{
    BILE_DUCT, BLOCK, CHAPTER, CLAML_SYSTEM, CLAML_VERSION, CLASSICAL, CM_CHAPTER, CM_VERSION,
    LIVER, LIVER_CELL, VAULT_CLOSED, claml, write_icd10cm_artifact,
};

#[test]
fn the_claml_classification_builds_an_artifact_the_store_graph_and_text_open() {
    let classification = read(&claml()).expect("reads");
    let out = tempfile::tempdir().expect("tempdir");
    let report =
        classification::build(&classification, CLAML_SYSTEM, None, out.path()).expect("builds");
    assert_eq!(report.system, CLAML_SYSTEM);
    assert_eq!(report.version, CLAML_VERSION);
    assert_eq!(report.concepts, 19);
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.path().join("manifest.json")).expect("manifest"),
    )
    .expect("json");
    assert_eq!(manifest["kind"], KIND);
    assert_eq!(manifest["system"], CLAML_SYSTEM);
    assert_eq!(manifest["hierarchyMeaning"], HIERARCHY_MEANING);
    assert_eq!(manifest["language"], "nl");
    assert_eq!(manifest["languages"], serde_json::json!(["en", "nl"]));
    let store = Store::open(&report.store).expect("opens");
    let keys = |name: &str| {
        store
            .vocabulary_ordinal(Vocabulary::PropertyKeys, name)
            .expect("read")
            .expect("key")
    };
    let liver = store.ordinal(LIVER).expect("read").expect("liver");
    let properties = store.properties(liver).expect("read");
    let of = |name: &str| {
        properties
            .iter()
            .find(|(k, _)| *k == keys(name))
            .map(|(_, v)| v.clone())
    };
    assert_eq!(
        of(KIND_KEY),
        Some(vec![PropertyValue::Code(String::from("category"))])
    );
    assert_eq!(
        of("exclusion"),
        Some(vec![PropertyValue::String(String::from(
            "secundaire maligne nieuwvorming van lever (C78.7)"
        ))])
    );
    assert_eq!(of(USAGE_KEY), None);
    let bile = store.ordinal(BILE_DUCT).expect("read").expect("bile");
    assert!(
        store
            .properties(bile)
            .expect("read")
            .iter()
            .any(|(k, v)| *k == keys(USAGE_KEY)
                && *v == vec![PropertyValue::Code(String::from("dagger"))])
    );
    let designations = store.designations(liver).expect("read");
    assert_eq!(designations.len(), 2, "the Dutch and English titles");
    let cell = store.ordinal(LIVER_CELL).expect("read").expect("cell");
    assert_eq!(
        store.designations(cell).expect("read").len(),
        3,
        "two titles and an inclusion term"
    );
    let graph = std::fs::read(out.path().join("hierarchy.bin")).expect("hierarchy");
    let hierarchy = Hierarchy::read_from(&mut graph.as_slice()).expect("reads");
    let chapter = store.ordinal(CHAPTER).expect("read").expect("chapter");
    let block = store.ordinal(BLOCK).expect("read").expect("block");
    let closed = store.ordinal(VAULT_CLOSED).expect("read").expect("closed");
    assert!(hierarchy.closure.is_ancestor(chapter, cell));
    assert!(hierarchy.closure.is_ancestor(block, cell));
    assert!(!hierarchy.closure.is_ancestor(chapter, closed));
    let text = std::fs::read(out.path().join("text.bin")).expect("text");
    let index = read_from(&mut text.as_slice()).expect("index");
    let hits = index.search(
        &Query {
            text: String::from("hepatocell"),
            language: Some(String::from("nl")),
            ..Query::default()
        },
        0,
        10,
    );
    assert_eq!(hits.total, 1, "inclusion terms are indexed");
    assert_eq!(
        index.entry(hits.designations[0]).expect("entry").concept,
        cell
    );
}

#[test]
fn the_icd10cm_release_builds_with_the_valid_flag() {
    let out = tempfile::tempdir().expect("tempdir");
    write_icd10cm_artifact(out.path()).expect("builds");
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.path().join("manifest.json")).expect("manifest"),
    )
    .expect("json");
    assert_eq!(manifest["system"], ICD10CM_SYSTEM);
    assert_eq!(manifest["version"], CM_VERSION);
    assert_eq!(manifest["concepts"], 12);
    let store = Store::open(&out.path().join("store.redb")).expect("opens");
    let valid = store
        .vocabulary_ordinal(Vocabulary::PropertyKeys, VALID_KEY)
        .expect("read")
        .expect("key");
    let classical = store.ordinal(CLASSICAL).expect("read").expect("classical");
    assert!(
        store
            .properties(classical)
            .expect("read")
            .iter()
            .any(|(k, v)| *k == valid && *v == vec![PropertyValue::Boolean(true)])
    );
    let chapter = store.ordinal(CM_CHAPTER).expect("read").expect("chapter");
    assert!(
        !store
            .properties(chapter)
            .expect("read")
            .iter()
            .any(|(k, _)| *k == valid),
        "chapters are not in the order file"
    );
}

#[test]
fn the_build_refuses_a_missing_version_a_duplicate_and_an_unknown_parent() {
    let mut classification = read(&claml()).expect("reads");
    let out = tempfile::tempdir().expect("tempdir");
    classification.version = None;
    assert!(matches!(
        classification::build(&classification, CLAML_SYSTEM, None, out.path()),
        Err(classification::Error::NoVersion)
    ));
    let report = classification::build(&classification, CLAML_SYSTEM, Some("2022"), out.path())
        .expect("builds with the flag");
    assert_eq!(report.version, "2022");
    let mut duplicated = read(&claml()).expect("reads");
    duplicated.classes.push(duplicated.classes[0].clone());
    assert!(matches!(
        classification::build(&duplicated, CLAML_SYSTEM, None, out.path()),
        Err(classification::Error::Duplicate(_))
    ));
    let mut orphan = read(&claml()).expect("reads");
    orphan.classes[3].parent = Some(String::from("Z99"));
    assert!(matches!(
        classification::build(&orphan, CLAML_SYSTEM, None, out.path()),
        Err(classification::Error::UnknownParent { .. })
    ));
}

fn zip_dir(root: &std::path::Path, zip_path: &std::path::Path) {
    let file = std::fs::File::create(zip_path).expect("creates");
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("reads") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path
                .strip_prefix(root)
                .expect("under root")
                .to_string_lossy()
                .replace('\\', "/");
            writer.start_file(name, options).expect("starts");
            let mut source = std::fs::File::open(&path).expect("opens");
            std::io::copy(&mut source, &mut writer).expect("copies");
        }
    }
    writer.finish().expect("finishes");
}

#[test]
fn the_command_line_builds_claml_and_icd10cm_from_zips() {
    let source = tempfile::tempdir().expect("tempdir");
    let claml_dir = source.path().join("claml");
    std::fs::create_dir_all(&claml_dir).expect("creates");
    ferroterm_testkit::classification::write_claml(&claml_dir.join("icd10nl.xml")).expect("writes");
    let claml_zip = source.path().join("icd10nl.zip");
    zip_dir(&claml_dir, &claml_zip);
    let out = tempfile::tempdir().expect("tempdir");
    let report = ferroterm_build::run(&ferroterm_build::Cli {
        claml: Some(claml_zip),
        system: Some(CLAML_SYSTEM.to_owned()),
        claml_version: Some(String::from("2022")),
        out: Some(out.path().to_path_buf()),
        index_root: None,
        ..no_input()
    })
    .expect("builds");
    let ferroterm_build::Report::Classification(report) = report else {
        panic!("a classification report");
    };
    assert_eq!(report.version, "2022", "the flag wins over the title");
    assert_eq!(report.concepts, 19);

    let cm_dir = source.path().join("cm");
    ferroterm_testkit::classification::write_icd10cm(&cm_dir).expect("writes");
    let tables_zip = source.path().join("tables.zip");
    zip_dir(&cm_dir.join("Table and Index"), &tables_zip);
    let order_dir = source.path().join("order");
    std::fs::create_dir_all(&order_dir).expect("creates");
    std::fs::rename(
        cm_dir.join("icd10cm_order_2099.txt"),
        order_dir.join("icd10cm_order_2099.txt"),
    )
    .expect("moves");
    let order_zip = source.path().join("order.zip");
    zip_dir(&order_dir, &order_zip);
    let out = tempfile::tempdir().expect("tempdir");
    let report = ferroterm_build::run(&ferroterm_build::Cli {
        icd10cm: vec![tables_zip, order_zip.clone()],
        out: Some(out.path().to_path_buf()),
        index_root: None,
        ..no_input()
    })
    .expect("builds from two zips");
    let ferroterm_build::Report::Classification(report) = report else {
        panic!("a classification report");
    };
    assert_eq!(report.system, ICD10CM_SYSTEM);
    assert_eq!(report.concepts, 12);
    assert!(
        matches!(
            ferroterm_build::run(&ferroterm_build::Cli {
                icd10cm: vec![order_zip],
                out: Some(out.path().to_path_buf()),
                index_root: None,
                ..no_input()
            }),
            Err(ferroterm_build::RunError::Icd10cm(_))
        ),
        "the order file alone lacks the tabular list"
    );
}

fn no_input() -> ferroterm_build::Cli {
    ferroterm_build::Cli {
        rf2: None,
        rf2_refset: Vec::new(),
        loinc: None,
        loinc_version: None,
        claml: None,
        system: None,
        claml_version: None,
        icd10cm: Vec::new(),
        rxnorm: None,
        rxnorm_version: None,
        icd11: None,
        icd11_api: None,
        icd11_release: None,
        icd11_languages: Vec::new(),
        atc: None,
        atc_version: None,
        dhd: None,
        dhd_version: None,
        gstandaard: None,
        gstandaard_version: None,
        labcodeset: None,
        rxnorm_sources: Vec::new(),
        out: None,
        index_root: None,
    }
}
