// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where a build writes: the `--out` directory, or a child of the index root
//! named for the source.
//!
//! The container's `build` service names the index root it mounts from the
//! host (`FERROTERM_INDEX_ROOT=/data/index`), so a build with no `--out` lands
//! on that mount, and an `--out` that resolves anywhere else is refused: the
//! container's own filesystem is discarded when it ends, and a build reported
//! as written there would be lost. No specification governs this: our own
//! design.

use std::path::{Component, Path, PathBuf};

use crate::Cli;

/// The environment variable naming the index root a build writes under.
pub const INDEX_ROOT_ENV: &str = "FERROTERM_INDEX_ROOT";

/// Why no output directory could be settled.
#[derive(Debug, thiserror::Error)]
pub enum OutputError {
    /// Neither `--out` nor an index root was given.
    #[error("give `--out`, or `--index-root` (or `FERROTERM_INDEX_ROOT`) to write under")]
    NoOut,
    /// `--out` resolves outside the index root.
    #[error(
        "the output {out} lies outside the index root {root}; write under the root, where the server reads, or leave out `--out`"
    )]
    OutsideRoot {
        /// The absolute output directory `--out` named.
        out: PathBuf,
        /// The absolute index root.
        root: PathBuf,
    },
    /// The current directory, which a relative path is read against, is unknown.
    #[error("cannot read the current directory")]
    CurrentDir(#[source] std::io::Error),
}

/// The absolute directory the build `cli` describes writes into, with a
/// relative path read against the current directory.
///
/// # Errors
///
/// Returns [`OutputError::CurrentDir`] when the current directory cannot be
/// read, and the errors of [`resolve`].
pub fn output(cli: &Cli) -> Result<PathBuf, OutputError> {
    let cwd = std::env::current_dir().map_err(OutputError::CurrentDir)?;
    resolve(cli, &cwd)
}

/// The absolute directory the build `cli` describes writes into, with a
/// relative path read against `cwd`.
///
/// `--out` wins when given. Without it, the build writes into a child of the
/// index root named for the source (`snomed`, `loinc`, `rxnorm`, `atc`, `dhd`,
/// `icd-10-cm`, the last segment of a `ClaML` system), or into the root itself
/// for the sources that already lay out their own children (ICD-11, the
/// G-Standaard, the Labcodeset).
///
/// # Errors
///
/// Returns [`OutputError::NoOut`] when neither `--out` nor an index root is
/// given, and [`OutputError::OutsideRoot`] when both are and `--out` does not
/// lie under the root.
pub fn resolve(cli: &Cli, cwd: &Path) -> Result<PathBuf, OutputError> {
    let root = cli.index_root.as_deref().map(|root| absolute(root, cwd));
    let out = match (&cli.out, &root) {
        (Some(out), _) => absolute(out, cwd),
        (None, Some(root)) => match default_child(cli) {
            Some(child) => root.join(child),
            None => root.clone(),
        },
        (None, None) => return Err(OutputError::NoOut),
    };
    if let Some(root) = root
        && !out.starts_with(&root)
    {
        return Err(OutputError::OutsideRoot { out, root });
    }
    Ok(out)
}

/// The child of the index root a source builds into, or `None` for a source
/// that writes its own children under the directory it is given.
fn default_child(cli: &Cli) -> Option<String> {
    if cli.icd11.is_some() || cli.gstandaard.is_some() || cli.labcodeset.is_some() {
        return None;
    }
    let name = if cli.loinc.is_some() {
        "loinc"
    } else if cli.rxnorm.is_some() {
        "rxnorm"
    } else if cli.atc.is_some() {
        "atc"
    } else if cli.dhd.is_some() {
        "dhd"
    } else if !cli.icd10cm.is_empty() {
        "icd-10-cm"
    } else if cli.claml.is_some() {
        return Some(claml_child(cli.system.as_deref()));
    } else {
        "snomed"
    };
    Some(name.to_owned())
}

/// The last path segment of a `ClaML` system URI (`icd-10-nl` for
/// `http://hl7.org/fhir/sid/icd-10-nl`), or `claml` when it is no plain name.
fn claml_child(system: Option<&str>) -> String {
    system
        .and_then(|system| system.trim_end_matches('/').rsplit('/').next())
        .filter(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
                && !segment.starts_with('.')
        })
        .map_or_else(|| String::from("claml"), str::to_owned)
}

/// `path` read against `cwd`, with every `.` and `..` step removed.
// NOTE: lexical on purpose, the output need not exist yet; a symlink under the root is not
// followed (<https://doc.rust-lang.org/std/path/fn.absolute.html> keeps `..` on Unix).
fn absolute(path: &Path, cwd: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in cwd.join(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}
