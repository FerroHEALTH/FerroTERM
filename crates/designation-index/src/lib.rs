// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The designation search index.
//!
//! An `fst` map over designation word tokens plus roaring posting bitmaps per
//! word, with language, designation-use, language-reference-set, and status
//! filters as bitmaps, sorted by term length then designation ordinal. This
//! is the `$expand` `filter` and `$find-matches` engine for every loaded
//! code system. No spec governs the index format: our own design
//! (`docs/architecture.md` §Text search).
#![doc(test(attr(deny(warnings))))]

pub mod index;
pub mod persist;
pub mod tokenize;
