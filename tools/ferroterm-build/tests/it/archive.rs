// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Building from the release zip gives the same bytes as from the directory.

use std::fs;
use std::io::Write;
use std::path::Path;

use ferroterm_build::archive::{ArchiveError, unpack_snapshot};
use ferroterm_build::{Cli, RunError};
use zip::write::SimpleFileOptions;

use crate::fixture::write_release;

/// Zips `root` as a release folder `SnomedCT_Test/`, with a `Full/` tree the
/// build must ignore and a directory entry.
fn zip_release(root: &Path, zip_path: &Path) {
    let file = fs::File::create(zip_path).expect("creates");
    let mut writer = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer
        .add_directory("SnomedCT_Test/Snapshot/", options)
        .expect("dir entry");
    for entry in walk(root) {
        let relative = entry.strip_prefix(root).expect("under root");
        let name = format!("SnomedCT_Test/{}", relative.display());
        writer.start_file(name, options).expect("starts");
        writer
            .write_all(&fs::read(&entry).expect("reads"))
            .expect("writes");
    }
    writer
        .start_file(
            "SnomedCT_Test/Full/Terminology/sct2_Concept_Full_XX_20240101.txt",
            options,
        )
        .expect("starts");
    writer.write_all(b"id\teffectiveTime\r\n").expect("writes");
    writer.finish().expect("finishes");
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).expect("reads") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out.sort();
    out
}

#[test]
fn a_release_zip_builds_the_same_bytes_as_the_directory() {
    let release = tempfile::tempdir().expect("tempdir");
    write_release(release.path());
    let zip_path = release.path().join("release.zip");
    zip_release(release.path(), &zip_path);
    let from_dir = tempfile::tempdir().expect("tempdir");
    let from_zip = tempfile::tempdir().expect("tempdir");
    ferroterm_build::run(&Cli {
        rf2: Some(release.path().to_path_buf()),
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
        out: Some(from_dir.path().to_path_buf()),
        index_root: None,
    })
    .expect("builds from the directory");
    ferroterm_build::run(&Cli {
        rf2: Some(zip_path),
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
        out: Some(from_zip.path().to_path_buf()),
        index_root: None,
    })
    .expect("builds from the zip");
    let a = walk(from_dir.path());
    let b = walk(from_zip.path());
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.file_name(), y.file_name());
        assert_eq!(
            fs::read(x).expect("reads"),
            fs::read(y).expect("reads"),
            "{}",
            x.display()
        );
    }
}

#[test]
fn only_the_snapshot_is_unpacked_and_a_zip_without_one_is_refused() {
    let release = tempfile::tempdir().expect("tempdir");
    write_release(release.path());
    let zip_path = release.path().join("release.zip");
    zip_release(release.path(), &zip_path);
    let scratch = tempfile::tempdir().expect("tempdir");
    let root = unpack_snapshot(&zip_path, scratch.path()).expect("unpacks");
    assert_eq!(root, scratch.path().join("SnomedCT_Test"));
    assert!(root.join("Snapshot").is_dir());
    assert!(!root.join("Full").exists(), "the Full tree is not unpacked");
    let empty = release.path().join("empty.zip");
    let mut writer = zip::ZipWriter::new(fs::File::create(&empty).expect("creates"));
    writer
        .start_file("readme.txt", SimpleFileOptions::default())
        .expect("starts");
    writer.write_all(b"nothing").expect("writes");
    writer.finish().expect("finishes");
    assert!(matches!(
        unpack_snapshot(&empty, scratch.path()),
        Err(ArchiveError::NoSnapshot { .. })
    ));
    let out = tempfile::tempdir().expect("tempdir");
    assert!(matches!(
        ferroterm_build::run(&Cli {
            rf2: Some(empty),
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
            out: Some(out.path().to_path_buf()),
            index_root: None,
        }),
        Err(RunError::Archive(_))
    ));
}
