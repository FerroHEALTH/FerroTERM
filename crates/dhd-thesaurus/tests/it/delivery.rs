// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reader over the Uitleverformaat 5.0 tables.

use ::classification::{Class, PREFERRED};
use ::dhd_thesaurus::{DUTCH, DhdError, ICD10, MAPPING, REPLACED_BY, ROLE, SNOMED, UMBRELLA, read};
use ferroterm_testkit::dhd::{
    FRACTURE, FRACTURE_SCTID, INJURY, OLD, SPRAIN, VERSION, write_delivery,
};

fn class<'a>(classes: &'a [Class], code: &str) -> &'a Class {
    classes.iter().find(|c| c.code == code).expect("class")
}

fn texts<'a>(class: &'a Class, kind: &str) -> Vec<&'a str> {
    class
        .rubrics
        .iter()
        .filter(|r| r.kind == kind)
        .map(|r| r.text.as_str())
        .collect()
}

#[test]
fn the_delivery_reads_into_a_flat_classification() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = write_delivery(dir.path()).expect("writes");
    let thesaurus = read(&root, None).expect("reads");
    let c = &thesaurus.classification;
    assert_eq!(c.name, "Diagnosethesaurus");
    assert_eq!(
        c.version.as_deref(),
        Some(VERSION),
        "from the directory name"
    );
    assert_eq!(c.kinds, ["Diagnose"]);
    assert_eq!(c.hierarchy, None, "a flat table");
    assert_eq!(c.designation_kinds[0], PREFERRED);
    assert_eq!(c.classes.len(), 4);

    let fracture = class(&c.classes, FRACTURE);
    assert!(fracture.active);
    assert_eq!(
        texts(fracture, PREFERRED),
        ["fractuur van de synthetische knobbel"]
    );
    assert_eq!(texts(fracture, "synonym"), ["knobbelfractuur"]);
    assert_eq!(texts(fracture, "pvt"), ["gebroken knobbel"]);
    assert!(
        texts(fracture, "search").is_empty(),
        "a term ended before the delivery date is skipped"
    );
    let fsn = fracture
        .rubrics
        .iter()
        .find(|r| r.kind == "fsn")
        .expect("fsn");
    assert_eq!(fsn.language, "en-GB");
    assert_eq!(texts(fracture, SNOMED), [FRACTURE_SCTID]);
    assert_eq!(texts(fracture, ICD10), ["Z99.1", "Z99.0"], "table order");
    assert_eq!(texts(fracture, "dbc"), ["1234 (0305)"]);
    assert_eq!(texts(fracture, ROLE), ["Hoofddiagnose=Ja [ORT]"]);
    assert_eq!(texts(fracture, UMBRELLA), [INJURY]);
    assert_eq!(texts(fracture, "laterality"), ["true"]);
    let preferred = fracture
        .rubrics
        .iter()
        .find(|r| r.kind == PREFERRED)
        .expect("preferred");
    assert_eq!(preferred.language, DUTCH);

    let sprain = class(&c.classes, SPRAIN);
    assert_eq!(texts(sprain, MAPPING), ["ICPC-2: L99"]);
    let old = class(&c.classes, OLD);
    assert!(!old.active, "ended before the delivery date");
    // NOTE: the relations table is read whole, so the successor of an ended
    // concept stays followable even after the relation's own validity ended
    // (`crates/dhd-thesaurus/src/lib.rs`, `read_relations`; #269).
    assert_eq!(
        texts(old, REPLACED_BY),
        [FRACTURE, SPRAIN],
        "the current replacement and the one whose validity ended"
    );
    assert!(
        texts(old, PREFERRED).is_empty(),
        "its term ended with it and is skipped"
    );

    assert_eq!(
        thesaurus.snomed,
        [(FRACTURE.to_owned(), FRACTURE_SCTID.to_owned())]
    );
    assert_eq!(
        thesaurus.icd10,
        [
            (
                FRACTURE.to_owned(),
                vec![String::from("Z99.0"), String::from("Z99.1")]
            ),
            (SPRAIN.to_owned(), vec![String::from("Z98.0")]),
        ],
        "the derivations follow Volgnummer"
    );
    assert_eq!(
        read(&root, Some("1.0"))
            .expect("reads")
            .classification
            .version
            .as_deref(),
        Some("1.0"),
        "the flag wins"
    );
}

#[test]
fn a_directory_without_the_concept_table_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("x_ThesaurusTerm.csv"), "a,b\n1,2\n").expect("writes");
    assert!(matches!(
        read(dir.path(), None),
        Err(DhdError::Missing {
            table: "ThesaurusConcept",
            ..
        })
    ));
    std::fs::write(
        dir.path().join("x_ThesaurusConcept.csv"),
        "\"Foo\",\"Bar\"\n\"1\",\"2\"\n",
    )
    .expect("writes");
    assert!(matches!(
        read(dir.path(), None),
        Err(DhdError::Column {
            column: "ConceptID",
            ..
        })
    ));
}
