// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Indexes the descriptions of the licensed release under `data/`, when present.
//!
//! Ignored by default (`.claude/rules/vendored-inputs.md`). Run with
//! `cargo nextest run -p designation-index --run-ignored all`; it prints the
//! footprint and query timings.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use concept_graph::ordinal::Ordinal;
use designation_index::index::{IndexBuilder, Input, Query};
use designation_index::persist::{read_from, write_to};
use rf2::component::{Description, Rows};
use rf2::constants;
use rf2::file::{ContentType, Release, ReleaseType};
use rf2::id::{DescriptionId, RefsetId};
use rf2::refset::{LanguageMember, Members};

fn local_release() -> Option<PathBuf> {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/snomed");
    std::fs::read_dir(data)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.is_dir())
}

/// Acceptable-or-preferred memberships per description, keyed by refset ordinal.
fn memberships(release: &Release) -> (BTreeMap<RefsetId, u32>, BTreeMap<DescriptionId, Vec<u32>>) {
    let mut refset_ordinals: BTreeMap<RefsetId, u32> = BTreeMap::new();
    let mut memberships: BTreeMap<DescriptionId, Vec<u32>> = BTreeMap::new();
    for file in release.refsets().filter(|f| f.name.summary == "Language") {
        let ContentType::Refset(kinds) = &file.name.content_type else {
            panic!("refset content type");
        };
        for member in Members::open(&file.path, kinds).expect("header") {
            let member = LanguageMember::try_from(member.expect("member parses")).expect("view");
            if !member.member.active {
                continue;
            }
            let next = u32::try_from(refset_ordinals.len()).unwrap();
            let refset = *refset_ordinals
                .entry(member.member.refset_id)
                .or_insert(next);
            let description = DescriptionId::try_from(member.member.referenced_component_id)
                .expect("description id");
            memberships.entry(description).or_default().push(refset);
        }
    }
    (refset_ordinals, memberships)
}

#[test]
#[ignore = "needs a licensed RF2 release under data/snomed/"]
fn the_local_edition_indexes_and_answers_prefix_queries() {
    let Some(root) = local_release() else {
        panic!("no release under data/snomed/");
    };
    let release = Release::open(&root, ReleaseType::Snapshot).expect("release opens");
    let started = Instant::now();
    let (refset_ordinals, memberships) = memberships(&release);
    let mut builder = IndexBuilder::new();
    let mut concepts: BTreeMap<_, (Ordinal, u32)> = BTreeMap::new();
    let mut rows = 0_u64;
    for file in release.of_type(&ContentType::Description) {
        for description in Rows::<_, Description>::open(&file.path).expect("header") {
            let description = description.expect("row parses");
            let next = u32::try_from(concepts.len()).unwrap();
            let slot = concepts
                .entry(description.concept_id)
                .or_insert((Ordinal::new(next), 0));
            let use_ordinal = if description.type_id == constants::FULLY_SPECIFIED_NAME {
                0
            } else if description.type_id == constants::SYNONYM {
                1
            } else {
                2
            };
            builder
                .add(&Input {
                    concept: slot.0,
                    index: slot.1,
                    term: &description.term,
                    language: &description.language_code,
                    use_ordinal,
                    active: description.base.active,
                    refsets: memberships.get(&description.id).map_or(&[], Vec::as_slice),
                })
                .expect("adds");
            slot.1 += 1;
            rows += 1;
        }
    }
    let loaded = started.elapsed();
    let index = builder.build().expect("builds");
    let built = started.elapsed().saturating_sub(loaded);
    let mut bytes = Vec::new();
    write_to(&index, &mut bytes).expect("writes");
    let reopened = Instant::now();
    let back = read_from(&mut bytes.as_slice()).expect("reads");
    let reopen = reopened.elapsed();
    let dutch = Query {
        text: "hart".to_owned(),
        language: Some("nl".to_owned()),
        active_only: true,
        ..Query::default()
    };
    let queried = Instant::now();
    let hits = back.search(&dutch, 0, 20);
    let query_time = queried.elapsed();
    let broad = Instant::now();
    let many = back.search(
        &Query {
            text: "a".to_owned(),
            ..Query::default()
        },
        0,
        20,
    );
    let broad_time = broad.elapsed();
    println!(
        "descriptions {rows}, concepts {}, words {}, language refsets {}, load {loaded:?}, build {built:?}, artifact {} bytes, reopen {reopen:?}, 'hart' nl active: {} hits in {query_time:?}, 'a' any: {} hits in {broad_time:?}",
        concepts.len(),
        index.words(),
        refset_ordinals.len(),
        bytes.len(),
        hits.total,
        many.total
    );
    assert_eq!(index.len(), usize::try_from(rows).unwrap());
    assert!(
        hits.total > 0,
        "the Dutch edition has active designations starting with 'hart'"
    );
    assert_eq!(hits.designations.len(), 20);
    assert_eq!(
        back.search(&dutch, 0, 20),
        hits,
        "deterministic across reopen"
    );
}
