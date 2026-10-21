// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: Apache-2.0

//! The `characteristicTypeId` filter over the relationship files.
//!
//! Appendix E.5 of the release file specification enumerates the column, and
//! only `900000000000011006 |Inferred relationship|` is part of the definition
//! of its source concept.

use std::path::Path;

use rf2::component::{Component, ConcreteRelationship, Relationship, Rows};
use rf2::constants;
use rf2::id::ConceptId;

use crate::fixture;

/// The whole enumeration appendix E.5 lists, inferred first.
fn characteristic_types() -> [ConceptId; 5] {
    [
        constants::INFERRED,
        constants::DEFINING,
        constants::STATED,
        constants::QUALIFYING,
        constants::ADDITIONAL,
    ]
}

/// An RF2 file with `columns` as its header and `rows` as its data.
fn file(columns: &[&str], rows: &[Vec<String>]) -> String {
    let mut text = columns.join("\t");
    text.push_str("\r\n");
    for row in rows {
        text.push_str(&row.join("\t"));
        text.push_str("\r\n");
    }
    text
}

/// One row per characteristic type, alike in everything else.
fn rows(value: &str, type_id: &str) -> Vec<Vec<String>> {
    characteristic_types()
        .into_iter()
        .enumerate()
        .map(|(index, characteristic_type)| {
            let item = u32::try_from(index).expect("five rows") + 1;
            vec![
                fixture::relationship(item),
                fixture::DATE.to_owned(),
                String::from("1"),
                fixture::extension_module(),
                fixture::concept(2),
                value.to_owned(),
                String::from("0"),
                type_id.to_owned(),
                characteristic_type.to_string(),
                fixture::EXISTENTIAL.to_owned(),
            ]
        })
        .collect()
}

#[test]
fn a_relationship_file_yields_only_its_inferred_rows() {
    let text = file(
        Relationship::COLUMNS,
        &rows(&fixture::concept(1), fixture::IS_A),
    );
    let path = Path::new("sct2_Relationship_Snapshot.txt");
    let every: Vec<Relationship> = Rows::new(path, text.as_bytes())
        .expect("header matches")
        .collect::<Result<_, _>>()
        .expect("rows parse");
    assert_eq!(every.len(), 5, "the file carries the whole enumeration");

    let inferred: Vec<Relationship> = Rows::new(path, text.as_bytes())
        .expect("header matches")
        .inferred()
        .collect::<Result<_, _>>()
        .expect("rows parse");
    assert_eq!(
        inferred
            .iter()
            .map(|r| r.characteristic_type_id)
            .collect::<Vec<_>>(),
        vec![constants::INFERRED],
        "the defining, stated, qualifying, and additional rows are left out"
    );
    assert_eq!(
        inferred.first().map(|r| r.id.to_string()),
        Some(fixture::relationship(1))
    );
}

#[test]
fn a_concrete_value_file_yields_only_its_inferred_rows() {
    let text = file(
        ConcreteRelationship::COLUMNS,
        &rows("#3", &fixture::concept(7)),
    );
    let path = Path::new("sct2_RelationshipConcreteValues_Snapshot.txt");
    let every: Vec<ConcreteRelationship> = Rows::new(path, text.as_bytes())
        .expect("header matches")
        .collect::<Result<_, _>>()
        .expect("rows parse");
    assert_eq!(every.len(), 5, "the file carries the whole enumeration");

    let inferred: Vec<ConcreteRelationship> = Rows::new(path, text.as_bytes())
        .expect("header matches")
        .inferred()
        .collect::<Result<_, _>>()
        .expect("rows parse");
    assert_eq!(
        inferred
            .iter()
            .map(|r| r.characteristic_type_id)
            .collect::<Vec<_>>(),
        vec![constants::INFERRED],
        "the defining, stated, qualifying, and additional rows are left out"
    );
}

#[test]
fn a_malformed_row_the_filter_would_drop_is_still_an_error() {
    // The qualifying row is the fourth, and its destination is corrupted, so a
    // reader that filtered before parsing would report success.
    let mut rows = rows(&fixture::concept(1), fixture::IS_A);
    if let Some(field) = rows.get_mut(3).and_then(|row| row.get_mut(5)) {
        *field = String::from("not-an-sctid");
    }
    let text = file(Relationship::COLUMNS, &rows);
    let outcome: Result<Vec<Relationship>, _> =
        Rows::<_, Relationship>::new(Path::new("sct2_Relationship_Snapshot.txt"), text.as_bytes())
            .expect("header matches")
            .inferred()
            .collect();
    assert!(
        outcome.is_err(),
        "a row the reader cannot parse is never silently dropped"
    );
}

#[test]
fn a_characteristic_type_outside_the_enumeration_is_left_out_and_counted() {
    // Appendix E.5 lists the values a release uses; a concept outside it is
    // neither the inferred view nor something to guess at.
    let mut rows = rows(&fixture::concept(1), fixture::IS_A);
    if let Some(field) = rows.get_mut(1).and_then(|row| row.get_mut(8)) {
        *field = fixture::concept(4);
    }
    let text = file(Relationship::COLUMNS, &rows);
    let mut inferred =
        Rows::<_, Relationship>::new(Path::new("sct2_Relationship_Snapshot.txt"), text.as_bytes())
            .expect("header matches")
            .inferred();
    let kept: Vec<Relationship> = inferred
        .by_ref()
        .collect::<Result<_, _>>()
        .expect("rows parse");
    assert_eq!(
        kept.iter()
            .map(|r| r.characteristic_type_id)
            .collect::<Vec<_>>(),
        vec![constants::INFERRED]
    );
    assert_eq!(
        inferred.skipped(),
        4,
        "the reader reports every row it left out, so an empty hierarchy is never unexplained"
    );
}
