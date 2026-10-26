// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The usage report's state on disk: the instance id and the time of the last
//! start report, under `usage_report.state_dir`.
//!
//! No FHIR or SNOMED CT specification governs this: our own design. Without a
//! stable instance id every restart would count as a new installation, so a
//! deployment with no state directory, or one that cannot be written, sends
//! nothing.

use std::path::{Path, PathBuf};

use uuid::Uuid;

/// The file holding the instance id, a hyphenated UUID on one line.
pub const INSTANCE_FILE: &str = "instance-id";

/// The file holding the time of the last start report, an RFC 3339 instant.
pub const LAST_START_FILE: &str = "last-start";

/// How close to the previous start report a new one is skipped.
pub const START_INTERVAL: jiff::SignedDuration = jiff::SignedDuration::from_mins(10);

/// The file a writability probe creates and removes.
const PROBE_FILE: &str = ".write-probe";

/// A state directory that does not serve.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// A file of the directory cannot be read, written, or removed.
    #[error("cannot write or read {path}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The instance id file holds something other than a UUID.
    #[error("{path} holds no UUID")]
    InstanceId {
        /// The file.
        path: PathBuf,
        /// The cause.
        #[source]
        source: uuid::Error,
    },
}

/// The instance id of this installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instance {
    /// The random id.
    pub id: Uuid,
    /// Whether this start created it, which marks a new installation.
    pub created: bool,
}

/// A writable state directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDir {
    root: PathBuf,
}

impl StateDir {
    /// Opens `root` as the state directory, creating it when absent, and
    /// checks that it can be written.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::Io`] when the directory cannot be created or a
    /// file in it cannot be written and removed.
    pub fn open(root: &Path) -> Result<Self, StateError> {
        std::fs::create_dir_all(root).map_err(|source| StateError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let probe = root.join(PROBE_FILE);
        let io = |source| StateError::Io {
            path: probe.clone(),
            source,
        };
        std::fs::write(&probe, b"").map_err(io)?;
        std::fs::remove_file(&probe).map_err(io)?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    /// The directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Reads the instance id, creating a fresh random one when the directory
    /// holds none.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::Io`] when the file cannot be read or written, and
    /// [`StateError::InstanceId`] when it holds no UUID: a damaged id is
    /// refused rather than replaced, because a new one would count as a new
    /// installation.
    pub fn instance(&self) -> Result<Instance, StateError> {
        let path = self.root.join(INSTANCE_FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let id = Uuid::parse_str(text.trim())
                    .map_err(|source| StateError::InstanceId { path, source })?;
                Ok(Instance { id, created: false })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let id = Uuid::new_v4();
                self.write_atomically(INSTANCE_FILE, &format!("{}\n", id.hyphenated()))?;
                Ok(Instance { id, created: true })
            }
            Err(source) => Err(StateError::Io { path, source }),
        }
    }

    /// Claims a start report at `now`: `false` when the previous one went out
    /// less than [`START_INTERVAL`] before, otherwise records `now` and
    /// returns `true`.
    ///
    /// An unreadable or malformed record is treated as no record, so it never
    /// stops the report for good.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::Io`] when `now` cannot be recorded.
    pub fn claim_start(&self, now: jiff::Timestamp) -> Result<bool, StateError> {
        let path = self.root.join(LAST_START_FILE);
        // NOTE: a missing or damaged record legitimately means no recent start
        // (no FHIR/SNOMED spec governs this: our own design).
        let last = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| text.trim().parse::<jiff::Timestamp>().ok());
        if let Some(last) = last
            && now.duration_since(last) < START_INTERVAL
            && now >= last
        {
            return Ok(false);
        }
        self.write_atomically(LAST_START_FILE, &format!("{now}\n"))?;
        Ok(true)
    }

    /// Writes `name` through a temporary file and a rename, so a crash never
    /// leaves half a file.
    fn write_atomically(&self, name: &str, text: &str) -> Result<(), StateError> {
        let target = self.root.join(name);
        let staged = self.root.join(format!(".{name}.tmp"));
        std::fs::write(&staged, text).map_err(|source| StateError::Io {
            path: staged.clone(),
            source,
        })?;
        std::fs::rename(&staged, &target).map_err(|source| StateError::Io {
            path: target,
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{INSTANCE_FILE, LAST_START_FILE, StateDir, StateError};

    #[test]
    fn the_first_open_creates_the_id_and_the_next_reads_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = StateDir::open(&dir.path().join("state")).expect("opens");
        let first = state.instance().expect("creates");
        assert!(first.created);
        let again = StateDir::open(state.root())
            .expect("opens")
            .instance()
            .expect("reads");
        assert_eq!(again.id, first.id);
        assert!(!again.created);
        let text = std::fs::read_to_string(state.root().join(INSTANCE_FILE)).expect("reads");
        assert_eq!(text, format!("{}\n", first.id.hyphenated()));
        assert_eq!(first.id.get_version_num(), 4);
    }

    #[test]
    fn a_damaged_id_is_refused_rather_than_replaced() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(INSTANCE_FILE), "not a uuid").expect("writes");
        let state = StateDir::open(dir.path()).expect("opens");
        assert!(matches!(
            state.instance(),
            Err(StateError::InstanceId { .. })
        ));
    }

    #[test]
    fn a_start_within_ten_minutes_is_skipped() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = StateDir::open(dir.path()).expect("opens");
        let t0: jiff::Timestamp = "2026-10-16T08:00:00Z".parse().expect("parses");
        assert!(state.claim_start(t0).expect("claims"));
        let nine = t0 + jiff::SignedDuration::from_mins(9);
        assert!(!state.claim_start(nine).expect("reads"));
        let ten = t0 + jiff::SignedDuration::from_mins(10);
        assert!(state.claim_start(ten).expect("claims"));
        std::fs::write(dir.path().join(LAST_START_FILE), "garbage").expect("writes");
        assert!(
            state.claim_start(ten).expect("claims"),
            "a damaged record is no record"
        );
        let earlier = ten - jiff::SignedDuration::from_hours(1);
        assert!(
            state.claim_start(earlier).expect("claims"),
            "a record from the future is no record"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_unwritable_directory_does_not_open() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let locked = dir.path().join("locked");
        std::fs::create_dir_all(&locked).expect("creates");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).expect("chmods");
        let probe = std::fs::write(locked.join("x"), b"");
        if probe.is_ok() {
            // Running as root, which writes anywhere; nothing to assert.
            return;
        }
        assert!(matches!(
            StateDir::open(&locked),
            Err(StateError::Io { .. })
        ));
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).expect("chmods");
    }
}
