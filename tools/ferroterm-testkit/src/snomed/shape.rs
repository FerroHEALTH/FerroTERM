// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Writing one shaped edition as the build writes it: the store, the hierarchy, the designation index, the reference set memberships, the attribute and member tables, and the manifest.

use super::{
    ALTERNATIVE, ALTERNATIVE_SCTID, ANIMAL, BIRD, CAT, CODES_MAP, COVERING, DOG, FISH, FUR,
    FixtureError, GB_LANGUAGE_REFSET, GB_REFSET, HISTORICAL, HISTORICAL_SCTID, ICD10_MAP,
    ICD10_MAP_SCTID, Item, LEGS, MODULE_CONCEPT, MODULE_DEPENDENCY, MODULE_DEPENDENCY_SCTID,
    NL_LANGUAGE_REFSET, NL_REFSET, PETS, POSSIBLY_EQUIVALENT_TO, POSSIBLY_EQUIVALENT_TO_SCTID,
    REPLACED_BY, REPLACED_BY_SCTID, Row, SAME_AS, SAME_AS_SCTID, SCHEME, Shape, TOP, code_of, item,
    sctid, write_concepts,
};
use crate::edition::{self, ord};
use concept_graph::attributes::{self, Attributes};
use concept_graph::identifiers::Identifiers;
use concept_graph::members::Memberships;
use concept_graph::ordinal::Ordinal;
use concept_graph::refsets::{self, RefsetMembers};
use concept_store::builder::StoreBuilder;
use concept_store::record::PropertyValue;
use concept_store::store::Vocabulary;
use concept_store::tables;
use designation_index::index::IndexBuilder;
use std::path::Path;

#[expect(
    clippy::too_many_lines,
    reason = "one synthetic edition, read top to bottom"
)]
pub(super) fn write_shape(dir: &Path, shape: &Shape<'_>) -> Result<(), FixtureError> {
    let module = sctid(Item::raw(99));
    let fsn = 0;
    let syn = 1;
    let (gb, nl) = (0, 1);
    let (preferred, acceptable) = (0, 1);
    // The association reference set concepts share one shape: a published SCTID,
    // the metadata fully specified name, and the bare name as the synonym.
    let metadata_refset = |ordinal: u32, code: &'static str, name: &'static str| Row {
        ordinal,
        code: Some(code),
        active: true,
        defined: false,
        designations: vec![
            (name, "en", fsn, true, vec![(gb, preferred)]),
            (
                name.trim_end_matches(" (foundation metadata concept)"),
                "en",
                syn,
                true,
                vec![(gb, preferred)],
            ),
        ],
    };
    let rows = [
        Row {
            ordinal: TOP,
            code: None,
            active: true,
            defined: false,
            designations: vec![
                (
                    "Living thing (synthetic)",
                    "en",
                    fsn,
                    true,
                    vec![(gb, preferred)],
                ),
                ("Living thing", "en", syn, true, vec![(gb, preferred)]),
            ],
        },
        Row {
            ordinal: ANIMAL,
            code: None,
            active: true,
            defined: false,
            designations: vec![
                ("Animal (synthetic)", "en", fsn, true, vec![(gb, preferred)]),
                ("Animal", "en", syn, true, vec![(gb, preferred)]),
                ("Dier", "nl", syn, true, vec![(nl, preferred)]),
            ],
        },
        Row {
            ordinal: CAT,
            code: None,
            active: true,
            defined: true,
            designations: vec![
                ("Cat (synthetic)", "en", fsn, true, vec![(gb, preferred)]),
                ("Cat", "en", syn, true, vec![(gb, preferred)]),
                ("Kat", "nl", syn, true, vec![(nl, preferred)]),
                ("Poes", "nl", syn, true, vec![(nl, acceptable)]),
                ("Moggy", "en", syn, false, vec![]),
            ],
        },
        Row {
            ordinal: DOG,
            code: None,
            active: true,
            defined: true,
            designations: vec![
                ("Dog (synthetic)", "en", fsn, true, vec![(gb, preferred)]),
                ("Dog", "en", syn, true, vec![(gb, preferred)]),
                ("Hond", "nl", syn, true, vec![(nl, preferred)]),
            ],
        },
        Row {
            ordinal: FISH,
            code: None,
            active: false,
            defined: false,
            designations: vec![
                ("Fish (synthetic)", "en", fsn, true, vec![(gb, preferred)]),
                ("Fish", "en", syn, true, vec![]),
            ],
        },
        Row {
            ordinal: FUR,
            code: None,
            active: true,
            defined: false,
            designations: vec![
                ("Fur (synthetic)", "en", fsn, true, vec![(gb, preferred)]),
                ("Fur", "en", syn, true, vec![(gb, preferred)]),
            ],
        },
        Row {
            ordinal: COVERING,
            code: None,
            active: true,
            defined: false,
            designations: vec![
                (
                    "Has covering (attribute)",
                    "en",
                    fsn,
                    true,
                    vec![(gb, preferred)],
                ),
                ("Has covering", "en", syn, true, vec![(gb, preferred)]),
            ],
        },
        Row {
            ordinal: LEGS,
            code: None,
            active: true,
            defined: false,
            designations: vec![
                (
                    "Leg count (attribute)",
                    "en",
                    fsn,
                    true,
                    vec![(gb, preferred)],
                ),
                ("Leg count", "en", syn, true, vec![(gb, preferred)]),
            ],
        },
        Row {
            ordinal: PETS,
            code: None,
            active: true,
            defined: false,
            designations: vec![
                (
                    "Pets reference set (foundation metadata concept)",
                    "en",
                    fsn,
                    true,
                    vec![(gb, preferred)],
                ),
                ("Pets reference set", "en", syn, true, vec![(gb, preferred)]),
            ],
        },
        Row {
            ordinal: CODES_MAP,
            code: None,
            active: true,
            defined: false,
            designations: vec![
                (
                    "Codes map reference set (foundation metadata concept)",
                    "en",
                    fsn,
                    true,
                    vec![(gb, preferred)],
                ),
                (
                    "Codes map reference set",
                    "en",
                    syn,
                    true,
                    vec![(gb, preferred)],
                ),
            ],
        },
        Row {
            ordinal: HISTORICAL,
            code: Some(HISTORICAL_SCTID),
            active: true,
            defined: false,
            designations: vec![
                (
                    "Historical association reference set (foundation metadata concept)",
                    "en",
                    fsn,
                    true,
                    vec![(gb, preferred)],
                ),
                (
                    "Historical association reference set",
                    "en",
                    syn,
                    true,
                    vec![(gb, preferred)],
                ),
            ],
        },
        Row {
            ordinal: SAME_AS,
            code: Some(SAME_AS_SCTID),
            active: true,
            defined: false,
            designations: vec![
                (
                    "SAME AS association reference set (foundation metadata concept)",
                    "en",
                    fsn,
                    true,
                    vec![(gb, preferred)],
                ),
                (
                    "SAME AS association reference set",
                    "en",
                    syn,
                    true,
                    vec![(gb, preferred)],
                ),
            ],
        },
        Row {
            ordinal: SCHEME,
            code: None,
            active: true,
            defined: false,
            designations: vec![
                (
                    "Zoo code system (identifier scheme)",
                    "en",
                    fsn,
                    true,
                    vec![(gb, preferred)],
                ),
                ("Zoo code system", "en", syn, true, vec![(gb, preferred)]),
                ("ZOO", "en", syn, true, vec![(gb, acceptable)]),
            ],
        },
        metadata_refset(
            REPLACED_BY,
            REPLACED_BY_SCTID,
            "REPLACED BY association reference set (foundation metadata concept)",
        ),
        metadata_refset(
            POSSIBLY_EQUIVALENT_TO,
            POSSIBLY_EQUIVALENT_TO_SCTID,
            "POSSIBLY EQUIVALENT TO association reference set (foundation metadata concept)",
        ),
        metadata_refset(
            ALTERNATIVE,
            ALTERNATIVE_SCTID,
            "ALTERNATIVE association reference set (foundation metadata concept)",
        ),
        metadata_refset(
            MODULE_DEPENDENCY,
            MODULE_DEPENDENCY_SCTID,
            "Module dependency reference set (foundation metadata concept)",
        ),
        Row {
            ordinal: MODULE_CONCEPT,
            code: None,
            active: true,
            defined: false,
            designations: vec![
                (
                    "Synthetic edition module (core metadata concept)",
                    "en",
                    fsn,
                    true,
                    vec![(gb, preferred)],
                ),
                (
                    "Synthetic edition module",
                    "en",
                    syn,
                    true,
                    vec![(gb, preferred)],
                ),
            ],
        },
        metadata_refset(
            ICD10_MAP,
            ICD10_MAP_SCTID,
            "ICD-10 extended map reference set (foundation metadata concept)",
        ),
        metadata_refset(
            GB_LANGUAGE_REFSET,
            GB_REFSET,
            "Great Britain English language reference set (foundation metadata concept)",
        ),
        metadata_refset(
            NL_LANGUAGE_REFSET,
            NL_REFSET,
            "Netherlands Dutch language reference set (foundation metadata concept)",
        ),
    ];
    let mut rows = Vec::from(rows);
    let mut is_a = vec![
        (ANIMAL, TOP),
        (CAT, ANIMAL),
        (DOG, ANIMAL),
        (FUR, TOP),
        (COVERING, TOP),
        (LEGS, TOP),
        (PETS, TOP),
        (CODES_MAP, TOP),
        (HISTORICAL, TOP),
        (SAME_AS, HISTORICAL),
        (REPLACED_BY, HISTORICAL),
        (POSSIBLY_EQUIVALENT_TO, HISTORICAL),
        (ALTERNATIVE, HISTORICAL),
        (MODULE_DEPENDENCY, TOP),
        (MODULE_CONCEPT, TOP),
        (SCHEME, TOP),
        (ICD10_MAP, TOP),
        (GB_LANGUAGE_REFSET, TOP),
        (NL_LANGUAGE_REFSET, TOP),
    ];
    if shape.bird {
        rows.push(Row {
            ordinal: BIRD,
            code: None,
            active: true,
            defined: false,
            designations: vec![
                ("Bird (synthetic)", "en", fsn, true, vec![(gb, preferred)]),
                ("Bird", "en", syn, true, vec![(gb, preferred)]),
                ("Vogel", "nl", syn, true, vec![(nl, preferred)]),
            ],
        });
        is_a.push((BIRD, ANIMAL));
    }
    let rows = rows;
    let is_a = is_a;
    let mut builder = StoreBuilder::create(
        &dir.join("store.redb"),
        "http://snomed.info/sct",
        shape.version,
    )?;
    edition::write_property_keys(&mut builder)?;
    // Attribute types after the fixed keys, sorted by SCTID as the build does.
    let mut attribute_types = [(COVERING, sctid(item(COVERING))), (LEGS, sctid(item(LEGS)))];
    attribute_types.sort_by(|a, b| a.1.cmp(&b.1));
    for (i, (_, code)) in attribute_types.iter().enumerate() {
        builder.vocabulary(Vocabulary::PropertyKeys, 3 + ord(i), code)?;
    }
    let attribute_key = |ordinal: u32| {
        3 + ord(attribute_types
            .iter()
            .position(|(o, _)| *o == ordinal)
            .unwrap_or_default())
    };
    edition::write_designation_vocabularies(&mut builder, gb, nl)?;

    let mut text = IndexBuilder::default();
    write_concepts(&mut builder, &mut text, &rows, &is_a, &module)?;
    // The cat and the dog have covering fur and four legs; the cat states both
    // in one role group, the dog in two.
    for animal in [CAT, DOG] {
        builder.properties(
            Ordinal::new(animal),
            attribute_key(COVERING),
            &[PropertyValue::Concept(Ordinal::new(FUR))],
        )?;
        builder.properties(
            Ordinal::new(animal),
            attribute_key(LEGS),
            &[PropertyValue::Decimal(String::from("4"))],
        )?;
    }
    let attribute_types = {
        let mut types: Vec<u64> = [COVERING, LEGS]
            .iter()
            .map(|o| sctid(item(*o)).parse().unwrap_or_default())
            .collect();
        types.sort_unstable();
        types
    };
    let kind = |ordinal: u32| -> u32 {
        let code: u64 = sctid(item(ordinal)).parse().unwrap_or_default();
        ord(attribute_types
            .iter()
            .position(|t| *t == code)
            .unwrap_or_default())
    };
    let edge =
        |source: u32, group: u32, attribute: u32, value: attributes::Value| attributes::Edge {
            source: Ordinal::new(source),
            group,
            kind: kind(attribute),
            value,
        };
    let attributes = Attributes::build(
        ord(rows.len()),
        attribute_types.clone(),
        vec![
            edge(
                CAT,
                1,
                COVERING,
                attributes::Value::Concept(Ordinal::new(FUR)),
            ),
            edge(CAT, 1, LEGS, attributes::Value::Number(String::from("4"))),
            edge(
                DOG,
                1,
                COVERING,
                attributes::Value::Concept(Ordinal::new(FUR)),
            ),
            edge(DOG, 2, LEGS, attributes::Value::Number(String::from("4"))),
        ],
    )
    .map_err(|e| FixtureError::Graph(e.to_string()))?;
    edition::write_graph_part(dir, "attributes.bin", |bytes| attributes.write_to(bytes))?;

    edition::write_hierarchy(dir, ord(rows.len()), is_a.iter().copied())?;
    edition::write_text(dir, text)?;
    let mut memberships = Memberships::default();
    let refset_id = |ordinal: u32| -> Result<u64, FixtureError> {
        rows.iter()
            .find(|r| r.ordinal == ordinal)
            .map(code_of)
            .and_then(|code| code.parse().ok())
            .ok_or_else(|| FixtureError::Graph(String::from("refset id")))
    };
    let (pets, codes_map, same_as) = (refset_id(PETS)?, refset_id(CODES_MAP)?, refset_id(SAME_AS)?);
    let replaced_by = refset_id(REPLACED_BY)?;
    let possibly_equivalent_to = refset_id(POSSIBLY_EQUIVALENT_TO)?;
    let alternative = refset_id(ALTERNATIVE)?;
    memberships.insert(pets, Ordinal::new(CAT));
    memberships.insert(pets, Ordinal::new(DOG));
    if shape.bird {
        memberships.insert(pets, Ordinal::new(BIRD));
    }
    memberships.insert(codes_map, Ordinal::new(CAT));
    memberships.insert(codes_map, Ordinal::new(DOG));
    memberships.insert(same_as, Ordinal::new(FISH));
    memberships.insert(replaced_by, Ordinal::new(FISH));
    memberships.insert(possibly_equivalent_to, Ordinal::new(FISH));
    memberships.insert(alternative, Ordinal::new(FISH));
    let module_dependency = refset_id(MODULE_DEPENDENCY)?;
    memberships.insert(module_dependency, Ordinal::new(MODULE_CONCEPT));
    let icd10_map = refset_id(ICD10_MAP)?;
    memberships.insert(icd10_map, Ordinal::new(CAT));
    let module_id: u64 = module.parse().unwrap_or_default();
    let member = |concept: u32, values: Vec<refsets::FieldValue>| refsets::MemberRow {
        concept: Ordinal::new(concept),
        effective_time: 20_260_101,
        module: module_id,
        values,
    };
    let mut tables = RefsetMembers::default();
    let mut pet_rows = vec![member(CAT, vec![]), member(DOG, vec![])];
    if shape.bird {
        pet_rows.push(member(BIRD, vec![]));
    }
    tables
        .insert(pets, &[], pet_rows)
        .map_err(|e| FixtureError::Graph(e.to_string()))?;
    // The Module Dependency reference set states which module an edition was
    // built on, in the columns RF2 gives it.
    tables
        .insert(
            module_dependency,
            &[
                (
                    String::from("sourceEffectiveTime"),
                    refsets::FieldKind::Integer,
                ),
                (
                    String::from("targetEffectiveTime"),
                    refsets::FieldKind::Integer,
                ),
            ],
            vec![member(
                MODULE_CONCEPT,
                vec![
                    refsets::FieldValue::Integer(20_260_101),
                    refsets::FieldValue::Integer(20_260_101),
                ],
            )],
        )
        .map_err(|e| FixtureError::Graph(e.to_string()))?;
    tables
        .insert(
            codes_map,
            &edition::extended_map_columns(),
            vec![
                member(CAT, edition::extended_map_values("C01")),
                member(DOG, edition::extended_map_values("D01")),
            ],
        )
        .map_err(|e| FixtureError::Graph(e.to_string()))?;
    // The ICD-10 extended map carries the same RF2 columns under a published
    // reference set id, so its targets are codes of a named code system.
    tables
        .insert(
            icd10_map,
            &edition::extended_map_columns(),
            vec![member(CAT, edition::extended_map_values("C01"))],
        )
        .map_err(|e| FixtureError::Graph(e.to_string()))?;
    let target_component = [(
        String::from("targetComponentId"),
        refsets::FieldKind::Component,
    )];
    for (refset, target) in [
        (same_as, CAT),
        (replaced_by, DOG),
        (possibly_equivalent_to, DOG),
        (alternative, CAT),
    ] {
        tables
            .insert(
                refset,
                &target_component,
                vec![member(
                    FISH,
                    vec![refsets::FieldValue::Concept(Ordinal::new(target))],
                )],
            )
            .map_err(|e| FixtureError::Graph(e.to_string()))?;
    }
    edition::write_graph_part(dir, "members.bin", |bytes| tables.write_to(bytes))?;
    let scheme: u64 = sctid(item(SCHEME)).parse().unwrap_or_default();
    let identifiers = Identifiers::new(vec![
        (scheme, String::from("cat-1"), Ordinal::new(CAT)),
        (scheme, String::from("dog-1"), Ordinal::new(DOG)),
    ]);
    edition::write_graph_part(dir, "identifiers.bin", |bytes| identifiers.write_to(bytes))?;
    edition::write_graph_part(dir, "refsets.bin", |bytes| memberships.write_to(bytes))?;
    edition::finish(builder, preferred)?;

    edition::write_manifest(
        dir,
        &serde_json::json!({
        "manifest": 2,
        "system": "http://snomed.info/sct",
        "edition": shape.edition,
        "version": shape.version,
        "releaseDate": shape.release_date,
        "store": "store.redb",
        "storeLayout": tables::LAYOUT_VERSION,
        "hierarchy": "hierarchy.bin",
        "text": "text.bin",
        "refsets": "refsets.bin",
        "attributes": "attributes.bin",
        "members": "members.bin",
        "identifiers": "identifiers.bin",
        "concepts": rows.len(),
        "languages": ["en", "nl"],
        }),
    )
}
