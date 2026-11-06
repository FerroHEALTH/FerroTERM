// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The artifacts' writers: the store's vocabularies, concepts and
//! designations, the hierarchy, the designation index and the files the ECL
//! evaluator reads.

use std::collections::BTreeMap;
use std::path::Path;

use super::{
    ACCEPTABILITIES, ATTRIBUTES_FILE, Acceptabilities, DESIGNATION_USES, Error, IDENTIFIERS_FILE,
    MEMBERS_FILE, PROPERTY_KEYS, Placed, Relationships, io_error, ordinal_of,
};
use concept_graph::attributes::Attributes;
use concept_graph::closure::Closure;
use concept_graph::csr::Csr;
use concept_graph::identifiers::Identifiers;
use concept_graph::ordinal::Ordinal;
use concept_graph::persist::Hierarchy;
use concept_graph::refsets::RefsetMembers;
use concept_store::builder::StoreBuilder;
use concept_store::record;
use concept_store::store::Vocabulary;
use designation_index::index::{IndexBuilder, Input};
use rf2::component::Concept;
use rf2::id::{ConceptId, RefsetId};

pub(super) fn write_vocabularies(
    builder: &mut StoreBuilder,
    refsets: &[RefsetId],
    attribute_types: &[ConceptId],
) -> Result<(), Error> {
    for (i, key) in PROPERTY_KEYS.iter().enumerate() {
        builder.vocabulary(
            Vocabulary::PropertyKeys,
            ordinal_of(i, "property keys")?,
            key,
        )?;
    }
    for (i, attribute) in attribute_types.iter().enumerate() {
        builder.vocabulary(
            Vocabulary::PropertyKeys,
            ordinal_of(PROPERTY_KEYS.len().saturating_add(i), "property keys")?,
            &attribute.to_string(),
        )?;
    }
    for (i, use_id) in DESIGNATION_USES.iter().enumerate() {
        builder.vocabulary(
            Vocabulary::DesignationUses,
            ordinal_of(i, "designation uses")?,
            &use_id.to_string(),
        )?;
    }
    for (i, acceptability) in ACCEPTABILITIES.iter().enumerate() {
        builder.vocabulary(
            Vocabulary::Acceptabilities,
            ordinal_of(i, "acceptabilities")?,
            &acceptability.to_string(),
        )?;
    }
    for (i, refset) in refsets.iter().enumerate() {
        builder.vocabulary(
            Vocabulary::LanguageRefsets,
            ordinal_of(i, "language reference sets")?,
            &refset.to_string(),
        )?;
    }
    Ok(())
}

/// Writes every concept and its properties; returns the is-a edge count.
pub(super) fn write_concepts(
    builder: &mut StoreBuilder,
    concepts: &[Concept],
    ordinals: &BTreeMap<ConceptId, Ordinal>,
    relationships: &Relationships,
) -> Result<u64, Error> {
    let mut parents: BTreeMap<Ordinal, Vec<record::PropertyValue>> = BTreeMap::new();
    for (child, parent) in &relationships.is_a {
        parents
            .entry(*child)
            .or_default()
            .push(record::PropertyValue::Concept(*parent));
    }
    for (i, concept) in concepts.iter().enumerate() {
        let ordinal = Ordinal::new(ordinal_of(i, "concepts")?);
        let module = ordinals.get(&concept.base.module_id.concept()).copied();
        builder.concept(
            ordinal,
            &record::Concept {
                code: concept.id.to_string(),
                active: concept.base.active,
                effective_time: Some(concept.base.effective_time.compact()),
                module,
            },
        )?;
        if let Some(values) = parents.get(&ordinal) {
            builder.properties(ordinal, 0, values)?;
        }
        builder.properties(
            ordinal,
            1,
            &[record::PropertyValue::Code(
                concept.definition_status_id.to_string(),
            )],
        )?;
        builder.properties(
            ordinal,
            2,
            &[record::PropertyValue::Code(
                concept.base.module_id.to_string(),
            )],
        )?;
        for ((_, attribute), values) in relationships
            .attributes
            .range((ordinal, ConceptId::published(0))..)
            .take_while(|((source, _), _)| *source == ordinal)
        {
            let Ok(position) = relationships.attribute_types.binary_search(attribute) else {
                return Err(Error::TooMany("attribute types"));
            };
            let key = ordinal_of(
                PROPERTY_KEYS.len().saturating_add(position),
                "property keys",
            )?;
            builder.properties(ordinal, key, values)?;
        }
    }
    Ok(u64::try_from(relationships.is_a.len()).unwrap_or(u64::MAX))
}

pub(super) fn write_designations(
    builder: &mut StoreBuilder,
    designations: &[Placed],
) -> Result<u64, Error> {
    for placed in designations {
        builder.designation(placed.ordinal, placed.index, &placed.record)?;
    }
    Ok(u64::try_from(designations.len()).unwrap_or(u64::MAX))
}

/// Writes the files the ECL evaluator reads beside the store.
pub(super) fn write_ecl_files(
    out: &Path,
    attributes: &Attributes,
    members: &RefsetMembers,
    identifiers: &Identifiers,
) -> Result<(), Error> {
    let mut bytes = Vec::new();
    attributes.write_to(&mut bytes)?;
    let path = out.join(ATTRIBUTES_FILE);
    std::fs::write(&path, &bytes).map_err(io_error(&path))?;
    bytes.clear();
    members.write_to(&mut bytes)?;
    let path = out.join(MEMBERS_FILE);
    std::fs::write(&path, &bytes).map_err(io_error(&path))?;
    bytes.clear();
    identifiers.write_to(&mut bytes)?;
    let path = out.join(IDENTIFIERS_FILE);
    std::fs::write(&path, &bytes).map_err(io_error(&path))?;
    Ok(())
}

pub(super) fn build_hierarchy(
    concepts: &[Concept],
    is_a: &[(Ordinal, Ordinal)],
) -> Result<Hierarchy, Error> {
    let nodes = ordinal_of(concepts.len(), "concepts")?;
    let csr = Csr::build(nodes, is_a.iter().copied())?;
    let closure = Closure::compute(&csr)?;
    Ok(Hierarchy { is_a: csr, closure })
}

pub(super) fn build_text(
    designations: &[Placed],
    acceptabilities: &Acceptabilities,
) -> Result<(Vec<u8>, u64), Error> {
    let mut index = IndexBuilder::default();
    for placed in designations {
        let refsets: Vec<u32> = acceptabilities
            .get(&(placed.ordinal, placed.index))
            .map(|m| m.iter().map(|(refset, _)| *refset).collect())
            .unwrap_or_default();
        index.add(&Input {
            concept: placed.ordinal,
            index: placed.index,
            term: &placed.record.term,
            language: &placed.record.language,
            use_ordinal: placed.record.use_ordinal,
            active: placed.record.active,
            refsets: &refsets,
        })?;
    }
    let index = index.build()?;
    let mut bytes = Vec::new();
    designation_index::persist::write_to(&index, &mut bytes)?;
    Ok((bytes, u64::try_from(index.words()).unwrap_or(u64::MAX)))
}
