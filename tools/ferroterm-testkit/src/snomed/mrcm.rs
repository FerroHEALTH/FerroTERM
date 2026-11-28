// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A concept model for the shaped edition.
//!
//! The MRCM metadata concepts sit under their published SCTIDs, beside one
//! attribute domain and one attribute range reference set with the rules the
//! expression tests check.
//!
//! The rules: covering is grouped, mandatory in the animal domain at most once
//! per group, ranges over `<< fur`, and is recommended (an optional rule) on the
//! cat only; legs is ungrouped, at most once, and ranges over `int(>#0..)`.

use super::{ANIMAL, CAT, COVERING, FUR, LEGS, Row, TOP, code_of, item, sctid};
use concept_graph::ordinal::Ordinal;
use concept_graph::refsets::{self, MemberRow, RefsetMembers};

/// `410662002 |Concept model attribute|`; the covering and legs sit under it.
pub const MODEL_ATTRIBUTE: u32 = 22;
/// `723604009 |MRCM attribute domain reference set|`.
pub const ATTRIBUTE_DOMAIN: u32 = 23;
/// The edition's attribute domain reference set, under [`ATTRIBUTE_DOMAIN`].
pub const DOMAIN_RULES: u32 = 24;
/// `723592007 |MRCM attribute range reference set|`.
pub const ATTRIBUTE_RANGE: u32 = 25;
/// The edition's attribute range reference set, under [`ATTRIBUTE_RANGE`].
pub const RANGE_RULES: u32 = 26;
/// `723595009 |All postcoordinated SNOMED CT content|`.
pub const POSTCOORDINATED: u32 = 27;
/// `723596005 |All SNOMED CT content|`, under [`POSTCOORDINATED`].
pub const ALL_CONTENT: u32 = 28;
/// `723597001 |Mandatory concept model rule|`.
pub const MANDATORY: u32 = 29;
/// `723598006 |Optional concept model rule|`.
pub const OPTIONAL: u32 = 30;

/// The concepts, each a published SCTID or an invented one.
pub(super) fn rows(row: impl Fn(u32, Option<&'static str>, &'static str) -> Row) -> Vec<Row> {
    vec![
        row(
            MODEL_ATTRIBUTE,
            Some("410662002"),
            "Concept model attribute (attribute)",
        ),
        row(
            ATTRIBUTE_DOMAIN,
            Some("723604009"),
            "MRCM attribute domain reference set (foundation metadata concept)",
        ),
        row(
            DOMAIN_RULES,
            None,
            "MRCM attribute domain synthetic reference set (foundation metadata concept)",
        ),
        row(
            ATTRIBUTE_RANGE,
            Some("723592007"),
            "MRCM attribute range reference set (foundation metadata concept)",
        ),
        row(
            RANGE_RULES,
            None,
            "MRCM attribute range synthetic reference set (foundation metadata concept)",
        ),
        row(
            POSTCOORDINATED,
            Some("723595009"),
            "All postcoordinated SNOMED CT content (foundation metadata concept)",
        ),
        row(
            ALL_CONTENT,
            Some("723596005"),
            "All SNOMED CT content (foundation metadata concept)",
        ),
        row(
            MANDATORY,
            Some("723597001"),
            "Mandatory concept model rule (foundation metadata concept)",
        ),
        row(
            OPTIONAL,
            Some("723598006"),
            "Optional concept model rule (foundation metadata concept)",
        ),
    ]
}

/// The is-a edges the concept model adds.
pub(super) fn is_a() -> Vec<(u32, u32)> {
    vec![
        (MODEL_ATTRIBUTE, TOP),
        (COVERING, MODEL_ATTRIBUTE),
        (LEGS, MODEL_ATTRIBUTE),
        (ATTRIBUTE_DOMAIN, TOP),
        (DOMAIN_RULES, ATTRIBUTE_DOMAIN),
        (ATTRIBUTE_RANGE, TOP),
        (RANGE_RULES, ATTRIBUTE_RANGE),
        (POSTCOORDINATED, TOP),
        (ALL_CONTENT, POSTCOORDINATED),
        (MANDATORY, TOP),
        (OPTIONAL, TOP),
    ]
}

/// The two rule tables, in the columns RF2 gives them.
pub(super) fn tables(
    tables: &mut RefsetMembers,
    rows: &[Row],
    member: impl Fn(u32, Vec<refsets::FieldValue>) -> MemberRow,
) -> Result<(), refsets::RefsetsError> {
    let id = |ordinal: u32| -> u64 {
        rows.iter()
            .find(|r| r.ordinal == ordinal)
            .map(code_of)
            .and_then(|code| code.parse().ok())
            .unwrap_or_default()
    };
    let concept = |ordinal: u32| refsets::FieldValue::Concept(Ordinal::new(ordinal));
    let text = |text: &str| refsets::FieldValue::String(text.to_owned());
    let domain = |attribute, domain, grouped, total: &str, in_group: &str, strength| {
        member(
            attribute,
            vec![
                concept(domain),
                refsets::FieldValue::Integer(grouped),
                text(total),
                text(in_group),
                concept(strength),
                concept(ALL_CONTENT),
            ],
        )
    };
    let column = |name: &str, kind| (name.to_owned(), kind);
    tables.insert(
        id(DOMAIN_RULES),
        &[
            column("domainId", refsets::FieldKind::Component),
            column("grouped", refsets::FieldKind::Integer),
            column("attributeCardinality", refsets::FieldKind::String),
            column("attributeInGroupCardinality", refsets::FieldKind::String),
            column("ruleStrengthId", refsets::FieldKind::Component),
            column("contentTypeId", refsets::FieldKind::Component),
        ],
        vec![
            domain(COVERING, ANIMAL, 1, "0..*", "0..1", MANDATORY),
            domain(COVERING, CAT, 1, "0..*", "0..1", OPTIONAL),
            domain(LEGS, ANIMAL, 0, "0..1", "0..0", MANDATORY),
        ],
    )?;
    let fur = format!("<< {}", sctid(item(FUR)));
    tables.insert(
        id(RANGE_RULES),
        &[
            column("rangeConstraint", refsets::FieldKind::String),
            column("attributeRule", refsets::FieldKind::String),
            column("ruleStrengthId", refsets::FieldKind::Component),
            column("contentTypeId", refsets::FieldKind::Component),
        ],
        vec![
            member(
                COVERING,
                vec![
                    text(&fur),
                    text(""),
                    concept(MANDATORY),
                    concept(ALL_CONTENT),
                ],
            ),
            member(
                LEGS,
                vec![
                    text("int(>#0..)"),
                    text(""),
                    concept(MANDATORY),
                    concept(ALL_CONTENT),
                ],
            ),
        ],
    )
}
