// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The built index a provider read its code system version from, which it
//! hands across the seam for the capability statement.

use std::path::Path;

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
