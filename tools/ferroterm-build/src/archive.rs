// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The RF2 release zip: the `Snapshot/` tree unpacked to a temporary
//! directory the pipeline reads, and nothing else.
//!
//! Releases are distributed as one zip holding a release folder
//! (`SnomedCT_<edition>_<date>/`) with `Full/`, `Snapshot/`, and `Delta/`
//! trees (the RF2 release file specification,
//! <https://docs.snomed.org/snomed-ct-specifications/release-file-specification>).
//! Only the Snapshot is unpacked; the temporary directory is removed with the
//! build, so no RF2 content persists outside the operator's index.

use std::fs::File;
use std::io;
use std::path::{Component, Path, PathBuf};

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

/// A failure to unpack a release zip.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    /// The zip cannot be read.
    #[error("cannot read the release zip {path}")]
    Read {
        /// The zip.
        path: PathBuf,
        /// The cause.
        #[source]
        source: zip::result::ZipError,
    },
    /// An entry cannot be written out.
    #[error("cannot unpack `{entry}`")]
    Unpack {
        /// The entry name.
        entry: String,
        /// The cause.
        #[source]
        source: io::Error,
    },
    /// The zip holds no `Snapshot/` tree (RF2) or no `Loinc.csv` (LOINC).
    #[error("{path} holds no Snapshot/ tree and no Loinc.csv")]
    NoSnapshot {
        /// The zip.
        path: PathBuf,
    },
    /// The zip holds no entry of the kind wanted.
    #[error("{path} holds no {wanted}")]
    NoEntry {
        /// The zip.
        path: PathBuf,
        /// What was looked for.
        wanted: &'static str,
    },
    /// More than one release folder carries a `Snapshot/` tree.
    #[error("{path} holds several Snapshot/ trees")]
    SeveralSnapshots {
        /// The zip.
        path: PathBuf,
    },
}

/// Unpacks the `Snapshot/` tree of the release zip at `zip_path` under
/// `into`, returning the release root (the directory holding `Snapshot/`).
///
/// Entry names are taken through `enclosed_name`, so a name that escapes the
/// target directory is skipped rather than followed.
///
/// # Errors
///
/// Returns [`ArchiveError`] when the zip does not read, an entry cannot be
/// written, or the zip holds no single `Snapshot/` tree.
pub fn unpack_snapshot(zip_path: &Path, into: &Path) -> Result<PathBuf, ArchiveError> {
    let read = |source| ArchiveError::Read {
        path: zip_path.to_path_buf(),
        source,
    };
    let mut archive = open(zip_path)?;
    let mut root: Option<PathBuf> = None;
    let mut wanted: Vec<(usize, PathBuf)> = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(read)?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let Some(prefix) = snapshot_root(&name) else {
            continue;
        };
        if entry.is_dir()
            || !name
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("txt"))
        {
            continue;
        }
        match &root {
            None => root = Some(prefix),
            Some(known) if *known != prefix => {
                return Err(ArchiveError::SeveralSnapshots {
                    path: zip_path.to_path_buf(),
                });
            }
            Some(_) => {}
        }
        wanted.push((index, name));
    }
    let root = root.ok_or_else(|| ArchiveError::NoSnapshot {
        path: zip_path.to_path_buf(),
    })?;
    for (_, name) in &wanted {
        if let Some(parent) = into.join(name).parent() {
            std::fs::create_dir_all(parent).map_err(|source| ArchiveError::Unpack {
                entry: name.display().to_string(),
                source,
            })?;
        }
    }
    // Every entry decompresses on its own, and a worker reads the archive
    // through its own handle because `ZipArchive` seeks. The files land at
    // fixed paths, so which worker wrote which one does not show.
    wanted
        .par_iter()
        .try_for_each(|entry| extract(zip_path, entry, into))?;
    Ok(into.join(root))
}

/// Opens the zip at `path` for reading.
fn open(path: &Path) -> Result<zip::ZipArchive<File>, ArchiveError> {
    let file = File::open(path).map_err(|source| ArchiveError::Read {
        path: path.to_path_buf(),
        source: zip::result::ZipError::Io(source),
    })?;
    zip::ZipArchive::new(file).map_err(|source| ArchiveError::Read {
        path: path.to_path_buf(),
        source,
    })
}

/// Writes the entry of `zip_path` that `entry` indexes to the path it names
/// under `into`.
fn extract(zip_path: &Path, entry: &(usize, PathBuf), into: &Path) -> Result<(), ArchiveError> {
    let (index, name) = entry;
    let mut archive = open(zip_path)?;
    let unpack = |source| ArchiveError::Unpack {
        entry: name.display().to_string(),
        source,
    };
    let mut source = archive
        .by_index(*index)
        .map_err(|source| ArchiveError::Read {
            path: zip_path.to_path_buf(),
            source,
        })?;
    let target = into.join(name);
    let mut out = File::create(&target).map_err(unpack)?;
    io::copy(&mut source, &mut out).map_err(unpack)?;
    Ok(())
}

/// The path up to (not including) the `Snapshot` component of `name`, when
/// the entry lies under a `Snapshot/` tree.
fn snapshot_root(name: &Path) -> Option<PathBuf> {
    let mut root = PathBuf::new();
    for component in name.components() {
        match component {
            Component::Normal(part) if part == "Snapshot" => return Some(root),
            Component::Normal(part) => root.push(part),
            _ => return None,
        }
    }
    None
}

/// The files of a LOINC release the build reads, by file name.
const LOINC_FILES: [&str; 6] = [
    "Loinc.csv",
    "Part.csv",
    "ComponentHierarchyBySystem.csv",
    "LoincPartLink_Primary.csv",
    "AnswerList.csv",
    "LoincAnswerListLink.csv",
];

/// Unpacks the tables of the LOINC release zip at `zip_path` under `into`.
///
/// Returns the directory to read as the release: the term table, the parts,
/// the hierarchy, the primary part links, the answer lists and links, and
/// every linguistic variant; the supplementary part links (a quarter of a
/// gigabyte) stay in the zip.
///
/// # Errors
///
/// Returns [`ArchiveError`] when the zip does not read, an entry cannot be
/// written, or the zip holds no `Loinc.csv`.
pub fn unpack_loinc(zip_path: &Path, into: &Path) -> Result<PathBuf, ArchiveError> {
    let read = |source| ArchiveError::Read {
        path: zip_path.to_path_buf(),
        source,
    };
    let file = File::open(zip_path).map_err(|source| ArchiveError::Read {
        path: zip_path.to_path_buf(),
        source: zip::result::ZipError::Io(source),
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(read)?;
    let mut found_terms = false;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(read)?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let Some(file_name) = name.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        let wanted = LOINC_FILES
            .iter()
            .any(|f| f.eq_ignore_ascii_case(file_name))
            || file_name.ends_with("LinguisticVariant.csv");
        // The panels and forms folder carries its own `Loinc.csv`, a subset the
        // build must not read as the term table.
        let panels = name
            .components()
            .any(|c| c.as_os_str().eq_ignore_ascii_case("PanelsAndForms"));
        if entry.is_dir() || !wanted || panels {
            continue;
        }
        found_terms |= file_name.eq_ignore_ascii_case("Loinc.csv");
        let target = into.join(&name);
        let entry_name = entry.name().map_err(read)?.into_owned();
        let unpack = |source| ArchiveError::Unpack {
            entry: entry_name.clone(),
            source,
        };
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(unpack)?;
        }
        let mut out = File::create(&target).map_err(unpack)?;
        io::copy(&mut entry, &mut out).map_err(unpack)?;
    }
    if !found_terms {
        return Err(ArchiveError::NoSnapshot {
            path: zip_path.to_path_buf(),
        });
    }
    Ok(into.to_path_buf())
}

/// Unpacks the entries of `zip_path` that `wanted` accepts under `into`,
/// returning how many were written.
fn unpack_matching(
    zip_path: &Path,
    into: &Path,
    wanted: &dyn Fn(&str) -> bool,
) -> Result<Vec<PathBuf>, ArchiveError> {
    let read = |source| ArchiveError::Read {
        path: zip_path.to_path_buf(),
        source,
    };
    let file = File::open(zip_path).map_err(|source| ArchiveError::Read {
        path: zip_path.to_path_buf(),
        source: zip::result::ZipError::Io(source),
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(read)?;
    let mut written = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(read)?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let Some(file_name) = name.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        if entry.is_dir() || !wanted(file_name) {
            continue;
        }
        let target = into.join(&name);
        let entry_name = entry.name().map_err(read)?.into_owned();
        let unpack = |source| ArchiveError::Unpack {
            entry: entry_name.clone(),
            source,
        };
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(unpack)?;
        }
        let mut out = File::create(&target).map_err(unpack)?;
        io::copy(&mut entry, &mut out).map_err(unpack)?;
        written.push(target);
    }
    Ok(written)
}

/// Unpacks the `ClaML` document of the zip at `zip_path` under `into`,
/// returning the XML file (the largest `.xml` entry when there are several).
///
/// # Errors
///
/// Returns [`ArchiveError`] when the zip does not read, an entry cannot be
/// written, or the zip holds no `.xml` entry.
pub fn unpack_claml(zip_path: &Path, into: &Path) -> Result<PathBuf, ArchiveError> {
    let written = unpack_matching(zip_path, into, &|name| {
        Path::new(name)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
    })?;
    written
        .into_iter()
        .max_by_key(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or_default())
        .ok_or(ArchiveError::NoEntry {
            path: zip_path.to_path_buf(),
            wanted: "ClaML `.xml` document",
        })
}

/// Unpacks the tabular XML and the order file of an ICD-10-CM zip at
/// `zip_path` under `into`, returning `into`.
///
/// Either file may be absent from one zip (CMS ships them in two); the
/// reader finds them across every root it is given.
///
/// # Errors
///
/// Returns [`ArchiveError`] when the zip does not read, an entry cannot be
/// written, or the zip holds neither file.
pub fn unpack_icd10cm(zip_path: &Path, into: &Path) -> Result<PathBuf, ArchiveError> {
    let written = unpack_matching(zip_path, into, &|name| {
        let lower = name.to_ascii_lowercase();
        let extension = |e: &str| {
            Path::new(name)
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case(e))
        };
        (lower.starts_with(::classification::icd10cm::TABULAR_PREFIX) && extension("xml"))
            || (lower.starts_with(::classification::icd10cm::ORDER_PREFIX) && extension("txt"))
    })?;
    if written.is_empty() {
        return Err(ArchiveError::NoEntry {
            path: zip_path.to_path_buf(),
            wanted: "icd10cm_tabular_<year>.xml or icd10cm_order_<year>.txt",
        });
    }
    Ok(into.to_path_buf())
}

/// Unpacks the `RRF` tables and the readme of an `RxNorm` zip at `zip_path`
/// under `into`, returning `into`.
///
/// # Errors
///
/// Returns [`ArchiveError`] when the zip does not read, an entry cannot be
/// written, or the zip holds no `RXNCONSO.RRF`.
pub fn unpack_rxnorm(zip_path: &Path, into: &Path) -> Result<PathBuf, ArchiveError> {
    let written = unpack_matching(zip_path, into, &|name| {
        [
            rxnorm_rrf::CONSO,
            rxnorm_rrf::REL,
            rxnorm_rrf::SAT,
            rxnorm_rrf::STY,
        ]
        .iter()
        .any(|f| f.eq_ignore_ascii_case(name))
            || (name.starts_with("Readme")
                && Path::new(name)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("txt")))
    })?;
    let has_conso = written.iter().any(|p| {
        p.file_name()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|f| f.eq_ignore_ascii_case(rxnorm_rrf::CONSO))
    });
    if !has_conso {
        return Err(ArchiveError::NoEntry {
            path: zip_path.to_path_buf(),
            wanted: "RXNCONSO.RRF",
        });
    }
    Ok(into.to_path_buf())
}

/// Unpacks the CSV tables of a DHD delivery zip at `zip_path` under `into`,
/// returning `into` (named after the zip, so the reader sees the version).
///
/// # Errors
///
/// Returns [`ArchiveError`] when the zip does not read, an entry cannot be
/// written, or the zip holds no `.csv` entry.
pub fn unpack_dhd(zip_path: &Path, into: &Path) -> Result<PathBuf, ArchiveError> {
    let written = unpack_matching(zip_path, into, &|name| {
        Path::new(name)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("csv"))
    })?;
    if written.is_empty() {
        return Err(ArchiveError::NoEntry {
            path: zip_path.to_path_buf(),
            wanted: "CSV tables",
        });
    }
    Ok(into.to_path_buf())
}

/// Unpacks a Labcodeset release zip: the `labconcepts-*.xml` document; returns
/// the directory holding it.
///
/// # Errors
///
/// Returns [`ArchiveError`] when the zip does not open, an entry cannot be
/// written, or no document is in it.
pub fn unpack_labcodeset(zip_path: &Path, into: &Path) -> Result<PathBuf, ArchiveError> {
    let written = unpack_matching(zip_path, into, &|name| {
        let path = Path::new(name);
        path.file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|n| n.starts_with("labconcepts"))
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
    })?;
    if written.is_empty() {
        return Err(ArchiveError::NoEntry {
            path: zip_path.to_path_buf(),
            wanted: "a labconcepts XML document",
        });
    }
    Ok(into.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use super::{
        ArchiveError, snapshot_root, unpack_claml, unpack_dhd, unpack_icd10cm, unpack_labcodeset,
        unpack_loinc, unpack_rxnorm, unpack_snapshot,
    };

    /// A zip holding `entries`, each an entry name and its bytes.
    ///
    /// A name ending in `/` is written as a directory, which is what a real
    /// release carries and what the unpackers have to pass over.
    fn zip_of(dir: &Path, name: &str, entries: &[(&str, &str)]) -> PathBuf {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).expect("a zip to write");
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, '_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (entry, body) in entries {
            if let Some(folder) = entry.strip_suffix('/') {
                writer
                    .add_directory(folder, options)
                    .expect("a directory entry");
                continue;
            }
            writer.start_file(*entry, options).expect("an entry");
            writer
                .write_all(body.as_bytes())
                .expect("the entry's bytes");
        }
        writer.finish().expect("the zip is finished");
        path
    }

    /// A directory the test writes into, and its path.
    fn workspace() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let into = dir.path().join("out");
        std::fs::create_dir_all(&into).expect("the target directory");
        (dir, into)
    }

    // NOTE: a release is one zip holding Full/, Snapshot/ and Delta/ trees
    // under a release folder, and only the Snapshot is read
    // (<https://docs.snomed.org/snomed-ct-specifications/release-file-specification>).
    #[test]
    fn the_snapshot_tree_is_unpacked_and_the_other_trees_are_left_in_the_zip() {
        let (dir, into) = workspace();
        let zip = zip_of(
            dir.path(),
            "release.zip",
            &[
                ("SnomedCT_X/Snapshot/", ""),
                (
                    "SnomedCT_X/Snapshot/Terminology/sct2_Concept_Snapshot_INT_20260101.txt",
                    "id\teffectiveTime\n",
                ),
                (
                    "SnomedCT_X/Full/Terminology/sct2_Concept_Full_INT_20260101.txt",
                    "full\n",
                ),
                ("SnomedCT_X/Snapshot/Readme.pdf", "not a table"),
            ],
        );
        let root = unpack_snapshot(&zip, &into).expect("one Snapshot tree");
        assert_eq!(root, into.join("SnomedCT_X"));
        assert!(
            root.join("Snapshot/Terminology/sct2_Concept_Snapshot_INT_20260101.txt")
                .is_file(),
            "the snapshot table is written out"
        );
        assert!(
            !into.join("SnomedCT_X/Full").exists(),
            "the Full tree stays in the zip, so no release content is copied twice"
        );
        assert!(
            !root.join("Snapshot/Readme.pdf").exists(),
            "only the tab-separated tables are read"
        );
    }

    #[test]
    fn a_zip_with_no_snapshot_tree_is_refused() {
        let (dir, into) = workspace();
        let zip = zip_of(
            dir.path(),
            "no-snapshot.zip",
            &[("SnomedCT_X/Full/Terminology/x.txt", "full\n")],
        );
        let error = unpack_snapshot(&zip, &into).expect_err("no Snapshot tree");
        assert!(matches!(error, ArchiveError::NoSnapshot { .. }), "{error}");
    }

    #[test]
    fn a_zip_holding_two_release_folders_is_refused() {
        let (dir, into) = workspace();
        let zip = zip_of(
            dir.path(),
            "two.zip",
            &[
                ("SnomedCT_A/Snapshot/Terminology/x.txt", "a\n"),
                ("SnomedCT_B/Snapshot/Terminology/x.txt", "b\n"),
            ],
        );
        let error = unpack_snapshot(&zip, &into).expect_err("two Snapshot trees");
        assert!(
            matches!(error, ArchiveError::SeveralSnapshots { .. }),
            "a build that picked one of two editions would load the wrong one: {error}"
        );
    }

    /// Entry names that leave the target directory once resolved.
    ///
    /// Each is shaped like an entry the unpacker would otherwise take, so what
    /// stops it is the escape and not the filter over what the build reads. A
    /// name that only climbs back to where it started (`a/b/../c.txt`) does
    /// not escape and is deliberately not here.
    const ESCAPING: [&str; 3] = [
        "../SnomedCT_X/Snapshot/Terminology/escaped.txt",
        "SnomedCT_X/Snapshot/Terminology/../../../../escaped.txt",
        "../../escaped.txt",
    ];

    // NOTE: an entry name is taken through `enclosed_name`, which answers
    // `None` for a name that leaves the target directory
    // (<https://docs.rs/zip/8.6.0/zip/read/struct.ZipFile.html#method.enclosed_name>).
    #[test]
    fn an_entry_named_to_climb_out_of_the_target_is_skipped() {
        for escaping in ESCAPING {
            let (dir, into) = workspace();
            let zip = zip_of(
                dir.path(),
                "escape.zip",
                &[
                    (escaping, "outside\n"),
                    ("SnomedCT_X/Snapshot/Terminology/kept.txt", "inside\n"),
                ],
            );
            let root = unpack_snapshot(&zip, &into).expect("the entry that stays inside");
            assert!(
                root.join("Snapshot/Terminology/kept.txt").is_file(),
                "the entry that stays inside is written"
            );
            assert!(
                !escaped_anywhere(dir.path()),
                "`{escaping}` was written outside the directory the unpacker was given"
            );
        }
    }

    // NOTE: `unpack_matching` joins an entry name onto the target the same way,
    // so every unpacker built on it holds the same property.
    #[test]
    fn an_escaping_entry_is_skipped_by_the_unpackers_that_match_on_a_name() {
        for escaping in ESCAPING {
            let (dir, into) = workspace();
            let zip = zip_of(
                dir.path(),
                "escape.zip",
                &[
                    (
                        &format!("{}.csv", escaping.trim_end_matches(".txt")),
                        "outside\n",
                    ),
                    ("tables/kept.csv", "inside\n"),
                ],
            );
            unpack_dhd(&zip, &into).expect("the entry that stays inside");
            assert!(into.join("tables/kept.csv").is_file());
            assert!(
                !escaped_anywhere(dir.path()),
                "`{escaping}` was written outside the directory the unpacker was given"
            );
        }
    }

    /// Whether an entry named `escaped` landed anywhere under `root`.
    ///
    /// The whole temporary directory is walked rather than the two or three
    /// paths a test could guess, because what is being proved is that the file
    /// is nowhere outside the target, not that it missed one guess.
    fn escaped_anywhere(root: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(root) else {
            return false;
        };
        entries.flatten().any(|entry| {
            let path = entry.path();
            if path.is_dir() {
                return escaped_anywhere(&path);
            }
            path.file_name()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|name| name.starts_with("escaped."))
        })
    }

    #[test]
    fn a_zip_that_is_not_a_zip_is_refused() {
        let (dir, into) = workspace();
        let path = dir.path().join("broken.zip");
        std::fs::write(&path, b"not a zip at all").expect("a file that is not a zip");
        let error = unpack_snapshot(&path, &into).expect_err("not a zip");
        assert!(matches!(error, ArchiveError::Read { .. }), "{error}");
    }

    #[test]
    fn the_loinc_tables_the_build_reads_are_unpacked_and_the_rest_stay() {
        let (dir, into) = workspace();
        let zip = zip_of(
            dir.path(),
            "loinc.zip",
            &[
                ("LoincTable/Loinc.csv", "LOINC_NUM\n"),
                ("LoincTable/Part.csv", "PartNumber\n"),
                (
                    "AccessoryFiles/LinguisticVariants/deDE26LinguisticVariant.csv",
                    "de\n",
                ),
                (
                    "AccessoryFiles/PartFile/LoincPartLink_Supplementary.csv",
                    "a quarter of a gigabyte\n",
                ),
            ],
        );
        unpack_loinc(&zip, &into).expect("a Loinc.csv");
        assert!(into.join("LoincTable/Loinc.csv").is_file());
        assert!(into.join("LoincTable/Part.csv").is_file());
        assert!(
            into.join("AccessoryFiles/LinguisticVariants/deDE26LinguisticVariant.csv")
                .is_file(),
            "every linguistic variant is read"
        );
        assert!(
            !into
                .join("AccessoryFiles/PartFile/LoincPartLink_Supplementary.csv")
                .exists(),
            "the supplementary part links stay in the zip"
        );
    }

    // NOTE: the panels and forms folder carries its own `Loinc.csv`, a subset
    // that is not the term table (the LOINC release layout).
    #[test]
    fn the_panels_folder_loinc_table_is_not_read_as_the_term_table() {
        let (dir, into) = workspace();
        let zip = zip_of(
            dir.path(),
            "panels.zip",
            &[("AccessoryFiles/PanelsAndForms/Loinc.csv", "a subset\n")],
        );
        let error = unpack_loinc(&zip, &into).expect_err("the subset is not the term table");
        assert!(matches!(error, ArchiveError::NoSnapshot { .. }), "{error}");
        assert!(
            !into
                .join("AccessoryFiles/PanelsAndForms/Loinc.csv")
                .exists(),
            "the subset is not written out either"
        );
    }

    #[test]
    fn a_loinc_zip_with_no_term_table_is_refused() {
        let (dir, into) = workspace();
        let zip = zip_of(
            dir.path(),
            "no-loinc.zip",
            &[("LoincTable/Part.csv", "x\n")],
        );
        let error = unpack_loinc(&zip, &into).expect_err("no Loinc.csv");
        assert!(matches!(error, ArchiveError::NoSnapshot { .. }), "{error}");
    }

    #[test]
    fn the_claml_unpacker_takes_the_largest_document_and_refuses_a_zip_with_none() {
        let (dir, into) = workspace();
        let zip = zip_of(
            dir.path(),
            "claml.zip",
            &[
                ("small.xml", "<ClaML/>"),
                ("big.xml", "<ClaML>a much longer document</ClaML>"),
                ("notes.txt", "not a document"),
            ],
        );
        let found = unpack_claml(&zip, &into).expect("an xml entry");
        assert_eq!(
            found.file_name().and_then(std::ffi::OsStr::to_str),
            Some("big.xml"),
            "a release that ships a stub beside the classification is read by size"
        );

        let empty = zip_of(dir.path(), "no-xml.zip", &[("notes.txt", "x")]);
        let error = unpack_claml(&empty, &into).expect_err("no xml entry");
        let ArchiveError::NoEntry { wanted, .. } = error else {
            panic!("a missing entry, not {error}");
        };
        assert_eq!(wanted, "ClaML `.xml` document");
    }

    #[test]
    fn the_icd10cm_unpacker_takes_either_file_and_refuses_a_zip_with_neither() {
        let (dir, into) = workspace();
        let tabular = format!("{}2026.xml", ::classification::icd10cm::TABULAR_PREFIX);
        let order = format!("{}2026.txt", ::classification::icd10cm::ORDER_PREFIX);
        let zip = zip_of(
            dir.path(),
            "icd10cm.zip",
            &[(tabular.as_str(), "<x/>"), (order.as_str(), "order\n")],
        );
        unpack_icd10cm(&zip, &into).expect("the two files CMS ships");
        assert!(into.join(&tabular).is_file());
        assert!(into.join(&order).is_file());

        let empty = zip_of(dir.path(), "icd10cm-empty.zip", &[("readme.txt", "x")]);
        let error = unpack_icd10cm(&empty, &into).expect_err("neither file");
        assert!(matches!(error, ArchiveError::NoEntry { .. }), "{error}");
    }

    #[test]
    fn the_rxnorm_unpacker_needs_the_concept_table() {
        let (dir, into) = workspace();
        let zip = zip_of(
            dir.path(),
            "rxnorm.zip",
            &[
                ("rrf/RXNCONSO.RRF", "conso\n"),
                ("rrf/RXNREL.RRF", "rel\n"),
                ("Readme.txt", "readme\n"),
                ("rrf/RXNDOC.RRF", "a table the build does not read\n"),
            ],
        );
        unpack_rxnorm(&zip, &into).expect("RXNCONSO.RRF");
        assert!(into.join("rrf/RXNCONSO.RRF").is_file());
        assert!(into.join("Readme.txt").is_file());
        assert!(
            !into.join("rrf/RXNDOC.RRF").exists(),
            "a table the build does not read stays in the zip"
        );

        let without = zip_of(dir.path(), "rxnorm-bad.zip", &[("rrf/RXNREL.RRF", "rel\n")]);
        let error = unpack_rxnorm(&without, &into).expect_err("no RXNCONSO.RRF");
        let ArchiveError::NoEntry { wanted, .. } = error else {
            panic!("a missing entry, not {error}");
        };
        assert_eq!(wanted, "RXNCONSO.RRF");
    }

    #[test]
    fn the_dhd_unpacker_takes_the_csv_tables_and_refuses_a_zip_with_none() {
        let (dir, into) = workspace();
        let zip = zip_of(
            dir.path(),
            "dhd.zip",
            &[("tables/concepts.csv", "a,b\n"), ("notes.pdf", "x")],
        );
        unpack_dhd(&zip, &into).expect("a csv table");
        assert!(into.join("tables/concepts.csv").is_file());
        assert!(!into.join("notes.pdf").exists());

        let empty = zip_of(dir.path(), "dhd-empty.zip", &[("notes.pdf", "x")]);
        let error = unpack_dhd(&empty, &into).expect_err("no csv table");
        assert!(matches!(error, ArchiveError::NoEntry { .. }), "{error}");
    }

    #[test]
    fn the_labcodeset_unpacker_takes_the_labconcepts_document_only() {
        let (dir, into) = workspace();
        let zip = zip_of(
            dir.path(),
            "labcodeset.zip",
            &[
                ("labconcepts-20260101.xml", "<labconcepts/>"),
                ("other-20260101.xml", "<other/>"),
            ],
        );
        unpack_labcodeset(&zip, &into).expect("a labconcepts document");
        assert!(into.join("labconcepts-20260101.xml").is_file());
        assert!(
            !into.join("other-20260101.xml").exists(),
            "another document in the same delivery is not the one the reader wants"
        );

        let empty = zip_of(dir.path(), "lab-empty.zip", &[("other.xml", "<other/>")]);
        let error = unpack_labcodeset(&empty, &into).expect_err("no labconcepts document");
        assert!(matches!(error, ArchiveError::NoEntry { .. }), "{error}");
    }

    #[test]
    fn the_release_root_precedes_the_snapshot_tree() {
        assert_eq!(
            snapshot_root(Path::new(
                "SnomedCT_X/Snapshot/Terminology/sct2_Concept_Snapshot_INT_20240101.txt"
            )),
            Some(PathBuf::from("SnomedCT_X"))
        );
        assert_eq!(
            snapshot_root(Path::new("Snapshot/Terminology/x.txt")),
            Some(PathBuf::new())
        );
        assert_eq!(
            snapshot_root(Path::new("SnomedCT_X/Full/Terminology/x.txt")),
            None
        );
    }
}
