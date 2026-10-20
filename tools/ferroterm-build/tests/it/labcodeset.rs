//! `--labcodeset` over a synthetic publication: the FHIR resource directory.

use std::io::Write;

use ferroterm_testkit::labcodeset::{
    CULTURE, GLUCOSE, OLD_SODIUM, ORDINAL_OID, RELEASE, SERUM, SODIUM, write_publication,
};
use fhir_types::codec::{Json, Path};
use fhir_types::r4b::code_system::{CodeSystem, CodeSystemConceptPropertyValue};
use fhir_types::r4b::value_set::ValueSet;

fn cli(labcodeset: std::path::PathBuf, out: &std::path::Path) -> ferroterm_build::Cli {
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
        labcodeset: Some(labcodeset),
        rxnorm_sources: Vec::new(),
        out: Some(out.to_path_buf()),
        index_root: None,
    }
}

fn read_json(path: &std::path::Path) -> serde_json::Value {
    let text = std::fs::read_to_string(path).expect("reads");
    serde_json::from_str(&text).expect("json")
}

/// A written resource in the codec's document model, ready to decode.
fn read_document(path: &std::path::Path) -> fhir_types::codec::Value {
    let text = std::fs::read_to_string(path).expect("reads");
    serde_json::from_str(&text).expect("json")
}

fn property<'a>(
    concept: &'a fhir_types::r4b::code_system::CodeSystemConcept,
    code: &str,
) -> Vec<&'a CodeSystemConceptPropertyValue> {
    concept
        .property
        .iter()
        .filter(|p| p.code.value.as_deref() == Some(code))
        .map(|p| &p.value)
        .collect()
}

#[test]
fn the_publication_builds_from_a_document_a_directory_or_a_zip() {
    let source = tempfile::tempdir().expect("tempdir");
    let document = write_publication(source.path()).expect("writes");
    let out = tempfile::tempdir().expect("tempdir");
    let report = ferroterm_build::run(&cli(document.clone(), out.path())).expect("builds");
    let ferroterm_build::Report::Labcodeset(report) = report else {
        panic!("a Labcodeset report");
    };
    assert_eq!(report.release, RELEASE);
    assert_eq!((report.active, report.retired, report.ordinals), (2, 1, 1));
    assert_eq!(report.dir, out.path().join("labcodeset"));
    let manifest = read_json(&report.dir.join("package.json"));
    assert_eq!(manifest["fhirVersions"][0], "4.3.0");
    assert_eq!(manifest["version"], RELEASE);

    // The value set: the active concepts over LOINC, Dutch displays, English
    // designations.
    let value = read_document(&report.dir.join("ValueSet-nl-labcodeset.json"));
    let value_set = ValueSet::from_json(
        value.as_object().expect("object"),
        &mut Path::root("ValueSet"),
    )
    .expect("an R4B ValueSet");
    assert_eq!(
        value_set.url.and_then(|u| u.value),
        Some(String::from(ferroterm_build::labcodeset::VALUE_SET_URL))
    );
    assert_eq!(
        value_set.version.and_then(|v| v.value).as_deref(),
        Some(RELEASE)
    );
    assert_eq!(
        value_set.date.and_then(|d| d.value).as_deref(),
        Some("2026-01-01")
    );
    let include = &value_set.compose.expect("compose").include[0];
    assert_eq!(
        include
            .system
            .as_ref()
            .and_then(|s| s.value.clone())
            .as_deref(),
        Some(ferroterm_build::labcodeset::LOINC)
    );
    let codes: Vec<&str> = include
        .concept
        .iter()
        .filter_map(|c| c.code.value.as_deref())
        .collect();
    assert_eq!(
        codes,
        [GLUCOSE, CULTURE],
        "retired concepts are not members"
    );
    assert_eq!(
        include.concept[0]
            .display
            .as_ref()
            .and_then(|d| d.value.clone())
            .as_deref(),
        Some("glucose [massa/volume] in serum of plasma")
    );
    assert_eq!(
        include.concept[0].designation[0].value.value.as_deref(),
        Some("Glucose [Mass/volume] in Serum or Plasma")
    );
}

#[test]
fn the_supplement_carries_the_dutch_names_and_the_facts() {
    let source = tempfile::tempdir().expect("tempdir");
    let document = write_publication(source.path()).expect("writes");
    let out = tempfile::tempdir().expect("tempdir");
    let report = ferroterm_build::run(&cli(document, out.path())).expect("builds");
    let ferroterm_build::Report::Labcodeset(report) = report else {
        panic!("a Labcodeset report");
    };
    // The LOINC supplement: every concept, with the Dutch name and the facts.
    let value = read_document(&report.dir.join("CodeSystem-nl-labcodeset-loinc.json"));
    let supplement = CodeSystem::from_json(
        value.as_object().expect("object"),
        &mut Path::root("CodeSystem"),
    )
    .expect("an R4B CodeSystem");
    assert_eq!(supplement.content.value.as_deref(), Some("supplement"));
    assert_eq!(
        supplement.supplements.and_then(|s| s.value).as_deref(),
        Some(ferroterm_build::labcodeset::LOINC)
    );
    assert_eq!(supplement.concept.len(), 3);
    let glucose = &supplement.concept[0];
    assert_eq!(glucose.code.value.as_deref(), Some(GLUCOSE));
    assert_eq!(
        glucose.designation[0].value.value.as_deref(),
        Some("glucose [massa/volume] in serum of plasma")
    );
    assert!(matches!(
        property(glucose, "material")[..],
        [CodeSystemConceptPropertyValue::Coding(coding)]
            if coding.code.as_ref().and_then(|c| c.value.as_deref()) == Some(SERUM)
    ));
    assert!(matches!(
        property(glucose, "unit")[..],
        [CodeSystemConceptPropertyValue::Coding(coding)]
            if coding.code.as_ref().and_then(|c| c.value.as_deref()) == Some("mmol/L")
    ));
    assert!(matches!(
        property(glucose, "outcome-valueset")[..],
        [CodeSystemConceptPropertyValue::String(s)]
            if s.value.as_deref() == Some(format!("urn:oid:{ORDINAL_OID}").as_str())
    ));
    assert!(matches!(
        property(glucose, "nl-scale")[..],
        [CodeSystemConceptPropertyValue::String(s)] if s.value.as_deref() == Some("kwantitatief")
    ));
    let sodium = &supplement.concept[1];
    assert_eq!(sodium.code.value.as_deref(), Some(OLD_SODIUM));
    assert!(
        sodium.designation.is_empty(),
        "no translation, no designation"
    );
    assert!(matches!(
        property(sodium, "labcodeset-status")[..],
        [CodeSystemConceptPropertyValue::Code(c)] if c.value.as_deref() == Some("retired")
    ));
    assert!(matches!(
        property(sodium, "replaced-by")[..],
        [CodeSystemConceptPropertyValue::Code(c)] if c.value.as_deref() == Some(SODIUM)
    ));
    assert!(matches!(
        property(sodium, "retired-reason")[..],
        [CodeSystemConceptPropertyValue::String(s)] if s.value.as_deref() == Some("Afgeraden voor gebruik")
    ));
    assert!(matches!(
        property(&supplement.concept[2], "outcome-refset")[..],
        [CodeSystemConceptPropertyValue::Coding(coding)]
            if coding.system.as_ref().and_then(|s| s.value.as_deref()) == Some("http://snomed.info/sct")
    ));
}

#[test]
fn the_ordinal_lists_and_the_zip_build_the_same_resources() {
    let source = tempfile::tempdir().expect("tempdir");
    let document = write_publication(source.path()).expect("writes");
    let out = tempfile::tempdir().expect("tempdir");
    let report = ferroterm_build::run(&cli(document.clone(), out.path())).expect("builds");
    let ferroterm_build::Report::Labcodeset(report) = report else {
        panic!("a Labcodeset report");
    };
    // The ordinal list: a value set under its OID, over SNOMED CT.
    let value = read_document(
        &report
            .dir
            .join(format!("ValueSet-{}.json", ORDINAL_OID.replace('.', "-"))),
    );
    let ordinal = ValueSet::from_json(
        value.as_object().expect("object"),
        &mut Path::root("ValueSet"),
    )
    .expect("an R4B ValueSet");
    assert_eq!(
        ordinal.url.and_then(|u| u.value),
        Some(format!("urn:oid:{ORDINAL_OID}"))
    );
    let include = &ordinal.compose.expect("compose").include[0];
    assert_eq!(
        include
            .system
            .as_ref()
            .and_then(|s| s.value.clone())
            .as_deref(),
        Some("http://snomed.info/sct")
    );
    assert_eq!(include.concept.len(), 2);
    assert_eq!(
        include.concept[0].designation[0]
            .language
            .as_ref()
            .and_then(|l| l.value.clone())
            .as_deref(),
        Some("nl-NL")
    );

    // The directory and a zip build the same.
    let from_dir = tempfile::tempdir().expect("tempdir");
    ferroterm_build::run(&cli(source.path().to_path_buf(), from_dir.path())).expect("builds");
    let zip_path = source.path().join("Labcodeset_v2026-01.zip");
    let file = std::fs::File::create(&zip_path).expect("creates");
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file(
        "Labcodeset v2026-01/labconcepts-20260101.xml",
        zip::write::SimpleFileOptions::default(),
    )
    .expect("starts");
    zip.write_all(std::fs::read(&document).expect("reads").as_slice())
        .expect("writes");
    zip.finish().expect("finishes");
    let from_zip = tempfile::tempdir().expect("tempdir");
    ferroterm_build::run(&cli(zip_path, from_zip.path())).expect("builds from the zip");
    for name in [
        "ValueSet-nl-labcodeset.json",
        "CodeSystem-nl-labcodeset-loinc.json",
    ] {
        assert_eq!(
            read_json(&from_dir.path().join("labcodeset").join(name)),
            read_json(&from_zip.path().join("labcodeset").join(name)),
            "{name}"
        );
    }
}
