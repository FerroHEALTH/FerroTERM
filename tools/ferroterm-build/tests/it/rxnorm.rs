// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `RxNorm` build over the testkit's release-shaped fixture.

use concept_graph::ordinal::Ordinal;
use concept_graph::relations::Relations;
use concept_store::keys::KeyTable;
use concept_store::record::PropertyValue;
use concept_store::store::{Store, Vocabulary};
use ferroterm_build::rxnorm::{self, RXNORM, SAB_KEY, STY_KEY, SYSTEM, TTY_KEY};
use ferroterm_testkit::rxnorm::{
    ASPIRIN, ASPIRIN_SYNONYM_ATOM, ASPIRIN_TABLET, BRANDED_TABLET, LABEL_ONLY, OLD_TABLET, VERSION,
    write_release,
};

#[test]
fn the_release_builds_codes_designations_properties_relations_and_atoms() {
    let release = tempfile::tempdir().expect("tempdir");
    write_release(release.path()).expect("writes");
    let out = tempfile::tempdir().expect("tempdir");
    let report = rxnorm::build(release.path(), None, &[], out.path()).expect("builds");
    assert_eq!(report.version, VERSION, "the date from the readme name");
    assert_eq!(report.concepts, 6, "the RXCUIs with an RXNORM atom");
    assert_eq!(
        report.atoms, 10,
        "the MTHSPL-only concept's atom is not one"
    );
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.path().join("manifest.json")).expect("manifest"),
    )
    .expect("json");
    assert_eq!(manifest["system"], SYSTEM);
    assert_eq!(manifest["semanticTypes"], true);
    assert_eq!(manifest["sources"], serde_json::json!(["MTHSPL", "RXNORM"]));
    let store = Store::open(&report.store).expect("opens");
    assert!(store.ordinal(LABEL_ONLY).expect("read").is_none());
    let tablet = store
        .ordinal(ASPIRIN_TABLET)
        .expect("read")
        .expect("tablet");
    let designations = store.designations(tablet).expect("read");
    assert_eq!(
        designations[0].term, "aspirin 81 MG Oral Tablet",
        "the SCD string first"
    );
    assert_eq!(designations.len(), 3);
    assert_eq!(
        store
            .vocabulary(Vocabulary::DesignationUses, designations[2].use_ordinal)
            .expect("read")
            .as_deref(),
        Some("DP")
    );
    let keys = |name: &str| {
        store
            .vocabulary_ordinal(Vocabulary::PropertyKeys, name)
            .expect("read")
            .expect("key")
    };
    let properties = store.properties(tablet).expect("read");
    let of = |name: &str| {
        properties
            .iter()
            .find(|(k, _)| *k == keys(name))
            .map(|(_, v)| v.clone())
    };
    assert_eq!(
        of(TTY_KEY),
        Some(vec![
            PropertyValue::Code(String::from("PSN")),
            PropertyValue::Code(String::from("SCD"))
        ])
    );
    assert_eq!(
        of(SAB_KEY),
        Some(vec![
            PropertyValue::Code(String::from("MTHSPL")),
            PropertyValue::Code(String::from(RXNORM))
        ])
    );
    assert_eq!(
        of("NDC"),
        Some(vec![
            PropertyValue::String(String::from("00000000101")),
            PropertyValue::String(String::from("00000000102"))
        ])
    );
    assert_eq!(
        of("RXN_AVAILABLE_STRENGTH"),
        Some(vec![PropertyValue::String(String::from("81 MG"))])
    );
    assert!(
        store
            .vocabulary_ordinal(Vocabulary::PropertyKeys, "SPL_SET_ID")
            .expect("read")
            .is_none(),
        "MTHSPL attributes are not properties"
    );
    let aspirin = store.ordinal(ASPIRIN).expect("read").expect("aspirin");
    assert!(
        store
            .properties(aspirin)
            .expect("read")
            .iter()
            .any(|(k, v)| *k == keys(STY_KEY)
                && v.contains(&PropertyValue::String(String::from("Organic Chemical"))))
    );
    let old = store.ordinal(OLD_TABLET).expect("read").expect("old");
    assert!(!store.concept(old).expect("read").expect("record").active);
}

#[test]
fn the_relationships_and_atoms_sit_beside_the_store() {
    let release = tempfile::tempdir().expect("tempdir");
    write_release(release.path()).expect("writes");
    let out = tempfile::tempdir().expect("tempdir");
    let report = rxnorm::build(release.path(), None, &[], out.path()).expect("builds");
    assert_eq!(
        report.relationships, 17,
        "eight RXNORM rows with REL and RELA, one atom-level SY row"
    );
    let store = Store::open(&report.store).expect("opens");
    let aspirin = store.ordinal(ASPIRIN).expect("read").expect("aspirin");
    let tablet = store
        .ordinal(ASPIRIN_TABLET)
        .expect("read")
        .expect("tablet");
    let old = store.ordinal(OLD_TABLET).expect("read").expect("old");
    let relations = Relations::read_from(
        &mut std::fs::read(out.path().join("relations.bin"))
            .expect("relations")
            .as_slice(),
    )
    .expect("reads");
    let has_ingredient = relations.kind("has_ingredient").expect("type");
    let sources: Vec<Ordinal> = relations.sources(aspirin, has_ingredient).collect();
    assert_eq!(
        sources,
        [tablet, old],
        "the second concept has the relationship to the first"
    );
    let ro = relations.kind("RO").expect("REL type");
    assert!(relations.sources(aspirin, ro).count() >= 2);
    let sy = relations.kind("SY").expect("atom-level type");
    assert_eq!(
        relations.targets(aspirin, sy).collect::<Vec<_>>(),
        [aspirin],
        "an atom-level row resolves to its concepts"
    );
    assert!(relations.kind("MTHSPL").is_none());
    let branded = store
        .ordinal(BRANDED_TABLET)
        .expect("read")
        .expect("branded");
    let tradename_of = relations.kind("tradename_of").expect("type");
    assert_eq!(
        relations.targets(branded, tradename_of).collect::<Vec<_>>(),
        [tablet]
    );
    let atoms = KeyTable::read_from(
        &mut std::fs::read(out.path().join("atoms.bin"))
            .expect("atoms")
            .as_slice(),
    )
    .expect("reads");
    assert_eq!(
        atoms.get(ASPIRIN_SYNONYM_ATOM.parse().expect("number")),
        Some(aspirin.index())
    );
}

#[test]
fn the_date_comes_from_the_readme_or_the_flag() {
    let release = tempfile::tempdir().expect("tempdir");
    write_release(release.path()).expect("writes");
    std::fs::remove_file(
        release
            .path()
            .join(format!("Readme_Full_Prescribe_{VERSION}.txt")),
    )
    .expect("removes");
    let out = tempfile::tempdir().expect("tempdir");
    assert!(matches!(
        rxnorm::build(release.path(), None, &[], out.path()),
        Err(rxnorm::Error::NoVersion)
    ));
    let report = rxnorm::build(release.path(), Some("02022099"), &[], out.path())
        .expect("builds with the flag");
    assert_eq!(report.version, "02022099");
}

#[test]
fn restricted_sources_are_kept_only_when_named() {
    let release = tempfile::tempdir().expect("tempdir");
    write_release(release.path()).expect("writes");
    let out = tempfile::tempdir().expect("tempdir");
    let report =
        rxnorm::build(release.path(), None, &[String::from("MMSL")], out.path()).expect("builds");
    assert_eq!(
        report.atoms, 11,
        "the MMSL atom joins the ten unrestricted ones"
    );
    let store = Store::open(&report.store).expect("opens");
    let tablet = store
        .ordinal(ASPIRIN_TABLET)
        .expect("read")
        .expect("tablet");
    let designations = store.designations(tablet).expect("read");
    assert_eq!(designations.len(), 4);
    assert!(designations.iter().any(|d| d.term == "Aspirin 81mg tablet"));
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.path().join("manifest.json")).expect("manifest"),
    )
    .expect("json");
    assert_eq!(
        manifest["sources"],
        serde_json::json!(["MMSL", "MTHSPL", "RXNORM"])
    );
}
