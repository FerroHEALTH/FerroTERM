// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The account the add-on authenticates with, handed in by the service.
//!
//! The service issues a personal account, so the secret is the deployment's to
//! hold. The synchronisation service reads it from its own configuration,
//! inline or from a mounted `<key>_file`, and hands it over; no rendering of
//! these types prints it.

use core::fmt;

use ferrohealth_secret::secret::Secret;

/// The client identifier the service documents for a command-line client.
///
/// Nictiz documents the password grant with this client for a personal
/// account
/// (<https://www.nictiz.nl/publicaties/nationale-terminologie-server-handleiding-voor-nieuwe-gebruikers/>).
pub const DEFAULT_CLIENT_ID: &str = "cli_client";

/// Credentials the add-on cannot authenticate with.
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    /// Neither grant the service admits is fully configured.
    #[error("no usable grant: configure a client secret, or an account name and password")]
    NoUsableGrant,
}

/// The account the add-on authenticates with.
///
/// The `Debug` rendering names which grants are configured and never a secret,
/// so the whole source can be logged.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    /// The OAuth 2 client identifier.
    pub client_id: String,
    /// The client secret, when the deployment has one.
    pub client_secret: Option<Secret>,
    /// The account name for the password grant.
    pub username: Option<String>,
    /// The account password for the password grant.
    pub password: Option<Secret>,
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
    /// Returns the credentials when at least one grant is fully configured.
    ///
    /// # Errors
    ///
    /// Returns [`CredentialError::NoUsableGrant`] when neither grant is fully
    /// configured.
    pub fn usable(self) -> Result<Self, CredentialError> {
        if self.has_client_credentials() || self.has_password_grant() {
            Ok(self)
        } else {
            Err(CredentialError::NoUsableGrant)
        }
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
    use ferrohealth_secret::secret::Secret;

    use super::{CredentialError, Credentials};

    fn filled() -> Credentials {
        Credentials {
            client_id: String::from("cli_client"),
            client_secret: Some(Secret::new("a-client-secret")),
            username: Some(String::from("an-account")),
            password: Some(Secret::new("a-password")),
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
            client_secret: Some(Secret::new("")),
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
    fn credentials_without_a_full_grant_are_refused() {
        let credentials = Credentials {
            username: Some(String::from("an-account")),
            password: Some(Secret::new("a-password")),
            ..Credentials::default()
        };
        assert_eq!(
            credentials.client_id, "cli_client",
            "the documented client identifier is the default"
        );
        assert!(credentials.has_password_grant());
        assert!(!credentials.has_client_credentials());
        assert!(credentials.usable().is_ok());
        assert!(matches!(
            Credentials {
                username: Some(String::from("an-account")),
                ..Credentials::default()
            }
            .usable(),
            Err(CredentialError::NoUsableGrant)
        ));
    }
}
