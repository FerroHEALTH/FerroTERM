// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The generated edition the benchmarks measure over: it opens, and it holds
//! what it says it holds.
//!
//! The benches assert latency; these assert that the edition behind them is a
//! real one, so a bench that answers fast is not answering over an empty
//! store.

use std::sync::Arc;

use concept_graph::subsumption::Outcome;
use fhir_terminology::compose::{Expander, Options};
use fhir_terminology::provider::CodeSystemProvider;
use fhir_terminology::registry::Registry;
use fhir_terminology::snomed::{SYSTEM, SnomedProvider};

use ferroterm_testkit::scaled;

/// Small enough to write in a test, large enough to have a real tree.
const CONCEPTS: u32 = 500;

fn provider() -> (tempfile::TempDir, SnomedProvider) {
    let dir = tempfile::tempdir().expect("tempdir");
    scaled::write(dir.path(), CONCEPTS).expect("writes the edition");
    let provider = SnomedProvider::open(dir.path(), "en").expect("opens");
    (dir, provider)
}

#[test]
fn every_concept_is_locatable_and_displays_its_preferred_term() {
    let (_dir, p) = provider();
    for ordinal in [0, 1, 8, 63, CONCEPTS - 1] {
        let code = scaled::code(ordinal);
        let found = p
            .locate(&code)
            .expect("reads")
            .unwrap_or_else(|| panic!("{code} is in the edition"));
        assert_eq!(
            p.display(found.concept, Some("en")).expect("reads"),
            Some(format!("Synthetic concept {ordinal}")),
            "the display is the preferred synonym, never the fully specified name"
        );
        assert_eq!(
            p.display(found.concept, Some("nl")).expect("reads"),
            Some(format!("Synthetisch concept {ordinal}")),
            "the Dutch synonym is the preferred term of its language"
        );
    }
}

#[test]
fn the_root_subsumes_every_other_concept() {
    let (_dir, p) = provider();
    let hierarchy = p.hierarchy().expect("snomed declares subsumption");
    let root = p
        .locate(&scaled::code(scaled::ROOT))
        .expect("reads")
        .expect("the root")
        .concept;
    for ordinal in [1, 9, 100, CONCEPTS - 1] {
        let other = p
            .locate(&scaled::code(ordinal))
            .expect("reads")
            .expect("in the edition")
            .concept;
        assert_eq!(
            hierarchy.subsumes(root, other),
            Outcome::Subsumes,
            "the root is above concept {ordinal}"
        );
    }
}

#[test]
fn the_implicit_descendants_value_set_holds_the_whole_edition() {
    let (_dir, p) = provider();
    let root = scaled::code(scaled::ROOT);
    let mut registry = Registry::new();
    registry.register(Arc::new(p)).expect("registers");
    let value_set = registry
        .implicit_value_set(&format!("{SYSTEM}?fhir_vs=isa/{root}"))
        .expect("snomed claims the URI")
        .expect("well formed");
    let expansion = Expander::new(&registry)
        .expand(
            &value_set,
            &Options {
                count: Some(10),
                offset: 100,
                ..Options::default()
            },
        )
        .expect("expands");
    assert_eq!(
        expansion.total,
        u64::from(CONCEPTS),
        "self-inclusive descendants of the root are the whole edition"
    );
    assert_eq!(expansion.items.len(), 10, "the page is the count asked for");
}

#[test]
fn a_served_edition_holds_one_copy_of_each_column_it_read() {
    // The residual a served edition carries over its counted structures was
    // the columns arriving twice, as the value the database materialized and
    // as the copy the store served from (#641). A column read from its own
    // file is its file, less the count word the reader consumes.
    let (dir, p) = provider();
    let store = dir.path().join("store.redb");
    for (column, bytes) in p.footprint().store_columns {
        let file = concept_store::column::Column::file(&store, column);
        let size = std::fs::metadata(&file).expect("the column file").len();
        assert_eq!(
            usize::try_from(size).expect("a column file fits usize"),
            bytes + 4,
            "the {column} column holds its file and nothing beside it"
        );
    }
}

#[test]
fn every_structure_of_a_served_edition_reports_what_it_holds() {
    let (_dir, p) = provider();
    let held = p.footprint();
    for (column, bytes) in held.store_columns {
        assert!(bytes > 0, "the {column} column holds bytes");
    }
    for (name, bytes) in [
        ("is-a adjacency", held.is_a),
        ("closure bitmaps", held.closure),
        ("child adjacency", held.children),
        ("text index", held.text),
        ("member tables", held.member_tables),
        ("attribute rows", held.attributes),
        ("memberships", held.memberships),
    ] {
        assert!(bytes > 0, "the {name} holds bytes");
    }
    // The generated edition states no attribute relationships and no alternate
    // identifiers, so those two structures are empty by construction.
    assert_eq!(
        held.attributes_inverted, 0,
        "no attribute type, so no inverted index"
    );
    assert_eq!(held.identifiers, 0, "the edition states no alternate ids");
}
