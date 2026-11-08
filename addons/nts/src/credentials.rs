// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The client the add-on authenticates as, handed in by the service.
//!
//! The service issues a confidential client for the client credentials grant
//! (RFC 6749 §4.4), so the secret is the deployment's to hold. The
//! synchronisation service reads it from its own configuration, inline or
//! from a mounted `<key>_file`, and hands it over; no rendering of these types
//! prints it.

use core::fmt;

use ferrohealth_secret::secret::Secret;

/// Credentials the add-on cannot authenticate with.
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    /// The client identifier or the client secret is empty.
    #[error("no usable client: configure a client identifier and a client secret")]
    NoUsableClient,
}

/// The client the add-on authenticates as.
///
/// The `Debug` rendering names the client and never the secret, so the whole
/// source can be logged.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    /// The OAuth 2 client identifier (RFC 6749 §2.2).
    pub client_id: String,
    /// The client secret (RFC 6749 §2.3.1).
    pub client_secret: Secret,
}

impl Credentials {
    /// Returns the credentials when both the identifier and the secret are
    /// set.
    ///
    /// # Errors
    ///
    /// Returns [`CredentialError::NoUsableClient`] when either is empty.
    pub fn usable(self) -> Result<Self, CredentialError> {
        if self.client_id.is_empty() || self.client_secret.is_empty() {
            Err(CredentialError::NoUsableClient)
        } else {
            Ok(self)
        }
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Credentials(client_id={})", self.client_id)
    }
}

#[cfg(test)]
mod tests {
    use ferrohealth_secret::secret::Secret;

    use super::{CredentialError, Credentials};

    fn filled() -> Credentials {
        Credentials {
            client_id: String::from("a-client"),
            client_secret: Secret::new("a-client-secret"),
        }
    }

    #[test]
    fn debug_names_the_client_and_no_secret() {
        let rendered = format!("{:?}", filled());
        assert!(
            !rendered.contains("a-client-secret"),
            "the client secret must never be rendered"
        );
        assert!(rendered.contains("client_id=a-client"), "{rendered}");
    }

    #[test]
    fn an_empty_secret_or_identifier_is_refused() {
        assert!(filled().usable().is_ok());
        assert!(matches!(
            Credentials {
                client_secret: Secret::new(""),
                ..filled()
            }
            .usable(),
            Err(CredentialError::NoUsableClient)
        ));
        assert!(matches!(
            Credentials {
                client_id: String::new(),
                ..filled()
            }
            .usable(),
            Err(CredentialError::NoUsableClient)
        ));
    }
}
