// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic package shaped like the MyHealth@EU Master Value Sets
//! Catalogue: a gzipped FHIR NPM tarball whose value sets enumerate invented
//! codes, never the real catalogue's licensed content.

use std::io::Write;
use std::path::Path;

/// The package name, the catalogue's own.
pub const NAME: &str = "myhealth.eu.fhir.mvc-package";
/// The code system the value sets draw on, served from the same package.
pub const SYSTEM: &str = "http://example.org/fhir/CodeSystem/mvc-synthetic";
/// A value set over [`SYSTEM`], one per release with a code the other lacks.
pub const VS_ALLERGY: &str = "http://example.org/fhir/ValueSet/eHDSIAllergyNoAllergy";
/// A value set over a code system no package or index serves.
pub const VS_UNSERVED: &str = "http://example.org/fhir/ValueSet/eHDSIUnserved";
/// The code system [`VS_UNSERVED`] names, with its version.
pub const UNSERVED_SYSTEM: &str = "http://example.org/fhir/CodeSystem/unserved";
/// The version [`VS_UNSERVED`] pins.
pub const UNSERVED_VERSION: &str = "2026-01";
/// A national code system the package maps from.
pub const NATIONAL: &str = "http://example.org/fhir/CodeSystem/national-allergy";
/// The map from [`NATIONAL`] to [`SYSTEM`], one version per release.
pub const MAP: &str = "http://example.org/fhir/ConceptMap/national-to-mvc";
/// Who validated the map of each release, as its `Provenance` names them.
pub const VALIDATOR: &str = "Synthetic Terminology Board";
/// Every value set a release of the package defines.
pub const VALUE_SETS: [&str; 2] = [VS_ALLERGY, VS_UNSERVED];

/// The files of the package for `version`, by path inside the tarball.
fn files(version: &str) -> Vec<(String, serde_json::Value)> {
    let codes: &[(&str, &str)] = if version.starts_with("10.0") {
        &[("a1", "Allergy one"), ("a2", "Allergy two")]
    } else {
        &[
            ("a1", "Allergy one"),
            ("a2", "Allergy two"),
            ("a3", "Allergy three"),
        ]
    };
    let concepts: Vec<serde_json::Value> = codes
        .iter()
        .map(|(code, display)| serde_json::json!({"code": code, "display": display}))
        .collect();
    vec![
        (
            String::from("package/package.json"),
            serde_json::json!({"name": NAME, "version": version, "fhirVersions": ["4.0.1"]}),
        ),
        (
            String::from("package/CodeSystem-mvc-synthetic.json"),
            serde_json::json!({"resourceType": "CodeSystem", "url": SYSTEM, "version": version,
                "status": "active", "content": "complete", "concept": concepts}),
        ),
        (
            String::from("package/ValueSet-eHDSIAllergyNoAllergy.json"),
            serde_json::json!({"resourceType": "ValueSet", "url": VS_ALLERGY, "version": version,
                "status": "active", "compose": {"include": [{"system": SYSTEM,
                    "version": version, "concept": codes.iter().map(|(code, _)| serde_json::json!({"code": code})).collect::<Vec<_>>()}]}}),
        ),
        (
            String::from("package/ValueSet-eHDSIUnserved.json"),
            serde_json::json!({"resourceType": "ValueSet", "url": VS_UNSERVED, "version": version,
                "status": "active", "compose": {"include": [{"system": UNSERVED_SYSTEM,
                    "version": UNSERVED_VERSION, "concept": [{"code": "u1"}]}]}}),
        ),
        (
            String::from("package/CodeSystem-national-allergy.json"),
            serde_json::json!({"resourceType": "CodeSystem", "url": NATIONAL, "version": version,
                "status": "active", "content": "complete",
                "concept": [{"code": "n1", "display": "National allergy one"}]}),
        ),
        (
            String::from("package/ConceptMap-national-to-mvc.json"),
            serde_json::json!({"resourceType": "ConceptMap",
                "id": format!("national-to-mvc-{}", version.replace('.', "-")),
                "url": MAP, "version": version, "status": "active",
                "group": [{"source": NATIONAL, "target": SYSTEM, "element": [{"code": "n1",
                    "target": [{"code": "a1", "equivalence": "equivalent"}]}]}]}),
        ),
        (
            String::from("package/Provenance-national-to-mvc.json"),
            serde_json::json!({"resourceType": "Provenance",
                "id": format!("national-to-mvc-{}", version.replace('.', "-")),
                "target": [{"reference": format!("{MAP}|{version}")}],
                "recorded": "2026-11-01T10:00:00Z",
                "activity": {"coding": [{"system": "http://terminology.hl7.org/CodeSystem/v3-DataOperation",
                    "code": "UPDATE"}]},
                "agent": [{"type": {"coding": [{"system": "http://terminology.hl7.org/CodeSystem/provenance-participant-type",
                    "code": "verifier"}]}, "who": {"display": VALIDATOR}}]}),
        ),
        (
            String::from("package/.index.json"),
            serde_json::json!({"index-version": 2, "files": []}),
        ),
        (
            String::from("package/example/ValueSet-example.json"),
            serde_json::json!({"resourceType": "ValueSet", "url": "http://example.org/fhir/ValueSet/example",
                "status": "active"}),
        ),
    ]
}

/// Writes the synthetic package for `version` to `path` as a `.tgz`.
///
/// # Errors
///
/// Returns the I/O error when the file cannot be written.
pub fn write_package(path: &Path, version: &str) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    let mut archive = tar::Builder::new(encoder);
    for (name, value) in files(version) {
        let body = serde_json::to_vec_pretty(&value).map_err(std::io::Error::other)?;
        let mut header = tar::Header::new_gnu();
        header.set_size(u64::try_from(body.len()).map_err(std::io::Error::other)?);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append_data(&mut header, name, body.as_slice())?;
    }
    archive.into_inner()?.finish()?.flush()
}

/// Writes the package for `version` under `dir` and imports it with
/// `ferroterm-build`'s package import, returning the directory to serve.
///
/// # Errors
///
/// Returns the error of the write or of the import.
pub fn imported(
    dir: &Path,
    version: &str,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let tgz = dir.join(format!("{version}.tgz"));
    write_package(&tgz, version)?;
    Ok(ferroterm_build::package::import(&tgz, None, &dir.join("packages"))?.dir)
}
