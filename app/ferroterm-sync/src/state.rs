// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the service remembers between runs.
//!
//! One JSON file holds three things: when the last run ended, so a restart
//! does not replay a run that already happened; what the service itself put in
//! front of the server and at which feed date, so the replace rule has
//! something to compare; and what is staged and waiting for activation, so a
//! restart in manual mode does not lose the release it built.
//!
//! The file is written whole, to a temporary name beside it and then renamed,
//! so a process that stops mid-write leaves the previous file intact.

use std::path::{Path, PathBuf};

/// One content item the service put in front of the server.
///
/// The identity is the canonical identifier and the version identifier of the
/// content item, which is what the feed's replace rule compares.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Held {
    /// The canonical identifier of the content item.
    pub canonical: String,
    /// The version identifier of the content item.
    pub version: String,
    /// The feed date of the entry the service took.
    pub date: jiff::Timestamp,
}

/// Which lane produced a staged item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lane {
    /// A built index directory, renamed into the index root.
    Index,
    /// A FHIR resource file, renamed into the managed resource directory.
    Resource,
}

impl Lane {
    /// The lane's name, as a run record and a log line write it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Resource => "resource",
        }
    }
}

/// One staged item waiting to be put in front of the server.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Staged {
    /// The source the item came from.
    pub source: String,
    /// The lane that produced it.
    pub lane: Lane,
    /// Where the item sits while it waits.
    pub path: PathBuf,
    /// Where it goes when it is activated.
    pub target: PathBuf,
    /// Where the file it replaces is kept until the reload succeeds.
    pub replaced: Option<PathBuf>,
    /// The canonical identifier of the content item.
    pub canonical: String,
    /// The version identifier of the content item.
    pub version: String,
    /// The feed date of the entry, which becomes the held date on success.
    pub date: jiff::Timestamp,
}

/// Everything the service remembers between runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct State {
    /// When the last run ended.
    pub last_run: Option<jiff::Timestamp>,
    /// What the service put in front of the server, and at which feed date.
    pub held: Vec<Held>,
    /// What is staged and waiting for activation.
    pub pending: Vec<Staged>,
}

/// A state file that does not read or does not write.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// The file does not read or does not write.
    #[error("cannot use the state file {path}")]
    Io {
        /// The file the service was working on.
        path: PathBuf,
        /// Why it did not read or write.
        #[source]
        source: std::io::Error,
    },
    /// The file is not the JSON this service writes.
    #[error("{path} is not a ferroterm-sync state file")]
    Parse {
        /// The file that did not parse.
        path: PathBuf,
        /// What JSON found wrong with it.
        #[source]
        source: serde_json::Error,
    },
    /// The state could not be written as JSON.
    #[error("the state cannot be written as JSON")]
    Serialize {
        /// Why it could not be written.
        #[source]
        source: serde_json::Error,
    },
}

impl State {
    /// Reads the state at `path`, answering the empty state when there is none.
    ///
    /// A missing file is a service that has never run, which is not a failure.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::Io`] when the file exists and does not read, and
    /// [`StateError::Parse`] when it is not the JSON this service writes.
    pub fn read(path: &Path) -> Result<Self, StateError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => {
                return Err(StateError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        serde_json::from_str(&text).map_err(|source| StateError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Writes the state to `path`, through a temporary file beside it.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::Serialize`] when the state does not serialize and
    /// [`StateError::Io`] when the directory, the temporary file, or the
    /// rename does not work.
    pub fn write(&self, path: &Path) -> Result<(), StateError> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|source| StateError::Serialize { source })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| StateError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let partial = partial_path(path);
        std::fs::write(&partial, text).map_err(|source| StateError::Io {
            path: partial.clone(),
            source,
        })?;
        std::fs::rename(&partial, path).map_err(|source| StateError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Records that `canonical` at `version` is held as of `date`.
    ///
    /// A held item is never forgotten when the release directory it came from
    /// is pruned: the feed keeps offering that version, and a run that forgot
    /// it would fetch and build it again.
    pub fn hold(&mut self, canonical: &str, version: &str, date: jiff::Timestamp) {
        if let Some(held) = self
            .held
            .iter_mut()
            .find(|held| held.canonical == canonical && held.version == version)
        {
            held.date = date;
            return;
        }
        self.held.push(Held {
            canonical: canonical.to_owned(),
            version: version.to_owned(),
            date,
        });
    }
}

/// The temporary name a state file is written under before it is renamed.
fn partial_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".part");
    PathBuf::from(name)
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
mod tests {
    use super::State;

    #[test]
    fn a_missing_file_is_a_service_that_has_never_run() -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let state = State::read(&dir.path().join("state.json"))?;
        assert_eq!(state, State::default(), "nothing has happened yet");
        Ok(())
    }

    #[test]
    fn the_state_survives_a_write_and_a_read() -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("sub").join("state.json");
        let mut state = State {
            last_run: Some("2026-09-22T03:00:00Z".parse()?),
            ..State::default()
        };
        state.hold(
            "http://snomed.info/sct/11000146104",
            "http://snomed.info/sct/11000146104/version/20260930",
            "2026-09-30T00:00:00Z".parse()?,
        );
        state.write(&path)?;
        assert_eq!(State::read(&path)?, state, "the file round trips");
        Ok(())
    }

    #[test]
    fn holding_the_same_identity_twice_moves_its_date() -> Result<(), Box<dyn core::error::Error>> {
        let mut state = State::default();
        state.hold("http://loinc.org", "2.83", "2026-01-01T00:00:00Z".parse()?);
        state.hold("http://loinc.org", "2.83", "2026-02-01T00:00:00Z".parse()?);
        assert_eq!(state.held.len(), 1, "one identity is held once");
        let Some(held) = state.held.first() else {
            return Err(Box::from("the held row is missing"));
        };
        assert_eq!(
            held.date,
            "2026-02-01T00:00:00Z".parse::<jiff::Timestamp>()?,
            "a corrected republish moves the date forward"
        );
        Ok(())
    }
}
