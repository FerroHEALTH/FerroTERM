// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where a build writes: `--out`, or a child of the index root named for the
//! source, and the refusal of an `--out` outside that root.

use std::path::{Path, PathBuf};

use clap::Parser;
use ferroterm_build::output::{OutputError, resolve};
use ferroterm_build::{Cli, Report, RunError};

/// Parses `args` with the index root set to `root`, whatever the test
/// process's environment holds.
fn cli(args: &[&str], root: Option<&str>) -> Cli {
    let mut cli =
        Cli::try_parse_from(std::iter::once("ferroterm-build").chain(args.iter().copied()))
            .expect("parses");
    cli.index_root = root.map(PathBuf::from);
    cli
}

#[test]
fn the_out_flag_is_optional_on_the_command_line() {
    let cli = cli(&["--loinc", "/releases/Loinc_2.83.zip"], None);
    assert_eq!(cli.out, None);
    assert!(matches!(
        resolve(&cli, Path::new("/")),
        Err(OutputError::NoOut)
    ));
}

#[test]
fn the_index_root_flag_names_the_root() {
    let cli = Cli::try_parse_from([
        "ferroterm-build",
        "--loinc",
        "x.zip",
        "--index-root",
        "/data/index",
    ])
    .expect("parses");
    assert_eq!(cli.index_root, Some(PathBuf::from("/data/index")));
}

#[test]
fn without_out_each_source_builds_into_its_own_child_of_the_root() {
    let root = Some("/data/index");
    let cwd = Path::new("/");
    let cases: [(&[&str], &str); 9] = [
        (&["--rf2", "/releases/SnomedCT.zip"], "/data/index/snomed"),
        (
            &["--loinc", "/releases/Loinc_2.83.zip"],
            "/data/index/loinc",
        ),
        (&["--rxnorm", "/releases/RxNorm.zip"], "/data/index/rxnorm"),
        (
            &["--atc", "/releases/atc.csv", "--atc-version", "2026"],
            "/data/index/atc",
        ),
        (&["--dhd", "/releases/dhd.zip"], "/data/index/dhd"),
        (&["--icd10cm", "/releases/cm.zip"], "/data/index/icd-10-cm"),
        (
            &[
                "--claml",
                "/releases/icd10nl.xml",
                "--system",
                "http://hl7.org/fhir/sid/icd-10-nl",
            ],
            "/data/index/icd-10-nl",
        ),
        (
            &[
                "--claml",
                "/releases/x.xml",
                "--system",
                "urn:oid:2.16.840.1.113883.6.3",
            ],
            "/data/index/claml",
        ),
        (&["--icd11", "/releases/icd11"], "/data/index"),
    ];
    for (args, expected) in cases {
        assert_eq!(
            resolve(&cli(args, root), cwd).expect("resolves"),
            PathBuf::from(expected),
            "{args:?}"
        );
    }
}

#[test]
fn a_relative_out_is_read_against_the_current_directory() {
    let cli = cli(
        &["--loinc", "x.zip", "--out", "./index/"],
        Some("/data/index"),
    );
    assert_eq!(
        resolve(&cli, Path::new("/data/index")).expect("resolves"),
        PathBuf::from("/data/index/index"),
        "the compose service runs in /data/index, so a relative out lands on the mount"
    );
    let cli = self::cli(&["--loinc", "x.zip", "--out", "loinc"], None);
    assert_eq!(
        resolve(&cli, Path::new("/srv/ferroterm")).expect("resolves"),
        PathBuf::from("/srv/ferroterm/loinc"),
        "with no root a relative out is printed absolute"
    );
}

#[test]
fn an_out_outside_the_root_is_refused() {
    for (out, cwd) in [
        ("./index/", "/home/nonroot"),
        ("/tmp/loinc", "/data/index"),
        ("../loinc", "/data/index"),
        ("/data/index-other/loinc", "/"),
    ] {
        let cli = cli(&["--loinc", "x.zip", "--out", out], Some("/data/index"));
        assert!(
            matches!(
                resolve(&cli, Path::new(cwd)),
                Err(OutputError::OutsideRoot { ref root, .. }) if root == Path::new("/data/index")
            ),
            "{out} from {cwd}"
        );
    }
}

#[test]
fn an_out_under_the_root_is_kept() {
    let cli = cli(
        &["--loinc", "x.zip", "--out", "/data/index/loinc-2.83"],
        Some("/data/index"),
    );
    assert_eq!(
        resolve(&cli, Path::new("/")).expect("resolves"),
        PathBuf::from("/data/index/loinc-2.83")
    );
}

#[test]
fn a_build_with_no_out_lands_in_the_child_of_the_root() {
    let release = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::loinc::write_release(release.path()).expect("writes");
    let root = tempfile::tempdir().expect("tempdir");
    let mut cli = cli(&["--loinc", "placeholder"], None);
    cli.loinc = Some(release.path().to_path_buf());
    cli.index_root = Some(root.path().to_path_buf());
    let Report::Loinc(report) = ferroterm_build::run(&cli).expect("builds") else {
        panic!("a LOINC report");
    };
    assert!(report.store.starts_with(root.path().join("loinc")));
    assert!(root.path().join("loinc").join("manifest.json").is_file());
}

#[test]
fn a_build_with_an_out_outside_the_root_writes_nothing() {
    let root = tempfile::tempdir().expect("tempdir");
    let elsewhere = tempfile::tempdir().expect("tempdir");
    let mut cli = cli(&["--loinc", "placeholder"], None);
    cli.out = Some(elsewhere.path().join("loinc"));
    cli.index_root = Some(root.path().to_path_buf());
    assert!(matches!(
        ferroterm_build::run(&cli),
        Err(RunError::Output(OutputError::OutsideRoot { .. }))
    ));
    assert!(!elsewhere.path().join("loinc").exists());
}
