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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use secrecy::{ExposeSecret, SecretString};

    use super::resolve;
    use crate::config::error::Error;

    /// A secret file holding `text` in a fresh directory.
    fn secret_file(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("client_secret");
        std::fs::write(&path, text).expect("the file is written");
        (dir, path)
    }

    #[test]
    fn an_inline_secret_is_taken_as_it_stands() {
        let inline = SecretString::from("inline-secret");
        let resolved = resolve("oidc.client_secret", Some(&inline), None).expect("resolved");
        assert_eq!(
            Some("inline-secret"),
            resolved.as_ref().map(ExposeSecret::expose_secret)
        );
    }

    #[test]
    fn a_secret_file_is_read_and_trimmed() {
        let (_dir, path) = secret_file("  file-secret\n");
        let resolved = resolve("oidc.client_secret", None, Some(&path)).expect("resolved");
        assert_eq!(
            Some("file-secret"),
            resolved.as_ref().map(ExposeSecret::expose_secret)
        );
    }

    #[test]
    fn no_secret_at_all_is_none() {
        assert!(
            resolve("oidc.client_secret", None, None)
                .expect("resolved")
                .is_none()
        );
    }

    #[test]
    fn a_secret_set_both_inline_and_in_a_file_is_refused_by_its_key() {
        let (_dir, path) = secret_file("file-secret");
        let inline = SecretString::from("inline-secret");
        let error = resolve("oidc.client_secret", Some(&inline), Some(&path))
            .expect_err("both are refused");
        assert!(
            matches!(&error, Error::Conflict { key } if key == "oidc.client_secret"),
            "{error:?}"
        );
    }

    #[test]
    fn a_secret_file_that_cannot_be_read_is_refused_by_its_file_key() {
        let missing = Path::new("/nonexistent/ferroterm-console/client_secret");
        let error = resolve("oidc.client_secret", None, Some(missing)).expect_err("refused");
        assert!(
            matches!(&error, Error::Secret { key, path, .. }
                if key == "oidc.client_secret_file" && path == missing),
            "{error:?}"
        );
    }

    #[test]
    fn a_secret_file_of_whitespace_alone_is_refused() {
        let (_dir, path) = secret_file(" \n\t\n");
        let error = resolve("oidc.client_secret", None, Some(&path)).expect_err("refused");
        assert!(
            matches!(&error, Error::EmptySecret { key, .. } if key == "oidc.client_secret_file"),
            "{error:?}"
        );
    }

    #[test]
    fn a_refusal_never_shows_the_secret() {
        let (_dir, path) = secret_file("file-secret");
        let inline = SecretString::from("inline-secret");
        let error = resolve("oidc.client_secret", Some(&inline), Some(&path))
            .expect_err("both are refused");
        let shown = format!("{error} {error:?}");
        assert!(!shown.contains("inline-secret"), "{shown}");
        assert!(!shown.contains("file-secret"), "{shown}");
    }
}
