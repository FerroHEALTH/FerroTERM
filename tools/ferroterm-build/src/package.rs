// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An operator-supplied FHIR NPM package (`--fhir-package`), imported.
//!
//! The MyHealth@EU Master Value Sets Catalogue is one: its terminology
//! resources are written to a directory the server serves under
//! `terminology.code_systems`.
//!
//! A package is a gzipped tarball whose resources sit in `package/`, with the
//! manifest `package/package.json` naming the package and its version (FHIR
//! Package specification, <https://hl7.org/fhir/packages.html>). The import
//! keeps the manifest and the `CodeSystem`, `ValueSet`, `ConceptMap` and
//! `Provenance` resources at the top of `package/` byte for byte, so each resource keeps its
//! canonical and version, and two releases land side by side.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

/// The resource types the server serves from a package, the provenance of
/// its maps among them.
const SERVED: [&str; 4] = ["CodeSystem", "ValueSet", "ConceptMap", "Provenance"];

/// What an import wrote.
#[derive(Debug)]
pub struct Report {
    /// The package name the manifest states.
    pub name: String,
    /// The package version the manifest states.
    pub version: String,
    /// The sha256 of the package file, in lower-case hex.
    pub sha256: String,
    /// The resources written, by type: `CodeSystem`, `ValueSet`, `ConceptMap`,
    /// then `Provenance`.
    pub counts: [(&'static str, usize); 4],
    /// The directory written, which `terminology.code_systems` names.
    pub dir: PathBuf,
}

/// Why a package was refused.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The package file could not be read.
    #[error("cannot read the package {path}")]
    Read {
        /// The package file.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// The pinned sha256 is not the file's.
    #[error("the package {path} has sha256 {actual}, not the pinned {expected}")]
    Digest {
        /// The package file.
        path: PathBuf,
        /// The sha256 the operator pinned.
        expected: String,
        /// The sha256 of the file.
        actual: String,
    },
    /// The tarball does not unpack.
    #[error("the package is not a gzipped tarball")]
    Archive(#[source] std::io::Error),
    /// The tarball has no `package/package.json`.
    #[error("the package has no package/package.json")]
    NoManifest,
    /// The manifest is not JSON naming a `name` and a `version`.
    #[error("package/package.json names no name and version: {reason}")]
    Manifest {
        /// What is wrong with it.
        reason: String,
    },
    /// The name or version is not usable as a directory name.
    #[error("the package {name}#{version} cannot name a directory")]
    Name {
        /// The package name.
        name: String,
        /// The package version.
        version: String,
    },
    /// The output already holds this package release.
    #[error("{0} already exists; remove it to import the release again")]
    Exists(PathBuf),
    /// A file could not be written.
    #[error("cannot write {path}")]
    Write {
        /// The file.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
}

/// Imports the package at `path` into `<out>/<name>-<version>/`, refusing it
/// when `pinned` is given and is not the file's sha256.
///
/// # Errors
///
/// Returns [`Error`] when the file does not read, its digest differs from the
/// pin, it does not unpack, its manifest is missing or names no release, the
/// release is already imported, or a file cannot be written.
pub fn import(path: &Path, pinned: Option<&str>, out: &Path) -> Result<Report, Error> {
    let bytes = fs::read(path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })?;
    let sha256 = hex(&Sha256::digest(&bytes));
    if let Some(expected) = pinned
        && !expected.eq_ignore_ascii_case(&sha256)
    {
        return Err(Error::Digest {
            path: path.to_owned(),
            expected: expected.to_owned(),
            actual: sha256,
        });
    }
    let files = top_level_files(&bytes)?;
    let manifest = files
        .iter()
        .find(|(name, _)| name == "package.json")
        .map(|(_, body)| body)
        .ok_or(Error::NoManifest)?;
    let (name, version) = release(manifest)?;
    let dir = out.join(format!("{name}-{version}"));
    if dir.exists() {
        return Err(Error::Exists(dir));
    }
    let written = |path: &Path, source| Error::Write {
        path: path.to_owned(),
        source,
    };
    fs::create_dir_all(&dir).map_err(|source| written(&dir, source))?;
    let mut counts = SERVED.map(|kind| (kind, 0_usize));
    for (file, body) in &files {
        let keep = if file == "package.json" {
            true
        } else {
            match resource_type(body) {
                Some(kind) => match counts.iter_mut().find(|(served, _)| *served == kind) {
                    Some((_, count)) => {
                        *count = count.saturating_add(1);
                        true
                    }
                    None => false,
                },
                None => false,
            }
        };
        if keep {
            let target = dir.join(file);
            fs::write(&target, body).map_err(|source| written(&target, source))?;
        }
    }
    Ok(Report {
        name,
        version,
        sha256,
        counts,
        dir,
    })
}

/// The `.json` files directly under `package/`, by file name.
fn top_level_files(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, Error> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut files = Vec::new();
    for entry in archive.entries().map_err(Error::Archive)? {
        let mut entry = entry.map_err(Error::Archive)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(Error::Archive)?.into_owned();
        let Some(name) = top_level(&path) else {
            continue;
        };
        let mut body = Vec::new();
        entry.read_to_end(&mut body).map_err(Error::Archive)?;
        files.push((name, body));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// The file name of `path` when it is `package/<name>.json`, one level deep
/// and with no other component; the example and other folders of a package
/// hold no resource the package defines (FHIR Package specification).
fn top_level(path: &Path) -> Option<String> {
    let mut components = path.components();
    let (Some(Component::Normal(folder)), Some(Component::Normal(file)), None) =
        (components.next(), components.next(), components.next())
    else {
        return None;
    };
    let file = file.to_str()?;
    (folder == "package"
        && Path::new(file)
            .extension()
            .is_some_and(|extension| extension == "json"))
    .then(|| file.to_owned())
}

/// The `name` and `version` the manifest states, each usable as a path segment.
fn release(manifest: &[u8]) -> Result<(String, String), Error> {
    let value: serde_json::Value =
        serde_json::from_slice(manifest).map_err(|error| Error::Manifest {
            reason: error.to_string(),
        })?;
    let field = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| Error::Manifest {
                reason: format!("`{key}` is not a string"),
            })
    };
    let (name, version) = (field("name")?, field("version")?);
    let plain = |text: &str| {
        !text.is_empty()
            && !text.starts_with('.')
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
    };
    if plain(&name) && plain(&version) {
        Ok((name, version))
    } else {
        Err(Error::Name { name, version })
    }
}

/// The `resourceType` of a JSON resource, when it is one.
fn resource_type(body: &[u8]) -> Option<String> {
    // NOTE: a package may carry JSON that is no resource (an index, a schema);
    // a file that is no JSON object with a `resourceType` is not imported.
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    value
        .get("resourceType")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// `bytes` as lower-case hex.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let digit = |nibble: u8| {
        DIGITS
            .get(usize::from(nibble))
            .map_or('0', |digit| char::from(*digit))
    };
    bytes
        .iter()
        .flat_map(|byte| [digit(byte >> 4), digit(byte & 0x0f)])
        .collect()
}
