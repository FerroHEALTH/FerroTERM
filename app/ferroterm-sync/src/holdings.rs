// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the deployment already serves, as the replace rule compares it.
//!
//! [`terminology_syndication::select`] takes an entry only when the same
//! canonical identifier and version identifier is not held already at the same
//! feed date or a later one. Two places say what is held: the ledger the
//! service keeps of what it put there itself, which carries the feed date, and
//! the deployment's own directories, which carry the identity of everything
//! else an operator built or wrote by hand.
//!
//! An artifact or a resource the service did not write is held at the latest
//! representable instant, so a run never replaces it. Remove it to let a run
//! take that version again. No FHIR or SNOMED CT specification governs this:
//! our own design.

use std::path::Path;

use terminology_syndication::select::Holdings;

use crate::state::Held;

/// The file name of an artifact's manifest.
///
/// A directory holding one is a built artifact, and a directory holding none
/// is either the index root itself or a staging directory a build has not
/// finished writing.
pub const MANIFEST_FILE: &str = "manifest.json";

/// What the deployment holds: the ledger first, the directories after it.
///
/// A directory or a file is read for its identity alone. An identity the
/// ledger already carries keeps the ledger's feed date, which is the date the
/// replace rule compares.
#[must_use]
pub fn holdings(ledger: &[Held], index_root: &Path, resources: &Path) -> Holdings {
    let mut holdings = Holdings::new();
    for held in ledger {
        holdings.record(&held.canonical, &held.version, held.date);
    }
    for (canonical, version) in built_artifacts(index_root) {
        record_unknown(&mut holdings, ledger, &canonical, &version);
    }
    for (canonical, version) in managed_resources(resources) {
        record_unknown(&mut holdings, ledger, &canonical, &version);
    }
    holdings
}

/// Records an identity the ledger does not carry, so a run never replaces it.
fn record_unknown(holdings: &mut Holdings, ledger: &[Held], canonical: &str, version: &str) {
    if ledger
        .iter()
        .any(|held| held.canonical == canonical && held.version == version)
    {
        return;
    }
    holdings.record(canonical, version, jiff::Timestamp::MAX);
}

/// The canonical and version identifier of every artifact under `index_root`.
///
/// An artifact is held under its manifest's system and, when the manifest
/// carries one, under its edition too: a feed names an edition release either
/// by the bare code system with the edition in the version URI (Ontoserver)
/// or by the edition URI itself, and the replace rule must recognise both.
fn built_artifacts(index_root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(index_root) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::debug!(root = %index_root.display(), %error, "the index root does not list");
            return out;
        }
    };
    for entry in entries.flatten() {
        let manifest = entry.path().join(MANIFEST_FILE);
        let Some(value) = read_json(&manifest) else {
            continue;
        };
        let Some(version) = field(&value, "version") else {
            continue;
        };
        for canonical in [field(&value, "system"), field(&value, "edition")]
            .into_iter()
            .flatten()
        {
            out.push((canonical, version.clone()));
        }
    }
    out.sort();
    out
}

/// The canonical and version of every FHIR resource under `resources`.
fn managed_resources(resources: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(resources) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::debug!(dir = %resources.display(), %error, "the resource directory does not list");
            return out;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let Some(value) = read_json(&path) else {
            continue;
        };
        if let (Some(canonical), Some(version)) = (field(&value, "url"), field(&value, "version")) {
            out.push((canonical, version));
        }
    }
    out.sort();
    out
}

/// Reads one JSON document, reporting a file that does not read as absent.
///
/// A file that is not readable JSON is held by nothing: the server reports it
/// when it meets the same file, and a run that stopped here would report a
/// failure it cannot act on.
fn read_json(path: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "the file is not readable JSON");
            None
        }
    }
}

/// One string field of a JSON object.
fn field(value: &serde_json::Value, name: &str) -> Option<String> {
    value
        .get(name)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
mod tests {
    use super::holdings;
    use crate::state::Held;

    #[test]
    fn a_built_artifact_is_held_and_never_replaced() -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let index = dir.path().join("index");
        let release = index.join("snomed-11000146104-20260930");
        std::fs::create_dir_all(&release)?;
        std::fs::write(
            release.join(super::MANIFEST_FILE),
            r#"{"manifest":1,"system":"http://snomed.info/sct","edition":"http://snomed.info/sct/11000146104","version":"http://snomed.info/sct/11000146104/version/20260930"}"#,
        )?;
        let held = holdings(&[], &index, &dir.path().join("codesystems"));
        assert_eq!(
            held.date_of(
                "http://snomed.info/sct/11000146104",
                "http://snomed.info/sct/11000146104/version/20260930"
            ),
            Some(jiff::Timestamp::MAX),
            "an artifact the service did not write is never replaced by it"
        );
        assert_eq!(
            held.date_of(
                "http://snomed.info/sct",
                "http://snomed.info/sct/11000146104/version/20260930"
            ),
            Some(jiff::Timestamp::MAX),
            "the same artifact is held under the bare code system, which is how Ontoserver names it"
        );
        Ok(())
    }

    #[test]
    fn a_managed_resource_is_held_by_its_canonical_and_version()
    -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let resources = dir.path().join("codesystems");
        std::fs::create_dir_all(&resources)?;
        std::fs::write(
            resources.join("example.json"),
            r#"{"resourceType":"ValueSet","url":"https://example.invalid/ValueSet/a","version":"1"}"#,
        )?;
        let held = holdings(&[], &dir.path().join("index"), &resources);
        assert_eq!(
            held.date_of("https://example.invalid/ValueSet/a", "1"),
            Some(jiff::Timestamp::MAX),
            "a resource on disk is held by its canonical and version"
        );
        Ok(())
    }

    #[test]
    fn the_ledger_carries_the_feed_date() -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let date: jiff::Timestamp = "2026-09-30T00:00:00Z".parse()?;
        let ledger = vec![Held {
            canonical: String::from("http://loinc.org"),
            version: String::from("2.83"),
            date,
        }];
        let held = holdings(&ledger, &dir.path().join("index"), &dir.path().join("res"));
        assert_eq!(
            held.date_of("http://loinc.org", "2.83"),
            Some(date),
            "what the service took carries the date the feed gave it"
        );
        Ok(())
    }

    #[test]
    fn the_ledger_wins_over_the_directory() -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let resources = dir.path().join("codesystems");
        std::fs::create_dir_all(&resources)?;
        std::fs::write(
            resources.join("a.json"),
            r#"{"resourceType":"ValueSet","url":"https://example.invalid/ValueSet/a","version":"1"}"#,
        )?;
        let date: jiff::Timestamp = "2026-01-01T00:00:00Z".parse()?;
        let ledger = vec![Held {
            canonical: String::from("https://example.invalid/ValueSet/a"),
            version: String::from("1"),
            date,
        }];
        let held = holdings(&ledger, &dir.path().join("index"), &resources);
        assert_eq!(
            held.date_of("https://example.invalid/ValueSet/a", "1"),
            Some(date),
            "a file the service wrote keeps the feed date it was taken at"
        );
        Ok(())
    }
}
