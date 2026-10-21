// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One JSON record per run, and the directory they are kept in.
//!
//! A record says what the feed offered, what the run took and what it left
//! behind with a reason, what each build cost, how the activation ended, what
//! the server answered, what retention removed, and every error on the way.
//! The admin listener serves the directory at `/runs`, so an operator reads a
//! run without reading the log.

use std::path::PathBuf;

/// How a run ended.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// Everything the run attempted worked.
    Ok,
    /// Something failed; what the server serves is unchanged.
    Failed,
}

impl Outcome {
    /// The outcome's name, as a metric label and a record write it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
        }
    }
}

/// What started a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Trigger {
    /// The schedule.
    Schedule,
    /// `POST /run` on the service's admin listener.
    Manual,
    /// The `run-once` command.
    RunOnce,
    /// `POST /activate` on the service's admin listener.
    Activate,
}

impl Trigger {
    /// The trigger's name, as a record and a log line write it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Schedule => "schedule",
            Self::Manual => "manual",
            Self::RunOnce => "run-once",
            Self::Activate => "activate",
        }
    }
}

/// Where a run saw an entry: the feed, or the service's FHIR API.
///
/// The feed is the default, which is what a record written before the API
/// lane existed means by saying nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    /// The syndication feed, whose entries carry a digest.
    #[default]
    Feed,
    /// The FHIR API, listed by search, whose resources carry none.
    FhirApi,
}

impl Origin {
    /// The origin's name, as a record and a log line write it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Feed => "feed",
            Self::FhirApi => "fhir-api",
        }
    }
}

/// One entry the run took, and what became of it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct TakenEntry {
    /// The feed entry's identifier.
    pub entry_id: String,
    /// The feed entry's title.
    pub title: String,
    /// Where the run saw the entry.
    #[serde(default)]
    pub origin: Origin,
    /// The canonical identifier of the content item.
    pub canonical: String,
    /// The version identifier of the content item.
    pub version: String,
    /// The category term the entry declared.
    pub category: String,
    /// The lane the entry went through.
    pub lane: String,
    /// How many bytes arrived.
    pub bytes: u64,
    /// How long the download took.
    pub download_ms: u64,
    /// How long the build took, for an entry that was built.
    pub build_ms: Option<u64>,
    /// Where the item was staged.
    pub staged: Option<PathBuf>,
    /// The corrections the source's add-on applied to the file.
    pub fixups: Vec<String>,
    /// How this entry ended.
    pub outcome: Outcome,
    /// Why it failed, when it did.
    pub error: Option<String>,
}

/// One entry the run left behind, and why.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SkippedEntry {
    /// The feed entry's identifier.
    pub entry_id: String,
    /// The feed entry's title.
    pub title: String,
    /// Where the run saw the entry.
    #[serde(default)]
    pub origin: Origin,
    /// Why the run left it behind.
    pub reason: String,
}

/// How many entries of one origin the run left behind only because the
/// subscription does not name their system or category.
///
/// These are counted rather than listed: a service's FHIR API lists every
/// resource it holds, and a line per unsubscribed one would bury the entries
/// that were decided. No specification governs the record: our own design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct NotSubscribed {
    /// Where the run saw the entries.
    pub origin: Origin,
    /// How many there were.
    pub entries: usize,
}

/// What one source contributed to a run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct SourceRun {
    /// The source's name.
    pub source: String,
    /// The address its feed was read from.
    pub feed_url: String,
    /// How many entries the feed offered.
    pub entries_seen: usize,
    /// The FHIR API the source lists beside its feed, when it has one.
    #[serde(default)]
    pub fhir_api_url: Option<String>,
    /// How many resources the FHIR API listed.
    #[serde(default)]
    pub api_entries_seen: usize,
    /// The subscribed canonicals that neither the feed nor the FHIR API
    /// listed: not visible to the account, which is a licence question and
    /// never proof that the service lacks the system.
    #[serde(default)]
    pub not_visible: Vec<String>,
    /// The entries the run took.
    pub taken: Vec<TakenEntry>,
    /// The entries the run left behind for a reason other than the
    /// subscription, one line each.
    pub skipped: Vec<SkippedEntry>,
    /// The entries left behind only because the subscription does not name
    /// them, one count per origin that had any.
    #[serde(default)]
    pub not_subscribed: Vec<NotSubscribed>,
    /// What went wrong while reading this source.
    pub errors: Vec<String>,
}

impl SourceRun {
    /// An empty contribution from the source `name` at `feed_url`.
    #[must_use]
    pub fn new(name: &str, feed_url: &str) -> Self {
        Self {
            source: name.to_owned(),
            feed_url: feed_url.to_owned(),
            entries_seen: 0,
            fhir_api_url: None,
            api_entries_seen: 0,
            not_visible: Vec::new(),
            taken: Vec::new(),
            skipped: Vec::new(),
            not_subscribed: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Counts one entry of `origin` the subscription does not name.
    pub fn count_not_subscribed(&mut self, origin: Origin) {
        if let Some(held) = self
            .not_subscribed
            .iter_mut()
            .find(|held| held.origin == origin)
        {
            held.entries = held.entries.saturating_add(1);
        } else {
            self.not_subscribed
                .push(NotSubscribed { origin, entries: 1 });
        }
    }

    /// How many entries the subscription left behind, over every origin.
    #[must_use]
    pub fn not_subscribed_total(&self) -> usize {
        self.not_subscribed
            .iter()
            .map(|held| held.entries)
            .fold(0_usize, usize::saturating_add)
    }
}

/// One staged item that reached the server.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Activated {
    /// The lane that produced it.
    pub lane: String,
    /// Where it now sits.
    pub target: PathBuf,
    /// The canonical identifier of the content item.
    pub canonical: String,
    /// The version identifier of the content item.
    pub version: String,
}

/// What the server answered when it was asked to reload.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ReloadReply {
    /// The status the server answered with.
    pub status: u16,
    /// The body it answered with.
    pub body: String,
}

/// How the activation step of a run ended.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ActivationReport {
    /// The activation mode the run worked in.
    pub mode: String,
    /// What reached the server.
    pub activated: Vec<Activated>,
    /// What is staged and still waiting.
    pub staged: usize,
    /// What the server answered, in the order it was asked.
    pub reloads: Vec<ReloadReply>,
    /// Whether the run put everything back where it was.
    pub rolled_back: bool,
    /// Why the activation failed, when it did.
    pub error: Option<String>,
}

/// What a new release did to a code a local resource names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FindingKind {
    /// The code is still in the code system and is no longer active.
    Inactive,
    /// The code is no longer in the code system at all.
    Absent,
    /// The code is still there and no longer falls inside the value set it was
    /// included through.
    OutsideValueSet,
}

impl FindingKind {
    /// The kind's name, as a record writes it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::Absent => "absent",
            Self::OutsideValueSet => "outside-value-set",
        }
    }
}

/// One local code a new release changed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Finding {
    /// The locally authored resource that names the code.
    pub resource: String,
    /// The code system the code comes from.
    pub system: String,
    /// The code itself.
    pub code: String,
    /// What the release did to it.
    pub kind: FindingKind,
    /// The release this run activated for that system, when it activated one.
    pub release: Option<String>,
}

/// What revalidating the deployment's own resources found.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Revalidation {
    /// How many locally authored resources were read.
    pub resources: usize,
    /// The locally authored resources that were checked, in the order they
    /// were read.
    pub checked_resources: Vec<String>,
    /// How many codes were checked.
    pub checked: usize,
    /// The local codes the release changed.
    pub findings: Vec<Finding>,
    /// What the check found, in one sentence.
    pub statement: String,
    /// Why the check did not run, when it did not.
    pub skipped: Option<String>,
}

/// One release directory retention removed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Pruned {
    /// The code system the release belonged to.
    pub system: String,
    /// The directory that was removed.
    pub path: PathBuf,
    /// How many bytes it held.
    pub bytes: u64,
}

/// Everything one run did.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct RunRecord {
    /// The run's identifier, which is also its file name.
    pub id: String,
    /// What started the run.
    pub trigger: String,
    /// When the run started.
    pub started: jiff::Timestamp,
    /// When the run ended.
    pub finished: jiff::Timestamp,
    /// How long the run took.
    pub duration_ms: u64,
    /// How the run ended.
    pub outcome: Outcome,
    /// What each source contributed.
    pub sources: Vec<SourceRun>,
    /// How the activation ended.
    pub activation: ActivationReport,
    /// What retention removed.
    pub retention: Vec<Pruned>,
    /// What revalidating the deployment's own resources found.
    pub revalidation: Revalidation,
    /// How many bytes the index root holds after the run.
    pub bytes_used: u64,
    /// How many bytes the run staged.
    pub bytes_staged: u64,
    /// How many entries the run took.
    pub entries_taken: usize,
    /// Everything that went wrong, in the order it happened.
    pub errors: Vec<String>,
}

impl RunRecord {
    /// An empty record for the run `id`, started by `trigger` at `started`.
    #[must_use]
    pub fn new(id: String, trigger: Trigger, started: jiff::Timestamp) -> Self {
        Self {
            id,
            trigger: trigger.as_str().to_owned(),
            started,
            finished: started,
            duration_ms: 0,
            outcome: Outcome::Ok,
            sources: Vec::new(),
            activation: ActivationReport::default(),
            retention: Vec::new(),
            revalidation: Revalidation::default(),
            bytes_used: 0,
            bytes_staged: 0,
            entries_taken: 0,
            errors: Vec::new(),
        }
    }

    /// Marks the run failed and records `error` as its reason.
    pub fn fail(&mut self, error: String) {
        self.outcome = Outcome::Failed;
        self.errors.push(error);
    }

    /// Closes the record at `finished`, filling in the totals.
    pub fn close(&mut self, finished: jiff::Timestamp) {
        self.finished = finished;
        self.duration_ms = u64::try_from(
            finished
                .duration_since(self.started)
                .as_millis()
                .max(0)
                .min(i128::from(u64::MAX)),
        )
        .unwrap_or(u64::MAX);
        self.entries_taken = self
            .sources
            .iter()
            .map(|source| source.taken.len())
            .sum::<usize>();
        self.bytes_staged = self
            .sources
            .iter()
            .flat_map(|source| source.taken.iter())
            .map(|taken| taken.bytes)
            .fold(0_u64, u64::saturating_add);
    }

    /// The short shape a webhook carries.
    #[must_use]
    pub fn summary(&self) -> Summary {
        Summary {
            id: self.id.clone(),
            trigger: self.trigger.clone(),
            outcome: self.outcome,
            started: self.started,
            finished: self.finished,
            duration_ms: self.duration_ms,
            entries_taken: self.entries_taken,
            entries_skipped: self
                .sources
                .iter()
                .map(|source| source.skipped.len())
                .sum::<usize>(),
            entries_not_subscribed: self
                .sources
                .iter()
                .map(SourceRun::not_subscribed_total)
                .fold(0_usize, usize::saturating_add),
            activated: self.activation.activated.len(),
            bytes_staged: self.bytes_staged,
            bytes_used: self.bytes_used,
            revalidation_findings: self.revalidation.findings.len(),
            revalidation: self.revalidation.statement.clone(),
            errors: self.errors.clone(),
        }
    }
}

/// The short shape of a run, as `/runs` and a webhook carry it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Summary {
    /// The run's identifier.
    pub id: String,
    /// What started the run.
    pub trigger: String,
    /// How the run ended.
    pub outcome: Outcome,
    /// When the run started.
    pub started: jiff::Timestamp,
    /// When the run ended.
    pub finished: jiff::Timestamp,
    /// How long the run took.
    pub duration_ms: u64,
    /// How many entries the run took.
    pub entries_taken: usize,
    /// How many entries the run left behind for a reason other than the
    /// subscription.
    pub entries_skipped: usize,
    /// How many entries the run left behind only because the subscription
    /// does not name them.
    #[serde(default)]
    pub entries_not_subscribed: usize,
    /// How many staged items reached the server.
    pub activated: usize,
    /// How many bytes the run staged.
    pub bytes_staged: u64,
    /// How many bytes the index root holds after the run.
    pub bytes_used: u64,
    /// How many local codes the activated release changed.
    pub revalidation_findings: usize,
    /// What revalidating the deployment's own resources found, in one sentence.
    pub revalidation: String,
    /// Everything that went wrong.
    pub errors: Vec<String>,
}

/// A record that could not be written or read.
#[derive(Debug, thiserror::Error)]
pub enum RecordError {
    /// The record directory or file does not read or does not write.
    #[error("cannot use the run record at {path}")]
    Io {
        /// The file or directory the service was working on.
        path: PathBuf,
        /// Why it did not read or write.
        #[source]
        source: std::io::Error,
    },
    /// The record is not the JSON this service writes.
    #[error("{path} is not a run record")]
    Parse {
        /// The file that did not parse.
        path: PathBuf,
        /// What JSON found wrong with it.
        #[source]
        source: serde_json::Error,
    },
    /// The record could not be written as JSON.
    #[error("the run record cannot be written as JSON")]
    Serialize {
        /// Why it could not be written.
        #[source]
        source: serde_json::Error,
    },
    /// The identifier names no record this service could have written.
    #[error("`{id}` is not a run identifier")]
    BadId {
        /// The identifier that was asked for.
        id: String,
    },
}

/// The directory one JSON record per run is kept in.
#[derive(Debug, Clone)]
pub struct RecordStore {
    dir: PathBuf,
}

impl RecordStore {
    /// The store in `dir`.
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Writes `record` into the directory.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError::Serialize`] when the record does not serialize
    /// and [`RecordError::Io`] when the directory or the file does not write.
    pub fn write(&self, record: &RunRecord) -> Result<PathBuf, RecordError> {
        let text = serde_json::to_string_pretty(record)
            .map_err(|source| RecordError::Serialize { source })?;
        std::fs::create_dir_all(&self.dir).map_err(|source| RecordError::Io {
            path: self.dir.clone(),
            source,
        })?;
        let path = self.dir.join(format!("{}.json", record.id));
        std::fs::write(&path, text).map_err(|source| RecordError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(path)
    }

    /// The summaries of the records held, newest first.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError::Io`] when the directory does not list and
    /// [`RecordError::Parse`] when a record does not read.
    pub fn summaries(&self) -> Result<Vec<Summary>, RecordError> {
        let mut ids = self.ids()?;
        ids.sort();
        ids.reverse();
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            out.push(self.read(&id)?.summary());
        }
        Ok(out)
    }

    /// The record `id` names.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError::BadId`] when the identifier is not one this
    /// service writes, [`RecordError::Io`] when the file does not read, and
    /// [`RecordError::Parse`] when it is not a record.
    pub fn read(&self, id: &str) -> Result<RunRecord, RecordError> {
        let path = self.path_of(id)?;
        let text = std::fs::read_to_string(&path).map_err(|source| RecordError::Io {
            path: path.clone(),
            source,
        })?;
        serde_json::from_str(&text).map_err(|source| RecordError::Parse { path, source })
    }

    /// The identifiers of the records held.
    fn ids(&self) -> Result<Vec<String>, RecordError> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(RecordError::Io {
                    path: self.dir.clone(),
                    source,
                });
            }
        };
        let mut out = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| RecordError::Io {
                path: self.dir.clone(),
                source,
            })?;
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(std::ffi::OsStr::to_str) {
                out.push(stem.to_owned());
            }
        }
        Ok(out)
    }

    /// The file `id` names, refusing an identifier that is not one of ours.
    fn path_of(&self, id: &str) -> Result<PathBuf, RecordError> {
        if !is_identifier(id) {
            return Err(RecordError::BadId { id: id.to_owned() });
        }
        Ok(self.dir.join(format!("{id}.json")))
    }
}

/// Whether `id` is a run identifier this service writes.
///
/// A request names a record, so the check is what keeps a path out of the
/// identifier: only ASCII letters, digits, and the two separators pass.
#[must_use]
pub fn is_identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// The identifier of a run that started at `started`, the `sequence`th so far.
///
/// The shape sorts by time, so the directory listing is the run history in
/// order, and the sequence separates two runs inside one second.
#[must_use]
pub fn identifier(started: jiff::Timestamp, sequence: u64) -> String {
    let stamp = started.strftime("%Y%m%dT%H%M%SZ");
    format!("{stamp}-{sequence:04}")
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
mod tests {
    use super::{RecordStore, RunRecord, Trigger, identifier, is_identifier};

    #[test]
    fn an_identifier_sorts_by_time() -> Result<(), jiff::Error> {
        let first = identifier("2026-09-22T03:00:00Z".parse()?, 1);
        let second = identifier("2026-09-22T04:00:00Z".parse()?, 2);
        assert_eq!(first, "20260922T030000Z-0001", "the shape is the timestamp");
        assert!(first < second, "the listing is the history in order");
        Ok(())
    }

    #[test]
    fn a_path_is_not_an_identifier() {
        assert!(!is_identifier("../state/state"), "a path is refused");
        assert!(!is_identifier(""), "an empty identifier is refused");
        assert!(
            is_identifier("20260922T030000Z-0001"),
            "the shape the service writes is accepted"
        );
    }

    #[test]
    fn a_record_round_trips_through_the_store() -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = RecordStore::new(dir.path().join("records"));
        let started: jiff::Timestamp = "2026-09-22T03:00:00Z".parse()?;
        let mut record = RunRecord::new(identifier(started, 1), Trigger::Schedule, started);
        record.close("2026-09-22T03:00:02Z".parse()?);
        store.write(&record)?;
        assert_eq!(store.read(&record.id)?, record, "the record round trips");
        assert_eq!(
            store.summaries()?.len(),
            1,
            "the listing carries the one record"
        );
        assert_eq!(record.duration_ms, 2000, "the record carries its duration");
        Ok(())
    }

    #[test]
    fn an_unknown_identifier_is_refused() -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = RecordStore::new(dir.path().to_path_buf());
        assert!(
            store.read("../../etc/passwd").is_err(),
            "a record is read by identifier, never by path"
        );
        Ok(())
    }
}
