// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The linguistic variants: one file per translation
//! (`nlNL22LinguisticVariant.csv`), the language and region in the file name.

use std::collections::BTreeMap;
use std::path::Path;

use crate::release::{Release, ReleaseError, Table, csv_at, field};

/// One translated term.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::struct_field_names,
    reason = "the fields are the LOINC linguistic variant columns, named as the release names them"
)]
pub struct Translation {
    /// `LONG_COMMON_NAME` in the variant, when given.
    pub long_common_name: Option<String>,
    /// `SHORTNAME` in the variant, when given.
    pub short_name: Option<String>,
    /// `LinguisticVariantDisplayName`, when given.
    pub display_name: Option<String>,
}

/// One variant file: the BCP 47 tag and its translations by code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    /// The BCP 47 tag (`nl-NL`).
    pub language: String,
    /// Translations by `LOINC_NUM`.
    pub terms: BTreeMap<String, Translation>,
}

/// The BCP 47 tag of a variant file name: `nlNL22LinguisticVariant.csv` is
/// `nl-NL` (two lower-case letters, two upper-case letters, then the variant
/// number). `None` when the name has another shape.
#[must_use]
pub fn language_of(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let bytes = name.as_bytes();
    if bytes.len() < 4
        || !bytes.get(0..2)?.iter().all(u8::is_ascii_lowercase)
        || !bytes.get(2..4)?.iter().all(u8::is_ascii_uppercase)
    {
        return None;
    }
    Some(format!("{}-{}", name.get(0..2)?, name.get(2..4)?))
}

/// Reads every variant file of the release.
///
/// # Errors
///
/// Returns [`ReleaseError`] when a file does not parse or lacks `LOINC_NUM`.
pub fn read(release: &Release) -> Result<Vec<Variant>, ReleaseError> {
    let mut out = Vec::new();
    for path in release.variants() {
        let Some(language) = language_of(path) else {
            continue;
        };
        let mut table = Table::open(path)?;
        let code_at = table.column("LOINC_NUM")?;
        let columns = table.columns();
        let at = |name: &str| columns.iter().position(|c| c.eq_ignore_ascii_case(name));
        let long_at = at("LONG_COMMON_NAME");
        let short_at = at("SHORTNAME");
        let display_at = at("LinguisticVariantDisplayName");
        let mut terms = BTreeMap::new();
        let path = table.path.clone();
        for record in table.reader.records() {
            let record = record.map_err(|e| csv_at(&path, e))?;
            let text = |at: Option<usize>| {
                at.map(|i| field(&record, i))
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned)
            };
            terms.insert(
                field(&record, code_at).to_owned(),
                Translation {
                    long_common_name: text(long_at),
                    short_name: text(short_at),
                    display_name: text(display_at),
                },
            );
        }
        out.push(Variant { language, terms });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::language_of;

    #[test]
    fn the_language_is_read_from_the_file_name() {
        assert_eq!(
            language_of(Path::new("LinguisticVariants/nlNL22LinguisticVariant.csv")).as_deref(),
            Some("nl-NL")
        );
        assert_eq!(
            language_of(Path::new("deAT24LinguisticVariant.csv")).as_deref(),
            Some("de-AT")
        );
        assert_eq!(language_of(Path::new("LinguisticVariant.csv")), None);
    }
}
