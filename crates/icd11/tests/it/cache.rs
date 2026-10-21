// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading a cache the walker wrote (the ICD-API's entity JSON).

use ::icd11::cache::{CacheError, read};
use ::icd11::{Linearization, MMS};
use ferroterm_testkit::icd11::{BLOCK, CHAPTER, CHOLERA, RELEASE, RESIDUAL, VIBRIO, write_cache};

#[test]
fn the_mms_cache_reads_into_entities_with_both_languages() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_cache(dir.path()).expect("writes");
    let cached = read(dir.path(), Linearization::Mms).expect("reads");
    assert_eq!(cached.release.as_deref(), Some(RELEASE));
    assert_eq!(cached.release_date.as_deref(), Some("2099-01-17"));
    assert_eq!(cached.languages, ["en", "fr"]);
    assert_eq!(
        cached.titles.get("en").map(String::as_str),
        Some("ICD-11 for Mortality and Morbidity Statistics")
    );
    assert_eq!(cached.entities.len(), 12);
    let cholera = &cached.entities[CHOLERA];
    assert_eq!(cholera.code.as_deref(), Some("1A00"));
    assert_eq!(cholera.class_kind.as_deref(), Some("category"));
    assert_eq!(cholera.parents, [BLOCK]);
    assert_eq!(cholera.title("en"), Some("Cholera"));
    assert_eq!(cholera.title("fr"), Some("Choléra"));
    assert_eq!(cholera.title("de"), Some("Cholera"), "English when missing");
    assert_eq!(cholera.languages(), ["en", "fr"]);
    assert_eq!(cholera.scales.len(), 2);
    assert_eq!(
        cholera.scales[0].axis,
        "http://id.who.int/icd/schema/infectiousAgent"
    );
    assert_eq!(cholera.scales[0].entities, [VIBRIO]);
    assert!(!cholera.scales[0].required);
    assert!(
        cholera
            .index_terms
            .iter()
            .any(|t| t.value == "asiatic cholera")
    );
    assert!(
        cholera
            .index_terms
            .iter()
            .any(|t| t.value == "Choléra" && t.language == "fr")
    );
    let block = &cached.entities[BLOCK];
    assert_eq!(block.code, None, "a block has no short code");
    assert_eq!(block.children.len(), 3);
    let chapter = &cached.entities[CHAPTER];
    assert!(chapter.parents.is_empty(), "the root is not a parent");
    let residual = &cached.entities[RESIDUAL];
    assert_eq!(residual.code.as_deref(), Some("1A0Y"));
    assert_eq!(residual.parents, [BLOCK]);
    assert_eq!(residual.id, "1001/other");
    assert_eq!(
        Linearization::Mms.uri(&residual.id),
        format!("{MMS}/1001/other")
    );
}

#[test]
fn the_foundation_and_icf_read_and_a_missing_cache_is_named() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_cache(dir.path()).expect("writes");
    let foundation = read(dir.path(), Linearization::Foundation).expect("reads");
    assert_eq!(
        foundation.release, None,
        "the Foundation's root names no release"
    );
    assert_eq!(foundation.entities.len(), 3);
    assert!(
        foundation.entities[CHOLERA]
            .index_terms
            .iter()
            .any(|t| t.value == "asiatic cholera"),
        "synonyms are index terms"
    );
    let icf = read(dir.path(), Linearization::Icf).expect("reads");
    assert_eq!(icf.entities.len(), 5);
    assert_eq!(
        icf.entities["5001/unspecified"].code.as_deref(),
        Some("d5409")
    );
    let empty = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(empty.path().join("mms/en")).expect("creates");
    assert!(matches!(
        read(empty.path(), Linearization::Mms),
        Err(CacheError::NoRoot { name: "mms", .. })
    ));
}
