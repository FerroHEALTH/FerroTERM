//! Builds an artifact from the licensed release under `data/`, when present.
//!
//! Ignored by default (`.claude/rules/vendored-inputs.md`). Run with
//! `cargo nextest run -p concept-store --run-ignored all`; it prints the
//! artifact size and open time the acceptance criterion asks for.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use concept_graph::ordinal::Ordinal;
use concept_store::builder::{PreferredRule, StoreBuilder};
use concept_store::record;
use concept_store::store::{Store, Vocabulary};
use rf2::component::{Concept, Description, Rows};
use rf2::constants;
use rf2::file::{ContentType, Release, ReleaseType};
use rf2::id::{ConceptId, DescriptionId};
use rf2::refset::{LanguageMember, Members};

fn local_release() -> Option<PathBuf> {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/snomed");
    std::fs::read_dir(data)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.is_dir())
}

#[test]
#[ignore = "needs a licensed RF2 release under data/snomed/"]
fn the_local_edition_builds_an_artifact_and_opens_fast() {
    let Some(root) = local_release() else {
        panic!("no release under data/snomed/");
    };
    let release = Release::open(&root, ReleaseType::Snapshot).expect("release opens");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("nl.redb");
    let started = Instant::now();
    let mut builder = StoreBuilder::create(
        &path,
        "http://snomed.info/sct",
        &format!("local/{}", release.date()),
    )
    .expect("creates");

    // Vocabularies: designation uses and acceptabilities by SCTID, language refsets by SCTID.
    let uses = [
        constants::FULLY_SPECIFIED_NAME,
        constants::SYNONYM,
        constants::DEFINITION,
    ];
    for (ordinal, use_id) in uses.iter().enumerate() {
        builder
            .vocabulary(
                Vocabulary::DesignationUses,
                u32::try_from(ordinal).expect("fits"),
                &use_id.to_string(),
            )
            .expect("vocab");
    }
    builder
        .vocabulary(
            Vocabulary::Acceptabilities,
            0,
            &constants::PREFERRED.to_string(),
        )
        .expect("vocab");
    builder
        .vocabulary(
            Vocabulary::Acceptabilities,
            1,
            &constants::ACCEPTABLE.to_string(),
        )
        .expect("vocab");

    let concept_file = release
        .of_type(&ContentType::Concept)
        .next()
        .expect("concept file");
    let mut ordinals: BTreeMap<ConceptId, Ordinal> = BTreeMap::new();
    for concept in Rows::<_, Concept>::open(&concept_file.path).expect("header") {
        let concept = concept.expect("row parses");
        let ordinal = Ordinal::new(u32::try_from(ordinals.len()).expect("fits"));
        ordinals.insert(concept.id, ordinal);
        builder
            .concept(
                ordinal,
                &record::Concept {
                    code: concept.id.to_string(),
                    active: concept.base.active,
                    effective_time: Some(concept.base.effective_time.compact()),
                    module: None,
                },
            )
            .expect("concept");
    }
    let mut designation_index: BTreeMap<DescriptionId, (Ordinal, u32)> = BTreeMap::new();
    let mut per_concept: BTreeMap<Ordinal, u32> = BTreeMap::new();
    for file in release.of_type(&ContentType::Description) {
        for description in Rows::<_, Description>::open(&file.path).expect("header") {
            let description = description.expect("row parses");
            let Some(ordinal) = ordinals.get(&description.concept_id).copied() else {
                continue;
            };
            let index = per_concept.entry(ordinal).or_insert(0);
            let use_ordinal = uses
                .iter()
                .position(|u| *u == description.type_id)
                .map_or(1, |p| u32::try_from(p).expect("fits"));
            builder
                .designation(
                    ordinal,
                    *index,
                    &record::Designation {
                        id: Some(description.id.to_string()),
                        term: description.term,
                        language: description.language_code,
                        use_ordinal,
                        active: description.base.active,
                    },
                )
                .expect("designation");
            designation_index.insert(description.id, (ordinal, *index));
            *index += 1;
        }
    }
    let mut refsets: BTreeMap<String, u32> = BTreeMap::new();
    for file in release.refsets().filter(|f| f.name.summary == "Language") {
        let ContentType::Refset(kinds) = &file.name.content_type else {
            panic!("refset content type");
        };
        for member in Members::open(&file.path, kinds).expect("header") {
            let member =
                LanguageMember::try_from(member.expect("member parses")).expect("language view");
            if !member.member.active {
                continue;
            }
            let Ok(description_id) = DescriptionId::try_from(member.member.referenced_component_id)
            else {
                continue;
            };
            let Some((ordinal, index)) = designation_index.get(&description_id).copied() else {
                continue;
            };
            let refset_name = member.member.refset_id.to_string();
            let next = u32::try_from(refsets.len()).expect("fits");
            let refset = *refsets.entry(refset_name.clone()).or_insert(next);
            if refset == next {
                builder
                    .vocabulary(Vocabulary::LanguageRefsets, refset, &refset_name)
                    .expect("vocab");
            }
            let acceptability = u32::from(member.acceptability_id != constants::PREFERRED);
            builder
                .acceptability(ordinal, index, refset, acceptability)
                .expect("acceptability");
        }
    }
    builder
        .finish(&PreferredRule {
            preferred: 0,
            display_use: Some(1),
        })
        .expect("finishes");
    let built = started.elapsed();
    let size = std::fs::metadata(&path).expect("metadata").len();

    let opened_at = Instant::now();
    let store = Store::open(&path).expect("opens");
    let root = store
        .ordinal("138875005")
        .expect("read")
        .expect("root present");
    let nl_refset = store
        .vocabulary_ordinal(Vocabulary::LanguageRefsets, "31000146106")
        .expect("read")
        .expect("NL refset");
    let preferred = store
        .preferred(root, nl_refset, 1)
        .expect("read")
        .expect("the root has a Dutch preferred synonym");
    let opened = opened_at.elapsed();
    println!(
        "concepts {}, designations {}, artifact {size} bytes, build {built:?}, open and three reads {opened:?}, root nl term {:?}",
        ordinals.len(),
        designation_index.len(),
        preferred.term
    );
    assert_ne!(preferred.term, "");
}
