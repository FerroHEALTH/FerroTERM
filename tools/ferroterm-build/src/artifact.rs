// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The parts of an artifact the classification, ICD-11, LOINC and `RxNorm`
//! builds all write: the store with its vocabularies and designations, the
//! hierarchy, the text index, a key table, and the manifest.
//!
//! Each build keeps its own error type for what its source can refuse and
//! carries [`Error`] for everything here. No specification governs the
//! artifact layout: our own design.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use concept_graph::closure::{Closure, ClosureError};
use concept_graph::csr::{Csr, CsrError};
use concept_graph::ordinal::Ordinal;
use concept_graph::persist::Hierarchy;
use concept_store::builder::{BuildError, PreferredRule, StoreBuilder};
use concept_store::keys::{KeyTable, KeyTableError};
use concept_store::record::Designation;
use concept_store::store::Vocabulary;
use designation_index::index::{IndexBuilder, Input};

use crate::pipeline::{HIERARCHY_FILE, MANIFEST_FILE, TEXT_FILE};

/// A part of an artifact that cannot be built or written.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The store cannot be written.
    #[error(transparent)]
    Store(#[from] BuildError),
    /// The hierarchy cannot be built.
    #[error(transparent)]
    Csr(#[from] CsrError),
    /// The hierarchy has a cycle.
    #[error(transparent)]
    Closure(#[from] ClosureError),
    /// The hierarchy cannot be written.
    #[error(transparent)]
    Graph(#[from] concept_graph::persist::PersistError),
    /// A key table cannot be written.
    #[error(transparent)]
    Keys(#[from] KeyTableError),
    /// The text index cannot be built.
    #[error(transparent)]
    Text(#[from] designation_index::index::BuildError),
    /// The text index cannot be written.
    #[error(transparent)]
    TextPersist(#[from] designation_index::persist::PersistError),
    /// A file cannot be written.
    #[error("cannot write {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: io::Error,
    },
    /// More concepts, designations, or keys than an ordinal can number.
    #[error("too many concepts")]
    TooMany,
    /// A property key the build asked for and never registered.
    #[error("the property key `{key}` is not in the vocabulary")]
    UnknownPropertyKey {
        /// The key the build asked for.
        key: String,
    },
}

/// The ordinal of `index`.
///
/// # Errors
///
/// Returns [`Error::TooMany`] past `u32::MAX`.
pub(crate) fn ordinal(index: usize) -> Result<Ordinal, Error> {
    // A count past u32::MAX is the whole message; the conversion error adds nothing.
    let Ok(index) = u32::try_from(index) else {
        return Err(Error::TooMany);
    };
    Ok(Ordinal::new(index))
}

/// A closure that names `path` in an I/O failure.
pub(crate) fn io_error(path: &Path) -> impl FnOnce(io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Writes `bytes` to `path`.
///
/// # Errors
///
/// Returns [`Error::Io`] naming `path` when the file cannot be written.
pub(crate) fn write_file(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    std::fs::write(path, bytes).map_err(io_error(path))
}

/// Creates the store at `path` and writes `uses` as its designation-use
/// vocabulary, in the order given.
///
/// # Errors
///
/// Returns [`Error`] when the store cannot be created, does not take the
/// vocabulary, or there are more uses than an ordinal can number.
pub(crate) fn open_store<S: AsRef<str>>(
    path: &Path,
    system: &str,
    version: &str,
    uses: &[S],
) -> Result<StoreBuilder, Error> {
    let mut builder = StoreBuilder::create(path, system, version)?;
    for (i, name) in uses.iter().enumerate() {
        builder.vocabulary(
            Vocabulary::DesignationUses,
            ordinal(i)?.index(),
            name.as_ref(),
        )?;
    }
    Ok(builder)
}

/// Finishes the store.
///
/// # Errors
///
/// Returns [`Error::Store`] when the store cannot be finished.
pub(crate) fn finish(builder: StoreBuilder) -> Result<(), Error> {
    builder.finish(&PreferredRule {
        preferred: 0,
        // Nothing reads a display column for these systems; each provider
        // chooses a display from the designations it already reads.
        display_use: None,
    })?;
    Ok(())
}

/// One designation to write and index.
pub(crate) struct Placed {
    /// The concept it belongs to.
    pub(crate) ordinal: Ordinal,
    /// Its index within that concept.
    pub(crate) index: u32,
    /// The designation itself.
    pub(crate) record: Designation,
}

/// The property keys of one release, by name.
pub(crate) struct PropertyKeys {
    keys: BTreeMap<String, u32>,
}

impl PropertyKeys {
    /// Writes `names` as the property-key vocabulary, in the order given, and
    /// returns the keys by name.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the store does not take the vocabulary or there
    /// are more keys than an ordinal can number.
    pub(crate) fn write<S: AsRef<str>>(
        builder: &mut StoreBuilder,
        names: &[S],
    ) -> Result<Self, Error> {
        let mut keys: BTreeMap<String, u32> = BTreeMap::new();
        for name in names {
            let key = ordinal(keys.len())?.index();
            builder.vocabulary(Vocabulary::PropertyKeys, key, name.as_ref())?;
            keys.insert(name.as_ref().to_owned(), key);
        }
        Ok(Self { keys })
    }

    /// The key of `name`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownPropertyKey`] when the vocabulary holds no such
    /// key, which is a defect in the build, never a capacity limit.
    pub(crate) fn key(&self, name: &str) -> Result<u32, Error> {
        self.keys
            .get(name)
            .copied()
            .ok_or_else(|| Error::UnknownPropertyKey {
                key: name.to_owned(),
            })
    }
}

/// How the text index keys a designation's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LanguageKey {
    /// The tag as the store holds it.
    Whole,
    /// The primary subtag alone, cut at the first `-` or `_`: the index keys
    /// designations the way the providers query it, while the store keeps the
    /// full tag.
    PrimarySubtag,
}

/// Writes the designations into the store.
///
/// # Errors
///
/// Returns [`Error::Store`] when the store refuses one.
pub(crate) fn write_designations(
    builder: &mut StoreBuilder,
    placed: &[Placed],
) -> Result<(), Error> {
    for p in placed {
        builder.designation(p.ordinal, p.index, &p.record)?;
    }
    Ok(())
}

/// Builds the hierarchy of `count` concepts over the child-to-parent `edges`
/// and writes it beside the store.
///
/// # Errors
///
/// Returns [`Error`] when an edge names no concept, the hierarchy has a
/// cycle, or the file cannot be written.
pub(crate) fn write_hierarchy(
    out: &Path,
    count: usize,
    edges: impl IntoIterator<Item = (Ordinal, Ordinal)>,
) -> Result<(), Error> {
    let csr = Csr::build(ordinal(count)?.index(), edges)?;
    let closure = Closure::compute(&csr)?;
    let hierarchy = Hierarchy { is_a: csr, closure };
    let mut bytes = Vec::new();
    hierarchy.write_to(&mut bytes)?;
    write_file(&out.join(HIERARCHY_FILE), &bytes)
}

/// Builds the text index over the designations, writes it beside the store,
/// and returns the number of words it holds.
///
/// # Errors
///
/// Returns [`Error`] when the index cannot be built or written.
pub(crate) fn write_text_index(
    out: &Path,
    placed: &[Placed],
    language: LanguageKey,
) -> Result<usize, Error> {
    let mut index = IndexBuilder::new();
    for p in placed {
        let tag = p.record.language.as_str();
        let tag = match language {
            LanguageKey::Whole => tag,
            LanguageKey::PrimarySubtag => tag.split(['-', '_']).next().unwrap_or(tag),
        };
        index.add(&Input {
            concept: p.ordinal,
            index: p.index,
            term: &p.record.term,
            language: tag,
            use_ordinal: p.record.use_ordinal,
            active: p.record.active,
            refsets: &[],
        })?;
    }
    let index = index.build()?;
    let mut bytes = Vec::new();
    designation_index::persist::write_to(&index, &mut bytes)?;
    write_file(&out.join(TEXT_FILE), &bytes)?;
    Ok(index.words())
}

/// Writes the key table of `pairs` to `path` and returns the number of keys
/// it holds.
///
/// # Errors
///
/// Returns [`Error`] when the table or the file cannot be written.
pub(crate) fn write_key_table(path: &Path, pairs: Vec<(u64, u32)>) -> Result<usize, Error> {
    let table = KeyTable::new(pairs);
    let mut bytes = Vec::new();
    table.write_to(&mut bytes)?;
    write_file(path, &bytes)?;
    Ok(table.len())
}

/// How [`write_json`] lays out its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    /// One line, no closing newline.
    Compact,
    /// Indented, with a closing newline.
    Pretty,
}

/// Writes `value` as JSON to `path`.
///
/// # Errors
///
/// Returns [`Error::Io`] naming `path` when the value cannot be rendered or
/// the file cannot be written.
pub(crate) fn write_json<T: serde::Serialize + ?Sized>(
    path: &Path,
    value: &T,
    layout: Layout,
) -> Result<(), Error> {
    let rendered = match layout {
        Layout::Compact => serde_json::to_string(value),
        Layout::Pretty => serde_json::to_string_pretty(value).map(|text| format!("{text}\n")),
    };
    let text = rendered.map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source: io::Error::other(source),
    })?;
    write_file(path, text.as_bytes())
}

/// Writes the manifest beside the store.
///
/// # Errors
///
/// Returns [`Error::Io`] when the manifest cannot be rendered or written.
pub(crate) fn write_manifest(out: &Path, manifest: &serde_json::Value) -> Result<(), Error> {
    write_json(&out.join(MANIFEST_FILE), manifest, Layout::Pretty)
}

#[cfg(test)]
mod tests {
    use concept_store::builder::StoreBuilder;

    use super::{Error, PropertyKeys};

    /// A key the vocabulary never received is a defect in the build, and says
    /// so: it is not the capacity limit `TooMany` reports.
    #[test]
    fn an_unregistered_property_key_names_itself() {
        for (system, version, registered, missing) in [
            ("http://loinc.org", "2.83", "COMPONENT", "SCALE_TYP"),
            (
                "http://www.nlm.nih.gov/research/umls/rxnorm",
                "09082026",
                "TTY",
                "SAB",
            ),
        ] {
            let dir = tempfile::tempdir().expect("tempdir");
            let mut builder = StoreBuilder::create(&dir.path().join("store.redb"), system, version)
                .expect("store");
            let keys = PropertyKeys::write(&mut builder, &[registered]).expect("writes");
            assert_eq!(keys.key(registered).expect("registered"), 0);
            let error = keys.key(missing).expect_err("never registered");
            assert!(
                matches!(&error, Error::UnknownPropertyKey { key } if key == missing),
                "the key is named: {error:?}"
            );
            assert_eq!(
                error.to_string(),
                format!("the property key `{missing}` is not in the vocabulary")
            );
        }
    }
}
