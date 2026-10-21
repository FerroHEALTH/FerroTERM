// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: Apache-2.0

//! Checks over the licensed release a developer keeps under `data/`.
//!
//! Ignored by default: the content is licence-gated and never committed
//! (`.claude/rules/vendored-inputs.md`). Run with
//! `cargo nextest run -p rf2 --run-ignored all` when a release is present.

use std::path::PathBuf;

use rf2::component::{Concept, Rows};
use rf2::constants;
use rf2::edition::Edition;
use rf2::file::{ContentType, Release, ReleaseType};
use rf2::id::ConceptId;
use rf2::refset::{DescriptorMember, Members, ModuleDependencyMember};

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
fn the_local_edition_loads_and_identifies_itself() {
    let Some(root) = local_release() else {
        panic!("no release under data/snomed/");
    };
    let release = Release::open(&root, ReleaseType::Snapshot).expect("release opens");
    assert!(release.files().len() > 20);
    let concept_file = release
        .of_type(&ContentType::Concept)
        .next()
        .expect("concept file");
    let mut concepts = 0_u64;
    for concept in Rows::<_, Concept>::open(&concept_file.path).expect("header") {
        concept.expect("every concept row parses");
        concepts += 1;
    }
    assert!(concepts > 100_000, "{concepts} concepts");

    let dependency_file = release
        .refsets()
        .find(|f| f.name.summary == "ModuleDependency")
        .expect("module dependency refset");
    let ContentType::Refset(kinds) = &dependency_file.name.content_type else {
        panic!("refset content type");
    };
    let members: Vec<ModuleDependencyMember> = Members::open(&dependency_file.path, kinds)
        .expect("header")
        .map(|m| {
            ModuleDependencyMember::try_from(m.expect("member parses")).expect("dependency view")
        })
        .collect();
    let edition = Edition::identify(&members, release.date()).expect("edition identifies");
    assert!(edition.version_uri().starts_with("http://snomed.info/sct/"));
    assert_eq!(edition.effective_time, release.date());

    // The descriptor refset describes itself with the specification's template:
    // the referenced component typed as a concept type component (order 0) and
    // the attribute order typed as an unsigned integer (order 3).
    let descriptor_file = release
        .refsets()
        .find(|f| f.name.summary == "RefsetDescriptor")
        .expect("descriptor refset");
    let ContentType::Refset(kinds) = &descriptor_file.name.content_type else {
        panic!("refset content type");
    };
    let own_rows: Vec<DescriptorMember> = Members::open(&descriptor_file.path, kinds)
        .expect("header")
        .map(|m| DescriptorMember::try_from(m.expect("member parses")).expect("descriptor view"))
        .filter(|d| {
            d.member.referenced_component_id
                == constants::REFSET_DESCRIPTOR_REFSET.concept().sctid()
        })
        .collect();
    let types: Vec<(i64, ConceptId)> = own_rows
        .iter()
        .map(|d| (d.attribute_order, d.attribute_type))
        .collect();
    assert!(
        types.contains(&(0, constants::attribute_type::CONCEPT)),
        "{types:?}"
    );
    assert!(
        types.contains(&(3, constants::attribute_type::UNSIGNED_INTEGER)),
        "{types:?}"
    );
}
