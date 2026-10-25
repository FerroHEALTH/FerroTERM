// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where each configuration key took its value from: the file, an
//! environment override, or neither, so the console's own default.
//!
//! The Configuration screen marks a value that does not come from the file
//! with FerroUI's `ValueSource`. No specification governs the configuration:
//! our own design.

use std::collections::BTreeSet;

/// Where one key took its value from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The configuration file.
    File,
    /// A `FERROTERM_CONSOLE__` environment override, which wins over the file.
    Environment,
    /// Nothing sets it: the console's default.
    Default,
}

/// The dotted keys the file and the environment set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Origins {
    /// Every key the file sets, as `section.key`.
    file: BTreeSet<String>,
    /// Every key an environment override sets, as `section.key`.
    environment: BTreeSet<String>,
}

impl Origins {
    /// The origins of a file that sets `file` and an environment that sets
    /// `environment`, each a set of dotted keys.
    #[must_use]
    pub fn new(file: BTreeSet<String>, environment: BTreeSet<String>) -> Self {
        Self { file, environment }
    }

    /// Where `key` took its value from. A secret set through its `_file`
    /// key counts as set, either way round.
    #[must_use]
    pub fn of(&self, key: &str) -> Origin {
        let names = [
            key.to_owned(),
            format!("{key}_file"),
            key.strip_suffix("_file").unwrap_or(key).to_owned(),
        ];
        if names.iter().any(|name| self.environment.contains(name)) {
            Origin::Environment
        } else if names.iter().any(|name| self.file.contains(name)) {
            Origin::File
        } else {
            Origin::Default
        }
    }
}

/// Every leaf key of `table` as a dotted path under `prefix`, into `keys`.
pub(crate) fn leaves(table: &toml::Table, prefix: &str, keys: &mut BTreeSet<String>) {
    for (name, value) in table {
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };
        match value {
            toml::Value::Table(inner) => leaves(inner, &path, keys),
            _ => {
                keys.insert(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{Origin, Origins, leaves};

    #[test]
    fn the_environment_wins_over_the_file_and_nothing_set_is_the_default() {
        let file: BTreeSet<String> = ["ferroterm.base_url", "oidc.client_secret_file"]
            .into_iter()
            .map(String::from)
            .collect();
        let environment: BTreeSet<String> = ["ferroterm.base_url", "session.secure_cookie"]
            .into_iter()
            .map(String::from)
            .collect();
        let origins = Origins::new(file, environment);
        assert_eq!(Origin::Environment, origins.of("ferroterm.base_url"));
        assert_eq!(Origin::Environment, origins.of("session.secure_cookie"));
        assert_eq!(Origin::File, origins.of("oidc.client_secret_file"));
        assert_eq!(Origin::File, origins.of("oidc.client_secret"));
        assert_eq!(Origin::Default, origins.of("ferroterm.timeout_ms"));
    }

    #[test]
    fn a_table_s_leaves_are_dotted() {
        let table: toml::Table =
            toml::from_str("[ferroterm]\nbase_url = \"x\"\n[oidc]\nscopes = [\"a\"]\n")
                .expect("TOML");
        let mut keys = BTreeSet::new();
        leaves(&table, "", &mut keys);
        assert_eq!(
            vec!["ferroterm.base_url", "oidc.scopes"],
            keys.iter().map(String::as_str).collect::<Vec<_>>()
        );
    }
}
