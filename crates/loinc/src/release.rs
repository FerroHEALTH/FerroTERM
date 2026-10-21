// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Locating the files of an unpacked release.

use std::path::{Path, PathBuf};

/// A failure to read a release.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    /// The directory cannot be read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// A file the loader needs is not in the release.
    #[error("the release under {root} has no {name}")]
    Missing {
        /// The release directory.
        root: PathBuf,
        /// The file name looked for.
        name: &'static str,
    },
    /// A CSV file does not parse.
    #[error("cannot parse {path}")]
    Csv {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: csv::Error,
    },
    /// A column the loader needs is not in the header.
    #[error("{path} has no column `{column}`")]
    Column {
        /// The path.
        path: PathBuf,
        /// The column name.
        column: &'static str,
    },
    /// A code is not shaped like a LOINC code.
    #[error("{path}: `{code}` is not a LOINC code")]
    Code {
        /// The path.
        path: PathBuf,
        /// The code.
        code: String,
    },
}

/// The term table, `Loinc.csv`.
pub const TERMS: &str = "Loinc.csv";
/// The part table, `Part.csv`.
pub const PARTS: &str = "Part.csv";
/// The multiaxial hierarchy, `ComponentHierarchyBySystem.csv`.
pub const HIERARCHY: &str = "ComponentHierarchyBySystem.csv";
/// The primary part links, `LoincPartLink_Primary.csv`: each term's part per
/// axis.
pub const LINKS: &str = "LoincPartLink_Primary.csv";
/// The answer lists and their answers, `AnswerList.csv`.
pub const ANSWER_LISTS: &str = "AnswerList.csv";
/// The links from terms to answer lists, `LoincAnswerListLink.csv`.
pub const ANSWER_LINKS: &str = "LoincAnswerListLink.csv";
/// The suffix of a linguistic variant file, `nlNL22LinguisticVariant.csv`.
pub const VARIANT_SUFFIX: &str = "LinguisticVariant.csv";

/// An unpacked release: the files found under its directory.
#[derive(Debug, Clone)]
pub struct Release {
    root: PathBuf,
    files: Vec<PathBuf>,
}

impl Release {
    /// Lists the files under `root`, any depth.
    ///
    /// # Errors
    ///
    /// Returns [`ReleaseError::Io`] when a directory does not read, and
    /// [`ReleaseError::Missing`] when there is no `Loinc.csv`.
    pub fn open(root: &Path) -> Result<Self, ReleaseError> {
        let mut files = Vec::new();
        walk(root, &mut files)?;
        files.sort();
        let release = Self {
            root: root.to_path_buf(),
            files,
        };
        release.file(TERMS)?;
        Ok(release)
    }

    /// The release directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The file named `name` (compared without case) nearest the root: the
    /// term table is `LoincTable/Loinc.csv`, and the panels folder carries a
    /// file of the same name that is not it.
    ///
    /// # Errors
    ///
    /// Returns [`ReleaseError::Missing`] when the release has none.
    pub fn file(&self, name: &'static str) -> Result<&Path, ReleaseError> {
        self.optional(name).ok_or_else(|| ReleaseError::Missing {
            root: self.root.clone(),
            name,
        })
    }

    /// The file named `name`, when the release has one.
    #[must_use]
    pub fn optional(&self, name: &str) -> Option<&Path> {
        self.files
            .iter()
            .filter(|p| {
                p.file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .is_some_and(|f| f.eq_ignore_ascii_case(name))
            })
            .min_by_key(|p| p.components().count())
            .map(PathBuf::as_path)
    }

    /// The linguistic variant files, sorted by name.
    pub fn variants(&self) -> impl Iterator<Item = &Path> {
        self.files
            .iter()
            .filter(|p| {
                p.file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .is_some_and(|f| f.ends_with(VARIANT_SUFFIX))
            })
            .map(PathBuf::as_path)
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ReleaseError> {
    let entries = std::fs::read_dir(dir).map_err(|source| ReleaseError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let path = entry
            .map_err(|source| ReleaseError::Io {
                path: dir.to_path_buf(),
                source,
            })?
            .path();
        if path.is_dir() {
            walk(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// A CSV reader over `path` with its header, for reading by column name.
pub(crate) struct Table {
    pub(crate) path: PathBuf,
    pub(crate) reader: csv::Reader<std::fs::File>,
    header: csv::StringRecord,
}

impl Table {
    pub(crate) fn open(path: &Path) -> Result<Self, ReleaseError> {
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .from_path(path)
            .map_err(|source| ReleaseError::Csv {
                path: path.to_path_buf(),
                source,
            })?;
        let header = reader
            .headers()
            .map_err(|source| ReleaseError::Csv {
                path: path.to_path_buf(),
                source,
            })?
            .clone();
        Ok(Self {
            path: path.to_path_buf(),
            reader,
            header,
        })
    }

    /// The index of `column` in the header.
    pub(crate) fn column(&self, column: &'static str) -> Result<usize, ReleaseError> {
        self.header
            .iter()
            .position(|h| h.eq_ignore_ascii_case(column))
            .ok_or_else(|| ReleaseError::Column {
                path: self.path.clone(),
                column,
            })
    }

    /// Every header name, in order.
    pub(crate) fn columns(&self) -> Vec<String> {
        self.header.iter().map(str::to_owned).collect()
    }
}

/// The value at `index` of `record`, trimmed; empty when absent.
pub(crate) fn field(record: &csv::StringRecord, index: usize) -> &str {
    record.get(index).map(str::trim).unwrap_or_default()
}

/// A CSV failure at `path`.
pub(crate) fn csv_at(path: &Path, source: csv::Error) -> ReleaseError {
    ReleaseError::Csv {
        path: path.to_path_buf(),
        source,
    }
}
