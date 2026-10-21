// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: Apache-2.0

use rf2::component::{
    Concept, ConcreteRelationship, ConcreteValue, Description, Relationship, Rows,
};
use rf2::constants;
use rf2::edition::Edition;
use rf2::file::{ContentType, FieldKind, Release, ReleaseError, ReleaseType};
use rf2::id::Sctid;
use rf2::reader::Rf2Error;
use rf2::refset::{AssociationMember, LanguageMember, Member, Members, ModuleDependencyMember};

use crate::fixture;

fn open() -> (fixture::Release, Release) {
    let synthetic = fixture::standard();
    let release = Release::open(synthetic.root(), ReleaseType::Snapshot).expect("release opens");
    (synthetic, release)
}

fn rows<T: rf2::component::Component>(release: &Release, content: &ContentType) -> Vec<T> {
    let file = release.of_type(content).next().expect("file present");
    Rows::<_, T>::open(&file.path)
        .expect("header matches")
        .collect::<Result<Vec<_>, _>>()
        .expect("rows parse")
}

fn members(release: &Release, summary: &str) -> Vec<Member> {
    let file = release
        .refsets()
        .find(|f| f.name.summary == summary)
        .expect("refset file present");
    let ContentType::Refset(kinds) = &file.name.content_type else {
        panic!("refset content type");
    };
    Members::open(&file.path, kinds)
        .expect("header matches")
        .collect::<Result<Vec<_>, _>>()
        .expect("members parse")
}

#[test]
fn a_release_lists_its_rf2_files_and_skips_the_rest() {
    let (_synthetic, release) = open();
    assert_eq!(release.release_type(), ReleaseType::Snapshot);
    assert_eq!(release.date().compact(), fixture::DATE);
    assert_eq!(release.files().len(), 9);
    assert_eq!(release.of_type(&ContentType::Description).count(), 2);
    assert_eq!(release.refsets().count(), 4);
    let language = release
        .refsets()
        .find(|f| f.name.summary == "Language")
        .expect("language refset");
    assert_eq!(
        language.name.content_type,
        ContentType::Refset(vec![FieldKind::Component])
    );
    assert_eq!(language.name.language.as_deref(), Some("en"));
}

#[test]
fn concepts_descriptions_and_relationships_stream_as_typed_rows() {
    let (_synthetic, release) = open();
    let concepts: Vec<Concept> = rows(&release, &ContentType::Concept);
    assert_eq!(concepts.len(), 4);
    let root = concepts
        .iter()
        .find(|c| c.id.to_string() == fixture::concept(1))
        .expect("root");
    assert!(root.base.active);
    assert_eq!(root.definition_status_id, constants::PRIMITIVE);
    assert_eq!(root.base.module_id.to_string(), fixture::extension_module());
    let grandchild = concepts
        .iter()
        .find(|c| c.id.to_string() == fixture::concept(3))
        .expect("grandchild");
    assert!(!grandchild.base.active);

    let descriptions: Vec<Description> = rows(&release, &ContentType::Description);
    assert_eq!(descriptions.len(), 3);
    assert_eq!(
        descriptions
            .iter()
            .filter(|d| d.type_id == constants::FULLY_SPECIFIED_NAME)
            .count(),
        2
    );
    assert_eq!(
        descriptions
            .iter()
            .find(|d| d.type_id == constants::SYNONYM)
            .map(|d| d.term.as_str()),
        Some("Synthetic root")
    );

    let relationships: Vec<Relationship> = rows(&release, &ContentType::Relationship);
    assert_eq!(relationships.len(), 2);
    assert!(
        relationships.iter().all(
            |r| r.type_id == constants::IS_A && r.characteristic_type_id == constants::INFERRED
        )
    );
    assert_eq!(
        relationships
            .iter()
            .find(|r| r.source_id.to_string() == fixture::concept(2))
            .map(|r| r.destination_id.to_string()),
        Some(fixture::concept(1))
    );

    let concrete: Vec<ConcreteRelationship> =
        rows(&release, &ContentType::RelationshipConcreteValues);
    assert_eq!(concrete.len(), 1);
    assert_eq!(
        concrete.first().map(|r| &r.value),
        Some(&ConcreteValue::Number("3".to_owned()))
    );
    assert_eq!(concrete.first().map(|r| r.relationship_group), Some(1));
}

#[test]
fn reference_set_members_carry_typed_fields() {
    let (_synthetic, release) = open();
    let language = members(&release, "Language");
    assert_eq!(language.len(), 3);
    let typed: Vec<LanguageMember> = language
        .into_iter()
        .map(|m| LanguageMember::try_from(m).expect("language view"))
        .collect();
    assert_eq!(
        typed
            .iter()
            .filter(|m| m.acceptability_id == constants::PREFERRED)
            .count(),
        2
    );
    assert_eq!(
        typed.first().map(|m| m.member.refset_id),
        Some(constants::GB_ENGLISH_LANGUAGE_REFSET)
    );

    let simple = members(&release, "Simple");
    assert_eq!(simple.len(), 1);
    assert!(simple.first().is_some_and(|m| m.fields.is_empty()));
    assert!(LanguageMember::try_from(simple.into_iter().next().expect("member")).is_err());

    let association = members(&release, "Association");
    let typed = AssociationMember::try_from(association.into_iter().next().expect("member"))
        .expect("association view");
    assert_eq!(
        typed.target_component_id,
        Sctid::parse(&fixture::concept(2)).expect("valid")
    );
}

#[test]
fn the_edition_is_the_module_no_other_module_depends_on() {
    let (_synthetic, release) = open();
    let dependencies: Vec<ModuleDependencyMember> = members(&release, "ModuleDependency")
        .into_iter()
        .map(|m| ModuleDependencyMember::try_from(m).expect("dependency view"))
        .collect();
    assert_eq!(dependencies.len(), 3);
    let edition = Edition::identify(&dependencies, release.date()).expect("edition identifies");
    assert_eq!(edition.module.to_string(), fixture::extension_module());
    assert_eq!(edition.effective_time.compact(), fixture::DATE);
    assert_eq!(edition.modules.len(), 3);
    assert_eq!(
        edition
            .modules
            .get(&constants::CORE_MODULE.into())
            .map(|t| t.compact()),
        Some("20251201".to_owned())
    );
    assert_eq!(
        edition.edition_uri(),
        format!("http://snomed.info/sct/{}", fixture::extension_module())
    );
    assert_eq!(
        edition.version_uri(),
        format!(
            "http://snomed.info/sct/{}/version/{}",
            fixture::extension_module(),
            fixture::DATE
        )
    );
}

#[test]
fn a_wrong_header_and_a_bad_identifier_name_file_line_and_column() {
    let (synthetic, release) = open();
    let concept_file = release
        .of_type(&ContentType::Concept)
        .next()
        .expect("file")
        .path
        .clone();
    let description_error =
        Rows::<_, Description>::open(&concept_file).expect_err("header mismatch");
    assert!(matches!(description_error, Rf2Error::Header { .. }));

    let broken = synthetic
        .root()
        .join("Snapshot/Terminology/sct2_Concept_Snapshot_XX1234567_20260101.txt");
    let mut text = std::fs::read_to_string(&broken).expect("read");
    text.push_str("116680004\t20260101\t1\t900000000000207008\t900000000000074008\r\n");
    std::fs::write(&broken, text).expect("write");
    let error = Rows::<_, Concept>::open(&broken)
        .expect("header")
        .collect::<Result<Vec<_>, _>>()
        .expect_err("check digit fails");
    match error {
        Rf2Error::Field {
            line, column, name, ..
        } => {
            assert_eq!((line, column, name.as_str()), (6, 0, "id"));
        }
        other => panic!("expected a field error, got {other:?}"),
    }
}

#[test]
fn a_directory_without_matching_files_is_refused() {
    let empty = tempfile::tempdir().expect("tempdir");
    assert!(matches!(
        Release::open(empty.path(), ReleaseType::Snapshot),
        Err(ReleaseError::NoFiles { .. })
    ));
    let (synthetic, _release) = open();
    assert!(matches!(
        Release::open(synthetic.root(), ReleaseType::Full),
        Err(ReleaseError::NoFiles { .. })
    ));
}

#[test]
fn a_sibling_root_module_is_reported_and_the_release_date_picks_the_edition() {
    use rf2::id::{MemberId, ModuleId, RefsetId, Sctid};
    use rf2::refset::{FieldValue, Member};
    use rf2::time::EffectiveTime;

    let module = |text: &str| ModuleId::parse(text).expect("module");
    let time = |text: &str| EffectiveTime::parse(text).expect("time");
    let dependency =
        |item: u32, source: &str, target: &str, source_time: &str, target_time: &str| {
            ModuleDependencyMember::try_from(Member {
                id: MemberId::parse(&fixture::member(item)).expect("uuid"),
                effective_time: time(source_time),
                active: true,
                module_id: module(source),
                refset_id: RefsetId::parse(fixture::MODULE_DEPENDENCY_REFSET).expect("refset"),
                referenced_component_id: Sctid::parse(target).expect("sctid"),
                fields: vec![
                    (
                        String::from("sourceEffectiveTime"),
                        FieldValue::String(source_time.to_owned()),
                    ),
                    (
                        String::from("targetEffectiveTime"),
                        FieldValue::String(target_time.to_owned()),
                    ),
                ],
            })
            .expect("dependency view")
        };
    // The extension module and a mapping module both depend on core and model; only the
    // extension carries the release date, as the NL edition ships it.
    let extension = fixture::extension_module();
    let mapping = fixture::concept(5);
    let members = vec![
        dependency(
            1,
            &extension,
            fixture::CORE_MODULE,
            fixture::DATE,
            "20251201",
        ),
        dependency(
            2,
            &extension,
            fixture::MODEL_MODULE,
            fixture::DATE,
            "20251201",
        ),
        dependency(3, &mapping, fixture::CORE_MODULE, "20251201", "20251201"),
        dependency(4, &mapping, fixture::MODEL_MODULE, "20251201", "20251201"),
        dependency(
            5,
            fixture::CORE_MODULE,
            fixture::MODEL_MODULE,
            "20251201",
            "20251201",
        ),
    ];
    let edition = Edition::identify(&members, time(fixture::DATE)).expect("identifies");
    assert_eq!(edition.module.to_string(), extension);
    assert_eq!(edition.sibling_roots, vec![module(&mapping)]);
    // Without a root at the release date the edition is ambiguous.
    assert!(matches!(
        Edition::identify(&members, time("20250101")),
        Err(rf2::edition::EditionError::AmbiguousRoot { .. })
    ));
    // A target that is not a concept identifier is a typed error, never dropped.
    let mut malformed = members;
    if let Some(first) = malformed.first_mut() {
        first.member.referenced_component_id =
            Sctid::parse(&fixture::description(1)).expect("sctid");
    }
    assert!(matches!(
        Edition::identify(&malformed, time(fixture::DATE)),
        Err(rf2::edition::EditionError::MalformedTarget { .. })
    ));
}

// The International Edition ships the ICD-10 mapping module, which depends on
// the core and model modules and is depended on by neither, so the graph alone
// makes it the only root. The URI Standard names the edition by its core
// module instead (§2 Table 2.1), which is what this pins (#359).
#[test]
fn an_international_release_is_its_core_module_not_the_mapping_module_that_roots_the_graph() {
    use rf2::id::{MemberId, ModuleId, RefsetId, Sctid};
    use rf2::refset::{FieldValue, Member};
    use rf2::time::EffectiveTime;

    let module = |text: &str| ModuleId::parse(text).expect("module");
    let time = |text: &str| EffectiveTime::parse(text).expect("time");
    let dependency = |item: u32, source: &str, target: &str| {
        ModuleDependencyMember::try_from(Member {
            id: MemberId::parse(&fixture::member(item)).expect("uuid"),
            effective_time: time(fixture::DATE),
            active: true,
            module_id: module(source),
            refset_id: RefsetId::parse(fixture::MODULE_DEPENDENCY_REFSET).expect("refset"),
            referenced_component_id: Sctid::parse(target).expect("sctid"),
            fields: vec![
                (
                    String::from("sourceEffectiveTime"),
                    FieldValue::String(fixture::DATE.to_owned()),
                ),
                (
                    String::from("targetEffectiveTime"),
                    FieldValue::String(fixture::DATE.to_owned()),
                ),
            ],
        })
        .expect("dependency view")
    };
    // `449080006 |SNOMED CT to ICD-10 rule-based mapping module|`, as the
    // International release ships it. No module here carries a namespace.
    let mapping = "449080006";
    let members = vec![
        dependency(1, mapping, fixture::CORE_MODULE),
        dependency(2, mapping, fixture::MODEL_MODULE),
        dependency(3, fixture::CORE_MODULE, fixture::MODEL_MODULE),
    ];
    let edition = Edition::identify(&members, time(fixture::DATE)).expect("identifies");
    assert_eq!(
        edition.module.to_string(),
        fixture::CORE_MODULE,
        "the edition is the core module the URI Standard names, not the graph's only root"
    );
    assert_eq!(
        edition.edition_uri(),
        "http://snomed.info/sct/900000000000207008"
    );
    assert_eq!(edition.sibling_roots, vec![module(mapping)]);
}
