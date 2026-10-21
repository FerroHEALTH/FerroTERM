// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `--dhd` over a synthetic delivery: the artifact and its concept maps.

use std::io::Write;

use ferroterm_testkit::dhd::{
    DIRECTORY, FRACTURE, FRACTURE_SCTID, SPRAIN, VERSION, write_delivery,
};
use fhir_types::codec::{Json, Path};
use fhir_types::r4b::concept_map::ConceptMap;

fn cli(
    dhd: std::path::PathBuf,
    version: Option<&str>,
    out: &std::path::Path,
) -> ferroterm_build::Cli {
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
        dhd: Some(dhd),
        dhd_version: version.map(str::to_owned),
        gstandaard: None,
        gstandaard_version: None,
        labcodeset: None,
        rxnorm_sources: Vec::new(),
        out: Some(out.to_path_buf()),
        index_root: None,
    }
}

fn read_map(path: &std::path::Path) -> ConceptMap {
    let text = std::fs::read_to_string(path).expect("reads");
    let value: fhir_types::codec::Value = serde_json::from_str(&text).expect("json");
    let object = value.as_object().expect("object");
    ConceptMap::from_json(object, &mut Path::root("ConceptMap")).expect("decodes")
}

#[test]
fn the_delivery_builds_from_a_directory_or_a_zip_with_its_concept_maps() {
    let source = tempfile::tempdir().expect("tempdir");
    let root = write_delivery(source.path()).expect("writes");
    let out = tempfile::tempdir().expect("tempdir");
    let report = ferroterm_build::run(&cli(root.clone(), None, out.path())).expect("builds");
    let ferroterm_build::Report::Classification(report) = report else {
        panic!("a classification report");
    };
    assert_eq!(report.system, ::dhd_thesaurus::SYSTEM);
    assert_eq!(report.version, VERSION, "from the directory name");
    assert_eq!(report.concepts, 4);
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.path().join("manifest.json")).expect("reads"),
    )
    .expect("json");
    assert!(manifest["hierarchyMeaning"].is_null(), "flat");

    let snomed = read_map(&out.path().join("conceptmaps/dhd-to-snomed.json"));
    assert_eq!(
        snomed.version.as_ref().and_then(|v| v.value.as_deref()),
        Some(VERSION)
    );
    let group = &snomed.group[0];
    assert_eq!(
        group.target.as_ref().and_then(|t| t.value.as_deref()),
        Some("http://snomed.info/sct")
    );
    assert_eq!(group.element.len(), 1);
    assert_eq!(
        group.element[0]
            .code
            .as_ref()
            .and_then(|c| c.value.as_deref()),
        Some(FRACTURE)
    );
    assert_eq!(
        group.element[0].target[0]
            .code
            .as_ref()
            .and_then(|c| c.value.as_deref()),
        Some(FRACTURE_SCTID)
    );
    let icd10 = read_map(&out.path().join("conceptmaps/dhd-to-icd10.json"));
    let elements = &icd10.group[0].element;
    assert_eq!(elements.len(), 2);
    let codes: Vec<&str> = elements[0]
        .target
        .iter()
        .filter_map(|t| t.code.as_ref().and_then(|c| c.value.as_deref()))
        .collect();
    assert_eq!(codes, ["Z99.0", "Z99.1"]);
    assert_eq!(
        elements[1].code.as_ref().and_then(|c| c.value.as_deref()),
        Some(SPRAIN)
    );
    assert_eq!(
        elements[1].target[0].equivalence.value.as_deref(),
        Some("wider")
    );

    let zip_path = source.path().join(format!("{DIRECTORY}.zip"));
    let file = std::fs::File::create(&zip_path).expect("creates");
    let mut zip = zip::ZipWriter::new(file);
    for entry in std::fs::read_dir(&root).expect("reads") {
        let path = entry.expect("entry").path();
        let name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .expect("name");
        zip.start_file(
            format!("{DIRECTORY}/{name}"),
            zip::write::SimpleFileOptions::default(),
        )
        .expect("starts");
        zip.write_all(&std::fs::read(&path).expect("reads"))
            .expect("writes");
    }
    zip.finish().expect("finishes");
    let out2 = tempfile::tempdir().expect("tempdir");
    let report = ferroterm_build::run(&cli(zip_path, Some("1.2"), out2.path()))
        .expect("builds from the zip");
    let ferroterm_build::Report::Classification(report) = report else {
        panic!("a classification report");
    };
    assert_eq!(report.version, "1.2", "the flag wins");
    assert_eq!(report.concepts, 4);
    assert!(out2.path().join("conceptmaps/dhd-to-snomed.json").is_file());
}
