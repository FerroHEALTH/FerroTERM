// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What an artifact directory holds, before a provider opens it.

use std::path::{Path, PathBuf};

/// The manifest cannot say which system the artifact serves.
#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    /// The manifest does not read.
    #[error("cannot read {path}")]
    Io {
        /// The manifest path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The manifest is not JSON with a `system`.
    #[error("{path} is not an artifact manifest")]
    Manifest {
        /// The manifest path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: serde_json::Error,
    },
    /// The manifest names no `system`.
    #[error("{path} names no system")]
    NoSystem {
        /// The manifest path.
        path: PathBuf,
    },
}

/// What a manifest says about its artifact before a provider opens it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Description {
    /// The code system URI served.
    pub system: String,
    /// The pipeline that built it (`classification`), when the manifest says.
    pub kind: Option<String>,
}

/// The file name of an artifact's manifest.
///
/// A directory holding one is an artifact; a directory holding none is either
/// a root whose children are artifacts or a staging directory a build has not
/// finished writing.
pub const MANIFEST_FILE: &str = concept_store::MANIFEST_FILE;

/// Opens `path` for a structure to be assembled from.
///
/// A side file is read through this rather than into a `Vec`, because the
/// whole file would then sit in memory beside the structure it becomes, and
/// the allocator does not hand those pages back the moment the `Vec` drops
/// (#322). The store reads its own columns through the same helper, so every
/// structure of an artifact arrives the same way.
///
/// # Errors
///
/// Returns the I/O error from opening the file.
pub fn reader(path: &Path) -> std::io::Result<std::io::BufReader<std::fs::File>> {
    concept_graph::read::buffered(path)
}

/// Whether `dir` holds an artifact manifest.
#[must_use]
pub fn is_artifact(dir: &Path) -> bool {
    dir.join(MANIFEST_FILE).is_file()
}

/// The system and kind of the artifact under `dir`, from its manifest.
///
/// # Errors
///
/// Returns [`ArtifactError`] when the manifest does not read or names no system.
pub fn describe(dir: &Path) -> Result<Description, ArtifactError> {
    let path = dir.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path).map_err(|source| ArtifactError::Io {
        path: path.clone(),
        source,
    })?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|source| ArtifactError::Manifest {
            path: path.clone(),
            source,
        })?;
    let system = value
        .get("system")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or(ArtifactError::NoSystem { path })?;
    Ok(Description {
        system,
        kind: value
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
    })
}

/// The canonical of the extension declaring which artifact a served code
/// system version was read from.
///
/// No FHIR or SNOMED specification governs this: our own design. No element of
/// `TerminologyCapabilities` in R4, R4B, R5, or the R6 ballot records the index
/// a server read, and FHIR reserves extensions for exactly that, "additional
/// information that is not part of the basic definition"
/// (<https://hl7.org/fhir/R4B/extensibility.html>).
pub const SOURCE_EXTENSION: &str =
    "https://ferroterm.eu/fhir/StructureDefinition/terminology-artifact";

/// The URL of the sub-extension carrying the artifact's own directory name.
pub const SOURCE_NAME: &str = "name";

/// The URL of the sub-extension carrying the recorded release identifier.
pub const SOURCE_RELEASE: &str = "release";

/// Which built index a served code system version was read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The artifact directory's own name, without the directories above it.
    pub name: Option<String>,
    /// The release identifier the offline build wrote into the manifest.
    pub release: String,
}

impl Source {
    /// The source of the artifact at `dir`, whose manifest recorded `release`.
    ///
    /// Only the directory's own name is kept, so the declaration identifies the
    /// artifact without publishing where the operator keeps it. A path with no
    /// final component, such as one ending in `..`, yields no name.
    #[must_use]
    pub fn new(dir: &Path, release: &str) -> Self {
        Self {
            name: dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            release: release.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Source;

    #[test]
    fn a_source_names_the_artifact_without_the_directories_above_it() {
        let source = Source::new(Path::new("/srv/ferroterm/indexes/int"), "20260901");
        assert_eq!(source.name.as_deref(), Some("int"));
        assert_eq!(source.release, "20260901");
    }

    #[test]
    fn a_path_with_no_final_component_yields_no_name() {
        let source = Source::new(Path::new("/srv/.."), "20260901");
        assert_eq!(source.name, None, "`..` names no artifact");
        assert_eq!(source.release, "20260901");
    }
}
