// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The closure tables `ConceptMap/$closure` maintains, persisted beside the
//! client resources so they outlive a restart.

use redb::ReadableDatabase;

use super::{CLOSURES, ResourceStore, StoreError};

/// One closure table: the concepts a client has registered and the version
/// the server has counted up to.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Closure {
    /// The name the client gave the table.
    pub name: String,
    /// `ConceptMap.version`, raised by every call that changes the table.
    pub version: u32,
    /// What the loaded code systems were when the table was initialised, so a
    /// table built over other content is refused rather than trusted.
    #[serde(default)]
    pub edition: Vec<String>,
    /// Every concept in the table, in the order it was registered.
    pub members: Vec<ClosureMember>,
    /// Every relationship the server has told the client about, each with the
    /// version at which it was told, so a resynchronisation can replay it.
    pub edges: Vec<ClosureEdge>,
}

/// One concept of a closure table.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClosureMember {
    /// The code system URI.
    pub system: String,
    /// The code system version, when the client pinned one.
    pub version: Option<String>,
    /// The code.
    pub code: String,
}

/// One relationship of a closure table.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClosureEdge {
    /// The table version this relationship was first reported at.
    pub version: u32,
    /// The concept the relationship is stated from.
    pub source: ClosureMember,
    /// The concept it is stated to.
    pub target: ClosureMember,
    /// The relationship code, as the R4 equivalence vocabulary spells it.
    pub relationship: String,
}

impl ResourceStore {
    /// The closure table named `name`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store cannot be read, and
    /// [`StoreError::Record`] when the table does not parse.
    pub fn closure(&self, name: &str) -> Result<Option<Closure>, StoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(CLOSURES)?;
        let Some(value) = table.get(name)? else {
            return Ok(None);
        };
        let held = serde_json::from_str(value.value()).map_err(|source| StoreError::Record {
            key: name.to_owned(),
            source,
        })?;
        Ok(Some(held))
    }

    /// Writes `closure`, replacing any table of the same name.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the write does not commit.
    pub fn put_closure(&self, closure: &Closure) -> Result<(), StoreError> {
        let value = serde_json::to_string(closure).map_err(|source| StoreError::Record {
            key: closure.name.clone(),
            source,
        })?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(CLOSURES)?;
            table.insert(closure.name.as_str(), value.as_str())?;
        }
        write.commit()?;
        Ok(())
    }
}
