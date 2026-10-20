//! `--gstandaard` over a synthetic release: four artifacts under one output.

use ferroterm_testkit::gstandaard::{VERSION, write_release};

#[test]
fn the_release_builds_the_four_rungs_under_the_output_directory() {
    let source = tempfile::tempdir().expect("tempdir");
    write_release(source.path()).expect("writes");
    let out = tempfile::tempdir().expect("tempdir");
    let report = ferroterm_build::run(&ferroterm_build::Cli {
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
        gstandaard: Some(source.path().to_path_buf()),
        gstandaard_version: Some(String::from(VERSION)),
        labcodeset: None,
        rxnorm_sources: Vec::new(),
        out: Some(out.path().to_path_buf()),
        index_root: None,
    })
    .expect("builds");
    let ferroterm_build::Report::Classifications(reports) = report else {
        panic!("four classification reports");
    };
    let systems: Vec<&str> = reports.iter().map(|r| r.system.as_str()).collect();
    assert_eq!(
        systems,
        [
            ::gstandaard::GPK_SYSTEM,
            ::gstandaard::PRK_SYSTEM,
            ::gstandaard::HPK_SYSTEM,
            ::gstandaard::ARTICLE_SYSTEM,
        ]
    );
    let concepts: Vec<u64> = reports.iter().map(|r| r.concepts).collect();
    assert_eq!(concepts, [1, 1, 1, 2]);
    assert!(reports.iter().all(|r| r.version == VERSION));
    for name in ["gpk", "prk", "hpk", "artikel"] {
        assert!(
            out.path().join(name).join("manifest.json").is_file(),
            "{name}"
        );
    }
}
