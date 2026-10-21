// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A refset-only RF2 package layered onto the synthetic edition: its concepts
//! and reference set join the edition, and an unmet module dependency is
//! refused.

use std::fs;
use std::path::PathBuf;

use concept_graph::members::Memberships;
use concept_store::store::{Store, Vocabulary};
use ferroterm_build::pipeline;
use ferroterm_testkit::snomed::{Item, PACKAGE_MODULE, PACKAGE_REFSET, Package, sctid};
use serde_json::Value;

use crate::fixture::{self, DATE, GB_LANGUAGE_REFSET, concept};

/// The edition and one package written beside it, ready to build.
struct Layered {
    edition: tempfile::TempDir,
    package: tempfile::TempDir,
    out: tempfile::TempDir,
}

/// Writes the edition and a package depending on `module` at `target`.
fn layered(module: &str, target: &str) -> Layered {
    let edition = tempfile::tempdir().expect("tempdir");
    fixture::write_release(edition.path());
    let package = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::snomed::write_refset_package(
        package.path(),
        &Package {
            depends_on: module,
            target,
            members: &[concept(3), concept(4)],
        },
    )
    .expect("writes the package");
    Layered {
        edition,
        package,
        out: tempfile::tempdir().expect("tempdir"),
    }
}

impl Layered {
    fn build(&self) -> Result<pipeline::Report, pipeline::Error> {
        pipeline::build(
            self.edition.path(),
            &[PathBuf::from(self.package.path())],
            self.out.path(),
        )
    }
}

#[test]
fn the_packages_concepts_and_reference_set_join_the_edition() {
    let layered = layered(&fixture::module(), DATE);
    let report = layered.build().expect("builds");
    assert_eq!(
        report.concepts, 11,
        "the edition's nine and the package's two"
    );
    let store = Store::open(&report.store).expect("store opens");
    let module = store
        .ordinal(&sctid(Item::raw(PACKAGE_MODULE)))
        .expect("read")
        .expect("the package's module resolves");
    let refset = store
        .ordinal(&sctid(Item::raw(PACKAGE_REFSET)))
        .expect("read")
        .expect("the package's reference set resolves");
    assert!(store.concept(module).expect("read").expect("module").active);
    let gb = store
        .vocabulary_ordinal(Vocabulary::LanguageRefsets, GB_LANGUAGE_REFSET)
        .expect("read")
        .expect("gb refset");
    assert_eq!(
        store
            .preferred(refset, gb, 1)
            .expect("read")
            .expect("the package's language members join the edition's")
            .term,
        "Zoo nursing reference set"
    );
    assert_eq!(
        report.refsets, 3,
        "the edition's simple reference set, the package's, and the module \
         dependency reference set, whose row names an edition module"
    );
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(&report.manifest).expect("manifest"))
            .expect("json");
    assert_eq!(
        manifest["layered"],
        serde_json::json!([{ "module": sctid(Item::raw(PACKAGE_MODULE)), "version": DATE }])
    );
}

#[test]
fn the_packages_members_are_the_editions_concepts() {
    let layered = layered(&fixture::module(), DATE);
    let report = layered.build().expect("builds");
    let store = Store::open(&report.store).expect("store opens");
    let cat = store.ordinal(&concept(3)).expect("read").expect("cat");
    let dog = store.ordinal(&concept(4)).expect("read").expect("dog");
    let memberships = Memberships::read_from(
        &mut fs::read(layered.out.path().join(pipeline::REFSETS_FILE))
            .expect("refsets")
            .as_slice(),
    )
    .expect("reads");
    let refset: u64 = sctid(Item::raw(PACKAGE_REFSET)).parse().expect("number");
    let members = memberships
        .members(refset)
        .expect("the package's reference set is served");
    assert!(members.contains(cat.index()) && members.contains(dog.index()));
    assert_eq!(members.len(), 2);
}

#[test]
fn a_package_needing_a_version_after_the_edition_is_refused() {
    let layered = layered(&fixture::module(), "20260102");
    let error = layered.build().expect_err("refused");
    let pipeline::Error::UnmetDependency {
        required, edition, ..
    } = &error
    else {
        panic!("expected an unmet dependency, got {error:?}");
    };
    assert_eq!((required.as_str(), edition.as_str()), ("20260102", DATE));
    let text = error.to_string();
    assert!(
        text.contains("20260102") && text.contains(DATE),
        "the error names both dates: {text}"
    );
}

#[test]
fn a_package_naming_a_module_outside_the_edition_is_refused() {
    let outside = concept(50);
    let layered = layered(&outside, DATE);
    let error = layered.build().expect_err("refused");
    let pipeline::Error::UnknownModule { module, .. } = &error else {
        panic!("expected an unknown module, got {error:?}");
    };
    assert_eq!(module, &outside);
}

#[test]
fn a_build_without_a_package_writes_no_layered_key() {
    let release = tempfile::tempdir().expect("tempdir");
    fixture::write_release(release.path());
    let out = tempfile::tempdir().expect("tempdir");
    let report = pipeline::build(release.path(), &[], out.path()).expect("builds");
    assert_eq!(report.concepts, 9);
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(&report.manifest).expect("manifest"))
            .expect("json");
    assert_eq!(manifest.get("layered"), None);
}
