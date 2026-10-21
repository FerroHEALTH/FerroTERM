// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Word-prefix matching over designations without a built text index.
//!
//! The fold and the word rules are `designation-index`'s, so a resource-backed
//! system and an indexed edition answer a text filter alike.

use designation_index::tokenize::{fold, prefixes};

/// The folded, non-empty words of a query (`designation-index`'s rule).
#[must_use]
pub fn query_words(query: &str) -> Vec<String> {
    prefixes(query)
}

/// Whether every query word is a prefix of some word of some term.
#[must_use]
pub fn matches_all(words: &[String], terms: &[&str]) -> bool {
    if words.is_empty() {
        return true;
    }
    let folded: Vec<String> = terms.iter().map(|t| fold(t)).collect();
    words.iter().all(|word| {
        folded.iter().any(|term| {
            term.split(|c: char| !c.is_alphanumeric())
                .any(|t| t.starts_with(word.as_str()))
        })
    })
}

/// Whether two BCP 47 tags share a primary language subtag, case-insensitively.
#[must_use]
pub fn same_language(a: &str, b: &str) -> bool {
    let primary = |t: &str| t.split(['-', '_']).next().unwrap_or(t).to_ascii_lowercase();
    primary(a) == primary(b)
}

#[cfg(test)]
mod tests {
    use super::{matches_all, query_words, same_language};

    #[test]
    fn prefixes_match_folded_words_in_any_order() {
        let words = query_words("Fail HEART");
        assert!(matches_all(&words, &["Heart failure"]));
        assert!(!matches_all(&words, &["Heart"]));
        assert!(matches_all(&query_words("meni"), &["Ménière's disease"]));
        assert!(matches_all(&query_words(""), &[]));
        assert!(same_language("en-GB", "EN"));
        assert!(!same_language("nl", "en"));
    }
}
