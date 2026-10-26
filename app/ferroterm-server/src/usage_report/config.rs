// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[usage_report]` section of the configuration.
//!
//! No FHIR or SNOMED CT specification governs this: our own design.

use std::path::PathBuf;

use crate::config::CI_ENVS;

/// The collector every report goes to unless the deployment names another.
pub const ENDPOINT: &str = "https://report.ferropulse.eu/v1/report";

/// How the server was deployed, as the report states it.
///
/// The deployment artifact sets it: the compose files say `compose`, and a
/// Helm chart says `helm`. It is never detected.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
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

/// The usage report configuration of one process, `[usage_report]`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UsageReportConfig {
    /// Whether the deployment left the report on (`usage_report.enabled`).
    pub enabled: bool,
    /// The collector endpoint: `https`, or `http` to a loopback host.
    pub endpoint: String,
    /// The writable directory the instance id lives in; `None` when the
    /// deployment names none, which keeps the report off.
    pub state_dir: Option<PathBuf>,
    /// How the server was deployed.
    pub deployment: Deployment,
    /// The CI variable that is set to `true` or `1`, when one is; the report
    /// is then off whatever `enabled` says. Read from the process, never from
    /// the file.
    #[serde(skip)]
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
    /// The section with the CI detection read through `lookup`, which
    /// answers the value of an environment variable.
    #[must_use]
    pub fn with_ci(mut self, lookup: impl Fn(&str) -> Option<String>) -> Self {
        self.ci = ci_of(lookup);
        self
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
pub(crate) fn endpoint_admitted(url: &str) -> bool {
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

    use super::{Deployment, ENDPOINT, UsageReportConfig, ci_of, endpoint_admitted};
    use crate::config::{Config, ConfigError};

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    fn read(pairs: &[(&str, &str)]) -> Result<UsageReportConfig, ConfigError> {
        let environment = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        Config::from_sources(None, &environment).map(|config| config.usage_report)
    }

    #[test]
    fn the_default_is_on_without_a_state_directory() {
        let config = read(&[]).expect("reads");
        assert_eq!(config, UsageReportConfig::default());
        assert!(config.enabled);
        assert_eq!(config.endpoint, ENDPOINT);
        assert_eq!(config.state_dir, None);
        assert_eq!(config.deployment, Deployment::Unknown);
    }

    #[test]
    fn every_key_is_read() {
        let config = read(&[
            ("FERROTERM__USAGE_REPORT__ENABLED", "false"),
            (
                "FERROTERM__USAGE_REPORT__ENDPOINT",
                "http://127.0.0.1:9/v1/report",
            ),
            ("FERROTERM__USAGE_REPORT__STATE_DIR", "/data/state"),
            ("FERROTERM__USAGE_REPORT__DEPLOYMENT", "compose"),
        ])
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
            read(&[("FERROTERM__USAGE_REPORT__ENABLED", "maybe")]),
            Err(ConfigError::Load(_))
        ));
        for endpoint in [
            "http://report.example/v1",
            "https://user:pw@report.example/v1",
            "ftp://report.example/",
            "not a url",
        ] {
            assert!(!endpoint_admitted(endpoint), "{endpoint}");
            assert!(
                matches!(
                    read(&[("FERROTERM__USAGE_REPORT__ENDPOINT", endpoint)]),
                    Err(ConfigError::UsageReportEndpoint(_))
                ),
                "{endpoint}"
            );
        }
        assert!(matches!(
            read(&[("FERROTERM__USAGE_REPORT__DEPLOYMENT", "k8s")]),
            Err(ConfigError::Load(_))
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
        let config = read(&[("FERROTERM__USAGE_REPORT__ENABLED", "true")])
            .expect("reads")
            .with_ci(lookup(&[("GITHUB_ACTIONS", "true")]));
        assert!(config.enabled);
        assert_eq!(config.ci, Some("GITHUB_ACTIONS"));
    }
}
