// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a configuration was refused: each refusal names the key, never a
//! value, so no secret reaches a message, a `Debug` rendering or a log.

use std::fmt;
use std::path::PathBuf;

/// Which text a [`ParseFault`] was found in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The configuration file as written: the position points into it.
    File,
    /// The tree after the environment overrides, which has no lines of its
    /// own: the fault carries no position.
    Merged,
}

/// What kind of fault the TOML reader met, with no text it quoted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// The text is not TOML.
    Syntax,
    /// A key the configuration does not define.
    UnknownKey,
    /// A key is set twice.
    DuplicateKey,
    /// A value of the wrong type, or outside the values its key admits.
    InvalidValue,
}

impl Problem {
    /// Classifies the reader's `message` without keeping any of its text,
    /// which can quote a value.
    fn of(message: &str) -> Self {
        if message.starts_with("unknown field") {
            Self::UnknownKey
        } else if message.starts_with("duplicate key") {
            Self::DuplicateKey
        } else if message.starts_with("invalid")
            || message.starts_with("missing field")
            || message.starts_with("unknown variant")
        {
            Self::InvalidValue
        } else {
            Self::Syntax
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Syntax => "the text is not TOML",
            Self::UnknownKey => "an unknown key",
            Self::DuplicateKey => "a key set twice",
            Self::InvalidValue => "a value its key does not admit",
        })
    }
}

/// Where a configuration parse fault sits and what kind it is: a position,
/// a key name and a [`Problem`], never a value from the configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseFault {
    /// The text the fault was found in.
    pub stage: Stage,
    /// The 1-based line and column, for [`Stage::File`].
    pub position: Option<(usize, usize)>,
    /// The dotted key of the offending line, when it reads as one.
    pub key: Option<String>,
    /// What kind of fault it is.
    pub problem: Problem,
}

impl ParseFault {
    /// Reads `error`, which the TOML reader raised over `text`, keeping
    /// nothing the reader quoted.
    #[must_use]
    pub fn from_toml(error: &toml::de::Error, text: Option<&str>, stage: Stage) -> Self {
        let problem = Problem::of(error.message());
        let located = text.zip(error.span()).and_then(|(text, span)| {
            let before = text.get(..span.start)?;
            let line = before.matches('\n').count() + 1;
            let start = before.rfind('\n').map_or(0, |at| at + 1);
            let column = before.get(start..)?.chars().count() + 1;
            let content = text.get(start..)?.lines().next().unwrap_or_default();
            Some(((line, column), key_of(content)))
        });
        Self {
            stage,
            position: located.as_ref().map(|(position, _)| *position),
            key: located.and_then(|(_, key)| key),
            problem,
        }
    }
}

/// The key a TOML line assigns, when the text before its `=` is a bare or
/// dotted key.
fn key_of(line: &str) -> Option<String> {
    let (key, _value) = line.split_once('=')?;
    let key = key.trim();
    let bare = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    bare.then(|| key.to_owned())
}

impl fmt::Display for ParseFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.problem)?;
        if let Some(key) = &self.key {
            write!(f, " at `{key}`")?;
        }
        match (self.stage, self.position) {
            (Stage::File, Some((line, column))) => write!(f, " (line {line}, column {column})"),
            (Stage::File, None) => f.write_str(" in the configuration file"),
            (Stage::Merged, _) => f.write_str(" after the environment overrides"),
        }
    }
}

/// A refused configuration.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The configuration file could not be read.
    #[error("cannot read the configuration file {path}")]
    Read {
        /// The file named.
        path: PathBuf,
        /// What reading it failed with.
        #[source]
        source: std::io::Error,
    },
    /// The configuration does not read as the tree.
    #[error("the configuration does not read: {fault}")]
    Parse {
        /// Where and what.
        fault: ParseFault,
    },
    /// An environment variable under the prefix names no section and key.
    #[error("{name} names no configuration key: it needs a section and a key")]
    EnvName {
        /// The variable's name.
        name: String,
    },
    /// An environment variable names a key under a value that is no table.
    #[error("{name} names a key under a value that is not a table")]
    EnvShape {
        /// The variable's name.
        name: String,
    },
    /// The tree with its overrides could not be written back as TOML.
    #[error("cannot assemble the configuration")]
    Assemble {
        /// What writing it failed with.
        #[source]
        source: toml::ser::Error,
    },
    /// A socket address that does not parse.
    #[error("`{key}` is not a socket address")]
    Address {
        /// The key.
        key: String,
        /// What parsing it failed with.
        #[source]
        source: std::net::AddrParseError,
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
    /// A secret is set inline and through its `_file` sibling.
    #[error("`{key}` and `{key}_file` are both set; set one")]
    Conflict {
        /// The key.
        key: String,
    },
    /// A secret file could not be read.
    #[error("cannot read `{key}` from {path}")]
    Secret {
        /// The `_file` key.
        key: String,
        /// The file named.
        path: PathBuf,
        /// What reading it failed with.
        #[source]
        source: std::io::Error,
    },
    /// A secret file holds nothing but whitespace.
    #[error("`{key}` names {path}, which is empty")]
    EmptySecret {
        /// The `_file` key.
        key: String,
        /// The file named.
        path: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use super::{Problem, key_of};

    #[test]
    fn a_reader_message_is_classified_and_never_kept() {
        assert_eq!(Problem::UnknownKey, Problem::of("unknown field `x`"));
        assert_eq!(Problem::DuplicateKey, Problem::of("duplicate key"));
        assert_eq!(
            Problem::InvalidValue,
            Problem::of("invalid type: string \"s\"")
        );
        assert_eq!(Problem::Syntax, Problem::of("expected `=`"));
    }

    #[test]
    fn only_a_bare_or_dotted_key_is_named() {
        assert_eq!(Some(String::from("listen")), key_of("listen = 1"));
        assert_eq!(Some(String::from("a.b")), key_of(" a.b=1"));
        assert_eq!(None, key_of("\"quoted key\" = 1"));
        assert_eq!(None, key_of("[server]"));
    }
}
