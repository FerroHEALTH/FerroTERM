// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ICD-11 build over the testkit's API-shaped cache.

use concept_graph::persist::Hierarchy;
use concept_store::keys::KeyTable;
use concept_store::store::Store;
use ferroterm_build::icd11::{self, KIND, StoredScale};
use ferroterm_testkit::icd11::{BLOCK, CHAPTER, CHOLERA, RELEASE, RESIDUAL, VIBRIO, write_cache};

#[test]
fn the_cache_builds_three_artifacts_with_keys_and_scales() {
    let cache = tempfile::tempdir().expect("tempdir");
    write_cache(cache.path()).expect("writes");
    let out = tempfile::tempdir().expect("tempdir");
    let reports = icd11::build_all(cache.path(), None, out.path()).expect("builds");
    assert_eq!(reports.len(), 3);
    let mms = reports
        .iter()
        .find(|r| r.system == ::icd11::MMS)
        .expect("mms");
    assert_eq!(mms.version, RELEASE);
    assert_eq!(mms.concepts, 12);
    assert_eq!(mms.scales, 4);
    let foundation = reports
        .iter()
        .find(|r| r.system == ::icd11::FOUNDATION)
        .expect("foundation");
    assert_eq!(
        foundation.version, RELEASE,
        "the Foundation takes the MMS release"
    );
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(mms.dir.join("manifest.json")).expect("manifest"),
    )
    .expect("json");
    assert_eq!(manifest["kind"], KIND);
    assert_eq!(manifest["linearization"], "mms");
    assert_eq!(manifest["languages"], serde_json::json!(["en", "fr"]));
    assert_eq!(
        manifest["title"],
        "ICD-11 for Mortality and Morbidity Statistics"
    );
    let store = Store::open(&mms.dir.join("store.redb")).expect("opens");
    let cholera = store.ordinal("1A00").expect("read").expect("cholera");
    let block = store
        .ordinal(&format!("{}/{BLOCK}", ::icd11::MMS))
        .expect("read")
        .expect("a block is keyed by its URI");
    assert_eq!(
        store.designations(cholera).expect("read").len(),
        7,
        "en and fr titles, the FSN, an inclusion, three index terms"
    );
    let keys = KeyTable::read_from(
        &mut std::fs::read(mms.dir.join("keys.bin"))
            .expect("keys")
            .as_slice(),
    )
    .expect("reads");
    assert_eq!(
        keys.get(::icd11::Linearization::key_of(CHOLERA).expect("key")),
        Some(cholera.index())
    );
    assert_eq!(
        keys.get(::icd11::Linearization::key_of(RESIDUAL).expect("key")),
        Some(
            store
                .ordinal("1A0Y")
                .expect("read")
                .expect("residual")
                .index()
        )
    );
    let graph = std::fs::read(mms.dir.join("hierarchy.bin")).expect("hierarchy");
    let hierarchy = Hierarchy::read_from(&mut graph.as_slice()).expect("reads");
    let chapter = store.ordinal("01").expect("read").expect("chapter");
    assert!(hierarchy.closure.is_ancestor(chapter, cholera));
    assert!(hierarchy.closure.is_ancestor(block, cholera));
    let scales: Vec<StoredScale> = serde_json::from_str(
        &std::fs::read_to_string(mms.dir.join("scales.json")).expect("scales"),
    )
    .expect("json");
    let agent = scales
        .iter()
        .find(|s| s.stem == cholera.index() && s.axis.ends_with("/infectiousAgent"))
        .expect("scale");
    let vibrio = store.ordinal("XN7N1").expect("read").expect("vibrio");
    assert_eq!(agent.entities, [vibrio.index()]);
    assert_eq!(CHAPTER, "1000");
    assert_eq!(VIBRIO, "2000");
}
