// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Locating the files and reading the rows
//! (<https://www.nlm.nih.gov/research/umls/rxnorm/docs/techdoc.html>).

use ferroterm_testkit::rxnorm::{
    ASPIRIN, ASPIRIN_ATOM, ASPIRIN_SYNONYM_ATOM, ASPIRIN_TABLET, VERSION, write_release,
};
use rxnorm_rrf::{Release, RrfError};

#[test]
fn the_release_locates_its_files_and_states_its_date() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_release(dir.path()).expect("writes");
    let release = Release::open(dir.path()).expect("opens");
    assert_eq!(release.version().as_deref(), Some(VERSION));
    assert!(
        release
            .file("RXNCONSO.RRF")
            .expect("conso")
            .ends_with("RXNCONSO.RRF")
    );
    assert!(
        release.optional("rxnsty.rrf").is_some(),
        "found without case"
    );
    assert!(matches!(
        release.file("RXNDOC.RRF"),
        Err(RrfError::Missing {
            name: "RXNDOC.RRF",
            ..
        })
    ));
    let empty = tempfile::tempdir().expect("tempdir");
    assert!(matches!(
        Release::open(empty.path()),
        Err(RrfError::Missing {
            name: "RXNCONSO.RRF",
            ..
        })
    ));
}

#[test]
fn the_rows_carry_the_documented_columns() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_release(dir.path()).expect("writes");
    let release = Release::open(dir.path()).expect("opens");
    let atoms: Vec<_> = release
        .atoms()
        .expect("opens")
        .collect::<Result<_, _>>()
        .expect("reads");
    assert_eq!(atoms.len(), 12);
    let aspirin = &atoms[0];
    assert_eq!(aspirin.rxcui.to_string(), ASPIRIN);
    assert_eq!(aspirin.rxaui.to_string(), ASPIRIN_ATOM);
    assert_eq!(aspirin.language, "ENG");
    assert_eq!(aspirin.sab, "RXNORM");
    assert_eq!(aspirin.tty, "IN");
    assert_eq!(aspirin.name, "aspirin");
    assert_eq!(aspirin.suppress, "N");
    let relationships: Vec<_> = release
        .relationships()
        .expect("opens")
        .collect::<Result<_, _>>()
        .expect("reads");
    assert_eq!(relationships.len(), 10);
    let first = &relationships[0];
    assert_eq!(
        first.rxcui1.map(|c| c.to_string()).as_deref(),
        Some(ASPIRIN)
    );
    assert_eq!(first.rel, "RO");
    assert_eq!(
        first.rxcui2.map(|c| c.to_string()).as_deref(),
        Some(ASPIRIN_TABLET)
    );
    assert_eq!(first.rela.as_deref(), Some("has_ingredient"));
    assert_eq!(first.sab, "RXNORM");
    let atom_level = &relationships[8];
    assert_eq!(atom_level.rxcui1, None);
    assert_eq!(
        atom_level.rxaui1.map(|a| a.to_string()).as_deref(),
        Some(ASPIRIN_ATOM)
    );
    assert_eq!(
        atom_level.rxaui2.map(|a| a.to_string()).as_deref(),
        Some(ASPIRIN_SYNONYM_ATOM)
    );
    assert_eq!(atom_level.rel, "SY");
    assert_eq!(atom_level.rela, None);
    let attributes: Vec<_> = release
        .attributes()
        .expect("opens")
        .collect::<Result<_, _>>()
        .expect("reads");
    assert_eq!(attributes.len(), 5);
    assert_eq!(attributes[0].name, "NDC");
    assert_eq!(attributes[0].value, "00000000101");
    assert_eq!(attributes[3].sab, "MTHSPL");
    let types: Vec<_> = release
        .semantic_types()
        .expect("opens")
        .expect("present")
        .collect::<Result<_, _>>()
        .expect("reads");
    assert_eq!(types[0].tui, "T109");
    assert_eq!(types[0].name, "Organic Chemical");
}

#[test]
fn a_short_row_and_a_bad_identifier_are_refused_with_their_line() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_release(dir.path()).expect("writes");
    std::fs::write(
        dir.path().join("rrf/RXNCONSO.RRF"),
        "1|ENG||||||2|2|1||RXNORM|IN|1|x||N|4096|\nshort|row|\n",
    )
    .expect("writes");
    let release = Release::open(dir.path()).expect("opens");
    let rows: Vec<_> = release.atoms().expect("opens").collect();
    assert!(rows[0].is_ok());
    assert!(matches!(
        rows[1],
        Err(RrfError::Columns {
            line: 2,
            columns: 3,
            expected: 18,
            ..
        })
    ));
    std::fs::write(
        dir.path().join("rrf/RXNCONSO.RRF"),
        "abc|ENG||||||2|2|1||RXNORM|IN|1|x||N|4096|\n",
    )
    .expect("writes");
    let rows: Vec<_> = release.atoms().expect("opens").collect();
    assert!(matches!(
        rows[0],
        Err(RrfError::Identifier { line: 1, ref value, .. }) if value == "abc"
    ));
}
