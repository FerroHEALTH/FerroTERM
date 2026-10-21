// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: Apache-2.0

//! RF2 file names and the layout of a release directory.
//!
//! A release file is named `[FileType]_[ContentType]_[ContentSubType]_
//! [CountryNamespace]_[VersionDate].txt`, for example
//! `sct2_Concept_Snapshot_INT_20240101.txt` or
//! `der2_cRefset_LanguageSnapshot-en_NL1000146_20260630.txt`. The content
//! type of a reference set file spells its additional columns as a pattern of
//! `c` (component), `i` (integer), and `s` (string) letters before `Refset`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::time::{EffectiveTime, EffectiveTimeError};

/// `sct2` (terminology) or `der2` (derivative).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileType {
    /// A terminology component file, `sct2`.
    Terminology,
    /// A derivative file, `der2`.
    Derivative,
}

/// The release type a file carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReleaseType {
    /// Every version of every component.
    Full,
    /// The current version of every component.
    Snapshot,
    /// The rows changed since the previous release.
    Delta,
}

/// The type of one additional reference set column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldKind {
    /// An SCTID or UUID reference to a component.
    Component,
    /// An integer.
    Integer,
    /// A string.
    String,
}

/// What a file holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentType {
    /// `sct2_Concept`.
    Concept,
    /// `sct2_Description`.
    Description,
    /// `sct2_TextDefinition`.
    TextDefinition,
    /// `sct2_Relationship`, the inferred view.
    Relationship,
    /// `sct2_StatedRelationship`.
    StatedRelationship,
    /// `sct2_RelationshipConcreteValues`.
    RelationshipConcreteValues,
    /// `sct2_Identifier`.
    Identifier,
    /// A reference set, with the kinds of its additional columns.
    Refset(Vec<FieldKind>),
}

/// A file name that is not an RF2 release file name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FileNameError {
    /// Not five underscore-separated parts ending in `.txt`.
    #[error("{name:?} is not an RF2 file name")]
    Shape {
        /// The offending name.
        name: String,
    },
    /// The first part is neither `sct2` nor `der2`.
    #[error("{name:?}: unknown file type {file_type:?}")]
    FileType {
        /// The offending name.
        name: String,
        /// The first part.
        file_type: String,
    },
    /// The content type is not a component file or a refset pattern.
    #[error("{name:?}: unknown content type {content_type:?}")]
    ContentType {
        /// The offending name.
        name: String,
        /// The second part.
        content_type: String,
    },
    /// The content subtype names no release type.
    #[error("{name:?}: no release type (Full, Snapshot, Delta) in {subtype:?}")]
    ReleaseType {
        /// The offending name.
        name: String,
        /// The third part.
        subtype: String,
    },
    /// The version date is malformed.
    #[error("{name:?}: {source}")]
    Date {
        /// The offending name.
        name: String,
        /// The underlying error.
        #[source]
        source: EffectiveTimeError,
    },
}

/// A parsed RF2 file name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileName {
    /// `sct2` or `der2`.
    pub file_type: FileType,
    /// What the file holds.
    pub content_type: ContentType,
    /// The summary before the release type, for example `Language` or
    /// `OWLExpression`; empty for component files.
    pub summary: String,
    /// The release type.
    pub release_type: ReleaseType,
    /// The language code after the release type, for example `en`.
    pub language: Option<String>,
    /// The country or namespace part, for example `INT` or `NL1000146`.
    pub namespace: String,
    /// The version date.
    pub date: EffectiveTime,
}

impl FileName {
    /// Parses a file name (no directory).
    ///
    /// # Errors
    ///
    /// Returns [`FileNameError`] when the name does not follow the convention.
    pub fn parse(name: &str) -> Result<Self, FileNameError> {
        let shape = || FileNameError::Shape {
            name: name.to_owned(),
        };
        let stem = name.strip_suffix(".txt").ok_or_else(shape)?;
        let parts: Vec<&str> = stem.split('_').collect();
        let [file_type, content_type, subtype, namespace, date] = parts.as_slice() else {
            return Err(shape());
        };
        let file_type = match *file_type {
            "sct2" => FileType::Terminology,
            "der2" => FileType::Derivative,
            other => {
                return Err(FileNameError::FileType {
                    name: name.to_owned(),
                    file_type: other.to_owned(),
                });
            }
        };
        let content_type =
            parse_content_type(content_type).ok_or_else(|| FileNameError::ContentType {
                name: name.to_owned(),
                content_type: (*content_type).to_owned(),
            })?;
        let (summary, release_type, language) =
            parse_subtype(subtype).ok_or_else(|| FileNameError::ReleaseType {
                name: name.to_owned(),
                subtype: (*subtype).to_owned(),
            })?;
        let date = EffectiveTime::parse(date).map_err(|source| FileNameError::Date {
            name: name.to_owned(),
            source,
        })?;
        Ok(Self {
            file_type,
            content_type,
            summary,
            release_type,
            language,
            namespace: (*namespace).to_owned(),
            date,
        })
    }
}

fn parse_content_type(text: &str) -> Option<ContentType> {
    match text {
        "Concept" => Some(ContentType::Concept),
        "Description" => Some(ContentType::Description),
        "TextDefinition" => Some(ContentType::TextDefinition),
        "Relationship" => Some(ContentType::Relationship),
        "StatedRelationship" => Some(ContentType::StatedRelationship),
        "RelationshipConcreteValues" => Some(ContentType::RelationshipConcreteValues),
        "Identifier" => Some(ContentType::Identifier),
        other => {
            let pattern = other.strip_suffix("Refset")?;
            pattern
                .chars()
                .map(|c| match c {
                    'c' => Some(FieldKind::Component),
                    'i' => Some(FieldKind::Integer),
                    's' => Some(FieldKind::String),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()
                .map(ContentType::Refset)
        }
    }
}

fn parse_subtype(text: &str) -> Option<(String, ReleaseType, Option<String>)> {
    for (word, release_type) in [
        ("Snapshot", ReleaseType::Snapshot),
        ("Full", ReleaseType::Full),
        ("Delta", ReleaseType::Delta),
    ] {
        if let Some(index) = text.rfind(word) {
            let summary = text.get(..index)?.to_owned();
            let rest = text.get(index + word.len()..)?;
            let language = match rest {
                "" => None,
                other => Some(other.strip_prefix('-')?.to_owned()),
            };
            return Some((summary, release_type, language));
        }
    }
    None
}

/// A failure while scanning a release directory.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    /// A directory could not be listed.
    #[error("cannot list {path}")]
    Io {
        /// The directory.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: io::Error,
    },
    /// The directory holds no RF2 files of the requested release type.
    #[error("{path} holds no {release_type:?} RF2 files")]
    NoFiles {
        /// The directory.
        path: PathBuf,
        /// The release type looked for.
        release_type: ReleaseType,
    },
    /// The files carry more than one version date.
    #[error("{path}: files carry several version dates: {dates:?}")]
    MixedDates {
        /// The directory.
        path: PathBuf,
        /// The dates found.
        dates: Vec<String>,
    },
}

/// One RF2 file in a release, located and named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseFile {
    /// The file on disk.
    pub path: PathBuf,
    /// Its parsed name.
    pub name: FileName,
}

/// The RF2 files of one release type under a directory.
#[derive(Debug, Clone)]
pub struct Release {
    root: PathBuf,
    kind: ReleaseType,
    date: EffectiveTime,
    files: Vec<ReleaseFile>,
}

impl Release {
    /// Scans `root` recursively for RF2 files of `release_type`.
    ///
    /// Files whose names do not follow the convention are skipped; files of
    /// other release types are skipped, so a package holding `Full/` and
    /// `Snapshot/` side by side opens as either.
    ///
    /// # Errors
    ///
    /// Returns [`ReleaseError`] when a directory cannot be listed, no file
    /// matches, or the matching files disagree on the version date.
    pub fn open(root: &Path, release_type: ReleaseType) -> Result<Self, ReleaseError> {
        let mut files = Vec::new();
        collect(root, release_type, &mut files)?;
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let mut dates: Vec<EffectiveTime> = files.iter().map(|f| f.name.date).collect();
        dates.sort();
        dates.dedup();
        let date = match dates.as_slice() {
            [] => {
                return Err(ReleaseError::NoFiles {
                    path: root.to_path_buf(),
                    release_type,
                });
            }
            [date] => *date,
            many => {
                return Err(ReleaseError::MixedDates {
                    path: root.to_path_buf(),
                    dates: many.iter().map(|d| d.compact()).collect(),
                });
            }
        };
        Ok(Self {
            root: root.to_path_buf(),
            kind: release_type,
            date,
            files,
        })
    }

    /// The directory the release was opened from.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The release type every file carries.
    #[must_use]
    pub const fn release_type(&self) -> ReleaseType {
        self.kind
    }

    /// The version date every file carries.
    #[must_use]
    pub const fn date(&self) -> EffectiveTime {
        self.date
    }

    /// Every file, in path order.
    #[must_use]
    pub fn files(&self) -> &[ReleaseFile] {
        &self.files
    }

    /// The files of one content type, in path order.
    pub fn of_type<'a>(
        &'a self,
        content_type: &'a ContentType,
    ) -> impl Iterator<Item = &'a ReleaseFile> + 'a {
        self.files
            .iter()
            .filter(move |f| &f.name.content_type == content_type)
    }

    /// Every reference set file, in path order.
    pub fn refsets(&self) -> impl Iterator<Item = &ReleaseFile> {
        self.files
            .iter()
            .filter(|f| matches!(f.name.content_type, ContentType::Refset(_)))
    }
}

fn collect(
    dir: &Path,
    release_type: ReleaseType,
    out: &mut Vec<ReleaseFile>,
) -> Result<(), ReleaseError> {
    let entries = fs::read_dir(dir).map_err(|source| ReleaseError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| ReleaseError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect(&path, release_type, out)?;
            continue;
        }
        let Some(name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        if let Ok(parsed) = FileName::parse(name)
            && parsed.release_type == release_type
        {
            out.push(ReleaseFile { path, name: parsed });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ContentType, FieldKind, FileName, FileNameError, FileType, ReleaseType};

    #[test]
    fn component_file_names_parse() {
        let name = FileName::parse("sct2_Concept_Snapshot_INT_20240101.txt").expect("valid");
        assert_eq!(name.file_type, FileType::Terminology);
        assert_eq!(name.content_type, ContentType::Concept);
        assert_eq!(name.release_type, ReleaseType::Snapshot);
        assert_eq!(name.language, None);
        assert_eq!(name.namespace, "INT");
        assert_eq!(name.date.compact(), "20240101");
        let description =
            FileName::parse("sct2_Description_Snapshot-nl_NL1000146_20260630.txt").expect("valid");
        assert_eq!(description.content_type, ContentType::Description);
        assert_eq!(description.language.as_deref(), Some("nl"));
        assert_eq!(description.namespace, "NL1000146");
    }

    #[test]
    fn refset_file_names_spell_their_columns() {
        let language =
            FileName::parse("der2_cRefset_LanguageSnapshot-en_INT_20240101.txt").expect("valid");
        assert_eq!(
            language.content_type,
            ContentType::Refset(vec![FieldKind::Component])
        );
        assert_eq!(language.summary, "Language");
        assert_eq!(language.language.as_deref(), Some("en"));
        let extended =
            FileName::parse("der2_iisssccRefset_ExtendedMapFull_INT_20240101.txt").expect("valid");
        assert_eq!(
            extended.content_type,
            ContentType::Refset(vec![
                FieldKind::Integer,
                FieldKind::Integer,
                FieldKind::String,
                FieldKind::String,
                FieldKind::String,
                FieldKind::Component,
                FieldKind::Component,
            ])
        );
        assert_eq!(extended.release_type, ReleaseType::Full);
        let simple = FileName::parse(
            "der2_Refset_110851000146103SimpleRefsetSnapshot_NL1000146_20260630.txt",
        )
        .expect("valid");
        assert_eq!(simple.content_type, ContentType::Refset(Vec::new()));
        assert_eq!(simple.summary, "110851000146103SimpleRefset");
        let owl =
            FileName::parse("sct2_sRefset_OWLExpressionSnapshot_INT_20240101.txt").expect("valid");
        assert_eq!(owl.file_type, FileType::Terminology);
        assert_eq!(owl.summary, "OWLExpression");
    }

    #[test]
    fn other_names_are_refused() {
        assert!(matches!(
            FileName::parse("Readme_en_20260630.txt"),
            Err(FileNameError::Shape { .. })
        ));
        assert!(matches!(
            FileName::parse("xyz2_Concept_Snapshot_INT_20240101.txt"),
            Err(FileNameError::FileType { .. })
        ));
        assert!(matches!(
            FileName::parse("sct2_Thing_Snapshot_INT_20240101.txt"),
            Err(FileNameError::ContentType { .. })
        ));
        assert!(matches!(
            FileName::parse("der2_xRefset_LanguageSnapshot_INT_20240101.txt"),
            Err(FileNameError::ContentType { .. })
        ));
        assert!(matches!(
            FileName::parse("sct2_Concept_Current_INT_20240101.txt"),
            Err(FileNameError::ReleaseType { .. })
        ));
        assert!(matches!(
            FileName::parse("sct2_Concept_Snapshot_INT_2024.txt"),
            Err(FileNameError::Date { .. })
        ));
    }
}
