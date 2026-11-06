// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: Apache-2.0

//! The kind of a reference set file, read from the additional columns its
//! header names.

use std::path::Path;

use super::MEMBER_COLUMNS;
use crate::reader::{Rf2Error, Rf2Reader};

/// What a reference set file holds, decided by the columns its header names.
///
/// A file name's summary is free-form: a derivative package writes its own
/// name into it, before the reference set type
/// (`der2_cRefset_ICNPLanguageSnapshot-en`) or after it
/// (`sct2_sRefset_OWLExpressionICNPFull`), so no substring of it is a reliable
/// signal. The additional columns are: the release file specification fixes
/// them per reference set pattern, and the header names them
/// (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-release-file-specification>).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefsetKind {
    /// A language reference set: `acceptabilityId`.
    Language,
    /// An OWL expression reference set: `owlExpression`.
    OwlExpression,
    /// The module dependency reference set: `sourceEffectiveTime` and
    /// `targetEffectiveTime`.
    ModuleDependency,
    /// Anything else: a simple, map, association, or attribute value set,
    /// which the build reads as content.
    Content,
}

impl RefsetKind {
    /// The kind the additional column names imply.
    #[must_use]
    pub fn of_columns(columns: &[String]) -> Self {
        let has = |name: &str| columns.iter().any(|column| column == name);
        if has("acceptabilityId") {
            Self::Language
        } else if has("owlExpression") {
            Self::OwlExpression
        } else if has("sourceEffectiveTime") && has("targetEffectiveTime") {
            Self::ModuleDependency
        } else {
            Self::Content
        }
    }
}

/// The kind of the reference set file at `path`, from its header alone.
///
/// # Errors
///
/// Returns [`Rf2Error`] when the file cannot be opened or its header does not
/// start with the six member columns.
pub fn read(path: &Path) -> Result<RefsetKind, Rf2Error> {
    let file = std::fs::File::open(path).map_err(|source| Rf2Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = Rf2Reader::new_with(path, std::io::BufReader::new(file), |actual| {
        if actual.len() >= MEMBER_COLUMNS.len()
            && actual.iter().zip(MEMBER_COLUMNS).all(|(a, e)| a == e)
        {
            Ok(())
        } else {
            Err(Rf2Error::Header {
                path: path.to_path_buf(),
                expected: MEMBER_COLUMNS.iter().map(|c| (*c).to_owned()).collect(),
                actual: actual.to_vec(),
            })
        }
    })?;
    let additional: Vec<String> = reader
        .columns()
        .iter()
        .skip(MEMBER_COLUMNS.len())
        .cloned()
        .collect();
    Ok(RefsetKind::of_columns(&additional))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{file, header};
    use super::{MEMBER_COLUMNS, RefsetKind, Rf2Error, read};

    /// The additional column names of one refset file, as `of_columns` reads
    /// them.
    fn columns(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    /// Reads `path` for a kind, over a file written for the test.
    fn kind_of(header: &str) -> Result<RefsetKind, Rf2Error> {
        let dir = tempfile::tempdir().expect("a directory to write the header into");
        let path = dir
            .path()
            .join("der2_Refset_SimpleSnapshot_INT_20260101.txt");
        std::fs::write(&path, file(&[header])).expect("the header is written");
        read(&path)
    }

    // NOTE: a reader identifies a reference set by the columns its descriptor
    // declares rather than by position, because the layouts differ by type
    // (<https://docs.snomed.org/snomed-ct-specifications/release-file-specification>).
    #[test]
    fn the_additional_columns_name_the_kind() {
        // The language reference set adds `acceptabilityId`.
        assert_eq!(
            RefsetKind::of_columns(&columns(&["acceptabilityId"])),
            RefsetKind::Language
        );
        // The OWL expression reference set adds `owlExpression`.
        assert_eq!(
            RefsetKind::of_columns(&columns(&["owlExpression"])),
            RefsetKind::OwlExpression
        );
        // The module dependency reference set adds both effective times, and
        // is the one kind no single column names.
        assert_eq!(
            RefsetKind::of_columns(&columns(&["sourceEffectiveTime", "targetEffectiveTime"])),
            RefsetKind::ModuleDependency
        );
        assert_eq!(
            RefsetKind::of_columns(&columns(&["sourceEffectiveTime"])),
            RefsetKind::Content,
            "one of the two effective times is not the module dependency layout"
        );
        // Every other layout the build reads as content: association,
        // attribute value, simple map, extended map, and the simple set with
        // no additional column at all.
        for additional in [
            vec!["targetComponentId"],
            vec!["valueId"],
            vec!["mapTarget"],
            vec![
                "mapGroup",
                "mapPriority",
                "mapRule",
                "mapAdvice",
                "mapTarget",
                "correlationId",
                "mapCategoryId",
            ],
            vec![],
        ] {
            assert_eq!(
                RefsetKind::of_columns(&columns(&additional)),
                RefsetKind::Content,
                "{additional:?} is a content layout"
            );
        }
    }

    #[test]
    fn a_kind_is_read_from_the_header_of_a_real_file() {
        assert_eq!(
            kind_of(&header(&["acceptabilityId"])).expect("a language refset header"),
            RefsetKind::Language
        );
        assert_eq!(
            kind_of(&header(&[])).expect("a simple refset header"),
            RefsetKind::Content
        );
    }

    #[test]
    fn a_header_that_does_not_start_with_the_member_columns_is_refused() {
        let wrong = "id\teffectiveTime\tactive\tmoduleId\trefsetId\tconceptId";
        let error = kind_of(wrong).expect_err("the sixth column is not a member column");
        let Rf2Error::Header {
            expected, actual, ..
        } = error
        else {
            panic!("a header error, not {error}");
        };
        assert_eq!(expected, MEMBER_COLUMNS.map(str::to_owned).to_vec());
        assert_eq!(
            actual.last().map(String::as_str),
            Some("conceptId"),
            "the error carries what the file actually had"
        );
    }

    #[test]
    fn a_header_shorter_than_the_member_columns_is_refused() {
        let error =
            kind_of("id\teffectiveTime\tactive").expect_err("three columns is not a member");
        assert!(matches!(error, Rf2Error::Header { .. }), "{error}");
    }
}
