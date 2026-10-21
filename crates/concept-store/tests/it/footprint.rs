// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Prints where the bytes of a built artifact go, table by table, for the
//! footprint work. Ignored by default: it reads the local artifact under
//! `artifacts/nl/` (a licensed edition, never committed).
#![expect(
    clippy::print_stdout,
    reason = "a diagnostic report a person reads from the test output"
)]

use std::path::PathBuf;

use concept_store::tables;
use redb::{
    ReadOnlyDatabase, ReadableDatabase, ReadableTableMetadata, TableDefinition, TableHandle,
};

fn local_artifact() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/nl/store.redb");
    path.exists().then_some(path)
}

fn report<K: redb::Key + 'static, V: redb::Value + 'static>(
    txn: &redb::ReadTransaction,
    def: TableDefinition<'_, K, V>,
) {
    let table = txn.open_table(def).expect("table opens");
    let stats = table.stats().expect("stats");
    let rows = table.len().expect("len");
    println!(
        "{:<18} rows {:>9}  stored {:>7} MiB  metadata {:>6} MiB  fragmented {:>6} MiB  leaves {:>8}  height {}",
        def.name(),
        rows,
        mib(stats.stored_bytes()),
        mib(stats.metadata_bytes()),
        mib(stats.fragmented_bytes()),
        stats.leaf_pages(),
        stats.tree_height(),
    );
}

#[test]
#[ignore = "needs the local NL artifact under artifacts/nl/"]
fn the_local_artifact_footprint_by_table() {
    let Some(path) = local_artifact() else {
        panic!("no artifact under artifacts/nl/");
    };
    let size = std::fs::metadata(&path).expect("metadata").len();
    println!("store.redb {} MiB", mib(size));
    for side in ["hierarchy.bin", "text.bin"] {
        if let Ok(meta) = std::fs::metadata(path.with_file_name(side)) {
            println!("{side} {} MiB", mib(meta.len()));
        }
    }
    let db = ReadOnlyDatabase::open(&path).expect("opens");
    let txn = db.begin_read().expect("read txn");
    report(&txn, tables::META);
    report(&txn, tables::CODES);
    report(&txn, tables::DESIGNATIONS);
    report(&txn, tables::PROPERTY_KEYS);
    report(&txn, tables::DESIGNATION_USES);
    report(&txn, tables::LANGUAGE_REFSETS);
    report(&txn, tables::ACCEPTABILITIES);
    // The columns are read into memory when the store opens, so each file's
    // size is what it adds to a served edition's resident memory.
    for name in tables::COLUMNS {
        let file = concept_store::column::Column::file(&path, name);
        let len = std::fs::metadata(&file).expect("the column file").len();
        println!("column {:<14} resident {:>7} MiB", name, mib(len));
    }
}

/// Bytes as whole mebibytes.
fn mib(bytes: u64) -> u64 {
    bytes >> 20
}
