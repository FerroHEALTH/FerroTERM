// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Read-only access to a built artifact.
//!
//! Every method is a point read: one position or one key, one record, decoded
//! with a typed error. Ordinal-keyed data is answered from a dense column, read
//! at open from its own file beside the database; the designation text is
//! point-read from the database, one row per concept. Scans belong to the
//! offline build, never to a request path.
//!
//! Opening takes one read snapshot and keeps the two tables a request path
//! reads open on it, so a lookup costs one descent rather than a transaction,
//! a table-name lookup, and a descent.

use std::path::{Path, PathBuf};

use concept_graph::ordinal::Ordinal;
use redb::{ReadOnlyDatabase, ReadOnlyTable, ReadableDatabase, ReadableTable, TableHandle};

use crate::column::{Column, ColumnError};
use crate::record::{self, Concept, Designation, PropertyValue, RecordError};
use crate::tables;

/// A failure while opening or reading an artifact.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The database could not be opened or read.
    #[error("cannot open the artifact at {path}")]
    Open {
        /// The artifact.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: redb::DatabaseError,
    },
    /// A transaction failed.
    #[error("cannot begin a read transaction")]
    Transaction(#[from] redb::TransactionError),
    /// A table is missing or has another type.
    #[error("cannot open table {table}")]
    Table {
        /// The table name.
        table: String,
        /// The underlying error.
        #[source]
        source: redb::TableError,
    },
    /// A read failed inside the database.
    #[error("storage read failed")]
    Storage(#[from] redb::StorageError),
    /// The artifact was written by another layout version.
    #[error("artifact layout {found:?}, expected {expected:?}")]
    Layout {
        /// The layout found, if any.
        found: Option<String>,
        /// The layout this build reads.
        expected: &'static str,
    },
    /// A column's side file is missing or is not the layout this build reads.
    #[error("the {column} column does not read from {path}")]
    Column {
        /// The column name.
        column: String,
        /// The side file it was read from.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: ColumnError,
    },
    /// A stored record is damaged.
    #[error("damaged record in table {table} at key {key}")]
    Record {
        /// The table name.
        table: String,
        /// The key, rendered.
        key: String,
        /// The underlying error.
        #[source]
        source: RecordError,
    },
}

/// How much memory the database may keep for the pages it has read.
///
// NOTE: redb's default is a gibibyte
// (<https://docs.rs/redb/4.2.0/redb/struct.Builder.html#method.set_cache_size>),
// which a store answering designation rows at random would fill and hold.
const CACHE_BYTES: usize = 64 << 20;

/// An opened artifact.
pub struct Store {
    path: PathBuf,
    db: ReadOnlyDatabase,
    /// The concepts, read once at open. An ordinal is a position, so this
    /// column answers by slicing where a b-tree would search.
    concepts: Column,
    /// The preferred designations the build chose, per concept, so choosing a
    /// display reads no table at all.
    displays: Column,
    /// The properties of every concept. The whole hierarchy is read through
    /// these on an ECL refinement, so they are answered by position.
    properties: Column,
    /// The acceptability of every designation, per language reference set.
    acceptability: Column,
    /// The native code to ordinal index, held open on the snapshot the store
    /// was opened on. A string key is what a b-tree is for; paying for a
    /// transaction and a table-name lookup on top of it is not.
    code_index: ReadOnlyTable<&'static str, u32>,
    /// The designations of every concept, one row per concept, held open on
    /// the same snapshot. The text is the largest thing an artifact holds, so
    /// it stays on disk rather than becoming a resident column (#338).
    designation_rows: ReadOnlyTable<u32, &'static [u8]>,
    /// The acceptability ordinal that marks a designation preferred.
    preferred: Option<u32>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

macro_rules! open_table {
    ($txn:expr, $def:expr) => {
        $txn.open_table($def).map_err(|source| StoreError::Table {
            table: $def.name().to_owned(),
            source,
        })
    };
}

impl Store {
    /// Opens the artifact at `path` read-only and checks its layout version.
    ///
    /// The four dense columns are read from their own files beside `path`, so
    /// each one arrives at the address it is served from.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file cannot be opened, is not an
    /// artifact of this layout, or a table or a column file is missing.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let db = redb::Builder::new()
            .set_cache_size(CACHE_BYTES)
            .open_read_only(path)
            .map_err(|source| StoreError::Open {
                path: path.to_path_buf(),
                source,
            })?;
        let txn = db.begin_read()?;
        let meta = open_table!(txn, tables::META)?;
        let layout = meta
            .get(tables::META_LAYOUT)?
            .map(|value| value.value().to_owned());
        if layout.as_deref() != Some(tables::LAYOUT_VERSION) {
            return Err(StoreError::Layout {
                found: layout,
                expected: tables::LAYOUT_VERSION,
            });
        }
        let column = |name: &str| -> Result<Column, StoreError> {
            let file = Column::file(path, name);
            let failed = |source| StoreError::Column {
                column: name.to_owned(),
                path: file.clone(),
                source,
            };
            let mut reader = concept_graph::read::buffered(&file)
                .map_err(|source| failed(ColumnError::Io(source)))?;
            Column::read_from(&mut reader).map_err(failed)
        };
        let concepts = column(tables::COLUMN_CONCEPTS)?;
        let displays = column(tables::COLUMN_DISPLAYS)?;
        let properties = column(tables::COLUMN_PROPERTIES)?;
        let acceptability = column(tables::COLUMN_ACCEPTABILITY)?;
        let preferred = meta
            .get(tables::META_PREFERRED)?
            .and_then(|value| value.value().parse().ok());
        let code_index = open_table!(txn, tables::CODES)?;
        let designation_rows = open_table!(txn, tables::DESIGNATIONS)?;
        Ok(Self {
            path: path.to_path_buf(),
            db,
            concepts,
            displays,
            properties,
            acceptability,
            code_index,
            designation_rows,
            preferred,
        })
    }

    /// The artifact's path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The heap bytes each resident column holds, by column name.
    ///
    /// The designation text and the code index stay in the database and are
    /// point-read, so they are not here; what `redb` keeps in its page cache
    /// is the database's own and no column of it.
    #[must_use]
    pub fn column_sizes(&self) -> [(&'static str, usize); 4] {
        [
            (tables::COLUMN_CONCEPTS, self.concepts.size_in_bytes()),
            (tables::COLUMN_DISPLAYS, self.displays.size_in_bytes()),
            (tables::COLUMN_PROPERTIES, self.properties.size_in_bytes()),
            (
                tables::COLUMN_ACCEPTABILITY,
                self.acceptability.size_in_bytes(),
            ),
        ]
    }

    /// The heap bytes the resident columns hold together.
    #[must_use]
    pub fn size_in_bytes(&self) -> usize {
        self.column_sizes()
            .into_iter()
            .fold(0_usize, |total, (_, bytes)| total.saturating_add(bytes))
    }

    /// An artifact-level fact by key.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the database cannot be read.
    pub fn meta(&self, key: &str) -> Result<Option<String>, StoreError> {
        let txn = self.db.begin_read()?;
        let table = open_table!(txn, tables::META)?;
        Ok(table.get(key)?.map(|v| v.value().to_owned()))
    }

    /// The ordinal of a native code, if the version has it.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the database cannot be read.
    pub fn ordinal(&self, code: &str) -> Result<Option<Ordinal>, StoreError> {
        Ok(self
            .code_index
            .get(code)?
            .map(|value| Ordinal::new(value.value())))
    }

    /// The concept at `ordinal`, if any.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the database cannot be read or the record is damaged.
    pub fn concept(&self, ordinal: Ordinal) -> Result<Option<Concept>, StoreError> {
        self.concepts
            .get(ordinal)
            .map(|bytes| Self::decode_concept(ordinal, bytes))
            .transpose()
    }

    /// The native code at `ordinal`, borrowed from the column.
    ///
    /// The code is the first field of the record, so this reads one length
    /// and one string and copies nothing.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the code is damaged.
    pub fn code(&self, ordinal: Ordinal) -> Result<Option<&str>, StoreError> {
        self.concepts
            .get(ordinal)
            .map(|bytes| {
                record::code(bytes).map_err(|source| StoreError::Record {
                    table: tables::COLUMN_CONCEPTS.to_owned(),
                    key: ordinal.to_string(),
                    source,
                })
            })
            .transpose()
    }

    /// Decodes one concept record, naming the ordinal it came from.
    fn decode_concept(ordinal: Ordinal, bytes: &[u8]) -> Result<Concept, StoreError> {
        Concept::decode(bytes).map_err(|source| StoreError::Record {
            table: tables::COLUMN_CONCEPTS.to_owned(),
            key: ordinal.to_string(),
            source,
        })
    }

    /// The concepts at `ordinals`, in the order given, `None` where the store
    /// has no such concept.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when a record is damaged.
    pub fn concepts(
        &self,
        ordinals: impl IntoIterator<Item = Ordinal>,
    ) -> Result<Vec<Option<Concept>>, StoreError> {
        ordinals
            .into_iter()
            .map(|ordinal| self.concept(ordinal))
            .collect()
    }

    /// The native codes at `ordinals`, in the order given, `None` where the
    /// store has no such concept.
    ///
    /// Each code is sliced out of the concept column and decoded only as far
    /// as its code, so the batch costs one string per code and nothing else.
    /// A caller that can borrow calls [`Self::code`] and pays not even that.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when a code is damaged.
    pub fn codes(
        &self,
        ordinals: impl IntoIterator<Item = Ordinal>,
    ) -> Result<Vec<Option<String>>, StoreError> {
        ordinals
            .into_iter()
            .map(|ordinal| Ok(self.code(ordinal)?.map(ToOwned::to_owned)))
            .collect()
    }

    /// Every designation of `ordinal`, in index order.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the database cannot be read or a record is damaged.
    pub fn designations(&self, ordinal: Ordinal) -> Result<Vec<Designation>, StoreError> {
        let Some(packed) = self.designation_rows.get(ordinal.index())? else {
            return Ok(Vec::new());
        };
        record::Designations::decode(packed.value()).map_err(|source| StoreError::Record {
            table: tables::DESIGNATIONS.name().to_owned(),
            key: ordinal.to_string(),
            source,
        })
    }

    /// The acceptability ordinal of a designation in a language reference set.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the column is damaged.
    pub fn acceptability(
        &self,
        ordinal: Ordinal,
        designation: u32,
        language_refset: u32,
    ) -> Result<Option<u32>, StoreError> {
        let Some(bytes) = self.acceptability.get(ordinal) else {
            return Ok(None);
        };
        record::Acceptability::find(bytes, designation, language_refset).map_err(|source| {
            StoreError::Record {
                table: tables::COLUMN_ACCEPTABILITY.to_owned(),
                key: ordinal.to_string(),
                source,
            }
        })
    }

    /// The preferred designation of `ordinal` for a language reference set and
    /// designation use: the lowest-indexed designation of that use the
    /// reference set marks preferred.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the database cannot be read or a record is
    /// damaged.
    pub fn preferred(
        &self,
        ordinal: Ordinal,
        language_refset: u32,
        use_ordinal: u32,
    ) -> Result<Option<Designation>, StoreError> {
        let (Some(preferred), Some(bytes)) = (self.preferred, self.acceptability.get(ordinal))
        else {
            return Ok(None);
        };
        let entries =
            record::Acceptability::decode(bytes).map_err(|source| StoreError::Record {
                table: tables::COLUMN_ACCEPTABILITY.to_owned(),
                key: ordinal.to_string(),
                source,
            })?;
        let Some(packed) = self.designation_rows.get(ordinal.index())? else {
            return Ok(None);
        };
        let damaged = |source| StoreError::Record {
            table: tables::DESIGNATIONS.name().to_owned(),
            key: ordinal.to_string(),
            source,
        };
        let mut chosen: Option<(u32, Designation)> = None;
        for (index, refset, acceptability) in entries {
            if refset != language_refset || acceptability != preferred {
                continue;
            }
            if chosen.as_ref().is_some_and(|(at, _)| *at <= index) {
                continue;
            }
            let Some(designation) =
                record::Designations::at(packed.value(), index).map_err(damaged)?
            else {
                continue;
            };
            if designation.use_ordinal == use_ordinal {
                chosen = Some((index, designation));
            }
        }
        Ok(chosen.map(|(_, designation)| designation))
    }

    /// The display of `ordinal` from the first of `language_refsets` whose
    /// language `accept` admits.
    ///
    /// The build chose one display per reference set, so this reads the column
    /// and stops at the first language the caller wants; no table is touched.
    /// The reference sets are asked in the order given, which is what decides
    /// the answer when several carry the same language.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the column is damaged.
    pub fn display(
        &self,
        ordinal: Ordinal,
        language_refsets: impl IntoIterator<Item = u32>,
        accept: impl Fn(&str) -> bool,
    ) -> Result<Option<String>, StoreError> {
        let Some(bytes) = self.displays.get(ordinal) else {
            return Ok(None);
        };
        let damaged = |source| StoreError::Record {
            table: tables::COLUMN_DISPLAYS.to_owned(),
            key: ordinal.to_string(),
            source,
        };
        for refset in language_refsets {
            let Some((language, term)) = record::Displays::find(bytes, refset).map_err(damaged)?
            else {
                continue;
            };
            if accept(&language) {
                return Ok(Some(term));
            }
        }
        Ok(None)
    }

    /// Every property of `ordinal`, as `(property key ordinal, values)`, in key order.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the column is damaged.
    pub fn properties(
        &self,
        ordinal: Ordinal,
    ) -> Result<Vec<(u32, Vec<PropertyValue>)>, StoreError> {
        let Some(bytes) = self.properties.get(ordinal) else {
            return Ok(Vec::new());
        };
        record::Properties::decode(bytes).map_err(|source| StoreError::Record {
            table: tables::COLUMN_PROPERTIES.to_owned(),
            key: ordinal.to_string(),
            source,
        })
    }

    /// A vocabulary entry: the name of a property key, designation use,
    /// language reference set, or acceptability ordinal.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the database cannot be read.
    pub fn vocabulary(&self, kind: Vocabulary, ordinal: u32) -> Result<Option<String>, StoreError> {
        let txn = self.db.begin_read()?;
        let table = open_table!(txn, kind.table())?;
        Ok(table.get(ordinal)?.map(|v| v.value().to_owned()))
    }

    /// The ordinal of a vocabulary name, by scanning the (small) vocabulary table.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the database cannot be read.
    pub fn vocabulary_ordinal(
        &self,
        kind: Vocabulary,
        name: &str,
    ) -> Result<Option<u32>, StoreError> {
        let txn = self.db.begin_read()?;
        let table = open_table!(txn, kind.table())?;
        for entry in table.iter()? {
            let (key, value) = entry?;
            if value.value() == name {
                return Ok(Some(key.value()));
            }
        }
        Ok(None)
    }
}

/// The small name tables of an artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vocabulary {
    /// Property key names.
    PropertyKeys,
    /// Designation use codes.
    DesignationUses,
    /// Language reference set codes.
    LanguageRefsets,
    /// Acceptability codes.
    Acceptabilities,
}

impl Vocabulary {
    fn table(self) -> redb::TableDefinition<'static, u32, &'static str> {
        match self {
            Self::PropertyKeys => tables::PROPERTY_KEYS,
            Self::DesignationUses => tables::DESIGNATION_USES,
            Self::LanguageRefsets => tables::LANGUAGE_REFSETS,
            Self::Acceptabilities => tables::ACCEPTABILITIES,
        }
    }
}
