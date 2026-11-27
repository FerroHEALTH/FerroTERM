// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The byte encodings of the stored records: round trips and damage.

use concept_graph::ordinal::Ordinal;
use concept_store::record::{
    Acceptability, Concept, Designation, DesignationRef, Designations, Properties, PropertyValue,
    PropertyValueRef, RecordError,
};

#[test]
fn records_round_trip() {
    let concept = Concept {
        code: "123456789".to_owned(),
        active: true,
        effective_time: Some("20260101".to_owned()),
        module: Some(Ordinal::new(7)),
    };
    assert_eq!(Concept::decode(&concept.encode()), Ok(concept));
    let designation = Designation {
        id: None,
        term: "Synthetisch kind".to_owned(),
        language: "nl".to_owned(),
        use_ordinal: 2,
        active: false,
    };
    assert_eq!(Designation::decode(&designation.encode()), Ok(designation));
    let values = vec![
        PropertyValue::Concept(Ordinal::new(3)),
        PropertyValue::Code("x".to_owned()),
        PropertyValue::String("s".to_owned()),
        PropertyValue::Integer(-42),
        PropertyValue::Boolean(true),
        PropertyValue::Decimal("2.50".to_owned()),
        PropertyValue::DateTime("2026-01-01".to_owned()),
    ];
    let encoded = PropertyValue::encode_list(&values);
    let decoded = PropertyValueRef::decode_list(&encoded).expect("decodes");
    assert_eq!(
        decoded
            .into_iter()
            .map(PropertyValue::from)
            .collect::<Vec<_>>(),
        values
    );
}

#[test]
fn the_per_concept_records_round_trip() {
    let first = Designation {
        id: Some("1".to_owned()),
        term: "Kat".to_owned(),
        language: "nl".to_owned(),
        use_ordinal: 1,
        active: true,
    };
    let second = Designation {
        id: None,
        term: "Cat".to_owned(),
        language: "en".to_owned(),
        use_ordinal: 0,
        active: false,
    };
    let (a, b) = (first.encode(), second.encode());
    // Index 2 is missing, so a reader that asks by index must not be
    // handed the record that happens to sit in that position.
    let packed = Designations::encode(&[(0, a.as_slice()), (3, b.as_slice())]);
    assert_eq!(
        Designations::decode(&packed),
        Ok(vec![first.clone(), second.clone()])
    );
    let at = |index| Designations::at(&packed, index).map(|d| d.map(Designation::from));
    assert_eq!(at(0), Ok(Some(first)));
    assert_eq!(at(3), Ok(Some(second)));
    assert_eq!(at(2), Ok(None));
    let mut seen = Vec::new();
    Designations::scan(&packed, |d: DesignationRef<'_>| {
        seen.push(d.term);
        false
    })
    .expect("scans");
    assert_eq!(
        seen,
        ["Kat"],
        "the scan stops when the visitor answers false"
    );

    let values = PropertyValue::encode_list(&[PropertyValue::Concept(Ordinal::new(4))]);
    let packed = Properties::encode(&[(7, values.as_slice()), (9, &[0, 0, 0, 0])]);
    assert_eq!(
        Properties::decode(&packed),
        Ok(vec![
            (7, vec![PropertyValueRef::Concept(Ordinal::new(4))]),
            (9, Vec::new()),
        ])
    );
    assert_eq!(
        Properties::find(&packed, 7),
        Ok(vec![PropertyValueRef::Concept(Ordinal::new(4))])
    );
    assert_eq!(Properties::find(&packed, 8), Ok(Vec::new()));

    let packed = Acceptability::encode(&[(0, 1, 0), (0, 2, 1)]);
    assert_eq!(
        Acceptability::decode(&packed),
        Ok(vec![(0, 1, 0), (0, 2, 1)])
    );
    assert_eq!(Acceptability::find(&packed, 0, 2), Ok(Some(1)));
    assert_eq!(Acceptability::find(&packed, 1, 2), Ok(None));
}

#[test]
fn damaged_per_concept_records_are_refused() {
    let packed = Designations::encode(&[(0, b"x".as_slice())]);
    assert!(matches!(
        Designations::decode(&packed[..6]),
        Err(RecordError::Truncated { .. })
    ));
    let packed = Properties::encode(&[(0, &[1, 0, 0, 0, 9])]);
    assert!(matches!(
        Properties::decode(&packed),
        Err(RecordError::Tag { tag: 9 })
    ));
    assert!(matches!(
        Acceptability::decode(&[2, 0, 0, 0, 0, 0, 0, 0]),
        Err(RecordError::Truncated { .. })
    ));
}

#[test]
fn damaged_records_are_refused() {
    let bytes = Concept {
        code: "1".to_owned(),
        active: true,
        effective_time: None,
        module: None,
    }
    .encode();
    assert!(matches!(
        Concept::decode(&bytes[..3]),
        Err(RecordError::Truncated { .. })
    ));
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        Concept::decode(&trailing),
        Err(RecordError::Trailing { remaining: 1 })
    ));
    assert!(matches!(
        PropertyValueRef::decode_list(&[1, 0, 0, 0, 9]),
        Err(RecordError::Tag { tag: 9 })
    ));
    assert!(matches!(
        Designation::decode(&[0, 1, 0, 0, 0, 0xff]),
        Err(RecordError::Utf8 { .. })
    ));
}
