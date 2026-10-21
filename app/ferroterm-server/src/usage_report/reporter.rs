// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The usage report client: decides whether the report runs, assembles a
//! [`Report`], sends it, and schedules the start and daily events.
//!
//! No FHIR or SNOMED CT specification governs this: our own design. The
//! channel is outbound only: nothing the collector answers is read beyond the
//! status code, and no answer changes configuration or behaviour. A send is
//! bounded by a short timeout and a body cap and logs its failure once at
//! DEBUG; the background task is detached from boot and from `/health`, so a
//! slow or unreachable collector reaches neither.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use http::{Request, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use tokio::task::JoinHandle;

use crate::config::{STATE_DIR_ENV, USAGE_REPORT_ENV};
use crate::reload::Serving;
use crate::usage_report::config::{Deployment, UsageReportConfig};
use crate::usage_report::payload::{
    CpuBucket, Event, Family, LicenceGrant, MAX_UPTIME_S, MemoryBucket, Product, Report, SchemaV2,
    Status, report_git_sha,
};
use crate::usage_report::state::{Instance, StateDir, StateError};
use crate::usage_report::window::Window;

/// The book page that carries the full notice.
pub const NOTICE_URL: &str = "https://ferroterm.eu/docs/operate/usage-report.html";

/// The largest body the collector accepts, in bytes.
pub const MAX_BODY_BYTES: usize = 64 * 1024;

/// How long after boot the first daily report is attempted.
pub const FIRST_DAILY_AFTER: Duration = Duration::from_mins(10);

/// How long a daily attempt that did not reach the collector waits to try
/// again.
pub const RETRY_AFTER_FAILURE: Duration = Duration::from_hours(1);

/// How long a connection to the collector may take to open.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long one send may take in total.
pub const TOTAL_TIMEOUT: Duration = Duration::from_secs(10);

/// The commit this binary was built from, as the build recorded it.
const GIT_SHA: &str = env!("FERROTERM_GIT_SHA");

/// Why the report is off for this process.
#[derive(Debug)]
pub enum Off {
    /// `FERROTERM_USAGE_REPORT=off`.
    Switch,
    /// A CI variable is set; it names which.
    Ci(&'static str),
    /// The deployment names no state directory.
    NoStateDir,
    /// The state directory does not serve.
    StateDir {
        /// The directory.
        path: PathBuf,
        /// The cause.
        error: StateError,
    },
}

/// What the report needs from disk when it runs.
#[derive(Debug, Clone)]
pub struct Prepared {
    /// The state directory, writable.
    pub state: StateDir,
    /// The instance id, created on the first start.
    pub instance: Instance,
}

/// Decides whether the report runs and, when it does, opens the state
/// directory and reads or creates the instance id.
///
/// The switch is read first, then CI detection, then the state directory, so
/// an explicit `on` never overrides a CI environment. Nothing is written while
/// the report is off.
///
/// # Errors
///
/// Returns [`Off`] with the reason the report is off.
pub fn prepare(config: &UsageReportConfig) -> Result<Prepared, Off> {
    if !config.enabled {
        return Err(Off::Switch);
    }
    if let Some(variable) = config.ci {
        return Err(Off::Ci(variable));
    }
    let Some(root) = &config.state_dir else {
        return Err(Off::NoStateDir);
    };
    let failed = |error| Off::StateDir {
        path: root.clone(),
        error,
    };
    let state = StateDir::open(root).map_err(failed)?;
    let instance = state.instance().map_err(failed)?;
    Ok(Prepared { state, instance })
}

/// Logs the one INFO boot line that says whether the report is on, what it
/// carries and how to switch it off, or why it is off.
pub fn announce(config: &UsageReportConfig, decision: &Result<Prepared, Off>) {
    match decision {
        Ok(_) => tracing::info!(
            endpoint = %config.endpoint,
            "usage report ON: instance id, version, licence type, deployment, uptime, host size, \
             loaded code system families and coarse operation latency, at each start and once a \
             day; no codes, URLs or request paths. Off: {USAGE_REPORT_ENV}=off. Details: \
             {NOTICE_URL}"
        ),
        Err(Off::Switch) => tracing::info!(
            "usage report OFF: switched off ({USAGE_REPORT_ENV}=off); nothing is sent. Details: \
             {NOTICE_URL}"
        ),
        Err(Off::Ci(variable)) => tracing::info!(
            "usage report OFF: CI detected ({variable}); nothing is sent. Details: {NOTICE_URL}"
        ),
        Err(Off::NoStateDir) => tracing::info!(
            "usage report OFF: no state directory ({STATE_DIR_ENV} is unset), and without a \
             stable instance id each restart would count as a new installation; nothing is sent. \
             On: mount a writable volume and set {STATE_DIR_ENV} to it. Details: {NOTICE_URL}"
        ),
        Err(Off::StateDir { path, error }) => tracing::info!(
            path = %path.display(),
            error = %error_chain(error),
            "usage report OFF: the state directory ({STATE_DIR_ENV}) cannot be written, and \
             without a stable instance id each restart would count as a new installation; \
             nothing is sent. On: mount a writable volume and set {STATE_DIR_ENV} to it. \
             Details: {NOTICE_URL}"
        ),
    }
}

/// The facts about this build and deployment a report states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The server version.
    pub version: String,
    /// The commit, in the contract's form.
    pub git_sha: String,
    /// The licence grant type.
    pub licence: LicenceGrant,
    /// How the server was deployed.
    pub deployment: Deployment,
}

impl Identity {
    /// Returns the identity of this build under `deployment`.
    ///
    /// The server installs no commercial licence, so it runs under what the
    /// Business Source License grants everyone, the non-commercial grant.
    #[must_use]
    pub fn of_build(deployment: Deployment) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            git_sha: report_git_sha(GIT_SHA),
            licence: LicenceGrant::NonCommercial,
            deployment,
        }
    }
}

/// What a start attempt did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartOutcome {
    /// The report was sent and the collector answered with this status.
    Sent(StatusCode),
    /// A start report of this instance went out in the last ten minutes.
    RateLimited,
}

/// Why a report was not sent.
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    /// The HTTPS client does not build, because the trust store does not open.
    #[error("cannot build the HTTPS client for the collector")]
    Client(#[source] std::io::Error),
    /// The endpoint is not `https`, or `http` to a loopback host.
    #[error("{0} is not an https URL, and only a loopback collector may be plain HTTP")]
    NotSecure(String),
    /// The request does not build from the endpoint.
    #[error("cannot build a request to the collector")]
    Url(#[source] http::Error),
    /// The request did not complete.
    #[error("cannot reach the collector")]
    Request(#[source] hyper_util::client::legacy::Error),
    /// No answer arrived within [`TOTAL_TIMEOUT`].
    #[error("the collector did not answer within {TOTAL_TIMEOUT:?}")]
    Timeout,
    /// The report did not serialize.
    #[error("the report does not serialize")]
    Json(#[source] serde_json::Error),
    /// The rendered report exceeds the collector's body limit.
    #[error("the report body of {0} bytes exceeds the {MAX_BODY_BYTES}-byte limit")]
    TooLarge(usize),
    /// The state directory could not record the start.
    #[error("the state directory")]
    State(#[source] StateError),
    /// The blocking task touching the state directory did not finish.
    #[error("the state directory task")]
    Task(#[source] tokio::task::JoinError),
}

/// The usage report client of one process.
pub struct Reporter {
    endpoint: String,
    client: Client<hyper_rustls::HttpsConnector<HttpConnector>, Body>,
    identity: Identity,
    prepared: Prepared,
    serving: Serving,
    window: Arc<Window>,
    started: Instant,
}

impl std::fmt::Debug for Reporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reporter")
            .field("endpoint", &self.endpoint)
            .field("identity", &self.identity)
            .field("prepared", &self.prepared)
            .finish_non_exhaustive()
    }
}

impl Reporter {
    /// Creates the client for `prepared`, reading the served set and the
    /// window from `serving`.
    ///
    /// `started` is when this process started; reports state the uptime from
    /// it.
    ///
    /// # Errors
    ///
    /// Returns [`ReportError::Client`] when the trust store does not open, and
    /// [`ReportError::NotSecure`] for an endpoint the report may not use.
    pub fn new(
        config: &UsageReportConfig,
        prepared: Prepared,
        serving: Serving,
        started: Instant,
    ) -> Result<Self, ReportError> {
        if !crate::usage_report::config::secure(&config.endpoint) {
            return Err(ReportError::NotSecure(config.endpoint.clone()));
        }
        let mut http = HttpConnector::new();
        http.set_connect_timeout(Some(CONNECT_TIMEOUT));
        http.enforce_http(false);
        // NOTE: `https_or_http` keeps a loopback collector reachable; `secure`
        // refuses plain HTTP anywhere else
        // (<https://docs.rs/hyper-rustls/0.27/hyper_rustls/struct.HttpsConnectorBuilder.html>).
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(ReportError::Client)?
            .https_or_http()
            .enable_http1()
            .wrap_connector(http);
        Ok(Self {
            endpoint: config.endpoint.clone(),
            client: Client::builder(TokioExecutor::new()).build(connector),
            identity: Identity::of_build(config.deployment),
            prepared,
            window: serving.usage_window(),
            serving,
            started,
        })
    }

    /// Returns the start report as a send would render it now.
    #[must_use]
    pub fn start_report(&self) -> Report {
        self.assemble(Event::Start {
            first_start: self.prepared.instance.created,
        })
    }

    /// Sends the start report unless one went out in the last ten minutes.
    ///
    /// # Errors
    ///
    /// Returns [`ReportError`] when the start cannot be recorded or the send
    /// fails.
    pub async fn send_start(&self) -> Result<StartOutcome, ReportError> {
        let state = self.prepared.state.clone();
        let claimed = tokio::task::spawn_blocking(move || {
            // NOTE: jiff is the workspace's one source of wall-clock time
            // (<https://docs.rs/jiff/latest/jiff/struct.Timestamp.html#method.now>).
            state.claim_start(jiff::Timestamp::now())
        })
        .await
        .map_err(ReportError::Task)?
        .map_err(ReportError::State)?;
        if !claimed {
            return Ok(StartOutcome::RateLimited);
        }
        Ok(StartOutcome::Sent(self.post(&self.start_report()).await?))
    }

    /// Sends the daily report over the window since the previous one.
    ///
    /// The window is taken first; a send that does not reach the collector
    /// puts it back, so the retry carries it. Any answer from the collector
    /// ends the window.
    ///
    /// # Errors
    ///
    /// Returns [`ReportError`] when the send fails.
    pub async fn send_daily(&self) -> Result<StatusCode, ReportError> {
        let counts = self.window.take();
        let report = self.assemble(Event::Daily {
            metrics: Box::new(counts.to_metrics()),
        });
        let sent = self.post(&report).await;
        if sent.is_err() {
            self.window.restore(&counts);
        }
        sent
    }

    /// Starts the background task: the start report at once, then the daily
    /// report [`FIRST_DAILY_AFTER`] boot and about every 24 hours.
    ///
    /// Returns the task handle; dropping it detaches the task.
    #[must_use]
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            match self.send_start().await {
                Ok(StartOutcome::Sent(status)) => {
                    tracing::debug!(status = status.as_u16(), "usage report: start report sent");
                }
                Ok(StartOutcome::RateLimited) => tracing::debug!(
                    "usage report: a start report of this instance went out in the last ten minutes"
                ),
                Err(error) => tracing::debug!(
                    error = %error_chain(&error),
                    "usage report: the start report was not sent"
                ),
            }
            let mut wait = FIRST_DAILY_AFTER;
            loop {
                tokio::time::sleep(wait).await;
                wait = match self.send_daily().await {
                    Ok(status) => {
                        tracing::debug!(
                            status = status.as_u16(),
                            "usage report: daily report answered"
                        );
                        next_daily_wait()
                    }
                    Err(error) => {
                        tracing::debug!(
                            error = %error_chain(&error),
                            "usage report: the daily report was not sent"
                        );
                        RETRY_AFTER_FAILURE
                    }
                };
            }
        })
    }

    /// Fills the envelope around `event` from the identity, the served set,
    /// and the host.
    fn assemble(&self, event: Event) -> Report {
        Report {
            schema: SchemaV2,
            event,
            product: Product::FerroTerm,
            instance_id: self.prepared.instance.id,
            version: self.identity.version.clone(),
            git_sha: self.identity.git_sha.clone(),
            licence: self.identity.licence,
            deployment: self.identity.deployment,
            uptime_s: self.started.elapsed().as_secs().min(MAX_UPTIME_S),
            cpu_bucket: CpuBucket::of(
                std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
            ),
            memory_bucket: MemoryBucket::of(host_memory_bytes().unwrap_or(0)),
            status: status_of(&self.serving),
        }
    }

    /// Posts one report and returns the collector's status; the response body
    /// is never read.
    async fn post(&self, report: &Report) -> Result<StatusCode, ReportError> {
        let body = serde_json::to_vec(report).map_err(ReportError::Json)?;
        if body.len() > MAX_BODY_BYTES {
            return Err(ReportError::TooLarge(body.len()));
        }
        let request = Request::post(&self.endpoint)
            .header(http::header::CONTENT_TYPE, "application/json")
            .header(
                http::header::USER_AGENT,
                format!("ferroterm/{}", self.identity.version),
            )
            .body(Body::from(body))
            .map_err(ReportError::Url)?;
        let response = tokio::time::timeout(TOTAL_TIMEOUT, self.client.request(request))
            .await
            .map_err(|_elapsed| ReportError::Timeout)?
            .map_err(ReportError::Request)?;
        Ok(response.status())
    }
}

/// Logs the boot line and, when the report runs, starts its background task.
///
/// `None` when the report is off, and when the client cannot be built, which
/// is logged at DEBUG: the report never fails boot. The state directory is
/// touched on a blocking task.
pub async fn start(
    config: &UsageReportConfig,
    serving: &Serving,
    started: Instant,
) -> Option<JoinHandle<()>> {
    let owned = config.clone();
    let decision = match tokio::task::spawn_blocking(move || prepare(&owned)).await {
        Ok(decision) => decision,
        Err(error) => {
            tracing::debug!(error = %error, "usage report: the state directory task failed");
            return None;
        }
    };
    announce(config, &decision);
    let prepared = decision.ok()?;
    match Reporter::new(config, prepared, serving.clone(), started) {
        Ok(reporter) => Some(reporter.spawn()),
        Err(error) => {
            tracing::debug!(
                error = %error_chain(&error),
                "usage report: the client could not be built"
            );
            None
        }
    }
}

/// The status of the served set as of now.
fn status_of(serving: &Serving) -> Status {
    let state = serving.current();
    let families: Vec<Family> = state
        .origins()
        .map(|(url, origin)| Family::of(url, origin))
        .collect();
    Status::new(
        !families.is_empty(),
        families,
        // NOTE: no specification governs this: our own design; the console is its own
        // binary now (app/ferroterm-viewer), so this server never mounts a viewer.
        false,
        state.persists(),
    )
}

/// The wait before the next daily report: 24 hours, spread at random by up
/// to 30 minutes either way.
fn next_daily_wait() -> Duration {
    // NOTE: a v4 UUID is 122 random bits from the operating system
    // (<https://docs.rs/uuid/latest/uuid/struct.Uuid.html#method.new_v4>).
    let spread = u64::try_from(uuid::Uuid::new_v4().as_u128() % 3601).unwrap_or(0);
    Duration::from_secs(23 * 3600 + 1800 + spread)
}

/// `error` and every cause under it, joined with `: `.
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut out = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        out.push_str(": ");
        out.push_str(&cause.to_string());
        source = cause.source();
    }
    out
}

/// The memory available to the process in bytes: the smaller of the host's
/// memory and the cgroup limit, where Linux reports either.
fn host_memory_bytes() -> Option<u64> {
    // NOTE: proc(5) `/proc/meminfo` and the cgroup v2/v1 memory files; absent
    // off Linux, where the report states the smallest bucket.
    let total = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| meminfo_total_bytes(&text));
    let limit = std::fs::read_to_string("/sys/fs/cgroup/memory.max")
        .or_else(|_absent| std::fs::read_to_string("/sys/fs/cgroup/memory/memory.limit_in_bytes"))
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok());
    match (total, limit) {
        (Some(total), Some(limit)) => Some(total.min(limit)),
        (total, limit) => total.or(limit),
    }
}

/// The `MemTotal` line of `/proc/meminfo`, in bytes (the file states kB).
fn meminfo_total_bytes(meminfo: &str) -> Option<u64> {
    meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|kib| kib.parse::<u64>().ok())
        .map(|kib| kib.saturating_mul(1024))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Off, meminfo_total_bytes, next_daily_wait, prepare};
    use crate::usage_report::config::UsageReportConfig;

    #[test]
    fn meminfo_total_is_read_in_bytes() {
        let meminfo = "MemTotal:       16303428 kB\nMemFree:         1000 kB\n";
        assert_eq!(meminfo_total_bytes(meminfo), Some(16_303_428 * 1024));
        assert_eq!(meminfo_total_bytes("MemFree: 1 kB\n"), None);
    }

    #[test]
    fn the_daily_wait_is_a_day_give_or_take_half_an_hour() {
        for _ in 0..64 {
            let wait = next_daily_wait();
            assert!(wait >= Duration::from_mins(23 * 60 + 30), "{wait:?}");
            assert!(wait <= Duration::from_mins(24 * 60 + 30), "{wait:?}");
        }
    }

    #[test]
    fn the_switch_then_ci_then_the_state_directory_decide() {
        let dir = tempfile::tempdir().expect("tempdir");
        let with_dir = UsageReportConfig {
            state_dir: Some(dir.path().join("state")),
            ..UsageReportConfig::default()
        };
        let off = UsageReportConfig {
            enabled: false,
            ci: Some("CI"),
            ..with_dir.clone()
        };
        assert!(matches!(prepare(&off), Err(Off::Switch)));
        let ci = UsageReportConfig {
            ci: Some("GITHUB_ACTIONS"),
            ..with_dir.clone()
        };
        assert!(matches!(prepare(&ci), Err(Off::Ci("GITHUB_ACTIONS"))));
        assert!(
            !dir.path().join("state").exists(),
            "nothing is written while the report is off"
        );
        let none = UsageReportConfig::default();
        assert!(matches!(prepare(&none), Err(Off::NoStateDir)));
        let on = prepare(&with_dir).expect("runs");
        assert!(on.instance.created);
        assert!(!prepare(&with_dir).expect("runs").instance.created);
    }

    #[test]
    fn a_state_directory_that_is_a_file_turns_the_report_off() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("file");
        std::fs::write(&file, b"").expect("writes");
        let config = UsageReportConfig {
            state_dir: Some(file),
            ..UsageReportConfig::default()
        };
        assert!(matches!(prepare(&config), Err(Off::StateDir { .. })));
    }
}
