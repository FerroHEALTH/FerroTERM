// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Keeping the last releases of each code system and removing the rest.
//!
//! A new release arrives beside the one it succeeds, so the previous release
//! is still there to roll back to. Retention is the one thing in the service
//! that removes anything, and it runs only after the server has reloaded the
//! set that includes the new release. It keeps the configured number of
//! release directories per code system, newest first, and reports what it
//! removed and how much of the disk the index root uses afterwards.
//!
//! No FHIR or SNOMED CT specification governs this: our own design.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::holdings::MANIFEST_FILE;
use crate::record::Pruned;

/// A retention pass that did not finish.
#[derive(Debug, thiserror::Error)]
pub enum RetentionError {
    /// The index root does not list.
    #[error("cannot list the index root at {path}")]
    List {
        /// The directory that did not list.
        path: PathBuf,
        /// Why it did not list.
        #[source]
        source: std::io::Error,
    },
    /// A release directory could not be removed.
    #[error("cannot remove the release at {path}")]
    Remove {
        /// The directory that did not go.
        path: PathBuf,
        /// Why it did not go.
        #[source]
        source: std::io::Error,
    },
}

/// What one retention pass did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// The releases that were removed.
    pub pruned: Vec<Pruned>,
    /// How many bytes the index root holds afterwards.
    pub bytes_used: u64,
}

/// Keeps the `keep` newest releases of each code system under `root`.
///
/// A release is newer than another when its recorded release date sorts
/// later, and its version identifier decides when two carry the same date. A
/// count of zero keeps everything: a service asked to hold nothing would empty
/// the index root, which is never what an operator means.
///
/// # Errors
///
/// Returns [`RetentionError::List`] when the index root does not list and
/// [`RetentionError::Remove`] when a release directory cannot be removed.
pub fn prune(root: &Path, keep: usize) -> Result<Report, RetentionError> {
    let mut report = Report::default();
    if keep > 0 {
        for (_, mut releases) in releases(root)? {
            releases.sort_by(|left, right| right.order.cmp(&left.order));
            for release in releases.into_iter().skip(keep) {
                let bytes = directory_bytes(&release.path);
                std::fs::remove_dir_all(&release.path).map_err(|source| {
                    RetentionError::Remove {
                        path: release.path.clone(),
                        source,
                    }
                })?;
                tracing::info!(
                    system = %release.system,
                    path = %release.path.display(),
                    bytes,
                    "a superseded release was removed"
                );
                report.pruned.push(Pruned {
                    system: release.system,
                    path: release.path,
                    bytes,
                });
            }
        }
    }
    report.bytes_used = directory_bytes(root);
    Ok(report)
}

/// One built release under the index root.
#[derive(Debug)]
struct Release {
    system: String,
    path: PathBuf,
    /// What decides which release is newer: the release date, then the version.
    order: (String, String),
}

/// Every built release under `root`, grouped by the code system it serves.
fn releases(root: &Path) -> Result<BTreeMap<String, Vec<Release>>, RetentionError> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(source) => {
            return Err(RetentionError::List {
                path: root.to_path_buf(),
                source,
            });
        }
    };
    let mut out: BTreeMap<String, Vec<Release>> = BTreeMap::new();
    for entry in entries {
        let entry = entry.map_err(|source| RetentionError::List {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let Some(manifest) = manifest(&path.join(MANIFEST_FILE)) else {
            continue;
        };
        let system = manifest.edition.unwrap_or_else(|| manifest.system.clone());
        out.entry(system.clone()).or_default().push(Release {
            system,
            path,
            order: (
                manifest.release_date.unwrap_or_default(),
                manifest.version.unwrap_or_default(),
            ),
        });
    }
    Ok(out)
}

/// The fields of an artifact manifest retention reads.
#[derive(Debug)]
struct Manifest {
    system: String,
    edition: Option<String>,
    version: Option<String>,
    release_date: Option<String>,
}

/// Reads the manifest at `path`, answering `None` when there is none to read.
fn manifest(path: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let field = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    Some(Manifest {
        system: field("system")?,
        edition: field("edition"),
        version: field("version"),
        release_date: field("releaseDate"),
    })
}

/// How many bytes the files under `path` hold.
///
/// A file the walk cannot read counts as nothing: the number is a report, and
/// a retention pass that failed over a permission bit would be worse than a
/// number that is slightly low.
#[must_use]
pub fn directory_bytes(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0_u64;
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            total = total.saturating_add(directory_bytes(&entry.path()));
        } else if let Ok(data) = entry.metadata() {
            total = total.saturating_add(data.len());
        }
    }
    total
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
mod tests {
    use super::prune;
    use std::path::Path;

    fn release(root: &Path, name: &str, date: &str, version: &str) -> std::io::Result<()> {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            dir.join("manifest.json"),
            format!(
                r#"{{"manifest":1,"system":"http://snomed.info/sct","edition":"http://snomed.info/sct/11000146104","version":"{version}","releaseDate":"{date}"}}"#
            ),
        )?;
        std::fs::write(dir.join("store.redb"), vec![0_u8; 128])
    }

    #[test]
    fn the_newest_releases_are_kept() -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("index");
        release(&root, "snomed-1-20260301", "2026-03-01", "v1")?;
        release(&root, "snomed-1-20260601", "2026-06-01", "v2")?;
        release(&root, "snomed-1-20260930", "2026-09-30", "v3")?;
        let report = prune(&root, 2)?;
        assert_eq!(report.pruned.len(), 1, "one superseded release goes");
        assert!(
            !root.join("snomed-1-20260301").exists(),
            "the oldest release is the one removed"
        );
        assert!(
            root.join("snomed-1-20260930").is_dir() && root.join("snomed-1-20260601").is_dir(),
            "the two newest stay for rollback"
        );
        assert!(report.bytes_used > 0, "the index size is reported");
        Ok(())
    }

    #[test]
    fn another_system_is_counted_on_its_own() -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("index");
        release(&root, "snomed-1-20260601", "2026-06-01", "v1")?;
        let loinc = root.join("loinc-2-83");
        std::fs::create_dir_all(&loinc)?;
        std::fs::write(
            loinc.join("manifest.json"),
            r#"{"manifest":1,"system":"http://loinc.org","edition":"http://loinc.org","version":"2.83"}"#,
        )?;
        let report = prune(&root, 1)?;
        assert!(
            report.pruned.is_empty(),
            "one release each is one release each"
        );
        Ok(())
    }

    #[test]
    fn keeping_nothing_removes_nothing() -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("index");
        release(&root, "snomed-1-20260601", "2026-06-01", "v1")?;
        let report = prune(&root, 0)?;
        assert!(
            report.pruned.is_empty(),
            "a retention count of zero never empties the index root"
        );
        Ok(())
    }

    #[test]
    fn a_directory_without_a_manifest_is_passed_over() -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("index");
        std::fs::create_dir_all(root.join("half-written"))?;
        release(&root, "snomed-1-20260601", "2026-06-01", "v1")?;
        release(&root, "snomed-1-20260930", "2026-09-30", "v2")?;
        let report = prune(&root, 1)?;
        assert_eq!(report.pruned.len(), 1, "only built releases are counted");
        assert!(
            root.join("half-written").is_dir(),
            "a directory a build is still writing is left alone"
        );
        Ok(())
    }
}
