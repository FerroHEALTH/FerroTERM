// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! RFC 4647 §3.3.2 extended filtering over language tags, which BCP 47
//! subsumption reads each tag through.

use super::Tag;

/// The subtags a tag is matched by, lower case (RFC 4647 §3.3.2, step 1).
///
/// A grandfathered tag is one opaque subtag: its subtags "either do not
/// individually appear in the registry or appear but with a different semantic
/// meaning", and the tag "in its entirety, represents a language or collection
/// of languages" (RFC 5646 §2.2.8).
pub(super) fn match_subtags(tag: &Tag) -> Vec<String> {
    if let Some(grandfathered) = &tag.grandfathered {
        return vec![grandfathered.to_ascii_lowercase()];
    }
    tag.canonical()
        .split('-')
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Whether `range` matches `tag` under RFC 4647 §3.3.2 extended filtering.
///
/// Both lists come from [`match_subtags`], so neither carries the wildcard
/// that steps 1 and 3.A admit, and every comparison is already case-folded.
pub(super) fn extended_filter(range: &[String], tag: &[String]) -> bool {
    let (Some(first_range), Some(first_tag)) = (range.first(), tag.first()) else {
        return false;
    };
    if first_range != first_tag {
        return false;
    }
    let mut at_range = 1;
    let mut at_tag = 1;
    while let Some(wanted) = range.get(at_range) {
        let Some(found) = tag.get(at_tag) else {
            return false;
        };
        if wanted == found {
            at_range += 1;
            at_tag += 1;
        } else if found.len() == 1 {
            return false;
        } else {
            at_tag += 1;
        }
    }
    true
}
