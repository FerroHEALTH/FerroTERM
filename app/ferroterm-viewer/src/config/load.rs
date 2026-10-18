// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading the configuration tree: the TOML file, then every
//! `FERROTERM_VIEWER__` environment override applied over it. No
//! specification governs the configuration: our own design.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::error::{Error, ParseFault, Stage};
use crate::config::{CONFIG_PATH_ENV, Config, ENV_PREFIX};

impl Config {
    /// Reads the configuration file `path` names, then the environment.
    ///
    /// `path` is what `--config` named; without it the file is the one
    /// `FERROTERM_VIEWER_CONFIG` names, and without that the defaults
    /// stand.
    ///
    /// # Errors
    /// Returns [`Error::Read`] when a named file cannot be read and every
    /// other [`Error`] [`Config::from_sources`] produces.
    pub fn load(path: Option<&Path>) -> Result<Self, Error> {
        let named = path
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os(CONFIG_PATH_ENV).map(PathBuf::from));
        let text = match &named {
            None => None,
            Some(path) => Some(std::fs::read_to_string(path).map_err(|source| Error::Read {
                path: path.clone(),
                source,
            })?),
        };
        let environment: BTreeMap<String, String> = std::env::vars()
            .filter(|(name, _)| name.starts_with(ENV_PREFIX))
            .collect();
        Self::from_sources(text.as_deref(), &environment)
    }

    /// Reads `text` as TOML and applies `environment` over it.
    ///
    /// Reading the sources and reading the process environment are separate
    /// so a test can state both: setting an environment variable is `unsafe`
    /// in edition 2024 and this workspace forbids `unsafe`.
    ///
    /// # Errors
    /// Returns [`Error::Parse`] for TOML that does not read as this tree,
    /// [`Error::EnvName`] and [`Error::EnvShape`] for an override that names
    /// no key, and [`Error::Assemble`] when the merged tree cannot be written.
    pub fn from_sources(
        text: Option<&str>,
        environment: &BTreeMap<String, String>,
    ) -> Result<Self, Error> {
        let mut table = match text {
            None => toml::Table::new(),
            Some(text) => toml::from_str(text).map_err(|error| Error::Parse {
                fault: ParseFault::from_toml(&error, Some(text), Stage::File),
            })?,
        };
        for (name, raw) in environment {
            apply_override(&mut table, name, raw)?;
        }
        let merged = toml::to_string(&table).map_err(|source| Error::Assemble { source })?;
        toml::from_str(&merged).map_err(|error| {
            // NOTE: no specification governs this: our own design; a fault the
            // file itself carries is reported at its line, which the merged tree has not.
            let file = text.and_then(|text| {
                toml::from_str::<Self>(text)
                    .err()
                    .map(|error| ParseFault::from_toml(&error, Some(text), Stage::File))
            });
            Error::Parse {
                fault: file.unwrap_or_else(|| ParseFault::from_toml(&error, None, Stage::Merged)),
            }
        })
    }
}

/// Applies one environment override onto `table`.
fn apply_override(table: &mut toml::Table, name: &str, raw: &str) -> Result<(), Error> {
    let Some(path) = name.strip_prefix(ENV_PREFIX) else {
        return Ok(());
    };
    let segments: Vec<String> = path.split("__").map(str::to_ascii_lowercase).collect();
    let Some((key, parents)) = segments.split_last() else {
        return Err(Error::EnvName {
            name: name.to_owned(),
        });
    };
    if key.is_empty() || parents.is_empty() || parents.iter().any(String::is_empty) {
        return Err(Error::EnvName {
            name: name.to_owned(),
        });
    }
    let mut cursor = table;
    for parent in parents {
        let entry = cursor
            .entry(parent.clone())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let toml::Value::Table(next) = entry else {
            return Err(Error::EnvShape {
                name: name.to_owned(),
            });
        };
        cursor = next;
    }
    cursor.insert(key.clone(), env_value(raw));
    Ok(())
}

/// Reads `raw` as a TOML value, or as the string it is.
fn env_value(raw: &str) -> toml::Value {
    // NOTE: no specification governs this: our own design. A parse failure IS
    // the answer here, because text that is not TOML syntax is a plain string.
    let parsed = toml::from_str::<toml::Table>(&format!("value = {raw}"))
        .ok()
        .and_then(|table| table.get("value").cloned());
    parsed.unwrap_or_else(|| toml::Value::String(raw.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{apply_override, env_value};
    use crate::config::error::Error;

    #[test]
    fn an_environment_value_reads_as_toml_syntax_or_as_the_string_it_is() {
        assert_eq!(toml::Value::Integer(5), env_value("5"));
        assert_eq!(toml::Value::Boolean(false), env_value("false"));
        assert_eq!(
            toml::Value::String(String::from("0.0.0.0:3000")),
            env_value("0.0.0.0:3000")
        );
    }

    #[test]
    fn an_override_name_without_a_section_and_a_key_is_refused() {
        let mut table = toml::Table::new();
        let error = apply_override(&mut table, "FERROTERM_VIEWER__LISTEN", "x")
            .expect_err("a section and a key are both required");
        assert!(matches!(error, Error::EnvName { .. }), "{error:?}");
    }

    #[test]
    fn an_override_into_a_value_that_is_no_table_is_refused() {
        let mut table = toml::Table::new();
        table.insert(String::from("server"), toml::Value::Integer(1));
        let error = apply_override(&mut table, "FERROTERM_VIEWER__SERVER__LISTEN", "x")
            .expect_err("a key under a value is refused");
        assert!(matches!(error, Error::EnvShape { .. }), "{error:?}");
    }
}
