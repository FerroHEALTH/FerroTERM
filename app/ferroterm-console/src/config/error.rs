// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a configuration was refused: each refusal names the key, never a
//! value, so no secret reaches a message, a `Debug` rendering or a log.

/// A refused configuration.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The tree does not load: a file that does not read, an unknown key or
    /// variable, or a value its key does not admit.
    #[error(transparent)]
    Load(#[from] ferrohealth_config::error::ConfigError),
    /// A socket address that does not parse.
    #[error("`{key}` is not a socket address")]
    Address {
        /// The key.
        key: String,
        /// What parsing it failed with.
        #[source]
        source: std::net::AddrParseError,
    },
    /// A locale that is no BCP 47 language tag.
    #[error("`{key}` is not a language tag")]
    Locale {
        /// The key.
        key: String,
        /// What parsing it failed with.
        #[source]
        source: ferrohealth_ui::i18n::UnknownLanguage,
    },
    /// A URL that does not parse.
    #[error("`{key}` is not a URL")]
    Url {
        /// The key.
        key: String,
        /// What parsing it failed with.
        #[source]
        source: url::ParseError,
    },
    /// A URL its key does not admit.
    #[error("`{key}` {reason}")]
    UrlShape {
        /// The key.
        key: String,
        /// What the key requires.
        reason: &'static str,
    },
    /// A value that must be positive is zero.
    #[error("`{key}` must be greater than zero")]
    Zero {
        /// The key.
        key: String,
    },
    /// A required value is absent.
    #[error("`{key}` is required")]
    Missing {
        /// The key.
        key: String,
    },
    /// A scope list names a value its key does not admit.
    #[error("`{key}` {reason}")]
    Scope {
        /// The key.
        key: String,
        /// What the key requires.
        reason: &'static str,
    },
    /// Cookies without `Secure` on a console that is not plain HTTP on
    /// loopback, where they would travel in the clear.
    #[error("`{key}` may be false only for a console whose redirect_uri is http on loopback")]
    InsecureCookie {
        /// The key.
        key: String,
    },
    /// A secret set both inline and through its `_file` sibling, or whose
    /// file cannot be read or is empty.
    #[error(transparent)]
    Secret(#[from] ferrohealth_secret::file::SecretError),
}
