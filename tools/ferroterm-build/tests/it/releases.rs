// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The publisher release builds from the command line over the testkit's
//! synthetic releases.

use clap::Parser;
use ferroterm_build::{Cli, Report};
use ferroterm_testkit::tables::{self, VERSION};

fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("ferroterm-build").chain(args.iter().copied()))
}

#[test]
fn each_release_builds_under_its_canonical_uri_and_version() {
    let releases = tempfile::tempdir().expect("tempdir");
    tables::write_releases(releases.path()).expect("writes");
    let path = |name: &str| releases.path().join(name).display().to_string();
    let cases: [(Vec<String>, &str, &str, u64); 7] = [
        (
            vec![
                "--npu".into(),
                path("npu-codes.csv"),
                "--npu-version".into(),
                VERSION.into(),
            ],
            "http://npu-terminology.org",
            VERSION,
            2,
        ),
        (
            vec![
                "--icdo".into(),
                path("Topoenglish.txt"),
                "--icdo".into(),
                path("icdo-morphology.xlsx"),
                "--icdo-version".into(),
                "3.2".into(),
            ],
            "http://terminology.hl7.org/CodeSystem/icd-o-3",
            "3.2",
            3,
        ),
        (
            vec!["--emdn".into(), path("emdn.xlsx")],
            "https://smt.esante.gouv.fr/terminologie-emdn",
            VERSION,
            3,
        ),
        (
            vec!["--isco".into(), path("isco08.xlsx")],
            "urn:oid:2.16.840.1.113883.2.9.6.2.7",
            "2008",
            4,
        ),
        (
            vec![
                "--orpha".into(),
                path("en_product1.xml"),
                "--orpha".into(),
                path("fr_product1.xml"),
            ],
            "https://www.orpha.net",
            "2099-01-31",
            2,
        ),
        (
            vec!["--dicom".into(), path("dcm.owl")],
            "http://dicom.nema.org/resources/ontology/DCM",
            tables::dicom::EDITION,
            2,
        ),
        (
            vec![
                "--edqm".into(),
                path("edqm.json"),
                "--edqm-version".into(),
                "2099".into(),
            ],
            "http://standardterms.edqm.eu",
            "2099",
            2,
        ),
    ];
    for (args, system, version, concepts) in cases {
        let out = tempfile::tempdir().expect("tempdir");
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let cli = parse(&args).expect("parses");
        let Report::Classification(report) =
            ferroterm_build::run_into(&cli, out.path()).expect("builds")
        else {
            panic!("{system} builds as a classification");
        };
        assert_eq!(report.system, system);
        assert_eq!(report.version, version, "{system}");
        assert_eq!(report.concepts, concepts, "{system}");
        assert!(out.path().join("manifest.json").is_file(), "{system}");
    }
}

#[test]
fn a_release_without_its_required_version_or_beside_another_source_is_refused() {
    assert!(
        parse(&["--npu", "npu.csv"]).is_err(),
        "NPU names no release"
    );
    assert!(
        parse(&["--icdo", "topo.txt"]).is_err(),
        "ICD-O names no revision"
    );
    assert!(parse(&["--emdn", "e.xlsx", "--isco", "i.xlsx"]).is_err());
    assert!(parse(&["--isco", "i.xlsx", "--loinc", "l.zip"]).is_err());
    assert!(parse(&["--emdn", "e.xlsx", "--rf2", "s.zip"]).is_err());
    assert!(
        parse(&["--emdn", "e.xlsx"]).is_ok(),
        "the sheet names its release"
    );
    assert!(parse(&["--labcodeset", "l.zip"]).is_ok());
    assert!(parse(&["--dicom", "dcm.owl", "--orpha", "en_product1.xml"]).is_err());
    assert!(parse(&["--edqm", "a.json", "--edqm", "b.json"]).is_ok());
}
