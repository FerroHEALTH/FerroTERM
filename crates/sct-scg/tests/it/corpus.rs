// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every example of the official corpus parses, and its printed form parses
//! to the same tree (`vendor/examples`, the commit in `vendor/PROVENANCE.md`).

use std::path::{Path, PathBuf};

fn examples() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/examples");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("reads")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "txt"))
        .collect();
    files.sort();
    files
}

#[test]
fn every_example_of_the_corpus_parses_and_round_trips() {
    let files = examples();
    assert_eq!(files.len(), 23, "the vendored corpus");
    for file in files {
        let text = std::fs::read_to_string(&file).expect("reads");
        let tree =
            sct_scg::parse(&text).unwrap_or_else(|e| panic!("{}: {e}\n{text}", file.display()));
        let printed = tree.to_string();
        let again = sct_scg::parse(&printed)
            .unwrap_or_else(|e| panic!("{}: reparse: {e}\n{printed}", file.display()));
        assert_eq!(again, tree, "{}: {printed}", file.display());
    }
}

#[test]
fn the_corpus_covers_every_construct_the_examples_are_named_for() {
    let names: Vec<String> = examples()
        .iter()
        .filter_map(|path| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    for construct in [
        "simple_expression",
        "multiple_focus_concepts",
        "expression_with_refinement",
        "expression_with_attribute_group",
        "expression_with_nested_refinement",
        "expression_with_concrete_value",
        "expression_with_definition_type",
    ] {
        assert!(
            names.iter().any(|name| name.starts_with(construct)),
            "the corpus carries no {construct} example"
        );
    }
}
