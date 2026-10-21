// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `RxNorm` Rich Release Format (RRF) reading.
//!
//! Locates the files of an unpacked release (`RXNCONSO.RRF`, `RXNREL.RRF`,
//! `RXNSAT.RRF`, and `RXNSTY.RRF` when the release has it) and reads them
//! streaming into typed rows by the documented column positions
//! (<https://www.nlm.nih.gov/research/umls/rxnorm/docs/techdoc.html>).
//! The version is the release date as the file names carry it (`09082026`),
//! the form the FHIR `RxNorm` page asks for (<https://hl7.org/fhir/R4B/rxnorm.html>).
#![doc(test(attr(deny(warnings))))]

pub mod row;

use std::fs::File;
use std::path::{Path, PathBuf};

use crate::row::{Atom, Attribute, FromRecord, Relationship, SemanticType};

/// A failure to read a release.
#[derive(Debug, thiserror::Error)]
pub enum RrfError {
    /// A directory or file cannot be read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// A file the reader needs is not in the release.
    #[error("the release under {root} has no {name}")]
    Missing {
        /// The release directory.
        root: PathBuf,
        /// The file name looked for.
        name: &'static str,
    },
    /// A row does not parse.
    #[error("cannot parse {path}")]
    Csv {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: csv::Error,
    },
    /// A row has fewer columns than the layout.
    #[error("{path}:{line}: {columns} columns, the layout has {expected}")]
    Columns {
        /// The path.
        path: PathBuf,
        /// The 1-based line.
        line: u64,
        /// The columns found.
        columns: usize,
        /// The columns expected.
        expected: usize,
    },
    /// A field is not UTF-8.
    #[error("{path}:{line}: a field is not UTF-8")]
    Utf8 {
        /// The path.
        path: PathBuf,
        /// The 1-based line.
        line: u64,
        /// The cause.
        #[source]
        source: std::str::Utf8Error,
    },
    /// An identifier is not a number.
    #[error("{path}:{line}: `{value}` is not an identifier")]
    Identifier {
        /// The path.
        path: PathBuf,
        /// The 1-based line.
        line: u64,
        /// The value.
        value: String,
        /// The cause.
        #[source]
        source: std::num::ParseIntError,
    },
}

/// The concept and atom names, `RXNCONSO.RRF`.
pub const CONSO: &str = "RXNCONSO.RRF";
/// The relationships, `RXNREL.RRF`.
pub const REL: &str = "RXNREL.RRF";
/// The attributes, `RXNSAT.RRF`.
pub const SAT: &str = "RXNSAT.RRF";
/// The semantic types, `RXNSTY.RRF` (the full release only).
pub const STY: &str = "RXNSTY.RRF";

/// An unpacked release, its files located by name at any depth.
#[derive(Debug, Clone)]
pub struct Release {
    root: PathBuf,
    files: Vec<PathBuf>,
}

impl Release {
    /// Opens the release under `root`.
    ///
    /// # Errors
    ///
    /// Returns [`RrfError`] when the directory does not read or has no
    /// `RXNCONSO.RRF`.
    pub fn open(root: &Path) -> Result<Self, RrfError> {
        let mut files = Vec::new();
        walk(root, &mut files)?;
        files.sort();
        let release = Self {
            root: root.to_path_buf(),
            files,
        };
        release.file(CONSO)?;
        Ok(release)
    }

    /// The release directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The path of the file `name`, compared without case.
    ///
    /// # Errors
    ///
    /// Returns [`RrfError::Missing`] when the release has no such file.
    pub fn file(&self, name: &'static str) -> Result<PathBuf, RrfError> {
        self.optional(name).ok_or_else(|| RrfError::Missing {
            root: self.root.clone(),
            name,
        })
    }

    /// The path of the file `name` when the release has it.
    #[must_use]
    pub fn optional(&self, name: &str) -> Option<PathBuf> {
        self.files
            .iter()
            .find(|p| {
                p.file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .is_some_and(|f| f.eq_ignore_ascii_case(name))
            })
            .cloned()
    }

    /// The release date from a `Readme_*_MMDDYYYY.txt` in the release, else
    /// from eight digits ending the directory name (`RxNorm_full_09082026`).
    #[must_use]
    pub fn version(&self) -> Option<String> {
        let from_readme = self.files.iter().find_map(|p| {
            let name = p.file_name()?.to_str()?;
            let stem = name.strip_suffix(".txt")?;
            (stem.starts_with("Readme"))
                .then(|| date_suffix(stem))
                .flatten()
        });
        from_readme.or_else(|| {
            self.root
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .and_then(date_suffix)
        })
    }

    /// The atoms.
    ///
    /// # Errors
    ///
    /// Returns [`RrfError`] when the file is missing or does not open.
    pub fn atoms(&self) -> Result<Rows<Atom>, RrfError> {
        Rows::open(self.file(CONSO)?)
    }

    /// The relationships.
    ///
    /// # Errors
    ///
    /// Returns [`RrfError`] when the file is missing or does not open.
    pub fn relationships(&self) -> Result<Rows<Relationship>, RrfError> {
        Rows::open(self.file(REL)?)
    }

    /// The attributes.
    ///
    /// # Errors
    ///
    /// Returns [`RrfError`] when the file is missing or does not open.
    pub fn attributes(&self) -> Result<Rows<Attribute>, RrfError> {
        Rows::open(self.file(SAT)?)
    }

    /// The semantic types, when the release has them.
    ///
    /// # Errors
    ///
    /// Returns [`RrfError`] when the file does not open.
    pub fn semantic_types(&self) -> Result<Option<Rows<SemanticType>>, RrfError> {
        self.optional(STY).map(Rows::open).transpose()
    }
}

/// Eight digits ending `name` (after the last `_`), as the version.
fn date_suffix(name: &str) -> Option<String> {
    let (_, tail) = name.rsplit_once('_')?;
    (tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_digit())).then(|| tail.to_owned())
}

fn walk(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), RrfError> {
    let io = |source| RrfError::Io {
        path: root.to_path_buf(),
        source,
    };
    for entry in std::fs::read_dir(root).map_err(io)? {
        let path = entry.map_err(io)?.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// The rows of one file, streamed.
pub struct Rows<T> {
    reader: csv::Reader<File>,
    record: csv::ByteRecord,
    path: PathBuf,
    line: u64,
    row: std::marker::PhantomData<T>,
}

impl<T> std::fmt::Debug for Rows<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rows")
            .field("path", &self.path)
            .field("line", &self.line)
            .finish_non_exhaustive()
    }
}

impl<T: FromRecord> Rows<T> {
    fn open(path: PathBuf) -> Result<Self, RrfError> {
        let file = File::open(&path).map_err(|source| RrfError::Io {
            path: path.clone(),
            source,
        })?;
        let reader = csv::ReaderBuilder::new()
            .delimiter(b'|')
            .has_headers(false)
            .flexible(true)
            .quoting(false)
            .from_reader(file);
        Ok(Self {
            reader,
            record: csv::ByteRecord::new(),
            path,
            line: 0,
            row: std::marker::PhantomData,
        })
    }
}

impl<T: FromRecord> Iterator for Rows<T> {
    type Item = Result<T, RrfError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.reader.read_byte_record(&mut self.record) {
            Ok(false) => None,
            Ok(true) => {
                self.line = self.line.saturating_add(1);
                Some(T::from_record(&self.record, &self.path, self.line))
            }
            Err(source) => Some(Err(RrfError::Csv {
                path: self.path.clone(),
                source,
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::date_suffix;

    #[test]
    fn the_version_is_the_eight_digit_date() {
        assert_eq!(
            date_suffix("Readme_Full_Prescribe_09082026").as_deref(),
            Some("09082026")
        );
        assert_eq!(
            date_suffix("RxNorm_full_09082026").as_deref(),
            Some("09082026")
        );
        assert_eq!(date_suffix("RxNorm_full_prescribe_current"), None);
        assert_eq!(date_suffix("Readme"), None);
    }
}
