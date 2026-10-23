// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The taxonomy tree's model: which rows are visible.
//!
//! The model is a plain value so the tree's behaviour is pinned by unit tests
//! rather than by a browser. It carries no code system knowledge: it is given
//! a root concept, the set of open codes, and whatever children have been read
//! for each of them, and it says what to draw.
//!
//! The keyboard and the tree view pattern are FerroUI's tree widget; this
//! model says which rows the address opens.

use std::collections::BTreeSet;

use crate::url::decode_query_component;
use crate::url::encode_query_component;

/// The separator between two codes in a list the address carries.
///
/// Every code is percent-encoded before it is joined, and the encoder escapes
/// this separator, so a code that contains one cannot split a list in two.
const SEPARATOR: char = ',';

/// The open codes as one value an address can carry.
///
/// The set is ordered, so the same open nodes always write the same address
/// and a link a reader shares is the one they were reading.
pub(crate) fn encode_open(open: &BTreeSet<String>) -> String {
    open.iter()
        .map(|code| encode_query_component(code))
        .collect::<Vec<String>>()
        .join(&SEPARATOR.to_string())
}

/// The open codes an address carries.
pub(crate) fn decode_open(value: &str) -> BTreeSet<String> {
    value
        .split(SEPARATOR)
        .filter(|part| !part.is_empty())
        .map(decode_query_component)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(codes: &[&str]) -> BTreeSet<String> {
        codes.iter().map(|code| (*code).to_owned()).collect()
    }

    #[test]
    fn the_open_nodes_round_trip_through_the_address() {
        let opened = open(&["a,b", "c d", "e"]);
        let written = encode_open(&opened);
        assert_eq!(written, "a%2Cb,c%20d,e");
        assert_eq!(
            decode_open(&written),
            opened,
            "a code carrying the separator or a space survives the address"
        );
    }

    #[test]
    fn an_address_that_opens_nothing_reads_as_no_open_nodes() {
        assert!(decode_open("").is_empty());
        assert!(
            decode_open(",,").is_empty(),
            "an empty part names no code, so it is left out"
        );
        assert_eq!(encode_open(&BTreeSet::new()), "");
    }
}
