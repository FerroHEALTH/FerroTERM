// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic SNOMED CT edition of any size, as an artifact directory.
//!
//! [`crate::snomed`] writes sixteen shaped concepts, which is the right fixture
//! for asserting behaviour and the wrong one for measuring latency: a store
//! that fits in a cache line answers every read at the same speed. This module
//! writes the same layout over as many concepts as a benchmark asks for, so a
//! point read is measured against an index of a realistic size. The content is
//! invented, in the same invented namespace, and carries no SNOMED CT content.

use std::path::Path;

use crate::snomed::Item;

use concept_graph::attributes::Attributes;
use concept_graph::closure::Closure;
use concept_graph::csr::Csr;
use concept_graph::identifiers::Identifiers;
use concept_graph::members::Memberships;
use concept_graph::ordinal::Ordinal;
use concept_graph::persist::Hierarchy;
use concept_graph::refsets::{self, RefsetMembers};
use concept_store::builder::{PreferredRule, StoreBuilder};
use concept_store::record::{Concept, Designation, PropertyValue};
use concept_store::store::Vocabulary;
use concept_store::tables;
use designation_index::index::{IndexBuilder, Input};
use rf2::constants;

use crate::snomed::{
    EDITION, FixtureError, GB_REFSET, ICD10_MAP_SCTID, NL_REFSET, VERSION, item, sctid,
};

/// The children each concept has, so the tree is broad and shallow the way a
/// SNOMED hierarchy is.
const BRANCHING: u32 = 8;
/// Every tenth concept joins the reference set.
const MEMBER_EVERY: usize = 10;
/// The item numbers start here, so a code of this edition is never a code of
/// the small fixture.
const FIRST_ITEM: u32 = 10_000;

/// The ordinal of the root, which every other concept descends from.
pub const ROOT: u32 = 0;

/// The code of the concept at `ordinal`.
#[must_use]
pub fn code(ordinal: u32) -> String {
    sctid(item(ordinal.saturating_add(FIRST_ITEM)))
}

/// The invented map-target code the ICD-10 extended map gives `ordinal`.
///
/// The shape is a letter and digits, the way an ICD-10 code reads; the values
/// are generated and stand for no real classification code.
#[must_use]
pub fn map_target(ordinal: u32) -> String {
    format!("X{ordinal:05}")
}

/// The parent of `ordinal`, or `None` for the root.
#[expect(
    clippy::integer_division,
    reason = "the tree is defined by the truncating division, not approximated by it"
)]
const fn parent(ordinal: u32) -> Option<u32> {
    match ordinal {
        0 => None,
        _ => Some((ordinal - 1) / BRANCHING),
    }
}

/// The reference set every tenth concept belongs to: the last ordinal, so the
/// concept count a caller asks for is the count the edition holds.
const fn refset(concepts: u32) -> u32 {
    concepts.saturating_sub(1)
}

/// Writes an edition of `concepts` concepts under `dir`, in the layout
/// [`crate::snomed::write`] uses.
///
/// The tree is broad and shallow: concept `n` is a child of `(n - 1) / 8`, so
/// the closure of the root is every concept and the closure of a leaf is
/// itself. Every concept carries an English fully specified name, an English
/// synonym, and a Dutch synonym; every tenth concept is a member of one
/// reference set.
///
/// # Errors
///
/// Returns [`FixtureError`] when a writer fails; the content is generated, so
/// a failure means the writers or the directory are at fault.
#[expect(
    clippy::too_many_lines,
    reason = "one generated edition, read top to bottom"
)]
pub fn write(dir: &Path, concepts: u32) -> Result<(), FixtureError> {
    // An edition holds at least the root concept.
    let concepts = concepts.max(1);
    let module = sctid(Item::raw(99));
    let (fsn, syn) = (0, 1);
    let (gb, nl) = (0, 1);
    let preferred = 0;

    let mut builder =
        StoreBuilder::create(&dir.join("store.redb"), "http://snomed.info/sct", VERSION)?;
    for (i, key) in ["parent", "definitionStatus", "module"].iter().enumerate() {
        builder.vocabulary(Vocabulary::PropertyKeys, u32::try_from(i).unwrap_or(0), key)?;
    }
    for (i, use_id) in [
        constants::FULLY_SPECIFIED_NAME,
        constants::SYNONYM,
        constants::DEFINITION,
    ]
    .iter()
    .enumerate()
    {
        builder.vocabulary(
            Vocabulary::DesignationUses,
            u32::try_from(i).unwrap_or(0),
            &use_id.to_string(),
        )?;
    }
    builder.vocabulary(
        Vocabulary::Acceptabilities,
        0,
        &constants::PREFERRED.to_string(),
    )?;
    builder.vocabulary(
        Vocabulary::Acceptabilities,
        1,
        &constants::ACCEPTABLE.to_string(),
    )?;
    builder.vocabulary(Vocabulary::LanguageRefsets, gb, GB_REFSET)?;
    builder.vocabulary(Vocabulary::LanguageRefsets, nl, NL_REFSET)?;

    let mut text = IndexBuilder::new();
    let mut edges = Vec::new();
    for ordinal in 0..concepts {
        let at = Ordinal::new(ordinal);
        builder.concept(
            at,
            &Concept {
                code: code(ordinal),
                active: true,
                effective_time: Some(String::from("20260101")),
                module: None,
            },
        )?;
        if let Some(above) = parent(ordinal) {
            edges.push((ordinal, above));
            builder.properties(at, 0, &[PropertyValue::Concept(Ordinal::new(above))])?;
        }
        builder.properties(
            at,
            1,
            &[PropertyValue::Code(constants::PRIMITIVE.to_string())],
        )?;
        builder.properties(at, 2, &[PropertyValue::Code(module.clone())])?;
        let terms = [
            (format!("Synthetic concept {ordinal} (finding)"), "en", fsn),
            (format!("Synthetic concept {ordinal}"), "en", syn),
            (format!("Synthetisch concept {ordinal}"), "nl", syn),
        ];
        for (index, (term, language, use_ordinal)) in terms.iter().enumerate() {
            let index = u32::try_from(index).unwrap_or(0);
            builder.designation(
                at,
                index,
                &Designation {
                    id: None,
                    term: term.clone(),
                    language: (*language).to_owned(),
                    use_ordinal: *use_ordinal,
                    active: true,
                },
            )?;
            // A release states the fully specified name and one synonym as
            // preferred in the language reference set, so the display is the
            // preferred SYNONYM of the requested language (RF2 §Language
            // Reference Set).
            let in_refset = if *language == "nl" { nl } else { gb };
            let acceptability = preferred;
            builder.acceptability(at, index, in_refset, acceptability)?;
            text.add(&Input {
                concept: at,
                index,
                term,
                language,
                use_ordinal: *use_ordinal,
                active: true,
                refsets: &[in_refset],
            })
            .map_err(|e| FixtureError::Text(e.to_string()))?;
        }
    }

    let graph_error = |e: &dyn std::fmt::Display| FixtureError::Graph(e.to_string());
    let csr = Csr::build(
        concepts,
        edges
            .iter()
            .map(|(child, above)| (Ordinal::new(*child), Ordinal::new(*above))),
    )
    .map_err(|e| graph_error(&e))?;
    let closure = Closure::compute(&csr).map_err(|e| graph_error(&e))?;
    let hierarchy = Hierarchy { is_a: csr, closure };
    let mut graph_bytes = Vec::new();
    hierarchy
        .write_to(&mut graph_bytes)
        .map_err(|e| graph_error(&e))?;
    std::fs::write(dir.join("hierarchy.bin"), &graph_bytes)?;

    let attributes =
        Attributes::build(concepts, Vec::new(), Vec::new()).map_err(|e| graph_error(&e))?;
    let mut attribute_bytes = Vec::new();
    attributes
        .write_to(&mut attribute_bytes)
        .map_err(|e| graph_error(&e))?;
    std::fs::write(dir.join("attributes.bin"), &attribute_bytes)?;

    let index = text
        .build()
        .map_err(|e| FixtureError::Text(e.to_string()))?;
    let mut text_bytes = Vec::new();
    designation_index::persist::write_to(&index, &mut text_bytes)
        .map_err(|e| FixtureError::Text(e.to_string()))?;
    std::fs::write(dir.join("text.bin"), &text_bytes)?;

    let set: u64 = code(refset(concepts)).parse().unwrap_or_default();
    let map: u64 = ICD10_MAP_SCTID.parse().unwrap_or_default();
    let module_id: u64 = module.parse().unwrap_or_default();
    let members: Vec<u32> = (0..concepts).step_by(MEMBER_EVERY).collect();
    let mut memberships = Memberships::new();
    for member in &members {
        memberships.insert(set, Ordinal::new(*member));
    }
    // Every concept maps, so the map reference set grows with the edition and a
    // translation over it is measured against a map of a realistic size.
    for ordinal in 0..concepts {
        memberships.insert(map, Ordinal::new(ordinal));
    }
    let mut member_bytes = Vec::new();
    memberships
        .write_to(&mut member_bytes)
        .map_err(|e| graph_error(&e))?;
    std::fs::write(dir.join("refsets.bin"), &member_bytes)?;

    let mut tables = RefsetMembers::new();
    tables
        .insert(
            set,
            &[],
            members
                .iter()
                .map(|member| refsets::MemberRow {
                    concept: Ordinal::new(*member),
                    effective_time: 20_260_101,
                    module: module_id,
                    values: Vec::new(),
                })
                .collect(),
        )
        .map_err(|e| graph_error(&e))?;
    // The ICD-10 extended map, in the columns RF2 gives it, under the published
    // reference set id, so the implicit concept map of the edition names a target
    // code system (RF2 §Extended Map Reference Set).
    tables
        .insert(
            map,
            &[
                (String::from("mapGroup"), refsets::FieldKind::Integer),
                (String::from("mapPriority"), refsets::FieldKind::Integer),
                (String::from("mapRule"), refsets::FieldKind::String),
                (String::from("mapAdvice"), refsets::FieldKind::String),
                (String::from("mapTarget"), refsets::FieldKind::String),
                (String::from("correlationId"), refsets::FieldKind::Component),
            ],
            (0..concepts)
                .map(|ordinal| refsets::MemberRow {
                    concept: Ordinal::new(ordinal),
                    effective_time: 20_260_101,
                    module: module_id,
                    values: vec![
                        refsets::FieldValue::Integer(1),
                        refsets::FieldValue::Integer(1),
                        refsets::FieldValue::String(String::from("TRUE")),
                        refsets::FieldValue::String(format!("ALWAYS {}", map_target(ordinal))),
                        refsets::FieldValue::String(map_target(ordinal)),
                        refsets::FieldValue::Component(447_561_005),
                    ],
                })
                .collect(),
        )
        .map_err(|e| graph_error(&e))?;
    let mut table_bytes = Vec::new();
    tables
        .write_to(&mut table_bytes)
        .map_err(|e| graph_error(&e))?;
    std::fs::write(dir.join("members.bin"), &table_bytes)?;

    let identifiers = Identifiers::new(Vec::new());
    let mut identifier_bytes = Vec::new();
    identifiers
        .write_to(&mut identifier_bytes)
        .map_err(|e| graph_error(&e))?;
    std::fs::write(dir.join("identifiers.bin"), &identifier_bytes)?;

    builder.finish(&PreferredRule {
        preferred,
        // The synonym, the second designation use written above: a SNOMED
        // display is the preferred synonym of a language reference set.
        display_use: Some(1),
    })?;

    let manifest = serde_json::json!({
        "manifest": 2,
        "system": "http://snomed.info/sct",
        "edition": EDITION,
        "version": VERSION,
        "releaseDate": "20260101",
        "store": "store.redb",
        "storeLayout": tables::LAYOUT_VERSION,
        "hierarchy": "hierarchy.bin",
        "text": "text.bin",
        "refsets": "refsets.bin",
        "attributes": "attributes.bin",
        "members": "members.bin",
        "identifiers": "identifiers.bin",
        "concepts": concepts,
        "languages": ["en", "nl"],
    });
    let rendered = serde_json::to_string_pretty(&manifest)
        .map_err(|e| FixtureError::Io(std::io::Error::other(e)))?;
    std::fs::write(dir.join(concept_store::MANIFEST_FILE), rendered)?;
    Ok(())
}
