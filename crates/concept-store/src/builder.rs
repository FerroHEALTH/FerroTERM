// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Writing an artifact, offline, in one transaction.
//!
//! The build tool feeds concepts, designations, acceptability, properties, and
//! nothing else; `finish` packs each ordinal-keyed set into a dense column
//! beside the database, chooses the displays, and commits. Two builds from the
//! same input produce the same bytes.

use std::collections::BTreeMap;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use concept_graph::ordinal::Ordinal;
use redb::{Database, TableHandle};

use crate::column::Column;
use crate::record::{self, Concept, Designation, PropertyValue};
use crate::tables;

/// How much of a column's side file the writer holds before it goes to disk.
///
/// A column is written as a count, an offset per record, and the records, each
/// a small write, so the buffer is what turns them into whole-page writes.
const WRITE_BUFFER: usize = 1 << 20;

/// A failure while building.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// The database could not be created.
    #[error("cannot create the artifact at {path}")]
    Create {
        /// The artifact.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: redb::DatabaseError,
    },
    /// A transaction failed.
    #[error("transaction failed")]
    Transaction(#[from] redb::TransactionError),
    /// A table could not be opened.
    #[error("cannot open table")]
    Table(#[from] redb::TableError),
    /// A write failed inside the database.
    #[error("storage write failed")]
    Storage(#[from] redb::StorageError),
    /// The commit failed.
    #[error("commit failed")]
    Commit(#[from] redb::CommitError),
    /// Reclaiming the unused pages after the commit failed.
    #[error("compaction failed")]
    Compaction(#[from] redb::CompactionError),
    /// A column's side file could not be written.
    #[error("cannot write {path}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// A vocabulary name was registered twice with different ordinals.
    #[error("{kind} {name:?} is already ordinal {existing}, not {requested}")]
    Vocabulary {
        /// The vocabulary.
        kind: String,
        /// The name.
        name: String,
        /// The ordinal it already has.
        existing: u32,
        /// The ordinal requested.
        requested: u32,
    },
}

/// The acceptability that marks a designation as preferred in a language
/// reference set, as the code system spells it (for SNOMED, the SCTID of
/// `|Preferred|`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreferredRule {
    /// The acceptability ordinal meaning preferred.
    pub preferred: u32,
    /// The designation use whose preferred designation is a concept display
    /// (for SNOMED, the synonym), or `None` when the system has no such use.
    ///
    /// Only this use goes into the display column. Carrying every use made the
    /// column a second copy of most of the designations (#322).
    pub display_use: Option<u32>,
}

/// An artifact under construction.
///
/// Rows are buffered in memory, sorted by key, and written in `finish`, each
/// table opened once. redb fills pages tightly when keys arrive in order, and
/// opening a table is not free; the first build wrote every row through its
/// own `open_table` in arrival order and took ten minutes for the NL edition.
pub struct StoreBuilder {
    path: PathBuf,
    db: Database,
    system: String,
    version: String,
    vocabularies: BTreeMap<&'static str, BTreeMap<u32, String>>,
    codes: BTreeMap<String, u32>,
    concepts: BTreeMap<u32, Vec<u8>>,
    designations: BTreeMap<(u32, u32), Vec<u8>>,
    acceptability: BTreeMap<(u32, u32, u32), u32>,
    designation_uses: BTreeMap<(u32, u32), u32>,
    properties: BTreeMap<(u32, u32), Vec<u8>>,
}

impl std::fmt::Debug for StoreBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreBuilder")
            .field("path", &self.path)
            .field("concepts", &self.concepts.len())
            .field("designations", &self.designations.len())
            .finish_non_exhaustive()
    }
}

impl StoreBuilder {
    /// Creates the artifact at `path`, replacing any file there, and records
    /// the layout version, system, and version.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError`] when the file cannot be created.
    pub fn create(path: &Path, system: &str, version: &str) -> Result<Self, BuildError> {
        if path.exists() {
            std::fs::remove_file(path).map_err(|source| BuildError::Create {
                path: path.to_path_buf(),
                source: redb::DatabaseError::Storage(redb::StorageError::Io(source)),
            })?;
        }
        let db = Database::create(path).map_err(|source| BuildError::Create {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(Self {
            path: path.to_path_buf(),
            db,
            system: system.to_owned(),
            version: version.to_owned(),
            vocabularies: BTreeMap::new(),
            codes: BTreeMap::new(),
            concepts: BTreeMap::new(),
            designations: BTreeMap::new(),
            acceptability: BTreeMap::new(),
            designation_uses: BTreeMap::new(),
            properties: BTreeMap::new(),
        })
    }

    /// Names a vocabulary ordinal (a property key, designation use, language
    /// reference set, or acceptability).
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Vocabulary`] when `name` already has another ordinal.
    pub fn vocabulary(
        &mut self,
        kind: crate::store::Vocabulary,
        ordinal: u32,
        name: &str,
    ) -> Result<(), BuildError> {
        let table_name = match kind {
            crate::store::Vocabulary::PropertyKeys => tables::PROPERTY_KEYS.name(),
            crate::store::Vocabulary::DesignationUses => tables::DESIGNATION_USES.name(),
            crate::store::Vocabulary::LanguageRefsets => tables::LANGUAGE_REFSETS.name(),
            crate::store::Vocabulary::Acceptabilities => tables::ACCEPTABILITIES.name(),
        };
        let entries = self.vocabularies.entry(table_name).or_default();
        if let Some((existing, _)) = entries
            .iter()
            .find(|(key, value)| value.as_str() == name && **key != ordinal)
        {
            return Err(BuildError::Vocabulary {
                kind: table_name.to_owned(),
                name: name.to_owned(),
                existing: *existing,
                requested: ordinal,
            });
        }
        entries.insert(ordinal, name.to_owned());
        Ok(())
    }

    /// Adds a concept under `ordinal`.
    ///
    /// # Errors
    ///
    /// Cannot fail today; the signature keeps the door open for a full buffer.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the build seam is fallible by contract: a full buffer fails here"
    )]
    pub fn concept(&mut self, ordinal: Ordinal, concept: &Concept) -> Result<(), BuildError> {
        self.codes.insert(concept.code.clone(), ordinal.index());
        self.concepts.insert(ordinal.index(), concept.encode());
        Ok(())
    }

    /// Adds designation `index` of concept `ordinal`.
    ///
    /// # Errors
    ///
    /// Cannot fail today; the signature keeps the door open for a full buffer.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the build seam is fallible by contract: a full buffer fails here"
    )]
    pub fn designation(
        &mut self,
        ordinal: Ordinal,
        index: u32,
        designation: &Designation,
    ) -> Result<(), BuildError> {
        self.designations
            .insert((ordinal.index(), index), designation.encode());
        self.designation_uses
            .insert((ordinal.index(), index), designation.use_ordinal);
        Ok(())
    }

    /// Records the acceptability of a designation in a language reference set.
    ///
    /// # Errors
    ///
    /// Cannot fail today; the signature keeps the door open for a full buffer.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the build seam is fallible by contract: a full buffer fails here"
    )]
    pub fn acceptability(
        &mut self,
        ordinal: Ordinal,
        index: u32,
        language_refset: u32,
        acceptability: u32,
    ) -> Result<(), BuildError> {
        self.acceptability
            .insert((ordinal.index(), index, language_refset), acceptability);
        Ok(())
    }

    /// Sets the values of property `key` on concept `ordinal`.
    ///
    /// # Errors
    ///
    /// Cannot fail today; the signature keeps the door open for a full buffer.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the build seam is fallible by contract: a full buffer fails here"
    )]
    pub fn properties(
        &mut self,
        ordinal: Ordinal,
        key: u32,
        values: &[PropertyValue],
    ) -> Result<(), BuildError> {
        self.properties
            .insert((ordinal.index(), key), PropertyValue::encode_list(values));
        Ok(())
    }

    /// The preferred designation index per `(concept, reference set, use)`:
    /// the lowest index among those the reference set marks preferred.
    fn chosen_preferred(&self, rule: &PreferredRule) -> BTreeMap<(u32, u32, u32), u32> {
        let mut chosen: BTreeMap<(u32, u32, u32), u32> = BTreeMap::new();
        for ((concept, index, refset), acceptability) in &self.acceptability {
            if *acceptability != rule.preferred {
                continue;
            }
            let Some(use_ordinal) = self.designation_uses.get(&(*concept, *index)) else {
                continue;
            };
            chosen
                .entry((*concept, *refset, *use_ordinal))
                .and_modify(|existing| *existing = (*existing).min(*index))
                .or_insert(*index);
        }
        chosen
    }

    /// The chosen designations themselves, packed by concept.
    ///
    /// Choosing a display is the hottest read the server has, and it should
    /// not have to find the designation this build already found.
    fn display_rows(
        &self,
        chosen: &BTreeMap<(u32, u32, u32), u32>,
        display_use: u32,
    ) -> BTreeMap<u32, Vec<u8>> {
        let mut by_concept: BTreeMap<u32, Vec<(u32, String, String)>> = BTreeMap::new();
        for ((concept, refset, use_ordinal), index) in chosen {
            if *use_ordinal != display_use {
                continue;
            }
            let Some(bytes) = self.designations.get(&(*concept, *index)) else {
                continue;
            };
            let Ok(designation) = Designation::decode(bytes) else {
                continue;
            };
            by_concept.entry(*concept).or_default().push((
                *refset,
                designation.language,
                designation.term,
            ));
        }
        by_concept
            .into_iter()
            .map(|(concept, entries)| {
                let borrowed: Vec<(u32, &str, &str)> = entries
                    .iter()
                    .map(|(refset, language, term)| (*refset, language.as_str(), term.as_str()))
                    .collect();
                (concept, record::Displays::encode(&borrowed))
            })
            .collect()
    }

    /// Calls `write` once per concept with that concept's rows.
    ///
    /// The rows arrive keyed by a concept ordinal followed by the rest of the
    /// key, so a single pass in key order groups them without sorting again.
    /// `write` takes the group rather than a finished record so a caller that
    /// streams to a table holds no second copy of the edition.
    fn per_concept<'a, K: 'a, V: 'a, R>(
        rows: &'a BTreeMap<K, V>,
        concept: impl Fn(&K) -> u32,
        row: impl Fn(&'a K, &'a V) -> R,
        mut write: impl FnMut(u32, &[R]) -> Result<(), BuildError>,
    ) -> Result<(), BuildError> {
        let mut at = None;
        let mut group: Vec<R> = Vec::new();
        for (key, value) in rows {
            let ordinal = concept(key);
            if at != Some(ordinal) {
                if let Some(previous) = at {
                    write(previous, &group)?;
                }
                at = Some(ordinal);
                group.clear();
            }
            group.push(row(key, value));
        }
        match at {
            Some(previous) => write(previous, &group),
            None => Ok(()),
        }
    }

    /// One record per concept, encoded by `pack`, keyed by concept ordinal.
    fn column_rows<'a, K: 'a, V: 'a, R>(
        rows: &'a BTreeMap<K, V>,
        concept: impl Fn(&K) -> u32,
        row: impl Fn(&'a K, &'a V) -> R,
        pack: impl Fn(&[R]) -> Vec<u8>,
    ) -> Result<BTreeMap<u32, Vec<u8>>, BuildError> {
        let mut packed: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
        Self::per_concept(rows, concept, row, |ordinal, group| {
            packed.insert(ordinal, pack(group));
            Ok(())
        })?;
        Ok(packed)
    }

    /// Writes one dense column to its own file beside the database.
    ///
    /// A column is a position-addressed array, which a b-tree of positions
    /// only pretends to be, and a reader of a file assembles it at the address
    /// it serves from (#641).
    fn write_column(
        &self,
        name: &str,
        count: u32,
        rows: &BTreeMap<u32, Vec<u8>>,
    ) -> Result<(), BuildError> {
        let path = Column::file(&self.path, name);
        let failed = |source| BuildError::Io {
            path: path.clone(),
            source,
        };
        let file = std::fs::File::create(&path).map_err(failed)?;
        let mut out = BufWriter::with_capacity(WRITE_BUFFER, file);
        Column::write_to(
            count,
            rows.iter()
                .map(|(ordinal, bytes)| (Ordinal::new(*ordinal), bytes.as_slice())),
            &mut out,
        )
        .map_err(failed)?;
        out.flush().map_err(failed)
    }

    /// Writes every buffered row in key order, computes the preferred
    /// designations per language reference set and use, records the concept
    /// count, and commits.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError`] when a table cannot be written or the commit fails.
    pub fn finish(self, rule: &PreferredRule) -> Result<PathBuf, BuildError> {
        let txn = self.db.begin_write()?;
        {
            let mut meta = txn.open_table(tables::META)?;
            meta.insert(tables::META_LAYOUT, tables::LAYOUT_VERSION)?;
            meta.insert(tables::META_SYSTEM, self.system.as_str())?;
            meta.insert(tables::META_VERSION, self.version.as_str())?;
            meta.insert(
                tables::META_CONCEPTS,
                self.concepts.len().to_string().as_str(),
            )?;
            meta.insert(tables::META_PREFERRED, rule.preferred.to_string().as_str())?;
        }
        {
            let mut codes = txn.open_table(tables::CODES)?;
            for (code, ordinal) in &self.codes {
                codes.insert(code.as_str(), *ordinal)?;
            }
        }
        // The count is the highest ordinal, not the entry count, so a gap in
        // the ordinals cannot push a record off the end of a column.
        let count = self
            .concepts
            .keys()
            .next_back()
            .map_or(0, |highest| highest.saturating_add(1));
        // An ordinal is a position, so these are dense columns in their own
        // files rather than b-trees keyed by that position.
        self.write_column(tables::COLUMN_CONCEPTS, count, &self.concepts)?;
        self.write_column(
            tables::COLUMN_ACCEPTABILITY,
            count,
            &Self::column_rows(
                &self.acceptability,
                |(concept, _, _)| *concept,
                |(_, index, refset), acceptability| (*index, *refset, *acceptability),
                record::Acceptability::encode,
            )?,
        )?;
        self.write_column(
            tables::COLUMN_PROPERTIES,
            count,
            &Self::column_rows(
                &self.properties,
                |(concept, _)| *concept,
                |(_, key), values| (*key, values.as_slice()),
                record::Properties::encode,
            )?,
        )?;
        // A code system with no display use still gets the file, empty, so a
        // store that opens one reads four columns or refuses.
        let displays = rule.display_use.map_or_else(BTreeMap::new, |display_use| {
            self.display_rows(&self.chosen_preferred(rule), display_use)
        });
        self.write_column(tables::COLUMN_DISPLAYS, count, &displays)?;
        self.write_designations(&txn)?;
        for table_def in [
            tables::PROPERTY_KEYS,
            tables::DESIGNATION_USES,
            tables::LANGUAGE_REFSETS,
            tables::ACCEPTABILITIES,
        ] {
            let mut table = txn.open_table(table_def)?;
            if let Some(entries) = self.vocabularies.get(table_def.name()) {
                for (ordinal, name) in entries {
                    table.insert(*ordinal, name.as_str())?;
                }
            }
        }
        txn.commit()?;
        // redb grows the file in regions ahead of use; compaction returns the
        // unused pages so the artifact is the size of its data. Repeated until
        // redb reports nothing further to reclaim.
        let mut db = self.db;
        while db.compact()? {}
        drop(db);
        Ok(self.path)
    }

    /// Writes the designations of each concept as one row.
    ///
    /// The designation text is the largest thing the artifact holds, so it
    /// stays in the database; one row per concept keys the tree by what a
    /// reader asks for and holds no key per designation (#338).
    fn write_designations(&self, txn: &redb::WriteTransaction) -> Result<(), BuildError> {
        let mut designations = txn.open_table(tables::DESIGNATIONS)?;
        Self::per_concept(
            &self.designations,
            |(concept, _)| *concept,
            |(_, index), bytes| (*index, bytes.as_slice()),
            |ordinal, group| {
                designations.insert(ordinal, record::Designations::encode(group).as_slice())?;
                Ok(())
            },
        )
    }
}
