//! The one configuration file the service reads.
//!
//! The file is TOML and every key is optional: a deployment writes what it
//! changes and inherits the rest from [`Config::default`], which describes the
//! layout the container image uses. An unknown key is refused rather than
//! ignored, so a typo in a directory name is a start-up failure and not a run
//! that writes somewhere nobody looks.
//!
//! No FHIR or SNOMED CT specification governs the file: our own design.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use ferrohealth_smart::config::CredentialsSection;

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
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScheduleConfig {
    /// An interval measured from the end of the last run, such as `24h`.
    pub every: Option<String>,
    /// A time of day in the configured zone, such as `03:00`.
    pub at: Option<String>,
}

/// One configured source: which add-on speaks to it, and its own settings.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConfig {
    /// The add-on that reads this service, such as `nts`.
    pub kind: String,
    /// The add-on's own configuration block, which the add-on reads.
    #[serde(default)]
    pub config: toml::Table,
}

/// The whole configuration of one running service.
#[derive(Debug, Clone, serde::Deserialize)]
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
            server_admin_credentials: CredentialsSection::default(),
        }
    }
}

/// A configuration that could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The file the service tried to read.
        path: PathBuf,
        /// Why it did not read.
        #[source]
        source: std::io::Error,
    },
    /// The file is not the TOML this service reads.
    #[error("{path} is not a ferroterm-sync configuration")]
    Parse {
        /// The file that did not parse.
        path: PathBuf,
        /// What TOML found wrong with it.
        #[source]
        source: toml::de::Error,
    },
    /// The schedule the file describes does not read.
    #[error("the schedule in {path} does not read")]
    Schedule {
        /// The file the schedule came from.
        path: PathBuf,
        /// Why the schedule does not read.
        #[source]
        source: crate::schedule::ScheduleError,
    },
}

impl Config {
    /// Reads the configuration at `path`, with its schedule checked.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Io`] when the file does not read,
    /// [`ConfigError::Parse`] when it is not the TOML this service reads, and
    /// [`ConfigError::Schedule`] when the schedule it describes does not.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let config: Self = toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        config.plan().map_err(|source| ConfigError::Schedule {
            path: path.to_path_buf(),
            source,
        })?;
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
    use super::{Activation, Config};

    #[test]
    fn an_empty_file_is_the_default_deployment() -> Result<(), toml::de::Error> {
        let config: Config = toml::from_str("")?;
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
        let config: Config = toml::from_str(
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
        let parsed = toml::from_str::<Config>("retetion = 4\n");
        assert!(
            parsed.is_err(),
            "a misspelled key is a start-up failure, never a silent default"
        );
    }

    #[test]
    fn an_unknown_key_inside_the_schedule_is_refused() {
        let parsed = toml::from_str::<Config>("[schedule]\ncron = \"0 3 * * *\"\n");
        assert!(parsed.is_err(), "the service takes no cron expression");
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
