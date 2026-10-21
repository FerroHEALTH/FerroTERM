// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The RF2 lane: the offline build, run as a subprocess.
//!
//! A SNOMED CT release arrives as one archive, and turning it into the
//! artifact the server reads is what `ferroterm-build` does. The service runs
//! it as a child process in the same image, writing into a staging directory,
//! and renames that directory into the index root only after the build
//! succeeds. A directory a build is still writing carries no manifest, so the
//! server passes over it while it is being written.
//!
//! No FHIR or SNOMED CT specification governs this: our own design.

use std::path::{Path, PathBuf};

/// A build that did not produce an artifact.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// The build binary did not start.
    #[error("{command} did not start")]
    Spawn {
        /// The binary the service tried to run.
        command: PathBuf,
        /// Why it did not start.
        #[source]
        source: std::io::Error,
    },
    /// The build ran and ended with a failure.
    #[error("{command} exited with {status} while building {archive}: {stderr}")]
    Failed {
        /// The binary that ran.
        command: PathBuf,
        /// The archive it was reading.
        archive: PathBuf,
        /// How it ended, as its exit status or the signal that ended it.
        status: String,
        /// What it wrote to its error stream.
        stderr: String,
    },
    /// The staging directory could not be prepared.
    #[error("cannot prepare the staging directory {path}")]
    Staging {
        /// The directory the service was preparing.
        path: PathBuf,
        /// Why it could not be prepared.
        #[source]
        source: std::io::Error,
    },
}

/// One finished build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildReport {
    /// Where the artifact was written.
    pub out: PathBuf,
    /// How long the build took.
    pub duration_ms: u64,
    /// What the build wrote to its output stream.
    pub stdout: String,
}

/// Builds the RF2 archive `archive` into `out` with `command`.
///
/// The staging directory is removed first, so a directory a previous run left
/// behind is never mistaken for part of this build. The service writes nothing
/// but staging directories there.
///
/// # Errors
///
/// Returns [`BuildError::Staging`] when the staging directory cannot be
/// prepared, [`BuildError::Spawn`] when the build binary does not start, and
/// [`BuildError::Failed`] when it ends with a failure.
pub async fn build_rf2(
    command: &Path,
    archive: &Path,
    out: &Path,
) -> Result<BuildReport, BuildError> {
    prepare(out).await?;
    let started = std::time::Instant::now();
    let output = tokio::process::Command::new(command)
        .arg("--rf2")
        .arg(archive)
        .arg("--out")
        .arg(out)
        .output()
        .await
        .map_err(|source| BuildError::Spawn {
            command: command.to_path_buf(),
            source,
        })?;
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    if !output.status.success() {
        return Err(BuildError::Failed {
            command: command.to_path_buf(),
            archive: archive.to_path_buf(),
            status: output.status.to_string(),
            stderr: clipped(&String::from_utf8_lossy(&output.stderr)),
        });
    }
    Ok(BuildReport {
        out: out.to_path_buf(),
        duration_ms,
        stdout: clipped(&String::from_utf8_lossy(&output.stdout)),
    })
}

/// Empties the staging directory `out` and creates it.
async fn prepare(out: &Path) -> Result<(), BuildError> {
    match tokio::fs::remove_dir_all(out).await {
        Ok(()) => {}
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(BuildError::Staging {
                path: out.to_path_buf(),
                source,
            });
        }
    }
    if let Some(parent) = out.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| BuildError::Staging {
                path: parent.to_path_buf(),
                source,
            })?;
    }
    Ok(())
}

/// The first 4000 characters of `text`, so one build cannot fill a record.
fn clipped(text: &str) -> String {
    text.chars()
        .take(4000)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
mod tests {
    use super::{build_rf2, clipped};

    #[tokio::test]
    async fn a_build_that_does_not_start_is_reported() -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let built = build_rf2(
            &dir.path().join("no-such-build-binary"),
            &dir.path().join("release.zip"),
            &dir.path().join("staging").join("out"),
        )
        .await;
        assert!(
            built.is_err(),
            "a build binary that is not there is a failure the run reports"
        );
        Ok(())
    }

    #[test]
    fn a_long_error_stream_is_clipped() {
        let long = "x".repeat(9000);
        assert_eq!(
            clipped(&long).chars().count(),
            4000,
            "one build cannot fill a run record"
        );
    }
}
