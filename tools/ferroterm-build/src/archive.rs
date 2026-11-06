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

pub mod sources;

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

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use super::sources::unpack_dhd;
    use super::{ArchiveError, snapshot_root, unpack_snapshot};

    /// A zip holding `entries`, each an entry name and its bytes.
    ///
    /// A name ending in `/` is written as a directory, which is what a real
    /// release carries and what the unpackers have to pass over.
    pub(super) fn zip_of(dir: &Path, name: &str, entries: &[(&str, &str)]) -> PathBuf {
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
    pub(super) fn workspace() -> (tempfile::TempDir, PathBuf) {
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
