// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The service's configuration tree, read by `ferrohealth-config`.
//!
//! The tree's defaults, then the TOML file `--config` or
//! `FERROTERM_SYNC_CONFIG` names, then every `FERROTERM_SYNC__SECTION__KEY`
//! variable, then every `--set key=value`. Every key is optional: a
//! deployment writes what it changes and inherits the rest from
//! [`Config::default`], which describes the layout the container image uses.
//! An unknown key or variable is refused rather than ignored, so a typo in a
//! directory name is a start-up failure and not a run that writes somewhere
//! nobody looks.
//!
//! No FHIR or SNOMED CT specification governs the tree: our own design.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use ferrohealth_config::loader::{Loaded, Loader};
use ferrohealth_secret::file::{SecretError, read_secret};
use ferrohealth_secret::secret::Secret;
use ferrohealth_smart::config::{AuthSection, CredentialsSection};

/// The environment prefix: `FERROTERM_SYNC__SCHEDULE__EVERY` sets
/// `[schedule] every`, and `FERROTERM_SYNC_CONFIG` names the file.
pub const PREFIX: &str = "FERROTERM_SYNC";

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
    /// An interval measured from the end of the last run, such as `24h`.
    pub every: Option<String>,
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

/// `[nts]`: the account the Nationale Terminologie Server add-on
/// authenticates with.
///
/// Each secret is set inline or through its `<key>_file` sibling, which names
/// a mounted file, never both.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct NtsSection {
    /// The OAuth 2 client identifier.
    pub client_id: String,
    /// The client secret, when the deployment has one.
    pub client_secret: Option<Secret>,
    /// A file holding the client secret.
    pub client_secret_file: Option<PathBuf>,
    /// The account name for the password grant.
    pub username: Option<String>,
    /// The account password for the password grant.
    pub password: Option<Secret>,
    /// A file holding the account password.
    pub password_file: Option<PathBuf>,
}

impl Default for NtsSection {
    fn default() -> Self {
        Self {
            client_id: String::from(addon_nts::credentials::DEFAULT_CLIENT_ID),
            client_secret: None,
            client_secret_file: None,
            username: None,
            password: None,
            password_file: None,
        }
    }
}

impl NtsSection {
    /// The account this section names, every secret read inline or from its
    /// file.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError`] when a secret is set both inline and through
    /// its file, or its file cannot be read or is empty.
    pub fn credentials(&self) -> Result<addon_nts::credentials::Credentials, SecretError> {
        Ok(addon_nts::credentials::Credentials {
            client_id: self.client_id.clone(),
            client_secret: read_secret(
                "nts.client_secret",
                self.client_secret.clone(),
                self.client_secret_file.as_deref(),
            )?,
            username: self.username.clone(),
            password: read_secret(
                "nts.password",
                self.password.clone(),
                self.password_file.as_deref(),
            )?,
        })
    }
}

/// The whole configuration of one running service.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The address the service's own admin listener binds.
    pub listen: SocketAddr,
    /// The base address of the server's admin listener, which serves
    /// `POST /reload`.
    pub server_admin_url: String,
    /// The FHIR base of the server, such as `http://127.0.0.1:8080/r4b`.
    ///
    /// It is what the revalidation reads after a release is activated. With
    /// no address the check does not run and the run record says so.
    pub fhir_base_url: Option<String>,
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
    /// The address one JSON summary per run is posted to.
    pub webhook_url: Option<String>,
    /// The time zone a daily schedule is read in, an IANA zone name.
    pub timezone: String,
    /// The offline build binary, found on `PATH` when it is a bare name.
    pub build_command: PathBuf,
    /// When a run starts by itself.
    pub schedule: ScheduleConfig,
    /// The credential the reload call to the server's admin listener carries:
    /// HTTP Basic with `user` and `password`, or the client credentials grant
    /// under `oauth`; none when the table is empty.
    pub server_admin_credentials: CredentialsSection,
    /// The credentials the admin listener admits: the issuers whose access
    /// tokens it accepts, the Basic users, and the product scope every route
    /// but `GET /health` requires ([`crate::auth`]).
    pub auth: AuthSection,
    /// The account the Nationale Terminologie Server add-on authenticates
    /// with.
    pub nts: NtsSection,
    /// The sources a run reads, in the order they are configured.
    pub source: Vec<SourceConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8181),
            server_admin_url: String::from("http://127.0.0.1:8081"),
            fhir_base_url: None,
            index_root: PathBuf::from("/data/index"),
            resources: PathBuf::from("/data/codesystems"),
            staging: PathBuf::from("/data/staging"),
            records: PathBuf::from("/data/records"),
            state: PathBuf::from("/data/state"),
            retention: 2,
            activation: Activation::Auto,
            webhook_url: None,
            timezone: String::from("UTC"),
            build_command: PathBuf::from("ferroterm-build"),
            schedule: ScheduleConfig::default(),
            source: Vec::new(),
            nts: NtsSection::default(),
            server_admin_credentials: CredentialsSection::default(),
            auth: AuthSection::default(),
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
/// # Errors
///
/// Returns the loader's error when the prefix is refused, which it never is
/// for [`PREFIX`].
pub fn loader() -> Result<Loader, ConfigError> {
    Ok(Loader::new(PREFIX)?)
}

impl Config {
    /// Reads the configuration of the running process: the file `path`
    /// names, or the one `FERROTERM_SYNC_CONFIG` names, the environment and
    /// the `--set` pairs `sets`, with its schedule checked.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Load`] when the tree does not load, and
    /// [`ConfigError::Schedule`] when the schedule it describes does not.
    pub fn load(
        path: Option<&Path>,
        sets: &[(String, String)],
    ) -> Result<Loaded<Self>, ConfigError> {
        let loaded = loader()?.load_with::<Self>(path, sets)?;
        loaded
            .settings()
            .plan()
            .map_err(|source| ConfigError::Schedule { source })?;
        Ok(loaded)
    }

    /// Reads `text` as the file and applies `environment` over it, with its
    /// schedule checked.
    ///
    /// Reading the sources and reading the process are separate so a test
    /// states both: setting a variable is `unsafe` in edition 2024.
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
        config
            .plan()
            .map_err(|source| ConfigError::Schedule { source })?;
        Ok(config)
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

    use super::{Activation, Config, ConfigError, Secret};

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
    fn a_file_names_the_directories_the_schedule_and_the_sources()
    -> Result<(), Box<dyn core::error::Error>> {
        let config = Config::from_sources(
            Some(
                r#"
listen = "0.0.0.0:9000"
server_admin_url = "http://server:8081"
index_root = "/srv/index"
retention = 4
activation = "manual"
webhook_url = "https://hooks.example.invalid/sync"
timezone = "Europe/Amsterdam"

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
    fn the_nts_account_reads_inline_or_from_a_file_never_both()
    -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("password");
        std::fs::write(&path, "a-password\n")?;
        let config = Config::from_sources(
            Some("[nts]\nusername = \"an-account\"\n"),
            &environment(&[(
                "FERROTERM_SYNC__NTS__PASSWORD_FILE",
                path.to_str().ok_or("a UTF-8 path")?,
            )]),
        )?;
        let credentials = config.nts.credentials()?;
        assert_eq!(credentials.client_id, "cli_client");
        assert_eq!(
            credentials.password.as_ref().map(Secret::expose),
            Some("a-password"),
            "the trailing line ending of the file is not part of the secret"
        );
        assert!(credentials.has_password_grant());
        let both = Config::from_sources(
            Some("[nts]\npassword = \"inline\"\n"),
            &environment(&[(
                "FERROTERM_SYNC__NTS__PASSWORD_FILE",
                path.to_str().ok_or("a UTF-8 path")?,
            )]),
        )?;
        assert!(
            both.nts.credentials().is_err(),
            "a secret set inline and through its file is refused"
        );
        let rendered = format!("{:?}", config.nts);
        assert!(!rendered.contains("a-password"), "{rendered}");
        Ok(())
    }

    #[test]
    fn a_schedule_that_does_not_parse_is_refused() {
        let config = Config {
            schedule: super::ScheduleConfig {
                every: Some(String::from("every other Tuesday")),
                at: None,
            },
            ..Config::default()
        };
        assert!(
            config.plan().is_err(),
            "an interval that does not read is refused before the first run"
        );
    }
}
