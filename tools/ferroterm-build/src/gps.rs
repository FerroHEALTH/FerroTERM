// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SNOMED CT Global Patient Set as a value set over a loaded edition.
//!
//! SNOMED International publishes the GPS freeset as a tab-separated `.txt`
//! file (`SnomedINTL_GPSRelease_PRODUCTION_20260101T120000Z.txt`,
//! <https://docs.snomed.org/implementation-guides/gps-implementation-guide/global-patient-set-content>)
//! whose columns are the concept id, the active flag, the fully specified
//! name, and the US English preferred term. The build writes one R4B
//! `ValueSet` of the active concepts over `http://snomed.info/sct`, unversioned
//! so it expands over whichever edition the server loads. The GPS has no
//! canonical on the FHIR wire as a freeset; the canonical below is this
//! server's. The RF2 refset package of the GPS loads with `--rf2-refset`
//! instead, served as `?fhir_vs=refset/[id]`.

use std::path::{Path, PathBuf};

use fhir_types::codec::Json;
use fhir_types::r4b::value_set::{
    ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
};

/// The directory under `--out` the value set is written to.
pub const DIR: &str = "gps";
/// The canonical of the GPS value set (our own).
pub const VALUE_SET_URL: &str = "https://ferroterm.eu/fhir/ValueSet/snomed-gps";
/// SNOMED CT.
const SNOMED: &str = "http://snomed.info/sct";

/// A failure to read the freeset or write the value set.
#[derive(Debug, thiserror::Error)]
pub enum GpsError {
    /// A file does not read or cannot be written.
    #[error("cannot access {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// A data row does not start with a concept id and an active flag.
    #[error("{path}:{line}: not a GPS row (concept id, active, FSN, preferred term)")]
    Row {
        /// The path.
        path: PathBuf,
        /// The 1-based line.
        line: usize,
    },
    /// No version was given and the file name carries no release date.
    #[error("the file name carries no release date; pass `--gps-version`")]
    NoVersion,
    /// The value set cannot be encoded.
    #[error("cannot encode the value set")]
    Encode(#[from] fhir_types::codec::EncodeError),
}

/// What the build wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The value set file.
    pub path: PathBuf,
    /// The release recorded (`20260101`).
    pub version: String,
    /// The active concepts in the value set.
    pub active: usize,
    /// The inactive rows left out.
    pub inactive: usize,
}

/// The release date in a freeset file name: the eight digits before a `T`
/// (`..._20260101T120000Z.txt`).
fn release_in(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    name.split(|c: char| !c.is_ascii_alphanumeric())
        .filter_map(|part| part.split_once('T').map(|(date, _)| date))
        .find(|date| date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()))
        .map(str::to_owned)
}

/// Reads the freeset at `path` and writes the value set under `<out>/gps/`.
///
/// # Errors
///
/// Returns [`GpsError`] when the file does not read, a row is malformed, no
/// version is known, or the value set cannot be written.
pub fn build(path: &Path, version: Option<&str>, out: &Path) -> Result<Report, GpsError> {
    let version = version
        .map(str::to_owned)
        .or_else(|| release_in(path))
        .ok_or(GpsError::NoVersion)?;
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| GpsError::Io { path, source }
    };
    let text = std::fs::read_to_string(path).map_err(io(path))?;
    let mut concepts = Vec::new();
    let mut inactive = 0;
    for (index, line) in text.lines().enumerate() {
        let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
        let Some(id) = fields.first().filter(|f| !f.is_empty()) else {
            continue;
        };
        if !id.bytes().all(|b| b.is_ascii_digit()) {
            if index == 0 {
                continue;
            }
            return Err(GpsError::Row {
                path: path.to_path_buf(),
                line: index.saturating_add(1),
            });
        }
        match fields.get(1).copied() {
            Some("1") => concepts.push(ValueSetComposeIncludeConcept {
                code: (*id).into(),
                display: fields.get(3).filter(|t| !t.is_empty()).map(|t| (*t).into()),
                ..Default::default()
            }),
            Some("0") => inactive += 1,
            _ => {
                return Err(GpsError::Row {
                    path: path.to_path_buf(),
                    line: index.saturating_add(1),
                });
            }
        }
    }
    let active = concepts.len();
    let value_set = ValueSet {
        url: Some(VALUE_SET_URL.into()),
        version: Some(version.as_str().into()),
        name: Some("SnomedGlobalPatientSet".into()),
        title: Some("SNOMED CT Global Patient Set".into()),
        status: "active".into(),
        experimental: Some(false.into()),
        publisher: Some("SNOMED International".into()),
        description: Some(
            "The active concepts of the SNOMED CT Global Patient Set freeset, over the loaded edition."
                .into(),
        ),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(SNOMED.into()),
                concept: concepts,
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let dir = out.join(DIR);
    std::fs::create_dir_all(&dir).map_err(io(&dir))?;
    let file = dir.join("ValueSet-snomed-gps.json");
    let object = fhir_types::codec::Value::Object(value_set.to_json()?);
    let text = serde_json::to_string_pretty(&object).map_err(|e| GpsError::Io {
        path: file.clone(),
        source: std::io::Error::other(e),
    })?;
    std::fs::write(&file, format!("{text}\n")).map_err(io(&file))?;
    Ok(Report {
        path: file,
        version,
        active,
        inactive,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::release_in;

    #[test]
    fn the_release_is_read_from_the_file_name() {
        assert_eq!(
            release_in(Path::new(
                "SnomedINTL_GPSRelease_PRODUCTION_20260101T120000Z.txt"
            ))
            .as_deref(),
            Some("20260101")
        );
        assert_eq!(release_in(Path::new("gps.txt")), None);
    }
}
