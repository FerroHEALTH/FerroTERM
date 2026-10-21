// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! The client resources a deployment persists: `CodeSystem`, `ValueSet`, and
//! `ConceptMap` written through the REST API, in a `redb` database beside the
//! indexes.
//!
//! No FHIR specification governs the storage; this layout is our own design.
//! Each record keeps the resource as the client sent it, with the FHIR version
//! it arrived in, so a read renders it in whatever version asks and an
//! operation converts it exactly as it converts a resource from disk.

use std::path::{Path, PathBuf};

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

/// The current resources, keyed `<type>/<id>`, each a JSON [`Record`].
const RESOURCES: TableDefinition<'_, &str, &str> = TableDefinition::new("resources");

/// Every version of every resource, keyed `<type>/<id>/<versionId>`.
///
/// A version read answers from here, so the history of a resource outlives the
/// delete of its current version
/// (<https://hl7.org/fhir/R4B/http.html#vread>).
const HISTORY: TableDefinition<'_, &str, &str> = TableDefinition::new("history");

/// The closure tables a client maintains, keyed by name.
///
/// `ConceptMap/$closure` names a table and adds concepts to it over time
/// (<https://hl7.org/fhir/R4B/conceptmap-operation-closure.html>); the table
/// lives beside the persisted resources so it outlives a restart.
const CLOSURES: TableDefinition<'_, &str, &str> = TableDefinition::new("closures");

/// The interaction that made each version, keyed `<type>/<id>/<versionId>`
/// like [`HISTORY`], a delete included.
///
/// A delete adds a version and a history entry
/// (<https://hl7.org/fhir/R4B/http.html#history>), and it leaves no resource
/// in [`HISTORY`], so this table is where it is recorded. A version written
/// before this table existed has no row here.
const EVENTS: TableDefinition<'_, &str, &str> = TableDefinition::new("events");

/// A resource type the server persists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourceType {
    /// `CodeSystem`.
    CodeSystem,
    /// `ValueSet`.
    ValueSet,
    /// `ConceptMap`.
    ConceptMap,
}

impl ResourceType {
    /// The type as the FHIR `resourceType` element spells it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CodeSystem => "CodeSystem",
            Self::ValueSet => "ValueSet",
            Self::ConceptMap => "ConceptMap",
        }
    }

    /// The type a `resourceType` names, when the server persists it.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "CodeSystem" => Some(Self::CodeSystem),
            "ValueSet" => Some(Self::ValueSet),
            "ConceptMap" => Some(Self::ConceptMap),
            _ => None,
        }
    }
}

/// One persisted resource.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Record {
    /// The resource type.
    pub resource_type: String,
    /// The logical id.
    pub id: String,
    /// `url`, when the resource carries one.
    pub url: Option<String>,
    /// `version`, when the resource carries one.
    pub version: Option<String>,
    /// The FHIR version the resource arrived in (`4.0.1`, `4.3.0`, ...).
    pub fhir_version: String,
    /// `meta.versionId`, counted from `1` and raised by every update.
    pub version_id: u32,
    /// `meta.lastModified`, an instant in the FHIR `instant` form.
    pub last_modified: String,
    /// The resource as the client sent it, with `id` and `meta` set.
    pub resource: fhir_types::codec::Object,
}

impl Record {
    /// The `ETag` of the record's current version, the FHIR weak form
    /// (<https://hl7.org/fhir/R4B/http.html#concurrency>).
    #[must_use]
    pub fn etag(&self) -> String {
        format!("W/\"{}\"", self.version_id)
    }
}

/// The HTTP method of the interaction that made one version of a resource,
/// which a history entry states in `entry.request.method`
/// (<https://hl7.org/fhir/R4B/http.html#history>).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    /// A create with a server-assigned id.
    Post,
    /// An update, or a create at an id the client chose.
    Put,
    /// A delete.
    Delete,
}

impl Method {
    /// The method as HTTP spells it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }
}

/// The interaction recorded for one version: its method and when it ran.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Event {
    /// The HTTP method.
    method: Method,
    /// When the interaction ran, an instant in the FHIR `instant` form.
    at: String,
}

/// One version in the history of a resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    /// The logical id.
    pub id: String,
    /// The version the interaction made.
    pub version_id: u32,
    /// The HTTP method of the interaction.
    pub method: Method,
    /// Whether the version began the resource: the first one, or the first
    /// after a delete.
    pub created: bool,
    /// When the interaction ran, an instant in the FHIR `instant` form.
    pub last_modified: String,
    /// The resource as the version holds it; `None` for a delete.
    pub record: Option<Record>,
}

impl HistoryEntry {
    /// The `ETag` of the version, the FHIR weak form
    /// (<https://hl7.org/fhir/R4B/http.html#concurrency>).
    #[must_use]
    pub fn etag(&self) -> String {
        format!("W/\"{}\"", self.version_id)
    }
}

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

/// A failure of the persisted store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The database cannot be opened or created.
    #[error("cannot open the resource database at {path}")]
    Open {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: Box<redb::DatabaseError>,
    },
    /// A transaction failed.
    #[error("the resource store transaction failed")]
    Transaction(#[from] redb::TransactionError),
    /// A table cannot be opened.
    #[error("the resource table cannot be opened")]
    Table(#[from] redb::TableError),
    /// A read or write failed.
    #[error("the resource store failed")]
    Storage(#[from] redb::StorageError),
    /// A commit failed.
    #[error("the resource store commit failed")]
    Commit(#[from] redb::CommitError),
    /// A recorded write has no stored version.
    #[error("the store records a write at {key} and holds no version there")]
    Missing {
        /// The history key.
        key: String,
    },
    /// A stored record is not the JSON this build writes.
    #[error("the record at {key} does not read")]
    Record {
        /// The key.
        key: String,
        /// The cause.
        #[source]
        source: serde_json::Error,
    },
}

/// The persisted resources of one deployment.
#[derive(Debug)]
pub struct ResourceStore {
    database: Database,
    path: PathBuf,
}

impl ResourceStore {
    /// Opens the database at `path`, creating it when it does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Open`] when the file cannot be opened or created.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let database = Database::create(path).map_err(|source| StoreError::Open {
            path: path.to_path_buf(),
            source: Box::new(source),
        })?;
        // The tables exist from the first open, so a read of an empty store is
        // not a missing-table error.
        let write = database.begin_write()?;
        {
            let resources = write.open_table(RESOURCES)?;
            let history = write.open_table(HISTORY)?;
            let closures = write.open_table(CLOSURES)?;
            let events = write.open_table(EVENTS)?;
            drop(resources);
            drop(history);
            drop(closures);
            drop(events);
        }
        write.commit()?;
        Ok(Self {
            database,
            path: path.to_path_buf(),
        })
    }

    /// The path the store lives at.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every record, sorted by key.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store cannot be read, and
    /// [`StoreError::Record`] when a record does not parse.
    pub fn all(&self) -> Result<Vec<Record>, StoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(RESOURCES)?;
        let mut out = Vec::new();
        for entry in table.iter()? {
            let (key, value) = entry?;
            let record =
                serde_json::from_str(value.value()).map_err(|source| StoreError::Record {
                    key: key.value().to_owned(),
                    source,
                })?;
            out.push(record);
        }
        Ok(out)
    }

    /// The record of `resource_type` with `id`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store cannot be read.
    pub fn get(&self, resource_type: ResourceType, id: &str) -> Result<Option<Record>, StoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(RESOURCES)?;
        let key = key_of(resource_type, id);
        let Some(value) = table.get(key.as_str())? else {
            return Ok(None);
        };
        let record = serde_json::from_str(value.value())
            .map_err(|source| StoreError::Record { key, source })?;
        Ok(Some(record))
    }

    /// The record of `resource_type` with `id` as of `version_id`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store cannot be read.
    pub fn version(
        &self,
        resource_type: ResourceType,
        id: &str,
        version_id: u32,
    ) -> Result<Option<Record>, StoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(HISTORY)?;
        let key = version_key_of(resource_type, id, version_id);
        let Some(value) = table.get(key.as_str())? else {
            return Ok(None);
        };
        let record = serde_json::from_str(value.value())
            .map_err(|source| StoreError::Record { key, source })?;
        Ok(Some(record))
    }

    /// Writes `record` as an update, replacing any record of the same type and
    /// id and keeping the written version in the history.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the write does not commit.
    pub fn put(&self, record: &Record) -> Result<(), StoreError> {
        self.put_by(record, Method::Put)
    }

    /// Writes `record` as the interaction `method` made it, replacing any
    /// record of the same type and id and keeping the written version and the
    /// interaction in the history.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the write does not commit.
    pub fn put_by(&self, record: &Record, method: Method) -> Result<(), StoreError> {
        let Some(resource_type) = ResourceType::parse(&record.resource_type) else {
            return Ok(());
        };
        let key = key_of(resource_type, &record.id);
        let value = serde_json::to_string(record).map_err(|source| StoreError::Record {
            key: key.clone(),
            source,
        })?;
        let versioned = version_key_of(resource_type, &record.id, record.version_id);
        let event = event_json(
            &versioned,
            &Event {
                method,
                at: record.last_modified.clone(),
            },
        )?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(RESOURCES)?;
            table.insert(key.as_str(), value.as_str())?;
            let mut history = write.open_table(HISTORY)?;
            history.insert(versioned.as_str(), value.as_str())?;
            let mut events = write.open_table(EVENTS)?;
            events.insert(versioned.as_str(), event.as_str())?;
        }
        write.commit()?;
        Ok(())
    }

    /// The highest version of `resource_type` with `id` the store has
    /// counted, a delete included; `None` for a resource never written.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store cannot be read.
    pub fn latest_version(
        &self,
        resource_type: ResourceType,
        id: &str,
    ) -> Result<Option<u32>, StoreError> {
        let read = self.database.begin_read()?;
        let prefix = format!("{}/{id}/", resource_type.name());
        let mut latest = None;
        for table in [HISTORY, EVENTS] {
            let table = read.open_table(table)?;
            for (_, version_id) in versions_under(&table, &prefix)? {
                latest = latest.max(Some(version_id));
            }
        }
        Ok(latest)
    }

    /// Every version of the resources of `resource_type`, or of the one with
    /// `id` when one is given, ordered by id and then by version.
    ///
    /// A version written before interactions were recorded states `POST`
    /// when it began the resource and `PUT` otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store cannot be read,
    /// [`StoreError::Record`] when a stored version does not parse, and
    /// [`StoreError::Missing`] when a recorded write has no stored version.
    pub fn history(
        &self,
        resource_type: ResourceType,
        id: Option<&str>,
    ) -> Result<Vec<HistoryEntry>, StoreError> {
        let read = self.database.begin_read()?;
        let prefix = match id {
            Some(id) => format!("{}/{id}/", resource_type.name()),
            None => format!("{}/", resource_type.name()),
        };
        let mut records = std::collections::BTreeMap::new();
        let history = read.open_table(HISTORY)?;
        for (key, value) in rows_under(&history, &prefix)? {
            let Some(at) = version_of(&key) else {
                continue;
            };
            let record: Record = serde_json::from_str(&value)
                .map_err(|source| StoreError::Record { key, source })?;
            records.insert(at, record);
        }
        let mut events = std::collections::BTreeMap::new();
        let recorded = read.open_table(EVENTS)?;
        for (key, value) in rows_under(&recorded, &prefix)? {
            let Some(at) = version_of(&key) else {
                continue;
            };
            let event: Event = serde_json::from_str(&value)
                .map_err(|source| StoreError::Record { key, source })?;
            events.insert(at, event);
        }
        let mut keys: Vec<(String, u32)> = records.keys().chain(events.keys()).cloned().collect();
        keys.sort();
        keys.dedup();
        let mut out: Vec<HistoryEntry> = Vec::with_capacity(keys.len());
        for key in keys {
            let begins = out
                .last()
                .is_none_or(|last| last.id != key.0 || last.method == Method::Delete);
            let event = events.remove(&key);
            let entry = match (records.remove(&key), event) {
                (Some(record), event) => HistoryEntry {
                    id: key.0,
                    version_id: key.1,
                    method: event
                        .map_or(if begins { Method::Post } else { Method::Put }, |event| {
                            event.method
                        }),
                    created: begins,
                    last_modified: record.last_modified.clone(),
                    record: Some(record),
                },
                (None, Some(event)) if event.method == Method::Delete => HistoryEntry {
                    id: key.0,
                    version_id: key.1,
                    method: Method::Delete,
                    created: false,
                    last_modified: event.at,
                    record: None,
                },
                (None, _) => {
                    return Err(StoreError::Missing {
                        key: version_key_of(resource_type, &key.0, key.1),
                    });
                }
            };
            out.push(entry);
        }
        Ok(out)
    }

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

    /// Removes the record of `resource_type` with `id`; `false` when there was
    /// none.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the delete does not commit.
    pub fn delete(&self, resource_type: ResourceType, id: &str) -> Result<bool, StoreError> {
        let key = key_of(resource_type, id);
        let write = self.database.begin_write()?;
        let removed;
        {
            let mut table = write.open_table(RESOURCES)?;
            let held = table
                .remove(key.as_str())?
                .map(|value| value.value().to_owned());
            removed = held.is_some();
            if let Some(value) = held {
                let record: Record =
                    serde_json::from_str(&value).map_err(|source| StoreError::Record {
                        key: key.clone(),
                        source,
                    })?;
                // NOTE: a delete makes a version of its own, which the history lists
                // (<https://hl7.org/fhir/R4B/http.html#history>).
                let versioned =
                    version_key_of(resource_type, id, record.version_id.saturating_add(1));
                let event = event_json(
                    &versioned,
                    &Event {
                        method: Method::Delete,
                        at: fhir_terminology::clock::now().to_string(),
                    },
                )?;
                let mut events = write.open_table(EVENTS)?;
                events.insert(versioned.as_str(), event.as_str())?;
            }
        }
        write.commit()?;
        Ok(removed)
    }

    /// Whether version `version_id` of `resource_type` with `id` is a delete.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the store cannot be read, and
    /// [`StoreError::Record`] when the recorded interaction does not parse.
    pub fn is_delete(
        &self,
        resource_type: ResourceType,
        id: &str,
        version_id: u32,
    ) -> Result<bool, StoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(EVENTS)?;
        let key = version_key_of(resource_type, id, version_id);
        let Some(value) = table.get(key.as_str())? else {
            return Ok(false);
        };
        let event: Event = serde_json::from_str(value.value())
            .map_err(|source| StoreError::Record { key, source })?;
        Ok(event.method == Method::Delete)
    }
}

/// `event` as the JSON [`EVENTS`] stores under `key`.
fn event_json(key: &str, event: &Event) -> Result<String, StoreError> {
    serde_json::to_string(event).map_err(|source| StoreError::Record {
        key: key.to_owned(),
        source,
    })
}

/// Every row of `table` whose key starts with `prefix`, in key order.
fn rows_under(
    table: &impl ReadableTable<&'static str, &'static str>,
    prefix: &str,
) -> Result<Vec<(String, String)>, StoreError> {
    let mut out = Vec::new();
    for entry in table.range(prefix..)? {
        let (key, value) = entry?;
        if !key.value().starts_with(prefix) {
            break;
        }
        out.push((key.value().to_owned(), value.value().to_owned()));
    }
    Ok(out)
}

/// The id and version of every row of `table` under `prefix`.
fn versions_under(
    table: &impl ReadableTable<&'static str, &'static str>,
    prefix: &str,
) -> Result<Vec<(String, u32)>, StoreError> {
    Ok(rows_under(table, prefix)?
        .iter()
        .filter_map(|(key, _)| version_of(key))
        .collect())
}

/// The id and version a `<type>/<id>/<versionId>` key names.
///
/// A logical id carries no `/` (<https://hl7.org/fhir/R4B/datatypes.html#id>),
/// so the key splits into exactly three parts.
fn version_of(key: &str) -> Option<(String, u32)> {
    let mut parts = key.splitn(3, '/');
    let (_, id, version) = (parts.next()?, parts.next()?, parts.next()?);
    // NOTE: every key this store writes has the three parts, so a key without them
    // is no version row and is passed over; no FHIR/SNOMED spec governs this: our own design.
    Some((id.to_owned(), version.parse().ok()?))
}

/// The store key of a resource: `<type>/<id>`.
fn key_of(resource_type: ResourceType, id: &str) -> String {
    format!("{}/{id}", resource_type.name())
}

/// The history key of one version of a resource: `<type>/<id>/<versionId>`.
fn version_key_of(resource_type: ResourceType, id: &str, version_id: u32) -> String {
    format!("{}/{id}/{version_id}", resource_type.name())
}

#[cfg(test)]
mod tests {
    use super::{Method, Record, ResourceStore, ResourceType};

    fn record(id: &str, version_id: u32) -> Record {
        Record {
            resource_type: String::from("ValueSet"),
            id: id.to_owned(),
            url: Some(format!("http://example.org/{id}")),
            version: Some(String::from("1.0")),
            fhir_version: String::from("4.3.0"),
            version_id,
            last_modified: String::from("2026-09-04T00:00:00Z"),
            resource: [
                (
                    String::from("resourceType"),
                    fhir_types::codec::Value::String(String::from("ValueSet")),
                ),
                (
                    String::from("id"),
                    fhir_types::codec::Value::String(String::from(id)),
                ),
            ]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn a_record_survives_a_reopen_and_an_update_replaces_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("resources.redb");
        {
            let store = ResourceStore::open(&path).expect("opens");
            assert_eq!(store.all().expect("reads"), Vec::<Record>::new());
            store.put(&record("pets", 1)).expect("writes");
            store.put(&record("colours", 1)).expect("writes");
        }
        let store = ResourceStore::open(&path).expect("reopens");
        let all = store.all().expect("reads");
        assert_eq!(all.len(), 2, "both survive the reopen");
        let stored = store
            .get(ResourceType::ValueSet, "pets")
            .expect("reads")
            .expect("pets");
        assert_eq!(stored.version_id, 1);
        assert_eq!(stored.etag(), "W/\"1\"");

        store.put(&record("pets", 2)).expect("updates");
        let stored = store
            .get(ResourceType::ValueSet, "pets")
            .expect("reads")
            .expect("pets");
        assert_eq!(stored.version_id, 2, "the update replaces the record");
        assert_eq!(store.all().expect("reads").len(), 2, "and adds none");

        assert!(
            store
                .delete(ResourceType::ValueSet, "pets")
                .expect("deletes")
        );
        assert!(
            !store
                .delete(ResourceType::ValueSet, "pets")
                .expect("deletes"),
            "deleting twice is not an error"
        );
        assert_eq!(store.all().expect("reads").len(), 1);
        assert!(
            store
                .get(ResourceType::ValueSet, "pets")
                .expect("reads")
                .is_none()
        );
        let first = store
            .version(ResourceType::ValueSet, "pets", 1)
            .expect("reads")
            .expect("the first version");
        assert_eq!(
            first.version_id, 1,
            "a delete leaves the history of the resource"
        );
        assert!(
            store
                .version(ResourceType::ValueSet, "pets", 3)
                .expect("reads")
                .is_none(),
            "a version that was never written is absent"
        );
    }

    #[test]
    fn a_history_lists_writes_and_deletes_and_counts_on_after_a_delete() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ResourceStore::open(&dir.path().join("resources.redb")).expect("opens");
        store
            .put_by(&record("pets", 1), Method::Post)
            .expect("writes");
        store.put(&record("pets", 2)).expect("updates");
        store.put(&record("pets-2", 1)).expect("writes another");
        assert!(
            store
                .delete(ResourceType::ValueSet, "pets")
                .expect("deletes")
        );
        assert_eq!(
            store
                .latest_version(ResourceType::ValueSet, "pets")
                .expect("reads"),
            Some(3),
            "the delete is the third version"
        );
        assert!(
            store
                .is_delete(ResourceType::ValueSet, "pets", 3)
                .expect("reads")
        );

        let listed: Vec<(String, u32, Method, bool, bool)> = store
            .history(ResourceType::ValueSet, Some("pets"))
            .expect("reads")
            .into_iter()
            .map(|entry| {
                (
                    entry.id,
                    entry.version_id,
                    entry.method,
                    entry.created,
                    entry.record.is_some(),
                )
            })
            .collect();
        assert_eq!(
            listed,
            [
                (String::from("pets"), 1, Method::Post, true, true),
                (String::from("pets"), 2, Method::Put, false, true),
                (String::from("pets"), 3, Method::Delete, false, false),
            ],
            "the prefix of one id does not take in `pets-2`"
        );
        assert_eq!(
            store
                .history(ResourceType::ValueSet, None)
                .expect("reads")
                .len(),
            4,
            "the type level lists every resource"
        );
    }

    #[test]
    fn a_version_written_before_interactions_were_recorded_states_its_method() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = ResourceStore::open(&dir.path().join("resources.redb")).expect("opens");
        let write = store.database.begin_write().expect("begins");
        {
            let mut history = write.open_table(super::HISTORY).expect("opens");
            for version_id in [1, 2] {
                let value = serde_json::to_string(&record("old", version_id)).expect("encodes");
                let key = format!("ValueSet/old/{version_id}");
                history
                    .insert(key.as_str(), value.as_str())
                    .expect("inserts");
            }
        }
        write.commit().expect("commits");
        let methods: Vec<(Method, bool)> = store
            .history(ResourceType::ValueSet, Some("old"))
            .expect("reads")
            .into_iter()
            .map(|entry| (entry.method, entry.created))
            .collect();
        assert_eq!(methods, [(Method::Post, true), (Method::Put, false)]);
    }
}
