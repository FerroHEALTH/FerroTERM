// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Property tests: every token of an indexed term finds it, paging is a
//! partition of the full result, and the layout round-trips.

use concept_graph::ordinal::Ordinal;
use designation_index::index::{IndexBuilder, Input, Query, TextIndex};
use designation_index::persist::{read_from, write_to};
use designation_index::tokenize::{fold, tokens};
use proptest::prelude::*;

fn term() -> impl Strategy<Value = String> {
    prop::collection::vec("[A-Za-zÀ-ÿ0-9]{1,6}", 1..4).prop_map(|words| words.join(" "))
}

fn corpus() -> impl Strategy<Value = Vec<(String, bool, u32)>> {
    prop::collection::vec((term(), any::<bool>(), 0_u32..3), 1..40)
}

fn build(rows: &[(String, bool, u32)]) -> TextIndex {
    let mut builder = IndexBuilder::new();
    for (position, (term, active, use_ordinal)) in rows.iter().enumerate() {
        builder
            .add(&Input {
                concept: Ordinal::new(u32::try_from(position).unwrap()),
                index: 0,
                term,
                language: "en",
                use_ordinal: *use_ordinal,
                active: *active,
                refsets: &[],
            })
            .unwrap();
    }
    builder.build().unwrap()
}

proptest! {
    #[test]
    fn folding_is_idempotent_and_tokens_are_folded(text in "\\PC{0,20}") {
        let once = fold(&text);
        prop_assert_eq!(fold(&once), once.clone());
        for token in tokens(&text) {
            prop_assert_eq!(fold(&token), token.clone());
            prop_assert!(token.chars().all(char::is_alphanumeric));
        }
    }

    #[test]
    fn every_token_finds_its_designation(rows in corpus()) {
        let index = build(&rows);
        for (position, (term, _, _)) in rows.iter().enumerate() {
            let ordinal = u32::try_from(position).unwrap();
            for token in tokens(term) {
                let hits = index.matches(&Query { text: token, ..Query::default() });
                prop_assert!(hits.contains(ordinal));
            }
            let all = index.matches(&Query { text: term.clone(), ..Query::default() });
            prop_assert!(all.contains(ordinal));
        }
    }

    #[test]
    fn pages_partition_the_result_and_the_layout_round_trips(rows in corpus(), page in 1_usize..5) {
        let index = build(&rows);
        let query = Query { text: String::new(), active_only: true, ..Query::default() };
        let all = index.search(&query, 0, rows.len());
        let mut paged = Vec::new();
        let mut offset = 0;
        loop {
            let hits = index.search(&query, offset, page);
            prop_assert_eq!(hits.total, all.total);
            if hits.designations.is_empty() {
                break;
            }
            paged.extend(hits.designations);
            offset += page;
        }
        prop_assert_eq!(paged, all.designations.clone());
        let mut bytes = Vec::new();
        write_to(&index, &mut bytes).unwrap();
        let back = read_from(&mut bytes.as_slice()).unwrap();
        prop_assert_eq!(back.search(&query, 0, rows.len()), all);
        prop_assert_eq!(back.len(), index.len());
        prop_assert_eq!(back.words(), index.words());
    }
}
