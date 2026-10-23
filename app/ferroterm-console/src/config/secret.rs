// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A secret of the configuration: set inline under its key, or read from the
//! file its `<key>_file` sibling names, never both.
//!
//! A secret file is the shape a container runtime mounts a secret in, so a
//! value never needs to sit in the configuration file or the environment.
//! The text read is trimmed, refused when empty, and held in a
//! [`SecretString`] that never renders its value. No specification governs
//! this: our own design.

use std::path::Path;

use secrecy::SecretString;

use crate::config::error::Error;

/// Returns the secret set inline or in the file its `_file` sibling names.
///
/// `key` is the dotted key of the inline value, so a refusal names it and
/// `<key>_file`, never the value.
///
/// # Errors
/// Returns [`Error::Conflict`] when both are set, [`Error::Secret`] when the
/// file cannot be read, and [`Error::EmptySecret`] when it holds nothing but
/// whitespace.
pub fn resolve(
    key: &str,
    inline: Option<&SecretString>,
    file: Option<&Path>,
) -> Result<Option<SecretString>, Error> {
    match (inline, file) {
        (Some(_), Some(_)) => Err(Error::Conflict {
            key: key.to_owned(),
        }),
        (Some(value), None) => Ok(Some(value.clone())),
        (None, Some(path)) => read(&format!("{key}_file"), path).map(Some),
        (None, None) => Ok(None),
    }
}

/// Reads the secret file `path`, trimmed and refused when empty.
fn read(key: &str, path: &Path) -> Result<SecretString, Error> {
    let text = std::fs::read_to_string(path)
        .map(SecretString::from)
        .map_err(|source| Error::Secret {
            key: key.to_owned(),
            path: path.to_path_buf(),
            source,
        })?;
    let value = secrecy::ExposeSecret::expose_secret(&text).trim();
    if value.is_empty() {
        return Err(Error::EmptySecret {
            key: key.to_owned(),
            path: path.to_path_buf(),
        });
    }
    Ok(SecretString::from(value))
}
