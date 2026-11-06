// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading one RF2 release into the build: the concepts, the relationships with their concrete values, the descriptions placed in their language reference sets, the reference set members, and the alternate identifiers.

use super::{
    ACCEPTABILITIES, Acceptabilities, DESIGNATION_USES, Error, PendingTable, Placed, RefsetFile,
    RefsetPass, Relationships, ordinal_of,
};
use concept_graph::attributes::{self};
use concept_graph::identifiers::Identifiers;
use concept_graph::members::Memberships;
use concept_graph::ordinal::Ordinal;
use concept_graph::refsets::{self, MemberRow, RefsetMembers};
use concept_store::record;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use rayon::slice::ParallelSliceMut;
use rf2::component::{
    AlternateIdentifier, Concept, ConcreteRelationship, ConcreteValue, Description, Relationship,
    Rows,
};
use rf2::constants;
use rf2::file::{ContentType, FieldKind, Release, ReleaseFile};
use rf2::id::{ConceptId, DescriptionId, RefsetId};
use rf2::refset::{FieldValue, LanguageMember, Members, kind::RefsetKind};
use rf2::time::EffectiveTime;
use std::collections::BTreeMap;
use std::path::Path;

/// Every concept row of every release, sorted by identifier so ordinals are
/// stable; a concept a layered package restates keeps the edition's row,
/// which the stable sort leaves first.
pub(super) fn read_concepts(releases: &[Release]) -> Result<Vec<Concept>, Error> {
    let paths: Vec<&Path> = releases
        .iter()
        .map(|release| {
            release
                .of_type(&ContentType::Concept)
                .next()
                .map(|file| file.path.as_path())
                .ok_or(Error::MissingFile("concept"))
        })
        .collect::<Result<_, Error>>()?;
    let mut concepts = concat(read_files(&paths, component_rows::<Concept>)?);
    // The sort is stable and the release order is fixed, so a concept a
    // layered package restates keeps the edition's row, which stays first.
    concepts.par_sort_by_key(|concept| concept.id);
    concepts.dedup_by_key(|concept| concept.id);
    Ok(concepts)
}

/// Reads every path in parallel and returns what each one yielded, in the
/// order the paths were given.
pub(super) fn read_files<T: Send>(
    paths: &[&Path],
    read: impl Fn(&Path) -> Result<T, Error> + Sync,
) -> Result<Vec<T>, Error> {
    paths.par_iter().map(|path| read(path)).collect()
}

/// Every row of one component file.
pub(super) fn component_rows<T: rf2::component::Component>(path: &Path) -> Result<Vec<T>, Error> {
    let mut out = Vec::new();
    for row in Rows::<_, T>::open(path)? {
        out.push(row?);
    }
    Ok(out)
}

/// The parts of a per-file read, joined in file order.
pub(super) fn concat<T>(parts: Vec<Vec<T>>) -> Vec<T> {
    let mut out = Vec::with_capacity(parts.iter().map(Vec::len).sum());
    for part in parts {
        out.extend(part);
    }
    out
}

/// The ordinal of `concept`, named by the relationship that references it.
pub(super) fn ordinal_of_concept(
    ordinals: &BTreeMap<ConceptId, Ordinal>,
    relationship: &str,
    concept: ConceptId,
) -> Result<Ordinal, Error> {
    ordinals
        .get(&concept)
        .copied()
        .ok_or_else(|| Error::UnknownConcept {
            relationship: relationship.to_owned(),
            concept,
        })
}

/// Reads one RF2 relationship file: an active inferred is-a row becomes a
/// hierarchy edge, every other one an attribute value on its source.
pub(super) fn read_relationship_file(
    path: &Path,
    ordinals: &BTreeMap<ConceptId, Ordinal>,
    out: &mut Relationships,
) -> Result<(), Error> {
    let mut rows = Rows::<_, Relationship>::open(path)?.inferred();
    for relationship in rows.by_ref() {
        let relationship = relationship?;
        if !relationship.base.active {
            continue;
        }
        let id = relationship.id.to_string();
        let source = ordinal_of_concept(ordinals, &id, relationship.source_id)?;
        let destination = ordinal_of_concept(ordinals, &id, relationship.destination_id)?;
        if relationship.type_id == constants::IS_A {
            out.is_a.push((source, destination));
        } else {
            // NOTE: the FHIR SNOMED CT page says a concept model attribute
            // relationship becomes a property and is silent on
            // characteristicTypeId, so the inferred view is our own design.
            out.attributes
                .entry((source, relationship.type_id))
                .or_default()
                .push(record::PropertyValue::Concept(destination));
            out.edges.push((
                source,
                relationship.relationship_group,
                relationship.type_id,
                attributes::Value::Concept(destination),
            ));
        }
    }
    out.skipped = out.skipped.saturating_add(rows.skipped());
    Ok(())
}

/// Reads one RF2 concrete-value relationship file: every active inferred row
/// is an attribute whose value is a number or a string.
pub(super) fn read_concrete_relationship_file(
    path: &Path,
    ordinals: &BTreeMap<ConceptId, Ordinal>,
    out: &mut Relationships,
) -> Result<(), Error> {
    let mut rows = Rows::<_, ConcreteRelationship>::open(path)?.inferred();
    for relationship in rows.by_ref() {
        let relationship = relationship?;
        if !relationship.base.active {
            continue;
        }
        let id = relationship.id.to_string();
        let source = ordinal_of_concept(ordinals, &id, relationship.source_id)?;
        // NOTE: RF2 spells a numeric concrete value with a `#` prefix and a
        // string one in quotes; the reader strips both and keeps the kind.
        let (value, edge) = match relationship.value {
            ConcreteValue::Number(number) => (
                record::PropertyValue::Decimal(number.clone()),
                attributes::Value::Number(number),
            ),
            ConcreteValue::String(text) => (
                record::PropertyValue::String(text.clone()),
                attributes::Value::String(text),
            ),
        };
        out.attributes
            .entry((source, relationship.type_id))
            .or_default()
            .push(value);
        out.edges.push((
            source,
            relationship.relationship_group,
            relationship.type_id,
            edge,
        ));
    }
    out.skipped = out.skipped.saturating_add(rows.skipped());
    Ok(())
}

pub(super) fn read_relationships(
    releases: &[Release],
    ordinals: &BTreeMap<ConceptId, Ordinal>,
) -> Result<Relationships, Error> {
    let mut files: Vec<(&Path, bool)> = Vec::new();
    for release in releases {
        files.extend(
            release
                .of_type(&ContentType::Relationship)
                .map(|file| (file.path.as_path(), false)),
        );
        files.extend(
            release
                .of_type(&ContentType::RelationshipConcreteValues)
                .map(|file| (file.path.as_path(), true)),
        );
    }
    let parts: Vec<Relationships> = files
        .par_iter()
        .map(|(path, concrete)| {
            let mut part = Relationships::default();
            if *concrete {
                read_concrete_relationship_file(path, ordinals, &mut part)?;
            } else {
                read_relationship_file(path, ordinals, &mut part)?;
            }
            Ok(part)
        })
        .collect::<Result<_, Error>>()?;
    let mut out = Relationships::default();
    for part in parts {
        out.is_a.extend(part.is_a);
        for (key, values) in part.attributes {
            out.attributes.entry(key).or_default().extend(values);
        }
        out.edges.extend(part.edges);
        out.skipped = out.skipped.saturating_add(part.skipped);
    }
    out.is_a.par_sort_unstable();
    out.is_a.dedup();
    let mut attribute_types: Vec<ConceptId> = out.attributes.keys().map(|(_, t)| *t).collect();
    attribute_types.sort_unstable();
    attribute_types.dedup();
    out.attribute_types = attribute_types;
    Ok(out)
}

/// Every description and text definition of every release, numbered per
/// concept in identifier order.
pub(super) fn read_designations(
    releases: &[Release],
    ordinals: &BTreeMap<ConceptId, Ordinal>,
) -> Result<Vec<Placed>, Error> {
    let described = [ContentType::Description, ContentType::TextDefinition];
    let mut paths: Vec<&Path> = Vec::new();
    for release in releases {
        for content in &described {
            paths.extend(release.of_type(content).map(|file| file.path.as_path()));
        }
    }
    let mut rows: Vec<Description> = concat(read_files(&paths, |path| {
        component_rows::<Description>(path)
    })?);
    rows.par_sort_by_key(|row| row.id);
    let mut per_concept: BTreeMap<Ordinal, u32> = BTreeMap::new();
    let mut placed = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(ordinal) = ordinals.get(&row.concept_id).copied() else {
            // NOTE: a description of a concept outside the release is not an
            // error in RF2 (the concept lives in a dependency the release omits);
            // it has no home in this store and is skipped.
            continue;
        };
        let index = per_concept.entry(ordinal).or_insert(0);
        let use_ordinal = DESIGNATION_USES
            .iter()
            .position(|use_id| *use_id == row.type_id)
            .map(|p| ordinal_of(p, "designation uses"))
            .transpose()?
            .unwrap_or(1);
        placed.push(Placed {
            id: row.id,
            ordinal,
            index: *index,
            record: record::Designation {
                id: Some(row.id.to_string()),
                term: row.term,
                language: row.language_code,
                use_ordinal,
                active: row.base.active,
            },
        });
        *index = index.checked_add(1).ok_or(Error::TooMany("designations"))?;
    }
    Ok(placed)
}

/// The first reference set file of `release` of `wanted`, `None` when it has
/// none.
///
/// # Errors
///
/// Returns [`Error`] when a reference set file cannot be read.
pub(super) fn refset_of(
    release: &Release,
    wanted: RefsetKind,
) -> Result<Option<&ReleaseFile>, Error> {
    for file in release.refsets() {
        if rf2::refset::kind::read(&file.path)? == wanted {
            return Ok(Some(file));
        }
    }
    Ok(None)
}

/// The kind of one reference set file, from the columns its header names.
///
/// # Errors
///
/// Returns [`Error`] when the file cannot be read.
pub(super) fn refset_kind(file: &ReleaseFile) -> Result<RefsetKind, Error> {
    Ok(rf2::refset::kind::read(&file.path)?)
}

/// The language reference sets (by ordinal) and, per designation, its
/// (refset ordinal, acceptability ordinal) memberships.
pub(super) fn place_acceptabilities(
    mut members: Vec<(RefsetId, DescriptionId, u32)>,
    designations: &[Placed],
) -> Result<(Vec<RefsetId>, Acceptabilities), Error> {
    let by_id: BTreeMap<DescriptionId, (Ordinal, u32)> = designations
        .iter()
        .map(|d| (d.id, (d.ordinal, d.index)))
        .collect();
    members.par_sort_unstable();
    let mut refsets: Vec<RefsetId> = members.iter().map(|m| m.0).collect();
    refsets.dedup();
    let mut acceptabilities: Acceptabilities = BTreeMap::new();
    for (refset, description, acceptability) in members {
        let Some(place) = by_id.get(&description).copied() else {
            continue;
        };
        let Ok(position) = refsets.binary_search(&refset) else {
            return Err(Error::TooMany("language reference sets"));
        };
        let refset_ordinal = ordinal_of(position, "language reference sets")?;
        acceptabilities
            .entry(place)
            .or_default()
            .push((refset_ordinal, acceptability));
    }
    Ok((refsets, acceptabilities))
}

/// Reads every reference set file of every release, once.
///
/// The concept memberships, the member tables of the content reference sets,
/// and the language reference set members come from the same rows, so one
/// pass yields all three. The files are read together and joined in path
/// order, so a row's place in its table is the file's, never a worker's.
pub(super) fn read_refsets(
    releases: &[Release],
    ordinals: &BTreeMap<ConceptId, Ordinal>,
) -> Result<RefsetPass, Error> {
    let files: Vec<&ReleaseFile> = releases.iter().flat_map(Release::refsets).collect();
    let parts: Vec<RefsetFile> = files
        .par_iter()
        .map(|file| read_refset_file(file, ordinals))
        .collect::<Result<_, Error>>()?;
    let mut memberships = Memberships::default();
    let mut tables: BTreeMap<u64, PendingTable> = BTreeMap::new();
    let mut language = Vec::new();
    for part in parts {
        for (refset, ordinal) in part.memberships {
            memberships.insert(refset, ordinal);
        }
        for (refset, (fields, rows)) in part.tables {
            tables
                .entry(refset)
                .or_insert_with(|| (fields, Vec::new()))
                .1
                .extend(rows);
        }
        language.extend(part.language);
    }
    let mut member_tables = RefsetMembers::default();
    for (refset, (fields, rows)) in tables {
        member_tables.insert(refset, &fields, rows)?;
    }
    Ok(RefsetPass {
        memberships,
        member_tables,
        language,
    })
}

/// Reads one reference set file: its language members when it is a language
/// reference set, its concept memberships otherwise, and its member rows with
/// their fields when it is a content reference set.
pub(super) fn read_refset_file(
    file: &ReleaseFile,
    ordinals: &BTreeMap<ConceptId, Ordinal>,
) -> Result<RefsetFile, Error> {
    let kind = refset_kind(file)?;
    let mut out = RefsetFile::default();
    let ContentType::Refset(kinds) = &file.name.content_type else {
        return Ok(out);
    };
    for member in Members::open(&file.path, kinds)? {
        let member = member?;
        if kind == RefsetKind::Language {
            read_language_member(member, &mut out.language)?;
            continue;
        }
        if !member.active {
            continue;
        }
        // NOTE: every reference set of the edition is served, the metadata ones
        // included (<https://hl7.org/fhir/R4B/snomedct.html>, #272).
        // NOTE: a member referencing a description or a relationship is not a
        // concept membership; that is "absent" here, not a defect of the file.
        let Ok(concept) = ConceptId::try_from(member.referenced_component_id) else {
            continue;
        };
        let Some(&ordinal) = ordinals.get(&concept) else {
            continue;
        };
        let refset = member.refset_id.concept().value();
        out.memberships.push((refset, ordinal));
        if kind != RefsetKind::Content {
            continue;
        }
        let entry = out
            .tables
            .entry(refset)
            .or_insert_with(|| (field_columns(&member.fields, kinds), Vec::new()));
        entry.1.push(MemberRow {
            concept: ordinal,
            effective_time: compact_time(member.effective_time),
            module: member.module_id.concept().value(),
            values: field_values(member.fields, ordinals),
        });
    }
    Ok(out)
}

/// Reads one language reference set member: the reference set, the
/// description it places, and the acceptability ordinal.
pub(super) fn read_language_member(
    member: rf2::refset::Member,
    out: &mut Vec<(RefsetId, DescriptionId, u32)>,
) -> Result<(), Error> {
    let member = LanguageMember::try_from(member)?;
    if !member.member.active {
        return Ok(());
    }
    // The error names the member and component; the id error adds nothing.
    let Ok(description) = DescriptionId::try_from(member.member.referenced_component_id) else {
        return Err(Error::NotADescription {
            member: member.member.id.to_string(),
            component: member.member.referenced_component_id.to_string(),
        });
    };
    let acceptability = ACCEPTABILITIES
        .iter()
        .position(|a| *a == member.acceptability_id)
        .map(|p| ordinal_of(p, "acceptabilities"))
        .transpose()?
        .unwrap_or(1);
    out.push((member.member.refset_id, description, acceptability));
    Ok(())
}

/// The `YYYYMMDD` of an effective time as a number.
pub(super) fn compact_time(time: EffectiveTime) -> u32 {
    time.compact().parse().unwrap_or_default()
}

/// The stored columns of a reference set: the member's field names under the
/// kinds the file header declares.
pub(super) fn field_columns(
    fields: &[(String, FieldValue)],
    kinds: &[FieldKind],
) -> Vec<(String, refsets::FieldKind)> {
    fields
        .iter()
        .zip(kinds)
        .map(|((name, _), kind)| {
            (
                name.clone(),
                match kind {
                    FieldKind::Component => refsets::FieldKind::Component,
                    FieldKind::Integer => refsets::FieldKind::Integer,
                    FieldKind::String => refsets::FieldKind::String,
                },
            )
        })
        .collect()
}

/// The stored values of one member's fields; a component field that names a
/// loaded concept is stored as its ordinal.
pub(super) fn field_values(
    fields: Vec<(String, FieldValue)>,
    ordinals: &BTreeMap<ConceptId, Ordinal>,
) -> Vec<refsets::FieldValue> {
    fields
        .into_iter()
        .map(|(_, value)| match value {
            FieldValue::Component(id) => ConceptId::try_from(id)
                .ok()
                .and_then(|c| ordinals.get(&c))
                .map_or(refsets::FieldValue::Component(id.value()), |o| {
                    refsets::FieldValue::Concept(*o)
                }),
            FieldValue::Integer(value) => refsets::FieldValue::Integer(value),
            FieldValue::String(text) => refsets::FieldValue::String(text),
        })
        .collect()
}

/// The active alternate identifiers of every release's concepts.
pub(super) fn read_identifiers(
    releases: &[Release],
    ordinals: &BTreeMap<ConceptId, Ordinal>,
) -> Result<Identifiers, Error> {
    let mut entries = Vec::new();
    for release in releases {
        for file in release.of_type(&ContentType::Identifier) {
            for identifier in Rows::<_, AlternateIdentifier>::open(&file.path)? {
                let identifier = identifier?;
                if !identifier.base.active {
                    continue;
                }
                let Ok(concept) = ConceptId::try_from(identifier.referenced_component_id) else {
                    continue;
                };
                if let Some(&ordinal) = ordinals.get(&concept) {
                    entries.push((
                        identifier.identifier_scheme_id.value(),
                        identifier.alternate_identifier,
                        ordinal,
                    ));
                }
            }
        }
    }
    Ok(Identifiers::new(entries))
}
