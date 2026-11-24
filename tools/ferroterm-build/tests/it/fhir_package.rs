// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `--fhir-package` over a synthetic MyHealth@EU-shaped package.

use ferroterm_build::package::{Error, import};
use ferroterm_testkit::mvc::{NAME, write_package};

#[test]
fn a_package_keeps_its_manifest_and_terminology_resources() {
    let dir = tempfile::tempdir().expect("tempdir");
    let tgz = dir.path().join("package.tgz");
    write_package(&tgz, "10.1.0");
    let out = dir.path().join("out");
    let report = import(&tgz, None, &out).expect("imports");
    assert_eq!(
        (report.name.as_str(), report.version.as_str()),
        (NAME, "10.1.0")
    );
    assert_eq!(
        report.counts,
        [("CodeSystem", 1), ("ValueSet", 2), ("ConceptMap", 0)]
    );
    assert_eq!(report.dir, out.join(format!("{NAME}-10.1.0")));
    let mut written: Vec<String> = std::fs::read_dir(&report.dir)
        .expect("lists")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    written.sort();
    assert_eq!(
        written,
        [
            "CodeSystem-mvc-synthetic.json",
            "ValueSet-eHDSIAllergyNoAllergy.json",
            "ValueSet-eHDSIUnserved.json",
            "package.json"
        ],
        "the index and the example folder are left out"
    );
    assert_eq!(report.sha256.len(), 64);
}

#[test]
fn a_pinned_digest_the_file_does_not_have_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let tgz = dir.path().join("package.tgz");
    write_package(&tgz, "10.1.0");
    let pin = "0".repeat(64);
    let refused = import(&tgz, Some(&pin), &dir.path().join("out")).expect_err("refused");
    assert!(matches!(refused, Error::Digest { .. }), "{refused}");
    assert!(!dir.path().join("out").exists(), "nothing is written");

    let actual = import(&tgz, None, &dir.path().join("first"))
        .expect("imports")
        .sha256;
    import(
        &tgz,
        Some(&actual.to_uppercase()),
        &dir.path().join("second"),
    )
    .expect("the pinned digest admits the file in either case");
}

#[test]
fn two_releases_land_side_by_side_and_a_release_is_imported_once() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("out");
    for version in ["10.0.0", "10.1.0"] {
        let tgz = dir.path().join(format!("{version}.tgz"));
        write_package(&tgz, version);
        import(&tgz, None, &out).expect("imports");
    }
    assert!(out.join(format!("{NAME}-10.0.0/package.json")).is_file());
    assert!(out.join(format!("{NAME}-10.1.0/package.json")).is_file());
    let again = import(&dir.path().join("10.1.0.tgz"), None, &out).expect_err("refused");
    assert!(matches!(again, Error::Exists(_)), "{again}");
}

#[test]
fn a_file_that_is_no_gzipped_tarball_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let tgz = dir.path().join("package.tgz");
    std::fs::write(&tgz, b"not a package").expect("writes");
    let refused = import(&tgz, None, &dir.path().join("out")).expect_err("refused");
    assert!(matches!(refused, Error::Archive(_)), "{refused}");
}
