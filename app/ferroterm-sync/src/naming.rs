// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a run calls the directories and files it writes.
//!
//! A release directory and a resource file are named after the content item
//! they hold, so an operator reading the index root sees which release each
//! directory is. The names carry only ASCII letters, digits, and hyphens, so
//! they are the same on every filesystem.
//!
//! No FHIR or SNOMED CT specification governs this: our own design.

use terminology_syndication::model::Entry;

/// The file-name form of `value`.
///
/// Every run of characters that is not a letter or a digit becomes one hyphen,
/// the result is lowercased, and it is cut to 80 characters, which keeps a
/// version URI inside the path limits every filesystem holds.
#[must_use]
pub fn slug(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut hyphen = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            out.extend(character.to_lowercase());
            hyphen = false;
        } else if !hyphen && !out.is_empty() {
            out.push('-');
            hyphen = true;
        }
    }
    let trimmed = out.trim_end_matches('-');
    trimmed.chars().take(80).collect()
}

/// The directory name a release from `entry` is built and served under.
///
/// A SNOMED CT version URI names an edition and a release date, which makes
/// `snomed-<edition>-<date>` readable at a glance; anything else is the
/// canonical identifier and the version identifier, each slugged.
#[must_use]
pub fn artifact_name(entry: &Entry) -> String {
    if let Some(release) = entry.snomed_release() {
        return format!("snomed-{}-{}", release.edition, release.version);
    }
    let canonical = entry.content_item_identifier.as_deref().unwrap_or_default();
    let version = entry.content_item_version.as_deref().unwrap_or_default();
    format!("{}-{}", slug(canonical), slug(version))
}

/// The file name a resource with `canonical` at `version` is written under.
///
/// The name carries the version, so two versions of one canonical sit beside
/// each other and a corrected republish of one version replaces exactly that
/// file.
#[must_use]
pub fn resource_file_name(canonical: &str, version: &str) -> String {
    format!("{}-{}.json", slug(canonical), slug(version))
}

/// The file name a downloaded content item is written under.
#[must_use]
pub fn download_name(entry: &Entry) -> String {
    let stem = entry
        .content_item_identifier
        .as_deref()
        .map_or_else(|| slug(&entry.id), slug);
    let version = entry.content_item_version.as_deref().unwrap_or_default();
    format!("{}-{}", stem, slug(version))
}

#[cfg(test)]
mod tests {
    use super::{artifact_name, resource_file_name, slug};
    use terminology_syndication::model::Entry;

    #[test]
    fn a_uri_slugs_to_letters_digits_and_hyphens() {
        assert_eq!(
            slug("http://snomed.info/sct/11000146104/version/20260930"),
            "http-snomed-info-sct-11000146104-version-20260930",
            "a URI becomes one readable name"
        );
        assert_eq!(slug("2.83"), "2-83", "a dotted version keeps its parts");
        assert_eq!(slug("///"), "", "a value with nothing to keep slugs empty");
    }

    #[test]
    fn a_snomed_release_names_its_edition_and_date() {
        let entry = Entry {
            content_item_identifier: Some(String::from("http://snomed.info/sct/11000146104")),
            content_item_version: Some(String::from(
                "http://snomed.info/sct/11000146104/version/20260930",
            )),
            ..Entry::default()
        };
        assert_eq!(
            artifact_name(&entry),
            "snomed-11000146104-20260930",
            "an operator reads the edition and the release date from the directory"
        );
    }

    #[test]
    fn another_system_is_named_after_its_canonical_and_version() {
        let entry = Entry {
            content_item_identifier: Some(String::from("http://loinc.org")),
            content_item_version: Some(String::from("2.83")),
            ..Entry::default()
        };
        assert_eq!(
            artifact_name(&entry),
            "http-loinc-org-2-83",
            "a system with no version URI is named from what the feed gives"
        );
    }

    #[test]
    fn a_resource_file_carries_its_version() {
        assert_eq!(
            resource_file_name("https://example.invalid/ValueSet/a", "1.2"),
            "https-example-invalid-valueset-a-1-2.json",
            "two versions of one canonical sit beside each other"
        );
    }
}
