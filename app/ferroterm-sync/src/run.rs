// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One run, end to end, and the service that performs it.
//!
//! A run lists every configured source, its feed and, when it has one, its
//! FHIR API, decides what the deployment does not hold yet, fetches it (with
//! its digest verified when the feed advertised one), and puts it through one
//! of two lanes: an RF2 archive is built into a staging directory, a FHIR
//! resource is corrected and written as a staged file. In `auto` activation
//! the run then renames what it staged into place and asks the server to
//! reload; in `manual` activation it stops at staging and waits for
//! `POST /activate`.
//!
//! A run never returns an error to its caller. Everything that goes wrong is
//! written into the run record, the run is marked failed, and the webhook
//! fires either way, because a failed synchronisation is exactly the event an
//! operator has to hear about.
//!
//! No FHIR or SNOMED CT specification governs a run: our own design.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use terminology_syndication::model::CategoryTerm;
use terminology_syndication::select::{Holdings, Taken};

use crate::clock::Clock;
use crate::config::{Activation, Config};
use crate::metrics::Metrics;
use crate::record::{
    Activated, Origin, RecordStore, ReloadReply, RunRecord, SkippedEntry, SourceRun, TakenEntry,
    Trigger,
};
use crate::reload::{AdminClient, ReloadError};
use crate::schedule::{Plan, ScheduleError};
use crate::source::ConfiguredSource;
use crate::state::{Lane, Staged, State};
use crate::webhook::Webhook;
use crate::{naming, reason};

/// A service that cannot be built.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    /// The schedule does not read.
    #[error("the schedule does not read")]
    Schedule(#[from] ScheduleError),
    /// The state file does not read.
    #[error("the state does not read")]
    State(#[from] crate::state::StateError),
    /// The HTTP client could not be built.
    #[error("the HTTP client could not be built")]
    Client {
        /// Why it could not be built.
        #[source]
        source: reqwest::Error,
    },
    /// `[server_admin_credentials]` breaks a rule.
    #[error("[server_admin_credentials] is refused")]
    Credentials {
        /// The rule.
        #[source]
        source: Box<ferrohealth_smart::config::ConfigError>,
    },
    /// The token client of the client credentials grant cannot be built.
    #[error("the token client of [server_admin_credentials.oauth] cannot be built")]
    Token {
        /// Why.
        #[source]
        source: Box<ferrohealth_smart::client::TokenError>,
    },
}

/// One running synchronisation service.
#[derive(Debug)]
pub struct Service {
    config: Config,
    client: reqwest::Client,
    plan: Plan,
    sources: Vec<ConfiguredSource>,
    clock: Arc<dyn Clock>,
    metrics: Metrics,
    records: RecordStore,
    admin: AdminClient,
    webhook: Option<Webhook>,
    state: tokio::sync::Mutex<State>,
    running: tokio::sync::Mutex<()>,
    sequence: AtomicU64,
}

impl Service {
    /// The service `config` describes, reading `sources` on `clock`.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError::Schedule`] when the schedule does not read,
    /// [`ServiceError::State`] when the state file does not read, and
    /// [`ServiceError::Client`] when the HTTP client cannot be built, and
    /// [`ServiceError::Credentials`] or [`ServiceError::Token`] when the
    /// credential of the reload call does not resolve.
    pub fn new(
        config: Config,
        sources: Vec<ConfiguredSource>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, ServiceError> {
        let plan = config.plan()?;
        let state = State::read(&config.state_file())?;
        // A reload opens every artifact again, which takes as long as a start,
        // so the call waits far longer than an ordinary request would.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_mins(30))
            .build()
            .map_err(|source| ServiceError::Client { source })?;
        let mut admin = AdminClient::new(client.clone(), &config.server_admin_url);
        let outbound = config
            .server_admin_credentials
            .resolve()
            .map_err(|source| ServiceError::Credentials {
                source: Box::new(source),
            })?;
        if let Some(outbound) = outbound {
            admin = admin
                .with_credentials(outbound)
                .map_err(|source| ServiceError::Token {
                    source: Box::new(source),
                })?;
        }
        let webhook = config
            .webhook_url
            .as_deref()
            .map(|url| Webhook::new(client.clone(), url));
        let records = RecordStore::new(config.records.clone());
        Ok(Self {
            config,
            client,
            plan,
            sources,
            clock,
            metrics: Metrics::new(),
            records,
            admin,
            webhook,
            state: tokio::sync::Mutex::new(state),
            running: tokio::sync::Mutex::new(()),
            sequence: AtomicU64::new(0),
        })
    }

    /// The configuration the service runs under.
    #[must_use]
    pub const fn config(&self) -> &Config {
        &self.config
    }

    /// The metrics a scrape reads.
    #[must_use]
    pub const fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    /// The run records an operator reads.
    #[must_use]
    pub const fn records(&self) -> &RecordStore {
        &self.records
    }

    /// When the next scheduled run is due.
    ///
    /// # Errors
    ///
    /// Returns [`ScheduleError`] when the due time falls outside the range a
    /// timestamp holds.
    pub async fn next_due(&self) -> Result<Option<jiff::Timestamp>, ScheduleError> {
        let last = self.state.lock().await.last_run;
        self.plan.next_after(last, self.clock.now())
    }

    /// Waits for the next scheduled run and performs it.
    ///
    /// Answers `None` when no schedule is configured, which is a service that
    /// runs only when it is asked to.
    pub async fn wait_and_run(&self) -> Option<RunRecord> {
        let due = match self.next_due().await {
            Ok(due) => due?,
            Err(error) => {
                tracing::error!(error = reason(&error), "the next run cannot be computed");
                return None;
            }
        };
        tracing::info!(due = %due, "waiting for the next scheduled run");
        self.clock.sleep_until(due).await;
        Some(self.run(Trigger::Schedule).await)
    }

    /// Performs scheduled runs until `shutdown` completes.
    pub async fn serve_schedule(self: Arc<Self>, shutdown: impl Future<Output = ()> + Send) {
        if self.plan.is_empty() {
            tracing::info!("no schedule is configured; runs start on request only");
            shutdown.await;
            return;
        }
        let mut shutdown = Box::pin(shutdown);
        loop {
            let service = Arc::clone(&self);
            tokio::select! {
                run = service.wait_and_run() => {
                    if run.is_none() {
                        break;
                    }
                }
                () = &mut shutdown => break,
            }
        }
    }

    /// Performs one run, whatever started it.
    ///
    /// The record it answers is the same one written to the records directory,
    /// counted in the metrics, and posted to the webhook.
    pub async fn run(&self, trigger: Trigger) -> RunRecord {
        let running = self.running.lock().await;
        let started = self.clock.now();
        let sequence = self
            .sequence
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        let mut record = RunRecord::new(
            crate::record::identifier(started, sequence),
            trigger,
            started,
        );
        tracing::info!(run = %record.id, trigger = trigger.as_str(), "a run started");
        let mut state = self.state.lock().await;
        self.collect(&mut state, &mut record).await;
        if self.config.activation == Activation::Auto && !state.pending.is_empty() {
            self.place_pending(&mut state, &mut record).await;
        }
        self.config
            .activation
            .as_str()
            .clone_into(&mut record.activation.mode);
        record.activation.staged = state.pending.len();
        self.close(&mut state, &mut record, trigger).await;
        drop(state);
        drop(running);
        record
    }

    /// Puts what is staged in front of the server, whatever staged it.
    ///
    /// This is `POST /activate`: in `manual` activation it is how a release
    /// that a run built reaches the server.
    pub async fn activate(&self) -> RunRecord {
        let running = self.running.lock().await;
        let started = self.clock.now();
        let sequence = self
            .sequence
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        let mut record = RunRecord::new(
            crate::record::identifier(started, sequence),
            Trigger::Activate,
            started,
        );
        let mut state = self.state.lock().await;
        if state.pending.is_empty() {
            tracing::info!(run = %record.id, "nothing is staged");
        } else {
            self.place_pending(&mut state, &mut record).await;
        }
        self.config
            .activation
            .as_str()
            .clone_into(&mut record.activation.mode);
        record.activation.staged = state.pending.len();
        self.close(&mut state, &mut record, Trigger::Activate).await;
        drop(state);
        drop(running);
        record
    }

    /// Reads every source and stages what the deployment does not hold.
    async fn collect(&self, state: &mut State, record: &mut RunRecord) {
        let held = self.holdings(state).await;
        for source in &self.sources {
            let mut run = SourceRun::new(source.source().name(), source.source().feed_url());
            let mut listed = Vec::new();
            match source.source().list().await {
                Err(error) => {
                    let text = reason(&error);
                    tracing::error!(
                        source = source.source().name(),
                        error = text,
                        "the feed did not answer"
                    );
                    run.errors.push(text.clone());
                    record.fail(text);
                }
                Ok(feed) => {
                    run.entries_seen = feed.entries.len();
                    self.stage_listed(source, &feed, Origin::Feed, &held, state, &mut run, record)
                        .await;
                    listed.push(feed);
                }
            }
            if let Some(api_url) = source.source().fhir_api_url() {
                run.fhir_api_url = Some(api_url.to_owned());
                match source.source().list_api().await {
                    Err(error) => {
                        let text = reason(&error);
                        tracing::error!(
                            source = source.source().name(),
                            error = text,
                            "the FHIR API did not list"
                        );
                        run.errors.push(text.clone());
                        record.fail(text);
                    }
                    Ok(feed) => {
                        run.api_entries_seen = feed.entries.len();
                        self.stage_listed(
                            source,
                            &feed,
                            Origin::FhirApi,
                            &held,
                            state,
                            &mut run,
                            record,
                        )
                        .await;
                        listed.push(feed);
                    }
                }
            }
            if run.errors.is_empty() {
                run.not_visible = source.unseen(&listed.iter().collect::<Vec<_>>());
                for canonical in &run.not_visible {
                    tracing::warn!(
                        source = source.source().name(),
                        canonical,
                        "a subscribed system was not visible to the account"
                    );
                }
            }
            record.sources.push(run);
        }
    }

    /// Selects from one listing and stages what it takes.
    #[expect(
        clippy::too_many_arguments,
        reason = "one listing, its origin, and the three things a run writes into"
    )]
    async fn stage_listed(
        &self,
        source: &ConfiguredSource,
        feed: &terminology_syndication::model::Feed,
        origin: Origin,
        held: &Holdings,
        state: &mut State,
        run: &mut SourceRun,
        record: &mut RunRecord,
    ) {
        let selection = source.select(feed, held);
        for skipped in selection.skipped {
            run.skipped.push(SkippedEntry {
                entry_id: skipped.entry_id,
                title: skipped.title,
                origin,
                reason: skipped.reason.to_string(),
            });
        }
        for taken in selection.taken {
            let entry = self.take(source, &taken, origin, state).await;
            if let Some(error) = entry.error.clone() {
                record.fail(error);
            }
            run.taken.push(entry);
        }
    }

    /// What the deployment holds: the ledger, the directories, and what is
    /// already staged and waiting.
    async fn holdings(&self, state: &State) -> Holdings {
        let ledger = state.held.clone();
        let index_root = self.config.index_root.clone();
        let resources = self.config.resources.clone();
        let scanned = tokio::task::spawn_blocking(move || {
            crate::holdings::holdings(&ledger, &index_root, &resources)
        })
        .await;
        let mut held = match scanned {
            Ok(held) => held,
            Err(error) => {
                tracing::error!(%error, "the holdings could not be read");
                Holdings::new()
            }
        };
        for staged in &state.pending {
            held.record(&staged.canonical, &staged.version, staged.date);
        }
        held
    }

    /// Fetches one entry and puts it through its lane.
    async fn take(
        &self,
        source: &ConfiguredSource,
        taken: &Taken,
        origin: Origin,
        state: &mut State,
    ) -> TakenEntry {
        let term = taken.entry.term();
        let lane = if term.is_rf2() {
            Some(Lane::Index)
        } else if matches!(
            term,
            CategoryTerm::FhirCodeSystem
                | CategoryTerm::FhirValueSet
                | CategoryTerm::FhirConceptMap
                | CategoryTerm::FhirBundle
        ) {
            Some(Lane::Resource)
        } else {
            None
        };
        let canonical = taken
            .entry
            .content_item_identifier
            .clone()
            .unwrap_or_default();
        let version = taken.entry.content_item_version.clone().unwrap_or_default();
        let mut entry = TakenEntry {
            entry_id: taken.entry.id.clone(),
            title: taken.entry.title.clone(),
            origin,
            canonical: canonical.clone(),
            version: version.clone(),
            category: term.to_string(),
            lane: lane.map_or("none", Lane::as_str).to_owned(),
            bytes: 0,
            download_ms: 0,
            build_ms: None,
            staged: None,
            fixups: Vec::new(),
            outcome: crate::record::Outcome::Ok,
            error: None,
        };
        let date = taken.entry.updated.unwrap_or_else(|| self.clock.now());
        let extension = if term.is_rf2() { "zip" } else { "json" };
        let download = self.config.downloads().join(format!(
            "{}.{extension}",
            naming::download_name(&taken.entry)
        ));
        if let Err(error) = tokio::fs::create_dir_all(self.config.downloads()).await {
            return failed(entry, format!("cannot use the download directory: {error}"));
        }
        let started = std::time::Instant::now();
        let fetched = match origin {
            Origin::Feed => source.source().fetch(&taken.link, &download).await,
            Origin::FhirApi => source.source().fetch_api(&taken.link, &download).await,
        };
        let fetched = match fetched {
            Ok(fetched) => fetched,
            Err(error) => return failed(entry, reason(&error)),
        };
        entry.download_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        entry.bytes = fetched.bytes;

        match lane {
            Some(Lane::Index) => {
                self.stage_index(source, taken, &download, date, state, entry)
                    .await
            }
            Some(Lane::Resource) => {
                self.stage_resource(source, &download, date, state, entry)
                    .await
            }
            None => failed(
                entry,
                format!(
                    "{term} is not a kind of content this service can put in front of the server"
                ),
            ),
        }
    }

    /// Builds a downloaded RF2 archive into a staging directory.
    async fn stage_index(
        &self,
        source: &ConfiguredSource,
        taken: &Taken,
        download: &std::path::Path,
        date: jiff::Timestamp,
        state: &mut State,
        mut entry: TakenEntry,
    ) -> TakenEntry {
        let name = naming::artifact_name(&taken.entry);
        let staged = self.config.staging.join(&name);
        let built = crate::build::build_rf2(&self.config.build_command, download, &staged).await;
        match built {
            Ok(report) => {
                entry.build_ms = Some(report.duration_ms);
                entry.staged = Some(staged.clone());
                state.pending.push(Staged {
                    source: source.source().name().to_owned(),
                    lane: Lane::Index,
                    path: staged,
                    target: self.config.index_root.join(&name),
                    replaced: None,
                    canonical: entry.canonical.clone(),
                    version: entry.version.clone(),
                    date,
                });
                entry
            }
            Err(error) => failed(entry, reason(&error)),
        }
    }

    /// Corrects a downloaded FHIR resource and stages it as a file.
    async fn stage_resource(
        &self,
        source: &ConfiguredSource,
        download: &std::path::Path,
        date: jiff::Timestamp,
        state: &mut State,
        mut entry: TakenEntry,
    ) -> TakenEntry {
        let arrived = match tokio::fs::read(download).await {
            Ok(bytes) => bytes,
            Err(error) => {
                return failed(
                    entry,
                    format!("cannot read {}: {error}", download.display()),
                );
            }
        };
        let mut bytes = arrived;
        if let Some(fixups) = source.fixups() {
            match fixups.apply(&bytes) {
                Ok(corrected) => {
                    entry.fixups.clone_from(&corrected.applied);
                    bytes = corrected.bytes;
                }
                Err(error) => return failed(entry, reason(&error)),
            }
        }
        let name = naming::resource_file_name(&entry.canonical, &entry.version);
        let staged = self.config.staged_resources().join(&name);
        if let Err(error) = tokio::fs::create_dir_all(self.config.staged_resources()).await {
            return failed(entry, format!("cannot use the staging directory: {error}"));
        }
        if let Err(error) = tokio::fs::write(&staged, &bytes).await {
            return failed(entry, format!("cannot write {}: {error}", staged.display()));
        }
        entry.staged = Some(staged.clone());
        state.pending.push(Staged {
            source: source.source().name().to_owned(),
            lane: Lane::Resource,
            path: staged,
            target: self.config.resources.join(&name),
            replaced: Some(self.config.replaced().join(&name)),
            canonical: entry.canonical.clone(),
            version: entry.version.clone(),
            date,
        });
        entry
    }

    /// Moves what is staged into place, reloads, and prunes what it replaced.
    async fn place_pending(&self, state: &mut State, record: &mut RunRecord) {
        let pending = core::mem::take(&mut state.pending);
        let placed = match crate::activate::place(&pending).await {
            Ok(placed) => placed,
            Err(error) => {
                let text = reason(&error);
                record.activation.error = Some(text.clone());
                record.fail(text);
                state.pending = pending;
                return;
            }
        };
        match self.admin.reload().await {
            Ok(reply) => record.activation.reloads.push(reply),
            Err(error) => {
                if let ReloadError::Refused { status, body, .. } = &error {
                    record.activation.reloads.push(ReloadReply {
                        status: *status,
                        body: body.clone(),
                    });
                }
                crate::activate::undo(&placed).await;
                record.activation.rolled_back = true;
                let text = reason(&error);
                record.activation.error = Some(text.clone());
                record.fail(text);
                state.pending = pending;
                return;
            }
        }
        for one in &placed {
            state.hold(&one.item.canonical, &one.item.version, one.item.date);
            record.activation.activated.push(Activated {
                lane: one.item.lane.as_str().to_owned(),
                target: one.item.target.clone(),
                canonical: one.item.canonical.clone(),
                version: one.item.version.clone(),
            });
        }
        self.revalidate(record).await;
        self.prune(record).await;
    }

    /// Checks the deployment's own value sets and maps against the new set.
    ///
    /// The release is served by the time this runs, so a check that cannot be
    /// performed is reported in the record rather than failing the run: the
    /// finding list is a report for a terminologist, and the service never
    /// edits local content.
    async fn revalidate(&self, record: &mut RunRecord) {
        let Some(base_url) = self.config.fhir_base_url.as_deref() else {
            record.revalidation.skipped = Some(String::from(
                "no fhir_base_url is configured, so local resources were not revalidated",
            ));
            return;
        };
        let client = crate::revalidate::FhirClient::new(self.client.clone(), base_url);
        match crate::revalidate::revalidate(&client, &record.activation.activated).await {
            Ok(report) => {
                for finding in &report.findings {
                    tracing::warn!(
                        resource = finding.resource,
                        system = finding.system,
                        code = finding.code,
                        kind = finding.kind.as_str(),
                        "the activated release changed a local code"
                    );
                }
                record.revalidation = report;
            }
            Err(error) => {
                let text = reason(&error);
                tracing::warn!(error = text, "the local resources were not revalidated");
                record.revalidation.skipped = Some(text);
            }
        }
    }

    /// Keeps the configured number of releases and reloads when it removed any.
    async fn prune(&self, record: &mut RunRecord) {
        let root = self.config.index_root.clone();
        let keep = self.config.retention;
        let pruned =
            tokio::task::spawn_blocking(move || crate::retention::prune(&root, keep)).await;
        let report = match pruned {
            Ok(Ok(report)) => report,
            Ok(Err(error)) => {
                let text = reason(&error);
                record.fail(text);
                return;
            }
            Err(error) => {
                record.fail(format!("the retention task did not finish: {error}"));
                return;
            }
        };
        if report.pruned.is_empty() {
            return;
        }
        record.retention = report.pruned;
        match self.admin.reload().await {
            Ok(reply) => record.activation.reloads.push(reply),
            Err(error) => {
                let text = reason(&error);
                tracing::warn!(
                    error = text,
                    "the server was not told that a superseded release is gone"
                );
                record.fail(text);
            }
        }
    }

    /// Closes the run: the state, the record, the metrics, and the webhook.
    async fn close(&self, state: &mut State, record: &mut RunRecord, trigger: Trigger) {
        let finished = self.clock.now();
        // NOTE: an activation is not a synchronisation run, so it never moves
        // the schedule (no spec governs this: our own design).
        if trigger != Trigger::Activate {
            state.last_run = Some(finished);
        }
        if let Err(error) = state.write(&self.config.state_file()) {
            let text = reason(&error);
            tracing::error!(error = text, "the state was not written");
            record.fail(text);
        }
        record.bytes_used = self.index_bytes().await;
        record.close(finished);
        match self.records.write(record) {
            Ok(path) => tracing::info!(
                run = %record.id,
                outcome = record.outcome.as_str(),
                path = %path.display(),
                "a run finished"
            ),
            Err(error) => tracing::error!(
                run = %record.id,
                error = reason(&error),
                "the run record was not written"
            ),
        }
        self.metrics.recorded(record);
        if let Some(webhook) = self.webhook.as_ref() {
            webhook.deliver(&record.summary()).await;
        }
    }

    /// How many bytes the index root holds.
    async fn index_bytes(&self) -> u64 {
        let root = self.config.index_root.clone();
        tokio::task::spawn_blocking(move || crate::retention::directory_bytes(&root))
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "the index size could not be read");
                0
            })
    }
}

/// Marks one entry failed with `error` as its reason.
fn failed(mut entry: TakenEntry, error: String) -> TakenEntry {
    tracing::error!(entry = entry.entry_id, error, "an entry was not taken");
    entry.outcome = crate::record::Outcome::Failed;
    entry.error = Some(error);
    entry
}
