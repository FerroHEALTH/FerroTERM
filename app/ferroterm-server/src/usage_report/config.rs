// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The environment the usage report reads.
//!
//! No FHIR or SNOMED CT specification governs this: our own design.

use std::path::PathBuf;

use crate::config::{
    CI_ENVS, ConfigError, DEPLOYMENT_ENV, STATE_DIR_ENV, USAGE_REPORT_ENDPOINT_ENV,
    USAGE_REPORT_ENV, switch_of,
};

/// The collector every report goes to unless the deployment names another.
pub const ENDPOINT: &str = "https://report.ferropulse.eu/v1/report";

/// How the server was deployed, as the report states it.
///
/// The deployment artifact sets it: the compose files say `compose`, and a
/// Helm chart says `helm`. It is never detected.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Deployment {
    /// A Helm chart.
    Helm,
    /// A Docker Compose file.
    Compose,
    /// The plain binary.
    Binary,
    /// Not stated.
    #[default]
    Unknown,
}

impl Deployment {
    /// Returns the deployment `value` names, or `None` when it names none.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "helm" => Some(Self::Helm),
            "compose" => Some(Self::Compose),
            "binary" => Some(Self::Binary),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// The usage report configuration of one process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageReportConfig {
    /// Whether the deployment left the report on (`FERROTERM_USAGE_REPORT`).
    pub enabled: bool,
    /// The collector endpoint: `https`, or `http` to a loopback host.
    pub endpoint: String,
    /// The writable directory the instance id lives in; `None` when the
    /// deployment names none, which keeps the report off.
    pub state_dir: Option<PathBuf>,
    /// How the server was deployed.
    pub deployment: Deployment,
    /// The CI variable that is set to `true` or `1`, when one is; the report
    /// is then off whatever `enabled` says.
    pub ci: Option<&'static str>,
}

impl Default for UsageReportConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            endpoint: ENDPOINT.to_owned(),
            state_dir: None,
            deployment: Deployment::Unknown,
            ci: None,
        }
    }
}

impl UsageReportConfig {
    /// Reads the configuration through `lookup`, which answers the value of an
    /// environment variable; an unset variable keeps the default.
    ///
    /// Reading through a function is what lets a test supply the environment:
    /// setting a variable needs `std::env::set_var`, which is unsafe in
    /// edition 2024, and this workspace forbids unsafe.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::UsageReport`] for a switch that names neither
    /// state, [`ConfigError::UsageReportEndpoint`] for an endpoint that is not
    /// `https` (or `http` to a loopback host) or carries credentials, and
    /// [`ConfigError::Deployment`] for an unknown deployment kind.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let defaults = Self::default();
        let enabled = match lookup(USAGE_REPORT_ENV) {
            Some(value) => switch_of(&value).ok_or(ConfigError::UsageReport(value))?,
            None => defaults.enabled,
        };
        let endpoint = match lookup(USAGE_REPORT_ENDPOINT_ENV) {
            Some(value) if !value.trim().is_empty() => {
                let value = value.trim().to_owned();
                if !endpoint_admitted(&value) {
                    return Err(ConfigError::UsageReportEndpoint(value));
                }
                value
            }
            _ => defaults.endpoint,
        };
        let deployment = match lookup(DEPLOYMENT_ENV) {
            Some(value) if !value.trim().is_empty() => {
                Deployment::parse(&value).ok_or(ConfigError::Deployment(value))?
            }
            _ => defaults.deployment,
        };
        Ok(Self {
            enabled,
            endpoint,
            state_dir: lookup(STATE_DIR_ENV)
                .filter(|dir| !dir.trim().is_empty())
                .map(PathBuf::from),
            deployment,
            ci: ci_of(&lookup),
        })
    }
}

/// The first CI variable `lookup` answers `true` or `1` for, in any case.
///
/// A CI runner sets one of them for every step it runs, so a server it starts
/// would otherwise count as an installation.
#[must_use]
pub fn ci_of(lookup: impl Fn(&str) -> Option<String>) -> Option<&'static str> {
    CI_ENVS.into_iter().find(|name| {
        lookup(name).is_some_and(|value| {
            let value = value.trim();
            value.eq_ignore_ascii_case("true") || value == "1"
        })
    })
}

/// Whether the report may be posted to `url`: `https`, or plain `http` to a
/// loopback host, and no credentials in the authority.
fn endpoint_admitted(url: &str) -> bool {
    let authority = url.split_once("://").map_or("", |(_, rest)| {
        rest.split(['/', '?', '#']).next().unwrap_or(rest)
    });
    !authority.is_empty() && !authority.contains('@') && secure(url)
}

/// Whether `url` may be fetched: `https`, or plain HTTP on the loopback host.
///
/// A loopback collector is the one place the transport cannot be observed,
/// and a test collector is that (no specification governs the exception: our
/// own design).
pub(crate) fn secure(url: &str) -> bool {
    if url.starts_with("https://") {
        return true;
    }
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    // An IPv6 literal is bracketed, so its colons are not the port separator
    // (RFC 3986 §3.2.2).
    let host = match authority.split_once(']') {
        Some((bracketed, _)) => bracketed.strip_prefix('[').unwrap_or(bracketed),
        None => authority.split(':').next().unwrap_or(authority),
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{Deployment, ENDPOINT, UsageReportConfig, ci_of};
    use crate::config::ConfigError;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn the_default_is_on_without_a_state_directory() {
        let config = UsageReportConfig::from_lookup(lookup(&[])).expect("reads");
        assert_eq!(config, UsageReportConfig::default());
        assert!(config.enabled);
        assert_eq!(config.endpoint, ENDPOINT);
        assert_eq!(config.state_dir, None);
        assert_eq!(config.deployment, Deployment::Unknown);
    }

    #[test]
    fn every_variable_is_read() {
        let config = UsageReportConfig::from_lookup(lookup(&[
            ("FERROTERM_USAGE_REPORT", "off"),
            (
                "FERROTERM_USAGE_REPORT_ENDPOINT",
                "http://127.0.0.1:9/v1/report",
            ),
            ("FERROTERM_STATE_DIR", "/data/state"),
            ("FERROTERM_DEPLOYMENT", "compose"),
        ]))
        .expect("reads");
        assert!(!config.enabled);
        assert_eq!(config.endpoint, "http://127.0.0.1:9/v1/report");
        assert_eq!(
            config.state_dir.as_deref(),
            Some(std::path::Path::new("/data/state"))
        );
        assert_eq!(config.deployment, Deployment::Compose);
    }

    #[test]
    fn a_bad_switch_endpoint_or_deployment_refuses_to_start() {
        assert!(matches!(
            UsageReportConfig::from_lookup(lookup(&[("FERROTERM_USAGE_REPORT", "maybe")])),
            Err(ConfigError::UsageReport(value)) if value == "maybe"
        ));
        for endpoint in [
            "http://report.example/v1",
            "https://user:pw@report.example/v1",
            "ftp://report.example/",
            "not a url",
        ] {
            assert!(
                matches!(
                    UsageReportConfig::from_lookup(lookup(&[(
                        "FERROTERM_USAGE_REPORT_ENDPOINT",
                        endpoint
                    )])),
                    Err(ConfigError::UsageReportEndpoint(_))
                ),
                "{endpoint}"
            );
        }
        assert!(matches!(
            UsageReportConfig::from_lookup(lookup(&[("FERROTERM_DEPLOYMENT", "k8s")])),
            Err(ConfigError::Deployment(value)) if value == "k8s"
        ));
    }

    #[test]
    fn ci_is_detected_from_either_variable_and_true_or_one_only() {
        assert_eq!(ci_of(lookup(&[])), None);
        assert_eq!(
            ci_of(lookup(&[("GITHUB_ACTIONS", "true")])),
            Some("GITHUB_ACTIONS")
        );
        assert_eq!(
            ci_of(lookup(&[("GITHUB_ACTIONS", "TRUE")])),
            Some("GITHUB_ACTIONS")
        );
        assert_eq!(ci_of(lookup(&[("CI", "1")])), Some("CI"));
        assert_eq!(ci_of(lookup(&[("CI", "True")])), Some("CI"));
        assert_eq!(
            ci_of(lookup(&[("CI", "true"), ("GITHUB_ACTIONS", "true")])),
            Some("GITHUB_ACTIONS"),
            "GITHUB_ACTIONS is read first"
        );
        for no in ["false", "0", "", "yes"] {
            assert_eq!(
                ci_of(lookup(&[("CI", no), ("GITHUB_ACTIONS", no)])),
                None,
                "{no}"
            );
        }
    }

    #[test]
    fn an_explicit_on_keeps_the_ci_flag() {
        let config = UsageReportConfig::from_lookup(lookup(&[
            ("FERROTERM_USAGE_REPORT", "on"),
            ("GITHUB_ACTIONS", "true"),
        ]))
        .expect("reads");
        assert!(config.enabled);
        assert_eq!(config.ci, Some("GITHUB_ACTIONS"));
    }
}
