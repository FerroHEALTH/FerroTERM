// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Building and querying the index.

use std::collections::BTreeMap;

use fst::automaton::Str;
use fst::{Automaton, IntoStreamer, Map, MapBuilder, Streamer};
use roaring::RoaringBitmap;

use concept_graph::ordinal::Ordinal;

use crate::tokenize::{prefixes, tokens};

/// A failure while building the index.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// The word dictionary could not be built.
    #[error("cannot build the word dictionary")]
    Fst(#[from] fst::Error),
    /// More designations than the index can address.
    #[error("too many designations for a u32 ordinal")]
    TooMany(#[source] std::num::TryFromIntError),
}

/// One indexed designation, by the concept and designation index it names in the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The concept ordinal.
    pub concept: Ordinal,
    /// The designation index within the concept.
    pub index: u32,
    /// The length of the term in characters, capped at `u16::MAX`.
    pub term_length: u16,
}

/// A designation offered to the builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input<'a> {
    /// The concept ordinal.
    pub concept: Ordinal,
    /// The designation index within the concept.
    pub index: u32,
    /// The term.
    pub term: &'a str,
    /// The BCP 47 language.
    pub language: &'a str,
    /// The designation use ordinal (the store's `DESIGNATION_USES`).
    pub use_ordinal: u32,
    /// Whether the designation is active.
    pub active: bool,
    /// The language reference sets the designation is acceptable or preferred in.
    pub refsets: &'a [u32],
}

/// Collects designations, then builds the [`TextIndex`].
#[derive(Debug, Default)]
pub struct IndexBuilder {
    words: BTreeMap<String, RoaringBitmap>,
    entries: Vec<Entry>,
    languages: BTreeMap<String, RoaringBitmap>,
    uses: BTreeMap<u32, RoaringBitmap>,
    refsets: BTreeMap<u32, RoaringBitmap>,
    active: RoaringBitmap,
    lengths: BTreeMap<u16, RoaringBitmap>,
}

impl IndexBuilder {
    /// An empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a designation; the returned designation ordinal is its position.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::TooMany`] past `u32::MAX` designations.
    pub fn add(&mut self, input: &Input<'_>) -> Result<u32, BuildError> {
        let ordinal = u32::try_from(self.entries.len()).map_err(BuildError::TooMany)?;
        let term_length = u16::try_from(input.term.chars().count()).unwrap_or(u16::MAX);
        self.entries.push(Entry {
            concept: input.concept,
            index: input.index,
            term_length,
        });
        self.lengths.entry(term_length).or_default().insert(ordinal);
        for word in tokens(input.term) {
            self.words.entry(word).or_default().insert(ordinal);
        }
        self.languages
            .entry(input.language.to_owned())
            .or_default()
            .insert(ordinal);
        self.uses
            .entry(input.use_ordinal)
            .or_default()
            .insert(ordinal);
        for refset in input.refsets {
            self.refsets.entry(*refset).or_default().insert(ordinal);
        }
        if input.active {
            self.active.insert(ordinal);
        }
        Ok(ordinal)
    }

    /// Builds the index.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Fst`] when the dictionary cannot be built.
    pub fn build(self) -> Result<TextIndex, BuildError> {
        let mut dictionary = MapBuilder::memory();
        let mut postings = Vec::with_capacity(self.words.len());
        for (position, (word, bitmap)) in self.words.into_iter().enumerate() {
            dictionary.insert(word.as_bytes(), u64::try_from(position).unwrap_or(u64::MAX))?;
            postings.push(bitmap);
        }
        let bytes = dictionary.into_inner()?;
        Ok(TextIndex {
            dictionary: Map::new(bytes)?,
            postings,
            entries: self.entries,
            languages: self.languages,
            uses: self.uses,
            refsets: self.refsets,
            active: self.active,
            lengths: self.lengths,
        })
    }
}

/// A search request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// The free text; every word is a prefix that must match some word of the term.
    pub text: String,
    /// Keep only designations in this BCP 47 language.
    pub language: Option<String>,
    /// Keep only designations acceptable or preferred in this language reference set.
    pub refset: Option<u32>,
    /// Keep only designations of these uses.
    pub uses: Option<Vec<u32>>,
    /// Keep only active designations.
    pub active_only: bool,
}

/// One page of results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hits {
    /// The number of designations matching before paging.
    pub total: u64,
    /// The designation ordinals of this page, shortest term first, then by ordinal.
    pub designations: Vec<u32>,
}

/// The built index.
#[derive(Debug)]
pub struct TextIndex {
    dictionary: Map<Vec<u8>>,
    postings: Vec<RoaringBitmap>,
    entries: Vec<Entry>,
    languages: BTreeMap<String, RoaringBitmap>,
    uses: BTreeMap<u32, RoaringBitmap>,
    refsets: BTreeMap<u32, RoaringBitmap>,
    active: RoaringBitmap,
    lengths: BTreeMap<u16, RoaringBitmap>,
}

impl TextIndex {
    /// Reassembles an index from its parts.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::Fst`] when the dictionary bytes are not an fst map.
    pub fn from_parts(parts: OwnedParts) -> Result<Self, BuildError> {
        Ok(Self {
            dictionary: Map::new(parts.dictionary)?,
            postings: parts.postings,
            entries: parts.entries,
            languages: parts.languages,
            uses: parts.uses,
            refsets: parts.refsets,
            active: parts.active,
            lengths: parts.lengths,
        })
    }

    /// The heap bytes the dictionary holds
    /// (`concept_graph::footprint`).
    ///
    /// `fst` keeps the map as the one byte buffer it was built from, so this
    /// is that buffer
    /// (<https://docs.rs/fst/0.4.7/fst/struct.Map.html#method.as_fst>).
    #[must_use]
    pub fn dictionary_size_in_bytes(&self) -> usize {
        self.dictionary.as_fst().as_inner().capacity()
    }

    /// The heap bytes the word postings hold.
    #[must_use]
    pub fn postings_size_in_bytes(&self) -> usize {
        concept_graph::footprint::bitmaps(&self.postings)
    }

    /// The heap bytes the designation entries hold.
    #[must_use]
    pub fn entries_size_in_bytes(&self) -> usize {
        concept_graph::footprint::vector(&self.entries)
    }

    /// The heap bytes the language, use, reference set, active, and term
    /// length filters hold.
    #[must_use]
    pub fn filters_size_in_bytes(&self) -> usize {
        let sets = |total: usize, set: &RoaringBitmap| {
            total.saturating_add(concept_graph::footprint::bitmap(set))
        };
        let tags = self
            .languages
            .keys()
            .fold(0_usize, |total, tag| total.saturating_add(tag.capacity()));
        [
            self.languages.values().fold(
                concept_graph::footprint::entries::<String, RoaringBitmap>(self.languages.len()),
                sets,
            ),
            self.uses.values().fold(
                concept_graph::footprint::entries::<u32, RoaringBitmap>(self.uses.len()),
                sets,
            ),
            self.refsets.values().fold(
                concept_graph::footprint::entries::<u32, RoaringBitmap>(self.refsets.len()),
                sets,
            ),
            self.lengths.values().fold(
                concept_graph::footprint::entries::<u16, RoaringBitmap>(self.lengths.len()),
                sets,
            ),
            concept_graph::footprint::bitmap(&self.active),
            tags,
        ]
        .into_iter()
        .fold(0_usize, usize::saturating_add)
    }

    /// The heap bytes the whole index holds.
    #[must_use]
    pub fn size_in_bytes(&self) -> usize {
        self.dictionary_size_in_bytes()
            .saturating_add(self.postings_size_in_bytes())
            .saturating_add(self.entries_size_in_bytes())
            .saturating_add(self.filters_size_in_bytes())
    }

    /// The number of indexed designations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is indexed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The number of distinct words.
    #[must_use]
    pub fn words(&self) -> usize {
        self.postings.len()
    }

    /// The entry of a designation ordinal.
    #[must_use]
    pub fn entry(&self, designation: u32) -> Option<Entry> {
        self.entries
            .get(usize::try_from(designation).unwrap_or(usize::MAX))
            .copied()
    }

    /// The designations whose terms contain a word starting with `prefix`.
    #[must_use]
    pub fn prefix_postings(&self, prefix: &str) -> RoaringBitmap {
        let mut out = RoaringBitmap::new();
        let mut stream = self
            .dictionary
            .search(Str::new(prefix).starts_with())
            .into_stream();
        while let Some((_, position)) = stream.next() {
            if let Some(bitmap) = usize::try_from(position)
                .ok()
                .and_then(|p| self.postings.get(p))
            {
                out |= bitmap;
            }
        }
        out
    }

    /// The designations matching `query`, before paging, as a bitmap.
    #[must_use]
    pub fn matches(&self, query: &Query) -> RoaringBitmap {
        let words = prefixes(&query.text);
        let mut result: Option<RoaringBitmap> = None;
        for word in &words {
            let postings = self.prefix_postings(word);
            result = Some(match result {
                None => postings,
                Some(current) => current & postings,
            });
        }
        let mut result = result.unwrap_or_else(|| self.active.clone() | self.inactive());
        if let Some(language) = &query.language {
            result &= self.languages.get(language).unwrap_or(&EMPTY);
        }
        if let Some(refset) = query.refset {
            result &= self.refsets.get(&refset).unwrap_or(&EMPTY);
        }
        if let Some(uses) = &query.uses {
            let mut allowed = RoaringBitmap::new();
            for use_ordinal in uses {
                allowed |= self.uses.get(use_ordinal).unwrap_or(&EMPTY);
            }
            result &= allowed;
        }
        if query.active_only {
            result &= &self.active;
        }
        result
    }

    fn inactive(&self) -> RoaringBitmap {
        let mut all = RoaringBitmap::new();
        all.insert_range(0..u32::try_from(self.entries.len()).unwrap_or(u32::MAX));
        all - &self.active
    }

    /// Runs `query` and returns the page at `offset` of `count` results, ordered
    /// by term length then designation ordinal.
    #[must_use]
    pub fn search(&self, query: &Query, offset: usize, count: usize) -> Hits {
        let matches = self.matches(query);
        let total = matches.len();
        let mut ranked: Vec<(u16, u32)> = matches
            .iter()
            .map(|d| (self.entry(d).map_or(u16::MAX, |e| e.term_length), d))
            .collect();
        ranked.sort_unstable();
        let designations = ranked
            .into_iter()
            .skip(offset)
            .take(count)
            .map(|(_, d)| d)
            .collect();
        Hits {
            total,
            designations,
        }
    }

    /// The parts of the index, for persistence.
    #[must_use]
    pub fn parts(&self) -> Parts<'_> {
        Parts {
            dictionary: self.dictionary.as_fst().as_bytes(),
            postings: &self.postings,
            entries: &self.entries,
            languages: &self.languages,
            uses: &self.uses,
            refsets: &self.refsets,
            active: &self.active,
            lengths: &self.lengths,
        }
    }
}

static EMPTY: std::sync::LazyLock<RoaringBitmap> = std::sync::LazyLock::new(RoaringBitmap::new);

/// Borrowed views of an index's parts.
#[derive(Debug)]
pub struct Parts<'a> {
    /// The fst map bytes.
    pub dictionary: &'a [u8],
    /// The posting bitmap per word position.
    pub postings: &'a [RoaringBitmap],
    /// The entries by designation ordinal.
    pub entries: &'a [Entry],
    /// The designations per language.
    pub languages: &'a BTreeMap<String, RoaringBitmap>,
    /// The designations per use.
    pub uses: &'a BTreeMap<u32, RoaringBitmap>,
    /// The designations per language reference set.
    pub refsets: &'a BTreeMap<u32, RoaringBitmap>,
    /// The active designations.
    pub active: &'a RoaringBitmap,
    /// The designations per term length, the ranking order.
    pub lengths: &'a BTreeMap<u16, RoaringBitmap>,
}

/// Owned parts of an index, for [`TextIndex::from_parts`].
#[derive(Debug, Default)]
pub struct OwnedParts {
    /// The fst map bytes.
    pub dictionary: Vec<u8>,
    /// The posting bitmap per word position.
    pub postings: Vec<RoaringBitmap>,
    /// The entries by designation ordinal.
    pub entries: Vec<Entry>,
    /// The designations per language.
    pub languages: BTreeMap<String, RoaringBitmap>,
    /// The designations per use.
    pub uses: BTreeMap<u32, RoaringBitmap>,
    /// The designations per language reference set.
    pub refsets: BTreeMap<u32, RoaringBitmap>,
    /// The active designations.
    pub active: RoaringBitmap,
    /// The designations per term length.
    pub lengths: BTreeMap<u16, RoaringBitmap>,
}

#[cfg(test)]
mod tests {
    use super::{IndexBuilder, Input, Query};
    use concept_graph::ordinal::Ordinal;

    fn index() -> super::TextIndex {
        let mut builder = IndexBuilder::new();
        let rows = [
            (0, 0, "Heart failure", "en", 1, true, vec![0]),
            (0, 1, "Cardiac failure", "en", 1, true, vec![0, 1]),
            (0, 2, "Hartfalen", "nl", 1, true, vec![2]),
            (1, 0, "Heart", "en", 1, true, vec![0]),
            (2, 0, "Heart transplant", "en", 0, false, vec![]),
        ];
        for (concept, index, term, language, use_ordinal, active, refsets) in rows {
            builder
                .add(&Input {
                    concept: Ordinal::new(concept),
                    index,
                    term,
                    language,
                    use_ordinal,
                    active,
                    refsets: &refsets,
                })
                .expect("adds");
        }
        builder.build().expect("builds")
    }

    #[test]
    fn prefixes_match_any_word_and_intersect_across_words() {
        let index = index();
        let hits = index.search(
            &Query {
                text: "hea".to_owned(),
                ..Query::default()
            },
            0,
            10,
        );
        assert_eq!(hits.total, 3);
        // Shortest term first, then ordinal.
        assert_eq!(hits.designations, vec![3, 0, 4]);
        let both = index.search(
            &Query {
                text: "fail hea".to_owned(),
                ..Query::default()
            },
            0,
            10,
        );
        assert_eq!(both.designations, vec![0]);
        let none = index.search(
            &Query {
                text: "xyz".to_owned(),
                ..Query::default()
            },
            0,
            10,
        );
        assert_eq!(none.total, 0);
    }

    #[test]
    fn filters_are_bitmap_intersections() {
        let index = index();
        let dutch = index.search(
            &Query {
                text: "hart".to_owned(),
                language: Some("nl".to_owned()),
                ..Query::default()
            },
            0,
            10,
        );
        assert_eq!(dutch.designations, vec![2]);
        let refset = index.search(
            &Query {
                text: String::new(),
                refset: Some(1),
                ..Query::default()
            },
            0,
            10,
        );
        assert_eq!(refset.designations, vec![1]);
        let active = index.search(
            &Query {
                text: "heart".to_owned(),
                active_only: true,
                ..Query::default()
            },
            0,
            10,
        );
        assert_eq!(active.designations, vec![3, 0]);
        let fsn = index.search(
            &Query {
                text: String::new(),
                uses: Some(vec![0]),
                ..Query::default()
            },
            0,
            10,
        );
        assert_eq!(fsn.designations, vec![4]);
    }

    #[test]
    fn paging_is_stable() {
        let index = index();
        let all = index.search(&Query::default(), 0, 10);
        assert_eq!(all.total, 5);
        let first = index.search(&Query::default(), 0, 2);
        let second = index.search(&Query::default(), 2, 2);
        let third = index.search(&Query::default(), 4, 2);
        let mut joined = first.designations.clone();
        joined.extend(second.designations);
        joined.extend(third.designations);
        assert_eq!(joined, all.designations);
        assert_eq!(
            index.entry(2).map(|e| (e.concept, e.index, e.term_length)),
            Some((Ordinal::new(0), 2, 9))
        );
    }
}
