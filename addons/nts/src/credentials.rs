// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The account the add-on authenticates with, read from outside the config.
//!
//! The service issues a personal account, so the secret is the deployment's to
//! hold. It arrives from a mounted file or from the process environment, never
//! from a configuration body, and no rendering of these types prints it.

use core::fmt;
use std::path::{Path, PathBuf};

use crate::config::CredentialSource;

/// The client identifier the service documents for a command-line client.
///
/// Nictiz documents the password grant with this client for a personal
/// account
/// (<https://www.nictiz.nl/publicaties/nationale-terminologie-server-handleiding-voor-nieuwe-gebruikers/>).
pub const DEFAULT_CLIENT_ID: &str = "cli_client";

/// The environment variable holding the client identifier.
pub const CLIENT_ID_VAR: &str = "FERROTERM_NTS_CLIENT_ID";
/// The environment variable holding the client secret.
pub const CLIENT_SECRET_VAR: &str = "FERROTERM_NTS_CLIENT_SECRET";
/// The environment variable holding the account name.
pub const USERNAME_VAR: &str = "FERROTERM_NTS_USERNAME";
/// The environment variable holding the account password.
pub const PASSWORD_VAR: &str = "FERROTERM_NTS_PASSWORD";

/// Credentials that could not be read.
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    /// The credential file could not be read.
    #[error("the credential file {path} could not be read")]
    Read {
        /// The file that was read.
        path: PathBuf,
        /// Why the read failed.
        #[source]
        source: std::io::Error,
    },
    /// The credential file is not the JSON object the add-on expects.
    #[error("the credential file {path} is not a credential document")]
    Parse {
        /// The file that was read.
        path: PathBuf,
        /// Why the document could not be read.
        #[source]
        source: serde_json::Error,
    },
    /// An environment variable holds something that is not valid Unicode.
    #[error("{variable} does not hold text")]
    Environment {
        /// The variable that was read.
        variable: &'static str,
        /// Why the variable could not be read.
        #[source]
        source: std::env::VarError,
    },
    /// Neither grant the service admits is fully configured.
    #[error("no usable grant: configure a client secret, or an account name and password")]
    NoUsableGrant,
}

/// The account the add-on authenticates with.
///
/// The `Debug` rendering names which grants are configured and never a secret,
/// so the whole source can be logged.
#[derive(Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Credentials {
    /// The OAuth 2 client identifier.
    pub client_id: String,
    /// The client secret, when the deployment has one.
    pub client_secret: Option<String>,
    /// The account name for the password grant.
    pub username: Option<String>,
    /// The account password for the password grant.
    pub password: Option<String>,
}

impl Default for Credentials {
    fn default() -> Self {
        Self {
            client_id: String::from(DEFAULT_CLIENT_ID),
            client_secret: None,
            username: None,
            password: None,
        }
    }
}

impl Credentials {
    /// Reads the credentials the source names.
    ///
    /// # Errors
    ///
    /// Returns [`CredentialError::Read`] or [`CredentialError::Parse`] when a
    /// credential file cannot be read, [`CredentialError::Environment`] when a
    /// variable holds no text, and [`CredentialError::NoUsableGrant`] when
    /// neither grant is fully configured.
    pub fn load(from: &CredentialSource) -> Result<Self, CredentialError> {
        let credentials = match from {
            CredentialSource::Environment => Self::from_environment()?,
            CredentialSource::File { path } => Self::from_file(path)?,
        };
        if !credentials.has_client_credentials() && !credentials.has_password_grant() {
            return Err(CredentialError::NoUsableGrant);
        }
        Ok(credentials)
    }

    /// Reads the credentials from the `FERROTERM_NTS_*` variables.
    ///
    /// An unset variable leaves its field at the default; a variable holding
    /// something that is not text is an error rather than an absent field.
    ///
    /// # Errors
    ///
    /// Returns [`CredentialError::Environment`] when a variable holds no text.
    pub fn from_environment() -> Result<Self, CredentialError> {
        let read = |variable: &'static str| -> Result<Option<String>, CredentialError> {
            match std::env::var(variable) {
                Ok(value) => Ok(Some(value)),
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(source) => Err(CredentialError::Environment { variable, source }),
            }
        };
        Ok(Self {
            client_id: read(CLIENT_ID_VAR)?.unwrap_or_else(|| String::from(DEFAULT_CLIENT_ID)),
            client_secret: read(CLIENT_SECRET_VAR)?,
            username: read(USERNAME_VAR)?,
            password: read(PASSWORD_VAR)?,
        })
    }

    /// Reads the credentials from a JSON file.
    ///
    /// # Errors
    ///
    /// Returns [`CredentialError::Read`] when the file cannot be read and
    /// [`CredentialError::Parse`] when it is not a credential document.
    pub fn from_file(path: &Path) -> Result<Self, CredentialError> {
        let body = std::fs::read(path).map_err(|source| CredentialError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        serde_json::from_slice(&body).map_err(|source| CredentialError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Whether the client-credentials grant can be attempted.
    #[must_use]
    pub fn has_client_credentials(&self) -> bool {
        self.client_secret
            .as_ref()
            .is_some_and(|secret| !secret.is_empty())
    }

    /// Whether the password grant can be attempted.
    #[must_use]
    pub fn has_password_grant(&self) -> bool {
        self.username.as_ref().is_some_and(|name| !name.is_empty()) && self.password.is_some()
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Credentials(client_id={}, client_credentials={}, password={})",
            self.client_id,
            self.has_client_credentials(),
            self.has_password_grant()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Credentials;

    fn filled() -> Credentials {
        Credentials {
            client_id: String::from("cli_client"),
            client_secret: Some(String::from("a-client-secret")),
            username: Some(String::from("an-account")),
            password: Some(String::from("a-password")),
        }
    }

    #[test]
    fn debug_names_the_grants_and_no_secret() {
        let rendered = format!("{:?}", filled());
        assert!(
            !rendered.contains("a-client-secret"),
            "the client secret must never be rendered"
        );
        assert!(
            !rendered.contains("a-password"),
            "the password must never be rendered"
        );
        assert!(
            !rendered.contains("an-account"),
            "the account name must never be rendered"
        );
        assert!(
            rendered.contains("client_credentials=true"),
            "the rendering states which grants are configured: {rendered}"
        );
    }

    #[test]
    fn an_empty_secret_is_no_client_credentials_grant() {
        let credentials = Credentials {
            client_secret: Some(String::new()),
            ..filled()
        };
        assert!(
            !credentials.has_client_credentials(),
            "an empty secret configures nothing"
        );
        assert!(
            credentials.has_password_grant(),
            "the password grant is still configured"
        );
    }

    #[test]
    fn a_credential_document_reads_from_json() {
        let credentials: Credentials =
            serde_json::from_str(r#"{"username":"an-account","password":"a-password"}"#)
                .expect("the document reads");
        assert_eq!(
            credentials.client_id, "cli_client",
            "the documented client identifier is the default"
        );
        assert!(credentials.has_password_grant());
        assert!(!credentials.has_client_credentials());
    }
}
