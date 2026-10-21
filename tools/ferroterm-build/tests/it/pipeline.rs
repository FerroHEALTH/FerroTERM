//! The pipeline end to end over the synthetic release: deterministic output,
//! the manifest, and the artifacts opened by the store, graph, and text crates.

use std::fs;

use concept_graph::persist::Hierarchy;
use concept_store::store::{Store, Vocabulary};
use concept_store::tables;
use designation_index::index::Query;
use designation_index::persist::read_from;
use ferroterm_build::pipeline::{self, MANIFEST_FILE, STORE_FILE};
use serde_json::Value;

use crate::fixture::{self, DATE, GB_LANGUAGE_REFSET, NL_LANGUAGE_REFSET, concept};

#[test]
fn two_builds_of_the_same_release_are_byte_identical() {
    let release = tempfile::tempdir().expect("tempdir");
    fixture::write_release(release.path());
    let first = tempfile::tempdir().expect("tempdir");
    let second = tempfile::tempdir().expect("tempdir");
    let a = pipeline::build(release.path(), &[], first.path()).expect("first build");
    let b = pipeline::build(release.path(), &[], second.path()).expect("second build");
    assert_eq!(
        (a.concepts, a.designations, a.is_a_edges),
        (b.concepts, b.designations, b.is_a_edges)
    );
    // Every file, not only the store: the hierarchy, the text index, and the
    // ECL files are written by the parallel stages, and it is those that a
    // completion-order dependency would show up in.
    let written = files_in(first.path());
    assert_eq!(written, files_in(second.path()));
    for file in [
        STORE_FILE,
        MANIFEST_FILE,
        pipeline::HIERARCHY_FILE,
        pipeline::TEXT_FILE,
        pipeline::REFSETS_FILE,
        pipeline::ATTRIBUTES_FILE,
        pipeline::MEMBERS_FILE,
        pipeline::IDENTIFIERS_FILE,
    ] {
        assert!(
            written.iter().any(|name| name == file),
            "the build wrote no {file}"
        );
    }
    for file in &written {
        assert_eq!(
            fs::read(first.path().join(file)).expect("first"),
            fs::read(second.path().join(file)).expect("second"),
            "{file} differs between two builds"
        );
    }
}

/// The names of the files in `dir`, sorted.
fn files_in(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read the output directory")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn the_manifest_records_the_edition_and_the_counts() {
    let release = tempfile::tempdir().expect("tempdir");
    fixture::write_release(release.path());
    let out = tempfile::tempdir().expect("tempdir");
    let report = pipeline::build(release.path(), &[], out.path()).expect("build");
    let module = fixture::module();
    assert_eq!(
        report.edition_uri,
        format!("http://snomed.info/sct/{module}")
    );
    assert_eq!(
        report.version_uri,
        format!("http://snomed.info/sct/{module}/version/{DATE}")
    );
    assert_eq!(
        (report.concepts, report.designations, report.is_a_edges),
        (9, 12, 4)
    );
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(&report.manifest).expect("manifest"))
            .expect("json");
    assert_eq!(manifest["manifest"], pipeline::MANIFEST_VERSION);
    assert_eq!(manifest["hierarchy"], "hierarchy.bin");
    assert_eq!(manifest["text"], "text.bin");
    assert_eq!(manifest["system"], "http://snomed.info/sct");
    assert_eq!(manifest["version"], report.version_uri);
    assert_eq!(manifest["releaseDate"], DATE);
    assert_eq!(manifest["store"], STORE_FILE);
    assert_eq!(manifest["concepts"], 9);
    assert_eq!(manifest["designations"], 12);
    assert_eq!(manifest["isAEdges"], 4);
    assert_eq!(manifest["languages"], serde_json::json!(["en", "nl"]));
    assert_eq!(report.languages, ["en", "nl"]);
}

#[test]
fn the_store_graph_and_text_crates_open_what_the_build_wrote() {
    let release = tempfile::tempdir().expect("tempdir");
    fixture::write_release(release.path());
    let out = tempfile::tempdir().expect("tempdir");
    let report = pipeline::build(release.path(), &[], out.path()).expect("build");
    let store = Store::open(&report.store).expect("store opens");
    assert_eq!(
        store.meta(tables::META_VERSION).expect("read"),
        Some(report.version_uri.clone())
    );

    // Point reads: the cat, its Dutch preferred synonym, its parent property.
    let cat = store.ordinal(&concept(3)).expect("read").expect("cat");
    let animal = store.ordinal(&concept(2)).expect("read").expect("animal");
    let fish = store.ordinal(&concept(5)).expect("read").expect("fish");
    assert!(store.concept(cat).expect("read").expect("cat").active);
    assert!(!store.concept(fish).expect("read").expect("fish").active);
    let nl = store
        .vocabulary_ordinal(Vocabulary::LanguageRefsets, NL_LANGUAGE_REFSET)
        .expect("read")
        .expect("nl refset");
    let gb = store
        .vocabulary_ordinal(Vocabulary::LanguageRefsets, GB_LANGUAGE_REFSET)
        .expect("read")
        .expect("gb refset");
    assert_eq!(
        store
            .preferred(cat, nl, 1)
            .expect("read")
            .expect("nl synonym")
            .term,
        "Kat"
    );
    assert_eq!(
        store
            .preferred(cat, gb, 1)
            .expect("read")
            .expect("gb synonym")
            .term,
        "Cat"
    );
    assert_eq!(
        store
            .preferred(cat, gb, 0)
            .expect("read")
            .expect("gb fsn")
            .term,
        "Cat (synthetic)"
    );
    let parent_key = store
        .vocabulary_ordinal(Vocabulary::PropertyKeys, "parent")
        .expect("read")
        .expect("parent key");
    let parents = |ordinal| {
        store
            .properties(ordinal)
            .expect("read")
            .into_iter()
            .find(|(key, _)| *key == parent_key)
            .map(|(_, values)| values)
    };
    assert_eq!(
        parents(cat),
        Some(vec![concept_store::record::PropertyValue::Concept(animal)])
    );
    assert!(parents(fish).is_none(), "the inactive edge is not a parent");

    // Attribute properties keyed by the attribute SCTID; concrete values typed.
    let property = |ordinal, key_name: &str| {
        let key = store
            .vocabulary_ordinal(Vocabulary::PropertyKeys, key_name)
            .expect("read")
            .expect("key");
        store
            .properties(ordinal)
            .expect("read")
            .into_iter()
            .find(|(k, _)| *k == key)
            .map(|(_, values)| values)
    };
    let fur = store.ordinal(&concept(7)).expect("read").expect("fur");
    assert_eq!(
        property(cat, &concept(6)),
        Some(vec![concept_store::record::PropertyValue::Concept(fur)])
    );
    assert_eq!(
        property(cat, &concept(8)),
        Some(vec![concept_store::record::PropertyValue::Decimal(
            String::from("4")
        )])
    );
    let dog = store.ordinal(&concept(4)).expect("read").expect("dog");
    assert!(
        property(dog, &concept(6)).is_none(),
        "the inactive attribute is not stored"
    );
}

/// Release file specification appendix E.5: a qualifying or additional row is
/// not part of the definition of its source concept, so it defines nothing.
#[test]
fn a_row_outside_the_inferred_view_defines_nothing() {
    let release = tempfile::tempdir().expect("tempdir");
    fixture::write_release(release.path());
    let out = tempfile::tempdir().expect("tempdir");
    let report = pipeline::build(release.path(), &[], out.path()).expect("build");
    assert_eq!(
        report.is_a_edges, 4,
        "the qualifying is-a row is not a hierarchy edge"
    );
    assert_eq!(
        report.attributes, 2,
        "the additional and the qualifying concrete row are not attributes"
    );
    assert_eq!(
        report.skipped_relationships, 4,
        "the build says how many rows it left out rather than dropping them quietly"
    );

    let store = Store::open(&report.store).expect("store opens");
    let animal = store.ordinal(&concept(2)).expect("read").expect("animal");
    let dog = store.ordinal(&concept(4)).expect("read").expect("dog");
    let graph = fs::read(&report.hierarchy).expect("hierarchy file");
    let hierarchy = Hierarchy::read_from(&mut graph.as_slice()).expect("hierarchy reads");
    assert_eq!(
        hierarchy.is_a.neighbours(dog),
        [animal.index()],
        "the dog's only parent is the animal"
    );

    let property = |ordinal, key_name: &str| {
        let key = store
            .vocabulary_ordinal(Vocabulary::PropertyKeys, key_name)
            .expect("read")
            .expect("key");
        store
            .properties(ordinal)
            .expect("read")
            .into_iter()
            .find(|(k, _)| *k == key)
            .map(|(_, values)| values)
    };
    assert!(
        property(dog, &concept(6)).is_none(),
        "the additional relationship is not a property"
    );
    assert!(
        property(dog, &concept(8)).is_none(),
        "the qualifying concrete value is not a property"
    );
    assert!(
        store.ordinal(&concept(50)).expect("read").is_none(),
        "the qualifying row's destination stays outside the edition, and the \
         build does not refuse it"
    );
}

#[test]
fn the_graph_and_text_files_open_beside_the_store() {
    let release = tempfile::tempdir().expect("tempdir");
    fixture::write_release(release.path());
    let out = tempfile::tempdir().expect("tempdir");
    let report = pipeline::build(release.path(), &[], out.path()).expect("build");
    let store = Store::open(&report.store).expect("store opens");
    let cat = store.ordinal(&concept(3)).expect("read").expect("cat");
    let animal = store.ordinal(&concept(2)).expect("read").expect("animal");
    let fish = store.ordinal(&concept(5)).expect("read").expect("fish");
    let nl = store
        .vocabulary_ordinal(Vocabulary::LanguageRefsets, NL_LANGUAGE_REFSET)
        .expect("read")
        .expect("nl refset");

    // The hierarchy file: cat is under animal and the root; fish is under nothing.
    let graph = fs::read(&report.hierarchy).expect("hierarchy file");
    let hierarchy = Hierarchy::read_from(&mut graph.as_slice()).expect("hierarchy reads");
    let top = store.ordinal(&concept(1)).expect("read").expect("top");
    assert!(hierarchy.closure.is_ancestor(animal, cat));
    assert!(hierarchy.closure.is_ancestor(top, cat));
    assert!(hierarchy.closure.ancestors(fish).is_empty());
    assert_eq!(hierarchy.is_a.nodes(), 9);

    // The text file: a Dutch prefix, filtered by the NL refset, ranks the shortest first.
    let text = fs::read(&report.text).expect("text file");
    let index = read_from(&mut text.as_slice()).expect("index reads");
    assert_eq!(index.len(), 12);
    let hits = index.search(
        &Query {
            text: String::from("po"),
            refset: Some(nl),
            active_only: true,
            ..Query::default()
        },
        0,
        10,
    );
    assert_eq!(hits.total, 1);
    let entry = index.entry(hits.designations[0]).expect("entry");
    assert_eq!(entry.concept, cat);
    let synth = index.search(
        &Query {
            text: String::from("synth"),
            ..Query::default()
        },
        0,
        10,
    );
    assert_eq!(synth.total, 5, "every FSN carries the semantic tag");
}

#[test]
fn a_release_without_module_dependencies_is_refused() {
    let release = tempfile::tempdir().expect("tempdir");
    fixture::write_release(release.path());
    let dependency = fs::read_dir(release.path().join("Snapshot/Refset/Metadata"))
        .expect("dir")
        .next()
        .expect("file")
        .expect("entry")
        .path();
    fs::remove_file(dependency).expect("remove");
    let out = tempfile::tempdir().expect("tempdir");
    assert!(matches!(
        pipeline::build(release.path(), &[], out.path()),
        Err(pipeline::Error::MissingFile(_))
    ));
}

#[test]
fn the_reference_set_memberships_are_written_beside_the_store() {
    let release = tempfile::tempdir().expect("tempdir");
    fixture::write_release(release.path());
    let out = tempfile::tempdir().expect("tempdir");
    let report = pipeline::build(release.path(), &[], out.path()).expect("builds");
    assert_eq!(
        report.refsets, 1,
        "the simple reference set; the language ones are acceptabilities"
    );
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(out.path().join("manifest.json")).expect("manifest"),
    )
    .expect("json");
    assert_eq!(manifest["refsets"], pipeline::REFSETS_FILE);
    assert_eq!(manifest["referenceSets"], 1);
    assert_eq!(manifest["memberships"], 2, "the inactive member is not one");
    let memberships = concept_graph::members::Memberships::read_from(
        &mut fs::read(out.path().join(pipeline::REFSETS_FILE))
            .expect("refsets")
            .as_slice(),
    )
    .expect("reads");
    let refset: u64 = concept(8).parse().expect("number");
    assert_eq!(memberships.refsets().collect::<Vec<_>>(), [refset]);
    let store = Store::open(&report.store).expect("opens");
    let cat = store.ordinal(&concept(3)).expect("read").expect("cat");
    let dog = store.ordinal(&concept(4)).expect("read").expect("dog");
    let fish = store.ordinal(&concept(5)).expect("read").expect("fish");
    let members = memberships.members(refset).expect("members");
    assert!(members.contains(cat.index()) && members.contains(dog.index()));
    assert!(!members.contains(fish.index()));
}

#[test]
fn the_attribute_graph_member_tables_and_identifiers_are_written_beside_the_store() {
    let release = tempfile::tempdir().expect("tempdir");
    fixture::write_release(release.path());
    let out = tempfile::tempdir().expect("tempdir");
    let report = pipeline::build(release.path(), &[], out.path()).expect("builds");
    assert_eq!(
        report.attributes, 2,
        "the cat's covering and leg count; the dog's inactive one is not"
    );
    assert_eq!(
        report.member_rows, 2,
        "the active simple reference set members"
    );
    assert_eq!(report.identifiers, 0, "the fixture has no identifier file");
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(out.path().join("manifest.json")).expect("manifest"),
    )
    .expect("json");
    assert_eq!(manifest["attributes"], pipeline::ATTRIBUTES_FILE);
    assert_eq!(manifest["members"], pipeline::MEMBERS_FILE);
    assert_eq!(manifest["identifiers"], pipeline::IDENTIFIERS_FILE);
    assert_eq!(manifest["attributeRows"], 2);
    let store = Store::open(&report.store).expect("opens");
    let ordinal = |item: u32| {
        store
            .ordinal(&concept(item))
            .expect("read")
            .expect("concept")
    };
    let (cat, fur) = (ordinal(3), ordinal(7));
    let attributes = concept_graph::attributes::Attributes::read_from(
        &mut fs::read(out.path().join(pipeline::ATTRIBUTES_FILE))
            .expect("attributes")
            .as_slice(),
    )
    .expect("reads");
    let covering: u64 = concept(6).parse().expect("number");
    let legs: u64 = concept(8).parse().expect("number");
    let covering_kind = attributes.kind(covering).expect("covering type");
    let legs_kind = attributes.kind(legs).expect("leg count type");
    let rows: Vec<_> = attributes.rows(cat).collect();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|r| {
        r.kind == covering_kind && r.value == concept_graph::attributes::ValueRef::Concept(fur)
    }));
    assert!(rows.iter().any(|r| {
        r.kind == legs_kind
            && r.group == 2
            && r.value == concept_graph::attributes::ValueRef::Number("4")
    }));
    assert_eq!(attributes.sources(covering_kind, fur), [cat.index()]);
    let members = concept_graph::refsets::RefsetMembers::read_from(
        &mut fs::read(out.path().join(pipeline::MEMBERS_FILE))
            .expect("members")
            .as_slice(),
    )
    .expect("reads");
    let table = members.table(legs).expect("the simple reference set table");
    assert_eq!(table.len(), 2);
    assert_eq!(table.fields(), Vec::<String>::new());
    assert!(table.members().contains(cat.index()));
    let identifiers = concept_graph::identifiers::Identifiers::read_from(
        &mut fs::read(out.path().join(pipeline::IDENTIFIERS_FILE))
            .expect("identifiers")
            .as_slice(),
    )
    .expect("reads");
    assert!(identifiers.is_empty());
}
