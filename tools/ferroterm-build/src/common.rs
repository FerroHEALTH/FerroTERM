// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the LOINC and `RxNorm` pipelines both do: number an index, name a file
//! failure, hold a designation until it is written, number the property keys,
//! and write the designations and the text index.
//!
//! Each pipeline keeps its own error type, so the shared items are generic
//! over it through [`PipelineError`]: a pipeline states the three failures
//! these items can produce, and the items stay one copy.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use concept_graph::ordinal::Ordinal;
use concept_store::builder::{BuildError, StoreBuilder};
use concept_store::record::Designation;
use concept_store::store::Vocabulary;
use designation_index::index::{IndexBuilder, Input};

use crate::pipeline::TEXT_FILE;

/// The failures the shared items produce, in a pipeline's own error type.
pub(crate) trait PipelineError:
    Sized
    + From<BuildError>
    + From<designation_index::index::BuildError>
    + From<designation_index::persist::PersistError>
{
    /// More concepts, designations, or keys than an ordinal can number.
    fn too_many() -> Self;

    /// A file that cannot be written.
    fn io(path: &Path, source: io::Error) -> Self;

    /// A property key the build asked for and never registered.
    fn unknown_property_key(key: &str) -> Self;
}

/// The ordinal of `index`.
///
/// # Errors
///
/// Returns the pipeline's capacity failure past `u32::MAX`.
pub(crate) fn ordinal<E: PipelineError>(index: usize) -> Result<Ordinal, E> {
    // A count past u32::MAX is the whole message; the conversion error adds nothing.
    let Ok(index) = u32::try_from(index) else {
        return Err(E::too_many());
    };
    Ok(Ordinal::new(index))
}

/// A closure that names `path` in the pipeline's I/O failure.
pub(crate) fn io_error<E: PipelineError>(path: &Path) -> impl FnOnce(io::Error) -> E + '_ {
    move |source| E::io(path, source)
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

/// The property keys of one release, by name, in one pipeline's error type.
pub(crate) struct PropertyKeys<E> {
    keys: BTreeMap<String, u32>,
    failure: std::marker::PhantomData<E>,
}

impl<E: PipelineError> PropertyKeys<E> {
    /// Writes `names` as the property-key vocabulary, in the order given, and
    /// returns the keys by name.
    ///
    /// # Errors
    ///
    /// Returns the pipeline's failure when the store does not take the
    /// vocabulary or there are more keys than an ordinal can number.
    pub(crate) fn write(builder: &mut StoreBuilder, names: &[String]) -> Result<Self, E> {
        let mut keys: BTreeMap<String, u32> = BTreeMap::new();
        for name in names {
            let key = ordinal::<E>(keys.len())?.index();
            builder.vocabulary(Vocabulary::PropertyKeys, key, name)?;
            keys.insert(name.clone(), key);
        }
        Ok(Self {
            keys,
            failure: std::marker::PhantomData,
        })
    }

    /// The key of `name`.
    ///
    /// # Errors
    ///
    /// Returns the pipeline's unknown-key failure when the vocabulary holds no
    /// such key, which is a defect in the build, never a capacity limit.
    pub(crate) fn key(&self, name: &str) -> Result<u32, E> {
        self.keys
            .get(name)
            .copied()
            .ok_or_else(|| E::unknown_property_key(name))
    }
}

/// How the text index keys a designation's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LanguageKey {
    /// The tag as the store holds it.
    Whole,
    /// The primary subtag alone: the index keys designations the way the
    /// providers query it, while the store keeps the full BCP 47 tag.
    PrimarySubtag,
}

/// Writes the designations into the store.
///
/// # Errors
///
/// Returns the pipeline's failure when the store refuses one.
pub(crate) fn write_designations<E: PipelineError>(
    builder: &mut StoreBuilder,
    placed: &[Placed],
) -> Result<(), E> {
    for p in placed {
        builder.designation(p.ordinal, p.index, &p.record)?;
    }
    Ok(())
}

/// Builds the text index over the designations, writes it beside the store,
/// and returns the number of words it holds.
///
/// # Errors
///
/// Returns the pipeline's failure when the index cannot be built or written.
pub(crate) fn write_text_index<E: PipelineError>(
    out: &Path,
    placed: &[Placed],
    language: LanguageKey,
) -> Result<usize, E> {
    let mut index = IndexBuilder::new();
    for p in placed {
        let tag = match language {
            LanguageKey::Whole => p.record.language.as_str(),
            LanguageKey::PrimarySubtag => p
                .record
                .language
                .split('-')
                .next()
                .unwrap_or(&p.record.language),
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
    let mut text_bytes = Vec::new();
    designation_index::persist::write_to(&index, &mut text_bytes)?;
    let text_path = out.join(TEXT_FILE);
    std::fs::write(&text_path, &text_bytes).map_err(io_error::<E>(&text_path))?;
    Ok(index.words())
}
