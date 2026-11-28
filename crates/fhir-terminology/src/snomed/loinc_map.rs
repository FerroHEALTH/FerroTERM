// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The implicit concept map of the LOINC Extension:
//! `[edition]?fhir_cm=11010000107`, between SNOMED CT concepts and the LOINC
//! codes the RF2 identifier file gives them, in both directions.
//!
//! No FHIR or SNOMED CT publication defines this map. The tx-ecosystem IG's
//! `tx-source` readme (LOINC Extension content) names it, "following the
//! pattern of the edition maps", pending review by the SNOMED committee; its
//! URL form, its two groups and its `equivalent` relationship are our own
//! design.

use concept_graph::ordinal::Ordinal;

use crate::provider::concept_map::{ConceptMapModel, Element, Group, Relationship, Target};
use crate::provider::{CodeSystemProvider, Concept, MapSelection, error::ProviderError};
use crate::snomed::properties::{LOINC_SCHEME, LOINC_SYSTEM};
use crate::snomed::{SYSTEM, SnomedProvider};
use crate::stored::storage;

/// `11010000107 |LOINC Extension module|`, the map's id.
pub(super) const LOINC_MODULE: u64 = 11_010_000_107;

/// The canonical of the map in the edition `version`.
pub(super) fn url(version: &str) -> String {
    format!("{version}?{}={LOINC_MODULE}", super::FHIR_CM)
}

/// Whether the edition carries any LOINC code, which is what makes the map
/// exist.
pub(super) fn held(provider: &SnomedProvider) -> bool {
    provider
        .system()
        .identifiers
        .schemes()
        .contains(&LOINC_SCHEME)
}

fn target(code: String, display: Option<String>) -> Target {
    Target {
        code: Some(code),
        display,
        relationship: Relationship::Equivalent,
        comment: None,
        depends_on: Vec::new(),
        product: Vec::new(),
    }
}

/// The concept map `url` names, with the elements `selection` asks for.
///
/// # Errors
///
/// Returns [`ProviderError::UnknownImplicitConceptMap`] when the edition
/// carries no LOINC code, and [`ProviderError`] when the substrate fails.
pub(super) fn concept_map(
    provider: &SnomedProvider,
    url: &str,
    selection: MapSelection<'_>,
) -> Result<ConceptMapModel, ProviderError> {
    if !held(provider) {
        return Err(ProviderError::UnknownImplicitConceptMap {
            url: url.to_owned(),
        });
    }
    let identifiers = &provider.system().identifiers;
    let concept_of = |code: &str| provider.stored().store.ordinal(code).map_err(storage);
    let concepts: Vec<Ordinal> = match selection {
        MapSelection::Whole => identifiers.concepts(LOINC_SCHEME).collect(),
        MapSelection::Source(code) | MapSelection::Target(code) => {
            let mut concepts: Vec<Ordinal> = concept_of(code)?.into_iter().collect();
            if let Some(concept) = identifiers.lookup(LOINC_SCHEME, code) {
                concepts.push(concept);
            }
            concepts
        }
    };
    let mut to_loinc = Vec::new();
    let mut to_snomed = Vec::new();
    for concept in concepts {
        let Some(sctid) = provider.stored().store.code(concept).map_err(storage)? else {
            continue;
        };
        let sctid = sctid.to_owned();
        let display = provider.display(Concept::new(concept.index()), None)?;
        let loinc: Vec<String> = identifiers
            .of_concept(concept)
            .filter(|(scheme, _)| *scheme == LOINC_SCHEME)
            .map(|(_, code)| code.to_owned())
            .collect();
        let wanted = |code: &str, side: Side| match selection {
            MapSelection::Whole => true,
            MapSelection::Source(asked) => asked == code && side == Side::Source,
            MapSelection::Target(asked) => asked == code && side == Side::Target,
        };
        let forward: Vec<Target> = loinc
            .iter()
            .filter(|code| wanted(&sctid, Side::Source) || wanted(code, Side::Target))
            .map(|code| target(code.clone(), None))
            .collect();
        if !forward.is_empty() {
            to_loinc.push(Element {
                code: Some(sctid.clone()),
                display: display.clone(),
                no_map: false,
                comment: None,
                targets: forward,
            });
        }
        for code in &loinc {
            if wanted(code, Side::Source) || wanted(&sctid, Side::Target) {
                to_snomed.push(Element {
                    code: Some(code.clone()),
                    display: None,
                    no_map: false,
                    comment: None,
                    targets: vec![target(sctid.clone(), display.clone())],
                });
            }
        }
    }
    to_snomed.sort_by(|a, b| a.code.cmp(&b.code));
    let version = provider.identity().version.clone();
    Ok(ConceptMapModel {
        id: None,
        url: url.to_owned(),
        version: Some(version.clone()),
        name: Some(String::from("SNOMED CT LOINC Extension Concept Map")),
        title: None,
        status: String::from("active"),
        experimental: None,
        date: None,
        publisher: None,
        description: None,
        source_scope: None,
        target_scope: None,
        groups: vec![
            Group {
                source: Some(SYSTEM.to_owned()),
                source_version: Some(version.clone()),
                target: Some(LOINC_SYSTEM.to_owned()),
                target_version: None,
                elements: to_loinc,
                unmapped: None,
            },
            Group {
                source: Some(LOINC_SYSTEM.to_owned()),
                source_version: None,
                target: Some(SYSTEM.to_owned()),
                target_version: Some(version),
                elements: to_snomed,
                unmapped: None,
            },
        ],
    })
}

/// Which end of a mapping a code sits at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Source,
    Target,
}
