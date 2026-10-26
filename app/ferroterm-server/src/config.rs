// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Configuration: one TOML tree read by `ferrohealth-config`, a [`Config`]
//! out.
//!
//! The tree's defaults, then the file `--config` or `FERROTERM_CONFIG` names,
//! then every `FERROTERM__SECTION__KEY` variable, then every `--set
//! key=value`. `RUST_LOG` is the alias of `telemetry.filter`. An unknown key
//! or variable is refused with its name. No specification governs the tree:
//! our own design.
//!
//! ```toml
//! [server]
//! listen = "0.0.0.0:8080"
//!
//! [admin]
//! listen = "0.0.0.0:8081"
//!
//! [terminology]
//! index = ["/data/index/snomed"]
//! code_systems = ["/data/codesystems"]
//! resources = "/data/resources.redb"
//! default_language = "en"
//!
//! [fhir]
//! base_url = "https://tx.example.org/fhir"
//! security_services = ["SMART-on-FHIR"]
//!
//! [telemetry]
//! format = "json"
//! filter = "info"
//!
//! [usage_report]
//! enabled = true
//! state_dir = "/data/state"
//! deployment = "compose"
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ferrohealth_config::loader::{Loaded, Loader};
use ferrohealth_smart::config::AuthSection;
use ferrohealth_telemetry::section::TelemetrySection;
use serde::{Deserialize, Serialize};

use ferrohealth_usage_report::section::UsageReportSection;

/// The environment prefix: `FERROTERM__SERVER__LISTEN` sets `[server]
/// listen`, and `FERROTERM_CONFIG` names the file.
pub const PREFIX: &str = "FERROTERM";

/// The variable that names the configuration file when `--config` does not.
pub const CONFIG_ENV: &str = "FERROTERM_CONFIG";

/// The variable that names the `tracing` filter, the alias of
/// `telemetry.filter`.
pub const FILTER_ENV: &str = "RUST_LOG";

/// The variable the build stamps the commit with; the build reads it, the
/// configuration never does.
pub const GIT_SHA_ENV: &str = "FERROTERM_GIT_SHA";

/// The variable the build stamps the release date with
/// (`CapabilityStatement.software.releaseDate`); the build reads it, the
/// configuration never does.
pub const RELEASE_DATE_ENV: &str = "FERROTERM_RELEASE_DATE";

/// The codes of the FHIR `restful-security-service` value set
/// (<http://hl7.org/fhir/ValueSet/restful-security-service>), each with its
/// display, in the code system's own order.
pub const SECURITY_SERVICES: [(&str, &str); 6] = [
    ("OAuth", "OAuth"),
    ("SMART-on-FHIR", "SMART-on-FHIR"),
    ("NTLM", "NTLM"),
    ("Basic", "Basic"),
    ("Kerberos", "Kerberos"),
    ("Certificates", "Certificates"),
];

/// The code system the security service codes come from.
pub const SECURITY_SERVICE_SYSTEM: &str =
    "http://terminology.hl7.org/CodeSystem/restful-security-service";

/// What the server needs to start: the whole tree.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// `[server]`: the FHIR listener.
    pub server: ServerSection,
    /// `[admin]`: the admin listener.
    pub admin: AdminSection,
    /// `[terminology]`: what the server serves.
    pub terminology: TerminologySection,
    /// `[fhir]`: what the capability statements declare.
    pub fhir: FhirSection,
    /// `[telemetry]`: the console log, the request log and the metrics export.
    pub telemetry: TelemetrySection,
    /// `[auth]`: who may write and reach the admin listener.
    pub auth: AuthSection,
    /// `[usage_report]`: the usage report to `FerroPULSE`.
    pub usage_report: UsageReportSection,
}

/// `[server]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerSection {
    /// The socket address the FHIR listener binds.
    pub listen: String,
}

impl Default for ServerSection {
    fn default() -> Self {
        Self {
            listen: String::from("127.0.0.1:8080"),
        }
    }
}

/// `[admin]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AdminSection {
    /// The socket address the admin listener binds; unset or empty means no
    /// admin listener and `SIGHUP` as the only reload trigger.
    ///
    /// The admin listener serves `POST /reload` and nothing else, on an
    /// address of its own so the FHIR surface never carries it. It is a
    /// guarded surface: with it set, the server starts only once `[auth]`
    /// names an issuer or turns authentication off.
    pub listen: Option<String>,
}

/// `[terminology]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TerminologySection {
    /// The artifact directories to load, each one code system version.
    pub index: Vec<PathBuf>,
    /// The directories of FHIR `CodeSystem` resources to load, each a FHIR
    /// package's `package/` directory or a directory of JSON files.
    pub code_systems: Vec<PathBuf>,
    /// The database of persisted client resources; unset means the server
    /// persists none and refuses every write.
    ///
    /// The `CodeSystem`, `ValueSet`, and `ConceptMap` resources written
    /// through the REST API live in this file, with the closure tables
    /// `$closure` maintains. The writes are a guarded surface: with this set,
    /// the server starts only once `[auth]` names an issuer or turns
    /// authentication off.
    pub resources: Option<PathBuf>,
    /// The display language (BCP 47) used when a request names none.
    pub default_language: String,
}

impl Default for TerminologySection {
    fn default() -> Self {
        Self {
            index: Vec::new(),
            code_systems: Vec::new(),
            resources: None,
            default_language: String::from("en"),
        }
    }
}

/// `[fhir]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FhirSection {
    /// The base URL clients reach this server at, without a version prefix;
    /// a trailing slash is dropped and an empty value means unset.
    ///
    /// A server behind a reverse proxy answers on a URL it cannot see, so the
    /// capability statements state this value as `implementation.url`, per
    /// version
    /// (<https://hl7.org/fhir/R4B/capabilitystatement-definitions.html#CapabilityStatement.implementation.url>).
    /// It also widens the audiences the write routes accept: every served
    /// version's base under this URL, which is what a client sends as `aud`
    /// (<https://hl7.org/fhir/smart-app-launch/app-launch.html>), beside the
    /// `[auth] audience` list.
    pub base_url: Option<String>,
    /// The authentication in front of the server, as codes of the FHIR
    /// `restful-security-service` value set (`SMART-on-FHIR`, `OAuth`,
    /// `Basic`, `Certificates`, `Kerberos`, `NTLM`); empty when the
    /// deployment declares none.
    pub security_services: Vec<String>,
}

/// A configuration the server refuses to start on.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The tree does not load: a file that does not read, an unknown key or
    /// variable, or a value of the wrong type.
    #[error(transparent)]
    Load(#[from] ferrohealth_config::error::ConfigError),
    /// `fhir.security_services` names a code the value set does not define.
    #[error(
        "fhir.security_services: `{0}` is not a code of http://hl7.org/fhir/ValueSet/restful-security-service"
    )]
    SecurityService(String),
}

/// The loader of the server's tree.
///
/// # Errors
///
/// Returns the loader's error when the prefix is refused, which it never is
/// for [`PREFIX`].
pub fn loader() -> Result<Loader, ConfigError> {
    Ok(Loader::new(PREFIX)?
        .alias(FILTER_ENV, "telemetry.filter")
        .reserve(GIT_SHA_ENV)
        .reserve(RELEASE_DATE_ENV))
}

impl Config {
    /// Reads the configuration of the running process: the file `path`
    /// names, or the one `FERROTERM_CONFIG` names, the environment, and the
    /// `--set` pairs `sets`, with the product's own checks applied.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Load`] when the tree does not load, and the
    /// errors of [`Config::check`].
    pub fn load(
        path: Option<&Path>,
        sets: &[(String, String)],
    ) -> Result<Loaded<Self>, ConfigError> {
        let loaded = loader()?.load_with::<Self>(path, sets)?;
        loaded.settings().check()?;
        Ok(loaded)
    }

    /// Reads `text` as the file and applies `environment` over it, with the
    /// product's own checks applied.
    ///
    /// Reading the sources and reading the process are separate so a test
    /// states both: setting a variable is `unsafe` in edition 2024.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Load`] when the tree does not load, and the
    /// errors of [`Config::check`].
    pub fn from_sources(
        text: Option<&str>,
        environment: &BTreeMap<String, String>,
    ) -> Result<Self, ConfigError> {
        let loaded = loader()?.load_sources::<Self>(text, None, environment)?;
        let (config, _provenance) = loaded.into_parts();
        config.check()?;
        Ok(config)
    }

    /// Checks what the tree's types cannot: every security service is a
    /// code of its value set.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::SecurityService`] for a code the value set does
    /// not define.
    pub fn check(&self) -> Result<(), ConfigError> {
        services_of(&self.fhir.security_services)?;
        Ok(())
    }

    /// The address the admin listener binds, or `None` when the deployment
    /// names none (unset or empty).
    #[must_use]
    pub fn admin_listen(&self) -> Option<&str> {
        self.admin
            .listen
            .as_deref()
            .map(str::trim)
            .filter(|address| !address.is_empty())
    }

    /// The base URL clients reach this server at, without its trailing
    /// slashes, or `None` when it names nothing.
    ///
    /// A trailing slash is dropped so a caller can join a path without
    /// doubling the separator, and an empty value means unset.
    #[must_use]
    pub fn base_url(&self) -> Option<String> {
        self.fhir.base_url.as_deref().and_then(base_url_of)
    }
}

/// Checks that every entry of `services` is a code of the FHIR
/// `restful-security-service` value set.
///
/// # Errors
///
/// Returns [`ConfigError::SecurityService`] for a code the value set does not
/// define.
fn services_of(services: &[String]) -> Result<(), ConfigError> {
    for name in services {
        if !SECURITY_SERVICES.iter().any(|(code, _)| code == name) {
            return Err(ConfigError::SecurityService(name.clone()));
        }
    }
    Ok(())
}

/// The base URL `value` names, without its trailing slashes, or `None` when it
/// names nothing.
fn base_url_of(value: &str) -> Option<String> {
    let trimmed = value.trim_end_matches('/');
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ferrohealth_smart::config::IssuerKey;
    use ferrohealth_telemetry::section::Format;

    use super::{Config, ConfigError, base_url_of};

    fn environment(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn a_security_service_list_admits_only_the_codes_the_value_set_defines() {
        let read = |value: &str| {
            Config::from_sources(
                None,
                &environment(&[("FERROTERM__FHIR__SECURITY_SERVICES", value)]),
            )
            .map(|config| config.fhir.security_services)
        };
        assert_eq!(
            Config::default().fhir.security_services,
            Vec::<String>::new()
        );
        assert_eq!(read("OAuth").expect("one"), vec![String::from("OAuth")]);
        // Spacing is the operator's, not the value set's.
        assert_eq!(
            read(" OAuth , SMART-on-FHIR").expect("several"),
            vec![String::from("OAuth"), String::from("SMART-on-FHIR")]
        );
        assert!(matches!(
            read("OAuth,Bearer"),
            Err(ConfigError::SecurityService(name)) if name == "Bearer"
        ));
        // The codes are case-sensitive, as the value set defines them.
        assert!(matches!(
            read("oauth"),
            Err(ConfigError::SecurityService(name)) if name == "oauth"
        ));
        assert!(matches!(
            Config::from_sources(Some("[fhir]\nsecurity_services = [\"Bearer\"]\n"), &BTreeMap::new()),
            Err(ConfigError::SecurityService(name)) if name == "Bearer"
        ));
    }

    #[test]
    fn a_base_url_loses_its_trailing_slashes_and_an_empty_one_is_unset() {
        assert_eq!(
            base_url_of("https://tx.example.org/fhir"),
            Some(String::from("https://tx.example.org/fhir"))
        );
        assert_eq!(
            base_url_of("https://tx.example.org/fhir///"),
            Some(String::from("https://tx.example.org/fhir"))
        );
        assert_eq!(base_url_of(""), None);
        assert_eq!(base_url_of("/"), None, "a slash alone names nothing");
        let config = Config::from_sources(
            None,
            &environment(&[("FERROTERM__FHIR__BASE_URL", "https://tx.example.org/fhir/")]),
        )
        .expect("reads");
        assert_eq!(
            config.base_url().as_deref(),
            Some("https://tx.example.org/fhir")
        );
    }

    #[test]
    fn an_empty_admin_address_is_no_admin_listener() {
        assert_eq!(Config::default().admin_listen(), None);
        let empty = Config::from_sources(None, &environment(&[("FERROTERM__ADMIN__LISTEN", "")]))
            .expect("reads");
        assert_eq!(empty.admin_listen(), None);
        let set = Config::from_sources(
            None,
            &environment(&[("FERROTERM__ADMIN__LISTEN", "127.0.0.1:8081")]),
        )
        .expect("reads");
        assert_eq!(set.admin_listen(), Some("127.0.0.1:8081"));
    }

    #[test]
    fn the_file_holds_the_tree_and_the_environment_goes_over_it() {
        let text = "[terminology]\nindex = [\"/a\", \"/b\"]\n\n[auth]\nissuer = \"https://as.example.org\"\naudience = [\"ferroterm\"]\n\n[[auth.basic.users]]\nuser = \"sync\"\npassword_hash_file = \"/run/secrets/sync\"\nscopes = [\"__ferroterm.admin\"]\n";
        let config = Config::from_sources(
            Some(text),
            &environment(&[
                ("FERROTERM__AUTH__AUDIENCE", "a,b"),
                ("FERROTERM__AUTH__PRODUCT_SCOPE", "__ferroterm.reload"),
                ("FERROTERM__TELEMETRY__FORMAT", "json"),
                ("RUST_LOG", "debug"),
            ]),
        )
        .expect("reads");
        assert_eq!(
            config.terminology.index,
            [
                std::path::PathBuf::from("/a"),
                std::path::PathBuf::from("/b")
            ]
        );
        let auth = &config.auth;
        assert!(
            matches!(&auth.issuer, IssuerKey::Flat(issuer) if issuer == "https://as.example.org")
        );
        assert_eq!(
            auth.audience,
            ["a", "b"],
            "the environment wins over the file"
        );
        assert_eq!(auth.product_scope, "__ferroterm.reload");
        let users = auth.basic.as_ref().expect("the Basic table").users.len();
        assert_eq!(users, 1);
        assert!(auth.enabled, "authentication is on unless it is turned off");
        assert_eq!(config.telemetry.format, Format::Json);
        assert_eq!(
            config.telemetry.filter.as_str(),
            "debug",
            "RUST_LOG sets the filter"
        );
    }

    #[test]
    fn no_file_and_no_override_is_the_default_tree() {
        let config = Config::from_sources(None, &BTreeMap::new()).expect("reads");
        assert!(config.auth.enabled, "the default fails closed");
        assert!(matches!(&config.auth.issuer, IssuerKey::Flat(issuer) if issuer.is_empty()));
        assert_eq!(config.server.listen, "127.0.0.1:8080");
        assert_eq!(config.terminology.default_language, "en");
        let off =
            Config::from_sources(None, &environment(&[("FERROTERM__AUTH__ENABLED", "false")]))
                .expect("reads");
        assert!(!off.auth.enabled);
    }

    #[test]
    fn an_unknown_key_or_variable_is_refused() {
        assert!(matches!(
            Config::from_sources(Some("listen = \"x\"\n"), &BTreeMap::new()),
            Err(ConfigError::Load(_))
        ));
        assert!(matches!(
            Config::from_sources(Some("[auth]\nissuers = \"x\"\n"), &BTreeMap::new()),
            Err(ConfigError::Load(_))
        ));
        assert!(matches!(
            Config::from_sources(None, &environment(&[("FERROTERM__AUTH__ISUER", "x")])),
            Err(ConfigError::Load(_))
        ));
        assert!(matches!(
            Config::from_sources(None, &environment(&[("FERROTERM__", "x")])),
            Err(ConfigError::Load(_))
        ));
    }

    #[test]
    fn a_sibling_binary_s_variable_or_a_flat_name_is_left_alone() {
        // The console's and the sync service's overrides share the server's
        // environment, and the flat names of earlier releases are never read.
        for foreign in [
            "FERROTERM_CONSOLE__SERVER__LISTEN",
            "FERROTERM_CONSOLE_CONFIG",
            "FERROTERM_SYNC__RETENTION",
            "FERROTERM_LISTEN",
            "FERROTERM_INDEX",
            "FERROTERM_AUTH_FILE",
            "FERROTERM_AUTH__ISSUER",
            "FERROTERM_USAGE_REPORT",
        ] {
            let config = Config::from_sources(None, &environment(&[(foreign, "x")]))
                .unwrap_or_else(|error| panic!("{foreign}: {error}"));
            assert_eq!(config.server.listen, "127.0.0.1:8080", "{foreign}");
            assert!(
                matches!(&config.auth.issuer, IssuerKey::Flat(issuer) if issuer.is_empty()),
                "{foreign}"
            );
        }
    }

    #[test]
    fn a_usage_report_endpoint_must_be_https_or_loopback_http() {
        let read = |value: &str| {
            Config::from_sources(
                None,
                &environment(&[("FERROTERM__USAGE_REPORT__ENDPOINT", value)]),
            )
        };
        read("https://collector.example.org/v1/report").expect("https");
        read("http://127.0.0.1:9/v1/report").expect("loopback http");
        assert!(matches!(
            read("http://collector.example.org/v1/report"),
            Err(ConfigError::Load(_))
        ));
        assert!(matches!(
            read("https://user:pw@collector.example.org/v1/report"),
            Err(ConfigError::Load(_))
        ));
    }
}
