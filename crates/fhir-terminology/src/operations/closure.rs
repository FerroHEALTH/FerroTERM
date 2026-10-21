// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ConceptMap/$closure` in the terms every served FHIR version shares.
//!
//! A client keeps a transitive closure table of the concepts it has seen, and
//! the server tells it which subsumption relationships hold between them
//! (<https://hl7.org/fhir/R4B/conceptmap-operation-closure.html>). The table
//! itself belongs to the client's session on the server; this module only
//! answers what relates to what, over the provider seam's hierarchy, so any
//! code system that declares subsumption maintains a closure.

use concept_graph::subsumption::Outcome;

use super::{Invocation, OperationError, locate, resolve};
use crate::conceptmap::model::Relationship;
use crate::registry::Registry;

/// One concept a closure table holds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Member {
    /// The code system URI.
    pub system: String,
    /// The code system version, when the client pinned one.
    pub version: Option<String>,
    /// The code.
    pub code: String,
}

/// One relationship a closure table records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// The concept the relationship is stated from.
    pub source: Member,
    /// The concept it is stated to.
    pub target: Member,
    /// How the source relates to the target.
    pub relationship: Relationship,
}

/// The relationship a subsumption outcome records in a closure table.
///
/// The equivalence is read from target to source, so a target that subsumes
/// its source is `subsumes` and the concept the source is broader than is
/// `specializes` (<https://hl7.org/fhir/R4B/terminology-service.html>,
/// "Maintaining a Closure Table"). A closure entry carries only `equal`,
/// `subsumes`, and `specializes`; two concepts that subsume neither way get no
/// entry, and a concept is never related to itself, which a client assumes.
const fn relationship(outcome: Outcome) -> Option<Relationship> {
    match outcome {
        Outcome::Equivalent => Some(Relationship::Equal),
        Outcome::Subsumes => Some(Relationship::Specializes),
        Outcome::SubsumedBy => Some(Relationship::Subsumes),
        Outcome::NotSubsumed => None,
    }
}

/// The relationships `added` brings to a table that already holds `held`:
/// every pair of one added concept with one held concept, and every pair of
/// two added concepts.
///
/// A pair of two systems relates through neither, because subsumption is
/// stated inside one code system
/// (<https://hl7.org/fhir/R4B/codesystem-operation-subsumes.html>).
///
/// # Errors
///
/// Returns [`OperationError`] when a concept names a system, version, or code
/// the server does not have, or a system that declares no subsumption.
pub fn relate(
    registry: &Registry,
    held: &[Member],
    added: &[Member],
) -> Result<Vec<Edge>, OperationError> {
    let mut edges = Vec::new();
    for (index, source) in added.iter().enumerate() {
        // NOTE: a concept is checked once here so an unknown code is refused even when
        // the table holds nothing to relate it to.
        outcome(registry, source, source)?;
        let others = held
            .iter()
            .chain(added.iter().skip(index.saturating_add(1)));
        for target in others {
            if source == target {
                continue;
            }
            let Some(found) = outcome(registry, source, target)? else {
                continue;
            };
            let Some(relationship) = relationship(found) else {
                continue;
            };
            edges.push(Edge {
                source: source.clone(),
                target: target.clone(),
                relationship,
            });
        }
    }
    Ok(edges)
}

/// The subsumption between two members, or `None` when they are of different
/// systems.
fn outcome(
    registry: &Registry,
    source: &Member,
    target: &Member,
) -> Result<Option<Outcome>, OperationError> {
    if source.system != target.system {
        return Ok(None);
    }
    let resolved = resolve(
        registry,
        &Invocation::Type,
        Some(&source.system),
        source.version.as_deref(),
    )?;
    let provider = &resolved.provider;
    let a = locate(provider, &source.code)?.concept;
    let b = locate(provider, &target.code)?.concept;
    if let Some(found) = provider.subsumes(a, b)? {
        return Ok(Some(found));
    }
    let hierarchy = provider.hierarchy().ok_or_else(|| {
        OperationError::NotSupported(format!(
            "code system `{}` declares no subsumption",
            provider.identity().url
        ))
    })?;
    Ok(Some(hierarchy.subsumes(a, b)))
}
