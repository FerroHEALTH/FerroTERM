//! Configuration: the environment in, a [`Config`] out.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ferrohealth_smart::config::AuthSection;

use crate::telemetry::{FILTER_ENV, FORMAT_ENV, FormatError, LogFormat};
use crate::usage_report::config::UsageReportConfig;

/// The environment variable naming the socket address to listen on.
pub const LISTEN_ENV: &str = "FERROTERM_LISTEN";
/// The environment variable naming the socket address the admin listener binds.
///
/// The admin listener serves `POST /reload` and nothing else, on an address of
/// its own so the FHIR surface never carries it. Unset means no admin listener
/// and `SIGHUP` as the only reload trigger. It is a guarded surface: with it
/// set, the server starts only once `[auth]` names an issuer or turns
/// authentication off.
pub const ADMIN_LISTEN_ENV: &str = "FERROTERM_ADMIN_LISTEN";
/// The environment variable listing the artifact directories to serve,
/// separated by the platform's path separator (`:` on Unix).
pub const INDEX_ENV: &str = "FERROTERM_INDEX";
/// The environment variable naming the default display language (BCP 47).
pub const LANGUAGE_ENV: &str = "FERROTERM_DEFAULT_LANGUAGE";
/// The environment variable listing the `CodeSystem` resource directories.
///
/// Each is a FHIR package's `package/` directory or a directory of JSON files;
/// the platform's path separator separates them.
pub const CODESYSTEMS_ENV: &str = "FERROTERM_CODESYSTEMS";
/// The environment variable naming the database of persisted client resources.
///
/// The `CodeSystem`, `ValueSet`, and `ConceptMap` resources written through the
/// REST API live in this file, with the closure tables `$closure` maintains,
/// and are served again after a restart. A deployment that names none refuses
/// every write. The writes are a guarded surface: with this set, the server
/// starts only once `[auth]` names an issuer or turns authentication off.
pub const RESOURCES_ENV: &str = "FERROTERM_RESOURCES";
/// The environment variable naming the authentication in front of the server.
///
/// Its value is codes of the FHIR `restful-security-service` value set
/// (`SMART-on-FHIR`, `OAuth`, `Basic`, `Certificates`, `Kerberos`, `NTLM`),
/// separated by commas.
pub const SECURITY_SERVICE_ENV: &str = "FERROTERM_SECURITY_SERVICE";
/// The environment variable naming the base URL clients reach this server at.
///
/// A server behind a reverse proxy answers on a URL it cannot see: the proxy
/// terminates TLS and forwards a plain request, so the address the process
/// bound is not the address a client used. The capability statements state
/// this value as `implementation.url`, per version, so a client that reads one
/// learns where to send the next request
/// (<https://hl7.org/fhir/R4B/capabilitystatement-definitions.html#CapabilityStatement.implementation.url>).
///
/// It also widens the audiences the write routes accept: every served
/// version's base under this URL, which is what a client sends as `aud`
/// (<https://hl7.org/fhir/smart-app-launch/app-launch.html>), beside the
/// `[auth] audience` list.
pub const BASE_URL_ENV: &str = "FERROTERM_BASE_URL";
/// The environment variable naming the TOML file that holds the `[auth]`
/// table.
///
/// The table is `ferrohealth-smart`'s `[auth]` section: the issuer or the
/// `[[auth.issuer]]` entries, the audiences, the token rules, the product
/// scope and the `[[auth.basic.users]]`. A file holds that one table and
/// nothing else, and every `FERROTERM_AUTH__<KEY>` variable is applied over
/// it. No specification governs the file: our own design.
pub const AUTH_FILE_ENV: &str = "FERROTERM_AUTH_FILE";
/// The prefix of an environment override of one `[auth]` key.
///
/// The rest of the name is the key's path below `[auth]`, its segments
/// separated by `__`: `FERROTERM_AUTH__ISSUER` for `issuer`,
/// `FERROTERM_AUTH__AUDIENCE` for `audience`. The value reads as TOML syntax
/// (`false`, `["a", "b"]`) or, when it is none, as the string it is.
pub const AUTH_ENV_PREFIX: &str = "FERROTERM_AUTH__";
/// The environment variable switching the usage report to `FerroPULSE` on or
/// off.
///
/// It reads `on` or `off` (`true`/`false`, `1`/`0`, and `yes`/`no` are taken
/// too, in any case), and is on when unset. Off always
/// wins, and so does a CI environment ([`CI_ENVS`]): the switch can only turn
/// the report off.
pub const USAGE_REPORT_ENV: &str = "FERROTERM_USAGE_REPORT";
/// The environment variable naming the collector the usage report is posted to.
pub const USAGE_REPORT_ENDPOINT_ENV: &str = "FERROTERM_USAGE_REPORT_ENDPOINT";
/// The environment variable naming the writable directory the server keeps its
/// own state in: the usage report's instance id and the time of its last start
/// report.
pub const STATE_DIR_ENV: &str = "FERROTERM_STATE_DIR";
/// The environment variable naming how the server was deployed, as the usage
/// report states it: `helm`, `compose`, `binary`, or `unknown`.
pub const DEPLOYMENT_ENV: &str = "FERROTERM_DEPLOYMENT";
/// The environment variables a CI runner sets, in the order they are read.
///
/// Either one set to `true` or `1`, in any case, switches the usage report
/// off for the process.
pub const CI_ENVS: [&str; 2] = ["GITHUB_ACTIONS", "CI"];

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

/// What the server needs to start.
#[derive(Debug, Clone)]
pub struct Config {
    /// The socket address to bind.
    pub listen: String,
    /// The socket address the admin listener binds; `None` when the deployment
    /// names none and the server serves no admin surface.
    pub admin_listen: Option<String>,
    /// The artifact directories to load, each one code system version.
    pub index: Vec<PathBuf>,
    /// The directories of FHIR `CodeSystem` resources to load.
    pub code_systems: Vec<PathBuf>,
    /// The database of persisted client resources; `None` when the deployment
    /// persists none.
    pub resources: Option<PathBuf>,
    /// The display language used when a request names none.
    pub default_language: String,
    /// The console log format.
    pub log_format: LogFormat,
    /// The `tracing` filter.
    pub log_filter: String,
    /// The authentication in front of the server, as codes of the FHIR
    /// `restful-security-service` value set; empty when the deployment
    /// declares none.
    pub security_services: Vec<String>,
    /// The base URL clients reach this server at, without a version prefix
    /// and without a trailing slash; `None` when the deployment names none.
    pub base_url: Option<String>,
    /// The `[auth]` table: who may write and reach the admin listener.
    pub auth: AuthSection,
    /// The usage report to `FerroPULSE`.
    pub usage_report: UsageReportConfig,
}

/// A configuration value that does not parse.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// `FERROTERM_LOG_FORMAT` names no format.
    #[error("{FORMAT_ENV}: {0}")]
    LogFormat(#[from] FormatError),
    /// `FERROTERM_SECURITY_SERVICE` names a code the value set does not define.
    #[error(
        "{SECURITY_SERVICE_ENV}: `{0}` is not a code of http://hl7.org/fhir/ValueSet/restful-security-service"
    )]
    SecurityService(String),
    /// `FERROTERM_USAGE_REPORT` names neither `on` nor `off`.
    #[error("{USAGE_REPORT_ENV}: `{0}` is neither `on` nor `off`")]
    UsageReport(String),
    /// `FERROTERM_USAGE_REPORT_ENDPOINT` is not an `https` URL without
    /// credentials, or plain `http` to a loopback host.
    #[error(
        "{USAGE_REPORT_ENDPOINT_ENV}: `{0}` is not an https URL without credentials (plain http is accepted for a loopback host only)"
    )]
    UsageReportEndpoint(String),
    /// `FERROTERM_DEPLOYMENT` names no deployment kind.
    #[error("{DEPLOYMENT_ENV}: `{0}` is none of `helm`, `compose`, `binary`, `unknown`")]
    Deployment(String),
    /// The file `FERROTERM_AUTH_FILE` names cannot be read.
    #[error("{AUTH_FILE_ENV}: {} cannot be read", path.display())]
    AuthFile {
        /// The file.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// The `[auth]` table, the file with the overrides applied, does not read
    /// as the section.
    #[error("[auth] does not read as the auth section")]
    Auth {
        /// What the TOML reader reported.
        #[source]
        source: Box<toml::de::Error>,
    },
    /// The file holds a key outside the `[auth]` table.
    #[error("{AUTH_FILE_ENV}: `{0}` is not the [auth] table, the one table the file holds")]
    AuthKey(String),
    /// An override names no key below `[auth]`, or a key that is no table.
    #[error("`{0}` names no key below [auth]")]
    AuthOverride(String),
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: String::from("127.0.0.1:8080"),
            admin_listen: None,
            index: Vec::new(),
            code_systems: Vec::new(),
            resources: None,
            default_language: String::from("en"),
            log_format: LogFormat::Auto,
            log_filter: String::from(crate::telemetry::DEFAULT_FILTER),
            security_services: Vec::new(),
            base_url: None,
            auth: AuthSection::default(),
            usage_report: UsageReportConfig::default(),
        }
    }
}

impl Config {
    /// Reads the configuration from the environment; an unset variable keeps
    /// the default.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when a variable holds a value that does not parse.
    pub fn from_env() -> Result<Self, ConfigError> {
        let defaults = Self::default();
        Ok(Self {
            listen: std::env::var(LISTEN_ENV).unwrap_or(defaults.listen),
            admin_listen: std::env::var(ADMIN_LISTEN_ENV)
                .ok()
                .filter(|address| !address.trim().is_empty()),
            index: std::env::var_os(INDEX_ENV)
                .map(|value| std::env::split_paths(&value).collect())
                .unwrap_or_default(),
            code_systems: std::env::var_os(CODESYSTEMS_ENV)
                .map(|value| std::env::split_paths(&value).collect())
                .unwrap_or_default(),
            resources: std::env::var_os(RESOURCES_ENV).map(PathBuf::from),
            default_language: std::env::var(LANGUAGE_ENV).unwrap_or(defaults.default_language),
            log_format: match std::env::var(FORMAT_ENV) {
                Ok(text) => text.parse()?,
                Err(_) => defaults.log_format,
            },
            log_filter: std::env::var(FILTER_ENV).unwrap_or(defaults.log_filter),
            security_services: security_services()?,
            base_url: std::env::var(BASE_URL_ENV)
                .ok()
                .and_then(|url| base_url_of(&url)),
            auth: auth_from_env()?,
            usage_report: UsageReportConfig::from_lookup(|name| std::env::var(name).ok())?,
        })
    }
}

/// The `[auth]` table of the file `FERROTERM_AUTH_FILE` names, with every
/// `FERROTERM_AUTH__` variable applied over it.
///
/// # Errors
///
/// Returns [`ConfigError::AuthFile`] when the named file cannot be read and
/// every error [`auth_of`] returns.
fn auth_from_env() -> Result<AuthSection, ConfigError> {
    let text = match std::env::var_os(AUTH_FILE_ENV) {
        None => None,
        Some(path) => Some(read_auth_file(Path::new(&path))?),
    };
    let overrides: BTreeMap<String, String> = std::env::vars()
        .filter(|(name, _)| name.starts_with(AUTH_ENV_PREFIX))
        .collect();
    auth_of(text.as_deref(), &overrides)
}

/// The text of the auth file at `path`.
fn read_auth_file(path: &Path) -> Result<String, ConfigError> {
    std::fs::read_to_string(path).map_err(|source| ConfigError::AuthFile {
        path: path.to_path_buf(),
        source,
    })
}

/// The `[auth]` table `text` holds, with `overrides` applied over it.
///
/// Reading the sources and reading the process environment are separate so
/// a test can state both: setting a variable needs `std::env::set_var`, which
/// is unsafe in edition 2024 and this workspace forbids unsafe.
///
/// # Errors
///
/// Returns [`ConfigError::AuthKey`] for a file key outside `[auth]`,
/// [`ConfigError::AuthOverride`] for an override that names no key below it,
/// and [`ConfigError::Auth`] for a table that does not read as the section.
pub fn auth_of(
    text: Option<&str>,
    overrides: &BTreeMap<String, String>,
) -> Result<AuthSection, ConfigError> {
    let mut file = match text {
        None => toml::Table::new(),
        Some(text) => toml::from_str(text).map_err(|source| ConfigError::Auth {
            source: Box::new(source),
        })?,
    };
    let mut table = match file.remove("auth") {
        None => toml::Table::new(),
        Some(toml::Value::Table(table)) => table,
        Some(_) => return Err(ConfigError::AuthKey(String::from("auth"))),
    };
    if let Some(key) = file.keys().next() {
        return Err(ConfigError::AuthKey(key.clone()));
    }
    for (name, raw) in overrides {
        apply_override(&mut table, name, raw)?;
    }
    toml::Value::Table(table)
        .try_into()
        .map_err(|source| ConfigError::Auth {
            source: Box::new(source),
        })
}

/// Applies the override `name` = `raw` onto the `[auth]` `table`.
fn apply_override(table: &mut toml::Table, name: &str, raw: &str) -> Result<(), ConfigError> {
    let refused = || ConfigError::AuthOverride(name.to_owned());
    let path = name.strip_prefix(AUTH_ENV_PREFIX).ok_or_else(refused)?;
    let segments: Vec<String> = path.split("__").map(str::to_ascii_lowercase).collect();
    let Some((key, parents)) = segments.split_last() else {
        return Err(refused());
    };
    if key.is_empty() || parents.iter().any(String::is_empty) {
        return Err(refused());
    }
    let mut cursor = table;
    for parent in parents {
        let entry = cursor
            .entry(parent.clone())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let toml::Value::Table(next) = entry else {
            return Err(refused());
        };
        cursor = next;
    }
    cursor.insert(key.clone(), override_value(raw));
    Ok(())
}

/// `raw` read as a TOML value, or as the string it is.
fn override_value(raw: &str) -> toml::Value {
    // NOTE: no specification governs this: our own design, the console's rule;
    // text that is no TOML value is a plain string, so the failed parse is the answer.
    toml::from_str::<toml::Table>(&format!("value = {raw}"))
        .ok()
        .and_then(|mut table| table.remove("value"))
        .unwrap_or_else(|| toml::Value::String(raw.to_owned()))
}

/// The security services `FERROTERM_SECURITY_SERVICE` names, each a code of
/// the FHIR `restful-security-service` value set.
///
/// # Errors
///
/// Returns [`ConfigError::SecurityService`] for a code the value set does not
/// define.
fn security_services() -> Result<Vec<String>, ConfigError> {
    let Ok(value) = std::env::var(SECURITY_SERVICE_ENV) else {
        return Ok(Vec::new());
    };
    services_of(&value)
}

/// The security services `value` lists, comma-separated.
///
/// Reading the variable and reading its value are separate so the second can
/// be tested: setting a variable needs `std::env::set_var`, which is unsafe in
/// edition 2024 and this workspace forbids unsafe.
///
/// # Errors
///
/// Returns [`ConfigError::SecurityService`] for a code the value set does not
/// define.
fn services_of(value: &str) -> Result<Vec<String>, ConfigError> {
    let mut out = Vec::new();
    for name in value.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        let Some((code, _)) = SECURITY_SERVICES.iter().find(|(code, _)| *code == name) else {
            return Err(ConfigError::SecurityService(name.to_owned()));
        };
        out.push((*code).to_owned());
    }
    Ok(out)
}

/// The state `value` names, or `None` when it names neither.
pub(crate) fn switch_of(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" => Some(true),
        "off" | "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

/// The base URL `value` names, without its trailing slashes, or `None` when it
/// names nothing.
///
/// A trailing slash is dropped so a caller can join a path without doubling
/// the separator, and a variable set to the empty string means unset.
fn base_url_of(value: &str) -> Option<String> {
    let trimmed = value.trim_end_matches('/');
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ferrohealth_smart::config::IssuerKey;

    use super::{ConfigError, auth_of, base_url_of, services_of, switch_of};

    fn overrides(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn a_security_service_list_admits_only_the_codes_the_value_set_defines() {
        assert_eq!(services_of("").expect("empty"), Vec::<String>::new());
        assert_eq!(
            services_of("OAuth").expect("one"),
            vec![String::from("OAuth")]
        );
        // Spacing and empty entries are the operator's, not the value set's.
        assert_eq!(
            services_of(" OAuth , SMART-on-FHIR ,,").expect("several"),
            vec![String::from("OAuth"), String::from("SMART-on-FHIR")]
        );
        assert!(matches!(
            services_of("OAuth,Bearer"),
            Err(ConfigError::SecurityService(name)) if name == "Bearer"
        ));
        // The codes are case-sensitive, as the value set defines them.
        assert!(matches!(
            services_of("oauth"),
            Err(ConfigError::SecurityService(name)) if name == "oauth"
        ));
    }

    #[test]
    fn an_on_off_switch_reads_both_states_and_refuses_anything_else() {
        for on in ["on", "ON", " true ", "1", "yes"] {
            assert_eq!(switch_of(on), Some(true), "{on}");
        }
        for off in ["off", "OFF", "false", "0", "no"] {
            assert_eq!(switch_of(off), Some(false), "{off}");
        }
        for neither in ["", "onn", "maybe", "2"] {
            assert_eq!(switch_of(neither), None, "{neither}");
        }
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
    }

    #[test]
    fn the_auth_file_holds_the_auth_table_and_the_environment_goes_over_it() {
        let text = "[auth]\nissuer = \"https://as.example.org\"\naudience = [\"ferroterm\"]\n\n[[auth.basic.users]]\nuser = \"sync\"\npassword_hash_file = \"/run/secrets/sync\"\nscopes = [\"__ferroterm.admin\"]\n";
        let auth = auth_of(
            Some(text),
            &overrides(&[
                ("FERROTERM_AUTH__AUDIENCE", "[\"a\", \"b\"]"),
                ("FERROTERM_AUTH__PRODUCT_SCOPE", "__ferroterm.reload"),
            ]),
        )
        .expect("reads");
        assert!(
            matches!(&auth.issuer, IssuerKey::Flat(issuer) if issuer == "https://as.example.org")
        );
        assert_eq!(
            auth.audience,
            ["a", "b"],
            "the environment wins over the file"
        );
        assert_eq!(auth.product_scope, "__ferroterm.reload");
        let users = auth.basic.expect("the Basic table").users;
        assert_eq!(users.len(), 1);
        assert!(auth.enabled, "authentication is on unless it is turned off");
    }

    #[test]
    fn no_file_and_no_override_is_the_default_section() {
        let auth = auth_of(None, &BTreeMap::new()).expect("reads");
        assert!(auth.enabled, "the default fails closed");
        assert!(matches!(&auth.issuer, IssuerKey::Flat(issuer) if issuer.is_empty()));
        let off =
            auth_of(None, &overrides(&[("FERROTERM_AUTH__ENABLED", "false")])).expect("reads");
        assert!(!off.enabled);
    }

    #[test]
    fn a_key_outside_the_auth_table_or_unknown_inside_it_is_refused() {
        assert!(matches!(
            auth_of(Some("listen = \"x\"\n"), &BTreeMap::new()),
            Err(ConfigError::AuthKey(key)) if key == "listen"
        ));
        assert!(matches!(
            auth_of(Some("[auth]\nissuers = \"x\"\n"), &BTreeMap::new()),
            Err(ConfigError::Auth { .. })
        ));
        assert!(matches!(
            auth_of(None, &overrides(&[("FERROTERM_AUTH__ISUER", "x")])),
            Err(ConfigError::Auth { .. })
        ));
        assert!(matches!(
            auth_of(None, &overrides(&[("FERROTERM_AUTH__", "x")])),
            Err(ConfigError::AuthOverride(_))
        ));
        assert!(matches!(
            auth_of(None, &overrides(&[("FERROTERM_AUTH__REALM__X", "x"), ("FERROTERM_AUTH__REALM", "y")])),
            Err(ConfigError::AuthOverride(name)) if name == "FERROTERM_AUTH__REALM__X"
        ));
    }
}
