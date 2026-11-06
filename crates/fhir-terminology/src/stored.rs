// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The code systems the server reads from a built artifact, served as one
//! provider.
//!
//! SNOMED CT, LOINC, `RxNorm`, ICD-11, and the classifications are each a
//! directory the offline build writes: `manifest.json`, the concept store,
//! the designation index, and, for a system with one, the hierarchy. A
//! [`system::StoredProvider`] answers what every such system answers alike
//! from the [`artifact::Artifact`] it opens, and its
//! [`system::StoredSystem`] decides the rest: the display rule, the status,
//! the properties, the codes it locates, and every filter, implicit value
//! set, and version rule of its own.
//!
//! No FHIR/SNOMED spec governs this split: our own design.

pub mod artifact;
pub mod system;

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use concept_graph::csr::CsrError;
use concept_store::store::StoreError;

use crate::provider::error::ProviderError;

/// The manifest version every stored system reads: the store beside the
/// hierarchy and the designation index as their own files.
pub const MANIFEST_VERSION: u32 = 2;

/// A failure to open an artifact directory, before any system reads what
/// only it carries.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// A file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The manifest does not parse.
    #[error("{path} is not an artifact manifest")]
    Manifest {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: serde_json::Error,
    },
    /// The manifest is of another layout version.
    #[error("the manifest is version {0}; this server reads version {MANIFEST_VERSION}")]
    ManifestVersion(u32),
    /// The store cannot be opened or read.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The hierarchy file does not read.
    #[error("cannot read the hierarchy file")]
    Hierarchy(#[from] concept_graph::persist::PersistError),
    /// The designation index file does not read.
    #[error("cannot read the designation index file")]
    Text(#[from] designation_index::persist::PersistError),
    /// The child adjacency cannot be derived.
    #[error("cannot derive the child adjacency")]
    Transpose(#[from] CsrError),
    /// The concept count meta entry is missing or malformed.
    #[error("the store's concept count is `{0:?}`")]
    ConceptCount(Option<String>),
}

/// The failure to map onto the seam's error.
pub(crate) fn storage(error: StoreError) -> ProviderError {
    ProviderError::Storage(Box::new(error))
}

/// The primary language subtag of a BCP 47 tag (`en-GB` is `en`), lowercased.
pub(crate) fn primary_subtag(language: &str) -> String {
    language
        .split(['-', '_'])
        .next()
        .unwrap_or(language)
        .to_ascii_lowercase()
}

/// Accepts an artifact whose manifest names `found` where the system reads
/// `wanted`, and refuses any other with `refuse`.
///
/// # Errors
///
/// Returns what `refuse` makes of `found` when it is not `wanted`.
pub(crate) fn serves<E>(
    found: &str,
    wanted: &str,
    refuse: impl FnOnce(String) -> E,
) -> Result<(), E> {
    if found == wanted {
        Ok(())
    } else {
        Err(refuse(found.to_owned()))
    }
}

/// The side file `name` of the artifact `dir`, opened for a buffered read.
///
/// # Errors
///
/// Returns [`OpenError::Io`] when the file does not open.
pub(crate) fn side_file(dir: &Path, name: &str) -> Result<BufReader<File>, OpenError> {
    let path = dir.join(name);
    concept_graph::read::buffered(&path).map_err(|source| OpenError::Io { path, source })
}

#[cfg(test)]
mod tests {
    use super::primary_subtag;

    #[test]
    fn the_primary_subtag_is_the_language() {
        assert_eq!(primary_subtag("en-GB"), "en");
        assert_eq!(primary_subtag("nl"), "nl");
        assert_eq!(primary_subtag("EN_us"), "en");
    }
}
