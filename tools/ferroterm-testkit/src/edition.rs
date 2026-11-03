// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The parts every synthetic SNOMED CT edition writes, the shaped one of
//! [`crate::snomed`] and the sized one of [`crate::scaled`]: the store's
//! vocabularies, the files beside the store, and the manifest, in the layout
//! `ferroterm-build` writes.

use std::fmt::Display;
use std::path::Path;

use concept_graph::closure::Closure;
use concept_graph::csr::Csr;
use concept_graph::ordinal::Ordinal;
use concept_graph::persist::Hierarchy;
use concept_graph::refsets::{FieldKind, FieldValue};
use concept_store::builder::{PreferredRule, StoreBuilder};
use concept_store::store::Vocabulary;
use designation_index::index::IndexBuilder;
use rf2::constants;

use crate::snomed::{FixtureError, GB_REFSET, NL_REFSET};

/// The property keys every concept carries, by ordinal.
const PROPERTY_KEYS: [&str; 3] = ["parent", "definitionStatus", "module"];

/// A small index as a `u32` ordinal (an edition has fewer than `u32::MAX`
/// of anything it numbers).
pub(crate) fn ord(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

/// Writes the property keys every concept carries.
pub(crate) fn write_property_keys(builder: &mut StoreBuilder) -> Result<(), FixtureError> {
    for (i, key) in PROPERTY_KEYS.iter().enumerate() {
        builder.vocabulary(Vocabulary::PropertyKeys, ord(i), key)?;
    }
    Ok(())
}

/// Writes the designation uses, the acceptabilities, and the GB and NL
/// language reference sets under the ordinals `gb` and `nl`.
pub(crate) fn write_designation_vocabularies(
    builder: &mut StoreBuilder,
    gb: u32,
    nl: u32,
) -> Result<(), FixtureError> {
    for (i, use_id) in [
        constants::FULLY_SPECIFIED_NAME,
        constants::SYNONYM,
        constants::DEFINITION,
    ]
    .iter()
    .enumerate()
    {
        builder.vocabulary(Vocabulary::DesignationUses, ord(i), &use_id.to_string())?;
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
    Ok(())
}

/// Writes one graph part beside the store as `name`, from what `write` puts
/// into a buffer.
pub(crate) fn write_graph_part<E: Display>(
    dir: &Path,
    name: &str,
    write: impl FnOnce(&mut Vec<u8>) -> Result<(), E>,
) -> Result<(), FixtureError> {
    let mut bytes = Vec::new();
    write(&mut bytes).map_err(|e| FixtureError::Graph(e.to_string()))?;
    std::fs::write(dir.join(name), &bytes)?;
    Ok(())
}

/// Writes the hierarchy of `concepts` concepts over the child-to-parent
/// `edges` as `hierarchy.bin`.
pub(crate) fn write_hierarchy(
    dir: &Path,
    concepts: u32,
    edges: impl IntoIterator<Item = (u32, u32)>,
) -> Result<(), FixtureError> {
    let csr = Csr::build(
        concepts,
        edges
            .into_iter()
            .map(|(child, parent)| (Ordinal::new(child), Ordinal::new(parent))),
    )
    .map_err(|e| FixtureError::Graph(e.to_string()))?;
    let closure = Closure::compute(&csr).map_err(|e| FixtureError::Graph(e.to_string()))?;
    let hierarchy = Hierarchy { is_a: csr, closure };
    write_graph_part(dir, "hierarchy.bin", |bytes| hierarchy.write_to(bytes))
}

/// Builds the text index and writes it as `text.bin`.
pub(crate) fn write_text(dir: &Path, text: IndexBuilder) -> Result<(), FixtureError> {
    let index = text
        .build()
        .map_err(|e| FixtureError::Text(e.to_string()))?;
    let mut bytes = Vec::new();
    designation_index::persist::write_to(&index, &mut bytes)
        .map_err(|e| FixtureError::Text(e.to_string()))?;
    std::fs::write(dir.join("text.bin"), &bytes)?;
    Ok(())
}

/// The columns of an extended map reference set (RF2 §Extended Map Reference
/// Set).
pub(crate) fn extended_map_columns() -> [(String, FieldKind); 6] {
    [
        (String::from("mapGroup"), FieldKind::Integer),
        (String::from("mapPriority"), FieldKind::Integer),
        (String::from("mapRule"), FieldKind::String),
        (String::from("mapAdvice"), FieldKind::String),
        (String::from("mapTarget"), FieldKind::String),
        (String::from("correlationId"), FieldKind::Component),
    ]
}

/// The values of one extended map row that always maps to `target`, in the
/// order of [`extended_map_columns`].
pub(crate) fn extended_map_values(target: &str) -> Vec<FieldValue> {
    vec![
        FieldValue::Integer(1),
        FieldValue::Integer(1),
        FieldValue::String(String::from("TRUE")),
        FieldValue::String(format!("ALWAYS {target}")),
        FieldValue::String(target.to_owned()),
        FieldValue::Component(447_561_005),
    ]
}

/// Finishes the store with `preferred` as the preferred acceptability.
pub(crate) fn finish(builder: StoreBuilder, preferred: u32) -> Result<(), FixtureError> {
    builder.finish(&PreferredRule {
        preferred,
        // The synonym, the second designation use written above: a SNOMED
        // display is the preferred synonym of a language reference set.
        display_use: Some(1),
    })?;
    Ok(())
}

/// Writes `manifest` beside the store.
pub(crate) fn write_manifest(dir: &Path, manifest: &serde_json::Value) -> Result<(), FixtureError> {
    let rendered = serde_json::to_string_pretty(manifest)
        .map_err(|e| FixtureError::Io(std::io::Error::other(e)))?;
    std::fs::write(dir.join(concept_store::MANIFEST_FILE), rendered)?;
    Ok(())
}
