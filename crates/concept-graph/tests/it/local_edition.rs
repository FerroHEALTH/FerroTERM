// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Builds the closure of the licensed release under `data/`, when present.
//!
//! Ignored by default (`.claude/rules/vendored-inputs.md`). Run with
//! `cargo nextest run -p concept-graph --run-ignored all`; it prints the
//! footprint numbers the architecture estimates.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use concept_graph::closure::Closure;
use concept_graph::csr::Csr;
use concept_graph::ordinal::Ordinal;
use concept_graph::persist::Hierarchy;
use rf2::component::{Concept, Relationship, Rows};
use rf2::constants;
use rf2::file::{ContentType, Release, ReleaseType};
use rf2::id::ConceptId;

fn local_release() -> Option<PathBuf> {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/snomed");
    std::fs::read_dir(data)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.is_dir())
}

#[test]
#[ignore = "needs a licensed RF2 release under data/snomed/"]
fn the_local_edition_closure_builds_and_fits_the_estimate() {
    let Some(root) = local_release() else {
        panic!("no release under data/snomed/");
    };
    let release = Release::open(&root, ReleaseType::Snapshot).expect("release opens");
    let started = Instant::now();
    let concept_file = release
        .of_type(&ContentType::Concept)
        .next()
        .expect("concept file");
    let mut ordinals: BTreeMap<ConceptId, Ordinal> = BTreeMap::new();
    let mut active = 0_u32;
    for concept in Rows::<_, Concept>::open(&concept_file.path).expect("header") {
        let concept = concept.expect("row parses");
        if concept.base.active {
            ordinals.insert(concept.id, Ordinal::new(active));
            active += 1;
        }
    }
    let relationship_file = release
        .of_type(&ContentType::Relationship)
        .next()
        .expect("relationship file");
    let mut edges = Vec::new();
    for relationship in Rows::<_, Relationship>::open(&relationship_file.path)
        .expect("header")
        .inferred()
    {
        let relationship = relationship.expect("row parses");
        if relationship.base.active
            && relationship.type_id == constants::IS_A
            && let (Some(child), Some(parent)) = (
                ordinals.get(&relationship.source_id),
                ordinals.get(&relationship.destination_id),
            )
        {
            edges.push((*child, *parent));
        }
    }
    let loaded = started.elapsed();
    let is_a = Csr::build(active, edges.iter().copied()).expect("edges in range");
    let closure = Closure::compute(&is_a).expect("the inferred hierarchy is acyclic");
    let computed = started.elapsed().saturating_sub(loaded);
    let hierarchy = Hierarchy { is_a, closure };
    let mut bytes = Vec::new();
    hierarchy.write_to(&mut bytes).expect("writes");
    let resident: usize = hierarchy
        .closure
        .ancestor_sets()
        .iter()
        .chain(hierarchy.closure.descendant_sets())
        .map(roaring::RoaringBitmap::serialized_size)
        .sum();
    println!(
        "active concepts {active}, is-a edges {}, load {loaded:?}, closure {computed:?}, closure bytes {resident}, artifact bytes {}",
        edges.len(),
        bytes.len()
    );
    // The SNOMED CT root concept subsumes every other active concept.
    let root = ordinals
        .get(&ConceptId::published(138_875_005))
        .copied()
        .expect("root concept present");
    assert_eq!(
        hierarchy.closure.descendants(root).len(),
        u64::from(active) - 1
    );
    assert!(hierarchy.closure.ancestors(root).is_empty());
    assert!(resident < 600 * 1024 * 1024, "closure {resident} bytes");
}
