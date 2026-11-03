// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading one concept map's groups for one code: the group that maps the
//! code's system in the requested direction, the elements and targets that
//! name the code, and the `unmapped` rule when none does
//! (<https://hl7.org/fhir/R5/conceptmap-operation-translate.html>).

use super::{Match, MatchOrigin, Product, Subject, in_source_scope};
use crate::conceptmap::model::{
    ConceptMapModel, DependsOn, Element, Group, Relationship, Target, Unmapped, UnmappedMode,
};
use crate::operations::{CodingRef, OperationError, Sources};
use crate::versioned::Versioned;

/// How deep `unmapped.mode = other-map` may chain (our own guard).
const OTHER_MAP_DEPTH: usize = 8;

/// The matches for `subject` in `map`; the maps an `other-map` rule chains
/// to are recorded in `used`.
pub(super) fn matches_in(
    sources: &Sources<'_>,
    map: &ConceptMapModel,
    subject: &Subject,
    target_system: Option<&str>,
    reverse: bool,
    depth: usize,
    used: &mut Vec<String>,
) -> Result<Vec<Match>, OperationError> {
    let mut out = Vec::new();
    for group in &map.groups {
        let (from, from_version, to, to_version) = group_direction(group, reverse);
        if !group_applies(from, from_version, to, subject, target_system) {
            continue;
        }
        let found = group_matches(map, group, subject, reverse, to, to_version, &mut out);
        if !found
            && !reverse
            && let Some(unmapped) = &group.unmapped
        {
            out.extend(unmapped_matches(
                sources,
                map,
                group,
                unmapped,
                subject,
                target_system,
                depth,
                used,
            )?);
        }
    }
    Ok(out)
}

/// The `(from, from_version, to, to_version)` of `group`, read in the
/// requested direction.
fn group_direction(
    group: &Group,
    reverse: bool,
) -> (Option<&str>, Option<&str>, Option<&str>, Option<&str>) {
    if reverse {
        (
            group.target.as_deref(),
            group.target_version.as_deref(),
            group.source.as_deref(),
            group.source_version.as_deref(),
        )
    } else {
        (
            group.source.as_deref(),
            group.source_version.as_deref(),
            group.target.as_deref(),
            group.target_version.as_deref(),
        )
    }
}

/// Whether a group reading `from` at `from_version` to `to` translates
/// `subject` into `target_system`.
fn group_applies(
    from: Option<&str>,
    from_version: Option<&str>,
    to: Option<&str>,
    subject: &Subject,
    target_system: Option<&str>,
) -> bool {
    if from != Some(subject.system.as_str()) {
        return false;
    }
    if let (Some(wanted), Some(named)) = (subject.version.as_deref(), from_version)
        && wanted != named
    {
        return false;
    }
    if target_system.is_some_and(|t| to != Some(t)) {
        return false;
    }
    true
}

/// Appends the matches `group` states for `subject` to `out`; whether any
/// element of the group matched at all.
fn group_matches(
    map: &ConceptMapModel,
    group: &Group,
    subject: &Subject,
    reverse: bool,
    to: Option<&str>,
    to_version: Option<&str>,
    out: &mut Vec<Match>,
) -> bool {
    let origin = |element: Option<&Element>| MatchOrigin {
        origin_map: map.canonical(),
        source_concept: Some(CodingRef {
            system: Some(subject.system.clone()),
            code: Some(subject.code.clone()),
            ..CodingRef::default()
        }),
        source_comment: element.and_then(|e| e.comment.clone()),
        target_comment: None,
        no_map: element.is_some_and(|e| e.no_map),
    };
    let mut found = false;
    for (element, targets) in element_targets(group, subject, reverse) {
        found = true;
        if targets.is_empty() {
            out.push(Match {
                relationship: Relationship::Unmatched,
                concept: None,
                products: Vec::new(),
                source: Some(map.canonical()),
                origin: origin(Some(element)),
            });
        }
        for (code, display, relationship, product, comment) in targets {
            out.push(Match {
                relationship,
                concept: Some(CodingRef {
                    system: to.map(str::to_owned),
                    version: to_version.map(str::to_owned),
                    code: code.clone(),
                    display: display.clone(),
                }),
                products: product.iter().map(product_of).collect(),
                source: Some(map.canonical()),
                origin: MatchOrigin {
                    target_comment: comment.clone(),
                    ..origin(Some(element))
                },
            });
        }
    }
    found
}

/// The elements of `group` for `subject`, each with its targets read in the
/// requested direction: `(code, display, relationship, product, comment)`.
#[expect(
    clippy::type_complexity,
    reason = "one tuple per matched target, consumed in place"
)]
fn element_targets<'a>(
    group: &'a Group,
    subject: &Subject,
    reverse: bool,
) -> Vec<(
    &'a Element,
    Vec<(
        &'a Option<String>,
        &'a Option<String>,
        Relationship,
        &'a [DependsOn],
        &'a Option<String>,
    )>,
)> {
    if reverse {
        let mut out = Vec::new();
        for element in &group.elements {
            let hits: Vec<_> = element
                .targets
                .iter()
                .filter(|t| t.code.as_deref() == Some(subject.code.as_str()))
                .map(|t: &Target| {
                    (
                        &element.code,
                        &element.display,
                        t.relationship.inverse(),
                        t.product.as_slice(),
                        &t.comment,
                    )
                })
                .collect();
            if !hits.is_empty() {
                out.push((element, hits));
            }
        }
        return out;
    }
    group
        .elements
        .iter()
        .filter(|e| e.code.as_deref() == Some(subject.code.as_str()))
        .map(|element| {
            (
                element,
                element
                    .targets
                    .iter()
                    .map(|t| {
                        (
                            &t.code,
                            &t.display,
                            t.relationship,
                            t.product.as_slice(),
                            &t.comment,
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

/// The matches an `unmapped` rule yields for a code no element names.
#[expect(
    clippy::too_many_arguments,
    reason = "the rule is described by exactly these facts"
)]
fn unmapped_matches(
    sources: &Sources<'_>,
    map: &ConceptMapModel,
    group: &Group,
    unmapped: &Unmapped,
    subject: &Subject,
    target_system: Option<&str>,
    depth: usize,
    used: &mut Vec<String>,
) -> Result<Vec<Match>, OperationError> {
    let origin = MatchOrigin {
        origin_map: map.canonical(),
        source_concept: Some(CodingRef {
            system: Some(subject.system.clone()),
            code: Some(subject.code.clone()),
            ..CodingRef::default()
        }),
        source_comment: None,
        target_comment: None,
        no_map: false,
    };
    let fixed = |code: Option<&str>, display: Option<&str>, relationship: Relationship| Match {
        relationship,
        concept: Some(CodingRef {
            system: group.target.clone(),
            version: group.target_version.clone(),
            code: code.map(str::to_owned),
            display: display.map(str::to_owned),
        }),
        products: Vec::new(),
        source: Some(map.canonical()),
        origin: origin.clone(),
    };
    Ok(match unmapped.mode {
        // NOTE: R4B's `provided` says "use the code as provided", so the target is
        // the same code, `equal` unless the map says otherwise
        // (<https://hl7.org/fhir/R4B/valueset-conceptmap-unmapped-mode.html>).
        UnmappedMode::Provided => vec![fixed(
            Some(&subject.code),
            None,
            unmapped.relationship.unwrap_or(Relationship::Equal),
        )],
        UnmappedMode::Fixed => vec![fixed(
            unmapped.code.as_deref(),
            unmapped.display.as_deref(),
            unmapped.relationship.unwrap_or(Relationship::RelatedTo),
        )],
        UnmappedMode::OtherMap => {
            let Some(other) = &unmapped.other_map else {
                return Ok(Vec::new());
            };
            if depth >= OTHER_MAP_DEPTH {
                return Err(OperationError::Invalid(format!(
                    "`unmapped.mode = other-map` chains deeper than {OTHER_MAP_DEPTH} maps at `{other}`"
                )));
            }
            let other_map = sources
                .concept_maps
                .resolve(other, None)
                .ok_or_else(|| OperationError::UnknownConceptMap(other.clone()))?;
            if !in_source_scope(sources, &other_map, subject)? {
                return Ok(Vec::new());
            }
            let chained = matches_in(
                sources,
                &other_map,
                subject,
                target_system,
                false,
                depth + 1,
                used,
            )?;
            // NOTE: a chained match is answered as this map's, and the map it came
            // from is reported in `used-conceptmap` (the ecosystem's `translate-5`).
            let canonical = other_map.canonical();
            if !used.contains(&canonical) {
                used.push(canonical);
            }
            chained
                .into_iter()
                .map(|mut m| {
                    m.origin.origin_map = map.canonical();
                    m.source = Some(map.canonical());
                    m
                })
                .collect()
        }
    })
}

fn product_of(d: &DependsOn) -> Product {
    Product {
        element: d.attribute.clone(),
        concept: CodingRef {
            system: d.system.clone(),
            version: None,
            code: Some(d.value.clone()),
            display: d.display.clone(),
        },
    }
}
