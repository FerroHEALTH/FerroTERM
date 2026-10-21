// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every example of the official corpus parses, and its printed form parses
//! to the same tree (`vendor/examples`, the tag in `vendor/PROVENANCE.md`).

use std::path::{Path, PathBuf};

fn examples() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/examples");
    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("reads") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "txt") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[test]
fn every_example_of_the_corpus_parses_and_round_trips() {
    let files = examples();
    assert_eq!(files.len(), 121, "the vendored corpus");
    for file in files {
        let text = std::fs::read_to_string(&file).expect("reads");
        let tree =
            sct_ecl::parse(&text).unwrap_or_else(|e| panic!("{}: {e}\n{text}", file.display()));
        let printed = tree.to_string();
        let again = sct_ecl::parse(&printed)
            .unwrap_or_else(|e| panic!("{}: reparse: {e}\n{printed}", file.display()));
        assert_eq!(again, tree, "{}: {printed}", file.display());
    }
}
