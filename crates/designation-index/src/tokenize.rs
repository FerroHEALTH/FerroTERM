// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Word tokens for indexing and querying.
//!
//! A term is decomposed (NFD), its combining marks dropped, lowercased, and
//! split on every character that is not alphanumeric, so `Ménière's disease`
//! yields `meniere`, `s`, `disease`. The same fold applies to a query prefix,
//! which makes matching case- and diacritic-insensitive. SNOMED's search
//! guidance is per-word prefix matching in any order; the fold itself is our
//! own design.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Folds `text`: NFD, combining marks removed, lowercase.
#[must_use]
pub fn fold(text: &str) -> String {
    text.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

/// The distinct word tokens of `term`, sorted.
#[must_use]
pub fn tokens(term: &str) -> Vec<String> {
    let folded = fold(term);
    let mut words: Vec<String> = folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect();
    words.sort();
    words.dedup();
    words
}

/// The folded, non-empty prefixes of a query, in query order.
#[must_use]
pub fn prefixes(query: &str) -> Vec<String> {
    fold(query)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{fold, prefixes, tokens};

    #[test]
    fn folding_drops_case_and_diacritics() {
        assert_eq!(fold("Ménière"), "meniere");
        assert_eq!(fold("Straße"), "straße");
        assert_eq!(fold("ÅNGSTRÖM"), "angstrom");
    }

    #[test]
    fn tokens_are_distinct_sorted_words() {
        assert_eq!(
            tokens("Ménière's disease (disorder)"),
            vec!["disease", "disorder", "meniere", "s"]
        );
        assert_eq!(tokens("Heart heart HEART"), vec!["heart"]);
        assert_eq!(tokens("  --  "), Vec::<String>::new());
        assert_eq!(tokens("COVID-19"), vec!["19", "covid"]);
    }

    #[test]
    fn prefixes_keep_query_order() {
        assert_eq!(prefixes("Hart fal"), vec!["hart", "fal"]);
        assert_eq!(prefixes("  "), Vec::<String>::new());
    }
}
