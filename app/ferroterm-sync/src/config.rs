// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The service's configuration tree, read by `ferrohealth-config`.
//!
//! The tree's defaults, then the TOML file `--config` or
//! `FERROTERM_SYNC_CONFIG` names, then every `FERROTERM_SYNC__SECTION__KEY`
//! variable, then every `--set key=value`. Every key is optional: a
//! deployment writes what it changes and inherits the rest from
//! [`Config::default`], which describes the layout the container image uses.
//! `RUST_LOG` is the alias of `telemetry.filter`.
//! An unknown key or variable is refused rather than ignored, so a typo in a
//! directory name is a start-up failure and not a run that writes somewhere
//! nobody looks.
//!
//! No FHIR or SNOMED CT specification governs the tree: our own design.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use ferrohealth_config::loader::{Loaded, Loader};
use ferrohealth_config::removed::{Removals, Removed, RemovedKeys, Retired};
use ferrohealth_config::units::Duration;
use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_smart::config::AuthSection;
use ferrohealth_smart::config::endpoint::{EndpointSection, RetrySection};
use ferrohealth_telemetry::section::TelemetrySection;

/// The environment prefix: `FERROTERM_SYNC__SCHEDULE__EVERY` sets
/// `[schedule] every`, and `FERROTERM_SYNC_CONFIG` names the file.
pub const PREFIX: &str = "FERROTERM_SYNC";

/// The variable that names the `tracing` filter, the alias of
/// `telemetry.filter`.
pub const FILTER_ENV: &str = "RUST_LOG";

/// How a staged release reaches the served set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Activation {
    /// A successful build is renamed into the index root and the server is
    /// asked to reload, in the same run.
    #[default]
    Auto,
    /// A successful build stops in the staging directory and waits for
    /// `POST /activate` on the service's admin listener.
    Manual,
}

impl Activation {
    /// The mode's name, as a run record and a log line write it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
        }
    }
}

/// When a run starts by itself.
///
/// Both shapes may be configured at once, in which case the earlier of the two
/// due times wins. With neither, the service runs only when it is asked to.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScheduleConfig {
    /// An interval measured from the end of the last run, a duration such
    /// as `24h`.
    pub every: Option<Duration>,
    /// A time of day in the configured zone, such as `03:00`.
    pub at: Option<String>,
}

/// One configured source: which add-on speaks to it, and its own settings.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConfig {
    /// The add-on that reads this service, such as `nts`.
    pub kind: String,
    /// The add-on's own configuration block, which the add-on reads.
    #[serde(default)]
    pub config: toml::Table,
}

/// `[nts]`: the family outbound endpoint table of the Nationale
/// Terminologie Server.
///
/// It starts from [`addon_nts::config::endpoint`]: the base `url`, the
/// timeouts, `retry`, `tls`, and the client credentials grant (RFC 6749
/// §4.4) under `[nts.credentials.oauth]`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NtsSection {
    /// The endpoint table itself.
    #[serde(flatten)]
    pub endpoint: EndpointSection,
    /// `client_id`, which `[nts.credentials.oauth] client_id` replaced in
    /// 0.4.5.
    pub client_id: Removed,
    /// `client_secret`, which `[nts.credentials.oauth] client_secret`
    /// replaced in 0.4.5.
    pub client_secret: Removed,
    /// `client_secret_file`, which `[nts.credentials.oauth]
    /// client_secret_file` replaced in 0.4.5.
    pub client_secret_file: Removed,
}

// NOTE: serde's `flatten` drops `deny_unknown_fields` (serde.rs/field-attrs.html#flatten),
// so the retired keys are taken out and the rest read strictly as the table.
impl<'de> serde::Deserialize<'de> for NtsSection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let mut table = toml::Table::deserialize(deserializer)?;
        let mut retired = |key: &str| {
            table
                .remove(key)
                .map_or(Ok(Removed::default()), Removed::deserialize)
                .map_err(D::Error::custom)
        };
        let client_id = retired("client_id")?;
        let client_secret = retired("client_secret")?;
        let client_secret_file = retired("client_secret_file")?;
        let endpoint =
            EndpointSection::deserialize(toml::Value::Table(table)).map_err(D::Error::custom)?;
        Ok(Self {
            endpoint,
            client_id,
            client_secret,
            client_secret_file,
        })
    }
}

impl Default for NtsSection {
    fn default() -> Self {
        Self {
            endpoint: addon_nts::config::endpoint(),
            client_id: Removed::default(),
            client_secret: Removed::default(),
            client_secret_file: Removed::default(),
        }
    }
}

/// The whole configuration of one running service.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The address the service's own admin listener binds.
    pub listen: SocketAddr,
    /// `[server_admin]`: the server's admin listener, which serves
    /// `POST /reload`, with the timeout, TLS and credential of the call.
    pub server_admin: EndpointSection,
    /// `[fhir]`: the FHIR base of the server, such as
    /// `http://127.0.0.1:8080/r4b`, which the revalidation reads after a
    /// release is activated. Unset, the check does not run and the run
    /// record says so.
    pub fhir: Option<EndpointSection>,
    /// The index root the server reads, one directory per built release.
    pub index_root: PathBuf,
    /// The managed resource directory the server reads FHIR resources from.
    pub resources: PathBuf,
    /// The directory a run downloads and builds into.
    pub staging: PathBuf,
    /// The directory one JSON run record per run is written to.
    pub records: PathBuf,
    /// The directory holding what the service remembers between runs.
    pub state: PathBuf,
    /// How many release directories per code system the index root keeps.
    pub retention: usize,
    /// How a staged release reaches the served set.
    pub activation: Activation,
    /// `[webhook]`: where one JSON summary per run is posted.
    pub webhook: Option<EndpointSection>,
    /// The time zone a daily schedule is read in, an IANA zone name.
    pub timezone: String,
    /// The offline build binary, found on `PATH` when it is a bare name.
    pub build_command: PathBuf,
    /// When a run starts by itself.
    pub schedule: ScheduleConfig,
    /// The credentials the admin listener admits: the issuers whose access
    /// tokens it accepts, the Basic users, and the product scope every route
    /// but `GET /health` requires ([`crate::auth`]).
    pub auth: AuthSection,
    /// The account the Nationale Terminologie Server add-on authenticates
    /// with.
    pub nts: NtsSection,
    /// The sources a run reads, in the order they are configured.
    pub source: Vec<SourceConfig>,
    /// `[telemetry]`: the console log and the metrics export.
    pub telemetry: TelemetrySection,
    /// `server_admin_url`, which `[server_admin] url` replaced in 0.4.5.
    pub server_admin_url: Removed,
    /// `[server_admin_credentials]`, which `[server_admin.credentials]`
    /// replaced in 0.4.5.
    pub server_admin_credentials: Removed,
    /// `fhir_base_url`, which `[fhir] url` replaced in 0.4.5.
    pub fhir_base_url: Removed,
    /// `webhook_url`, which `[webhook] url` replaced in 0.4.5.
    pub webhook_url: Removed,
}

/// The release these refusals belong to: each lives for that one release.
pub const PREPARING: &str = "0.4.5";

/// The keys 0.4.5 retired, each with the key that took its place.
// TODO(#1202): the refusals ship in 0.4.5 alone; delete them in 0.4.6.
pub const RETIRED: [Retired; 7] = [
    Retired::replaced(
        "server_admin_url",
        "server_admin.url, the admin listener's base URL",
        "0.4.5",
    ),
    Retired::replaced(
        "server_admin_credentials",
        "server_admin.credentials, the same table under [server_admin]",
        "0.4.5",
    ),
    Retired::replaced("fhir_base_url", "fhir.url, the server's FHIR base", "0.4.5"),
    Retired::replaced(
        "webhook_url",
        "webhook.url, the address the summary is posted to",
        "0.4.5",
    ),
    Retired::replaced("nts.client_id", "nts.credentials.oauth.client_id", "0.4.5"),
    Retired::replaced(
        "nts.client_secret",
        "nts.credentials.oauth.client_secret",
        "0.4.5",
    ),
    Retired::replaced(
        "nts.client_secret_file",
        "nts.credentials.oauth.client_secret_file",
        "0.4.5",
    ),
];

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8181),
            // A reload opens every artifact again, which takes as long as a
            // start, so the call waits far longer than an ordinary request and
            // is not repeated.
            server_admin: EndpointSection {
                url: SecretUrl::new("http://127.0.0.1:8081"),
                timeout: Duration::from_mins(30),
                retry: RetrySection {
                    max_attempts: 1,
                    ..RetrySection::default()
                },
                ..EndpointSection::default()
            },
            fhir: None,
            index_root: PathBuf::from("/data/index"),
            resources: PathBuf::from("/data/codesystems"),
            staging: PathBuf::from("/data/staging"),
            records: PathBuf::from("/data/records"),
            state: PathBuf::from("/data/state"),
            retention: 2,
            activation: Activation::Auto,
            webhook: None,
            timezone: String::from("UTC"),
            build_command: PathBuf::from("ferroterm-build"),
            schedule: ScheduleConfig::default(),
            source: Vec::new(),
            nts: NtsSection::default(),
            auth: AuthSection::default(),
            telemetry: TelemetrySection::default(),
            server_admin_url: Removed::default(),
            server_admin_credentials: Removed::default(),
            fhir_base_url: Removed::default(),
            webhook_url: Removed::default(),
        }
    }
}

/// A configuration that could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The tree does not load: a file that does not read, an unknown key or
    /// variable, or a value of the wrong type.
    #[error(transparent)]
    Load(#[from] ferrohealth_config::error::ConfigError),
    /// Keys the service no longer reads, each named with its replacement.
    #[error(transparent)]
    Removed(#[from] RemovedKeys),
    /// HTTP Basic on the reload call: the reload is a machine's call, and a
    /// Basic credential is a person's, so the table takes only `oauth`.
    #[error(
        "[server_admin.credentials] takes no Basic user or password: write the client credentials grant under oauth"
    )]
    BasicReload,
    /// The schedule the tree describes does not read.
    #[error("the schedule does not read")]
    Schedule {
        /// Why the schedule does not read.
        #[source]
        source: crate::schedule::ScheduleError,
    },
}

/// The loader of the service's tree.
///
/// The product's `[server_admin]` and `[nts]` defaults are layered under the
/// file key by key, so a table written in part keeps the reload's timeout and
/// single try, and the service's address and issuer.
///
/// # Errors
///
/// Returns the loader's error when the prefix is refused, which it never is
/// for [`PREFIX`], or when the defaults do not serialize to a table.
pub fn loader() -> Result<Loader, ConfigError> {
    Ok(Loader::new(PREFIX)?
        .alias(FILTER_ENV, "telemetry.filter")
        .defaults("server_admin", &Config::default().server_admin)?
        .defaults("nts", &addon_nts::config::endpoint())?)
}

impl Config {
    /// Reads the configuration of the running process: the file `path`
    /// names, or the one `FERROTERM_SYNC_CONFIG` names, the environment and
    /// the `--set` pairs `sets`, with its schedule checked.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Load`] when the tree does not load,
    /// [`ConfigError::Removed`] for a key 0.4.5 retired,
    /// [`ConfigError::BasicReload`] for a Basic credential on the reload, and
    /// [`ConfigError::Schedule`] when the schedule it describes does not read.
    pub fn load(
        path: Option<&Path>,
        sets: &[(String, String)],
    ) -> Result<Loaded<Self>, ConfigError> {
        let loaded = loader()?.load_with::<Self>(path, sets)?;
        Self::check(loaded.settings()).map(|()| loaded)
    }

    /// Reads `text` as the file with `environment` over it, checked as
    /// [`Config::load`] checks the running process's sources, which a test
    /// cannot set: changing a variable is `unsafe` in edition 2024.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Config::load`].
    pub fn from_sources(
        text: Option<&str>,
        environment: &BTreeMap<String, String>,
    ) -> Result<Self, ConfigError> {
        let (config, _provenance) = loader()?
            .load_sources::<Self>(text, None, environment)?
            .into_parts();
        config.check()?;
        Ok(config)
    }

    /// Refuses every retired key the sources set, then checks the schedule.
    fn check(&self) -> Result<(), ConfigError> {
        let mut removals = Removals::new();
        let set = [
            self.server_admin_url,
            self.server_admin_credentials,
            self.fhir_base_url,
            self.webhook_url,
            self.nts.client_id,
            self.nts.client_secret,
            self.nts.client_secret_file,
        ];
        for (removed, retired) in set.into_iter().zip(&RETIRED) {
            removals.check(removed, retired);
        }
        removals.finish()?;
        let credentials = &self.server_admin.credentials;
        if !credentials.user.is_empty()
            || credentials.password.is_some()
            || credentials.password_file.is_some()
        {
            return Err(ConfigError::BasicReload);
        }
        self.plan()
            .map_err(|source| ConfigError::Schedule { source })?;
        Ok(())
    }

    /// The schedule the configuration describes.
    ///
    /// # Errors
    ///
    /// Returns [`crate::schedule::ScheduleError`] when an interval, a time of
    /// day, or the time zone does not read.
    pub fn plan(&self) -> Result<crate::schedule::Plan, crate::schedule::ScheduleError> {
        crate::schedule::Plan::parse(&self.schedule, &self.timezone)
    }

    /// The directory a run downloads content items into.
    #[must_use]
    pub fn downloads(&self) -> PathBuf {
        self.staging.join("downloads")
    }

    /// The directory a run stages FHIR resource files in.
    #[must_use]
    pub fn staged_resources(&self) -> PathBuf {
        self.staging.join("resources")
    }

    /// The directory a run keeps a replaced resource file in until the reload
    /// succeeds.
    #[must_use]
    pub fn replaced(&self) -> PathBuf {
        self.staging.join("replaced")
    }

    /// The file holding what the service remembers between runs.
    #[must_use]
    pub fn state_file(&self) -> PathBuf {
        self.state.join("state.json")
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
mod tests {
    use std::collections::BTreeMap;

    use super::{Activation, Config, ConfigError, Duration};

    fn environment(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn an_empty_file_is_the_default_deployment() -> Result<(), ConfigError> {
        let config = Config::from_sources(Some(""), &BTreeMap::new())?;
        // The credentials table holds secrets and so compares by its `Debug`
        // rendering, which redacts them.
        assert_eq!(
            format!("{config:?}"),
            format!("{:?}", Config::default()),
            "every key is optional and falls back to the image layout"
        );
        Ok(())
    }

    #[test]
    fn a_server_admin_table_written_with_only_its_url_keeps_the_reload_defaults()
    -> Result<(), ConfigError> {
        let config = Config::from_sources(
            Some("[server_admin]\nurl = \"http://server:8081\"\n"),
            &BTreeMap::new(),
        )?;
        assert_eq!(
            config.server_admin.timeout,
            Duration::from_mins(30),
            "the reload's 30 min timeout holds under a table that names only its url"
        );
        assert_eq!(
            config.server_admin.retry.max_attempts, 1,
            "the reload is tried once under a table that names only its url"
        );
        assert_eq!(
            config.server_admin.url.expose(),
            "http://server:8081",
            "the url the file writes wins over the default"
        );
        Ok(())
    }

    #[test]
    fn a_file_names_the_directories_the_schedule_and_the_sources()
    -> Result<(), Box<dyn core::error::Error>> {
        let config = Config::from_sources(
            Some(
                r#"
listen = "0.0.0.0:9000"
index_root = "/srv/index"
retention = 4
activation = "manual"
timezone = "Europe/Amsterdam"

[server_admin]
url = "http://server:8081"
timeout = "30min"

[webhook]
url = "https://hooks.example.invalid/sync"

[schedule]
every = "12h"
at = "03:00"

[[source]]
kind = "nts"

[source.config]
base_url = "https://example.invalid"
"#,
            ),
            &BTreeMap::new(),
        )?;
        assert_eq!(
            config.activation,
            Activation::Manual,
            "the file chooses manual activation"
        );
        assert_eq!(config.retention, 4, "the file chooses the retention count");
        assert_eq!(
            config.schedule.at.as_deref(),
            Some("03:00"),
            "the daily time is read"
        );
        assert_eq!(config.source.len(), 1, "one source is configured");
        let Some(source) = config.source.first() else {
            return Err(Box::from("the configured source is missing"));
        };
        assert_eq!(source.kind, "nts", "the add-on kind is read");
        assert_eq!(
            source.config.get("base_url").and_then(toml::Value::as_str),
            Some("https://example.invalid"),
            "the add-on's own block is kept whole"
        );
        config.plan()?;
        Ok(())
    }

    #[test]
    fn an_unknown_key_is_refused() {
        let parsed = Config::from_sources(Some("retetion = 4\n"), &BTreeMap::new());
        assert!(
            matches!(parsed, Err(ConfigError::Load(_))),
            "a misspelled key is a start-up failure, never a silent default"
        );
        let variable =
            Config::from_sources(None, &environment(&[("FERROTERM_SYNC__RETETION", "4")]));
        assert!(
            matches!(variable, Err(ConfigError::Load(_))),
            "a misspelled override is refused too"
        );
    }

    #[test]
    fn an_unknown_key_inside_the_schedule_is_refused() {
        let parsed =
            Config::from_sources(Some("[schedule]\ncron = \"0 3 * * *\"\n"), &BTreeMap::new());
        assert!(parsed.is_err(), "the service takes no cron expression");
    }

    #[test]
    fn an_nts_table_written_in_part_keeps_the_service_and_its_issuer() -> Result<(), ConfigError> {
        let config = Config::from_sources(
            Some(
                "[nts.credentials.oauth]\nclient_id = \"a-client\"\nclient_secret = \"a-client-secret\"\n",
            ),
            &environment(&[("FERROTERM_SYNC__NTS__TIMEOUT", "2h")]),
        )?;
        let endpoint = &config.nts.endpoint;
        assert_eq!(endpoint.url.expose(), addon_nts::config::NTS_BASE_URL);
        assert_eq!(
            endpoint.timeout,
            Duration::from_hours(2),
            "the variable sets the timeout"
        );
        let oauth = endpoint
            .credentials
            .oauth
            .as_ref()
            .expect("the product's defaults name the grant");
        assert_eq!(oauth.client_id, "a-client");
        assert_eq!(
            oauth.issuer,
            addon_nts::config::NTS_ISSUER,
            "the issuer stays the service's"
        );
        let rendered = format!("{:?}", config.nts);
        assert!(!rendered.contains("a-client-secret"), "{rendered}");
        Ok(())
    }

    #[test]
    fn an_nts_key_0_4_5_retired_is_refused_naming_its_replacement() {
        for key in ["client_id", "client_secret", "client_secret_file"] {
            let refused =
                Config::from_sources(Some(&format!("[nts]\n{key} = \"x\"\n")), &BTreeMap::new())
                    .expect_err("a retired key is refused");
            assert!(matches!(refused, ConfigError::Removed(_)), "{refused:?}");
            let shown = refused.to_string();
            assert!(
                shown.contains(&format!("nts.credentials.oauth.{key}")),
                "{shown}"
            );
        }
        assert!(
            Config::from_sources(Some("[nts]\nclient_name = \"x\"\n"), &BTreeMap::new())
                .is_err_and(|error| matches!(error, ConfigError::Load(_))),
            "a key the table does not read is refused as unknown"
        );
    }

    #[test]
    fn the_password_grant_keys_are_refused() {
        for key in [
            "username = \"an-account\"",
            "password = \"a-password\"",
            "password_file = \"/run/secrets/nts\"",
        ] {
            assert!(
                Config::from_sources(Some(&format!("[nts]\n{key}\n")), &BTreeMap::new()).is_err(),
                "the add-on takes no password grant: {key}"
            );
        }
    }

    #[test]
    fn a_schedule_that_does_not_parse_is_refused() {
        for every in ["every other Tuesday", "PT24H", "30", "90m"] {
            assert!(
                Config::from_sources(
                    Some(&format!("[schedule]\nevery = \"{every}\"\n")),
                    &BTreeMap::new()
                )
                .is_err(),
                "an interval that is no duration of a whole number and a UCUM unit is refused at load: {every}"
            );
        }
        let config =
            Config::from_sources(Some("[schedule]\nevery = \"90min\"\n"), &BTreeMap::new())
                .expect("a UCUM duration reads");
        assert_eq!(config.schedule.every, Some(Duration::from_mins(90)));
    }
}
