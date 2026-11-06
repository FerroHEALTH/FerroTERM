// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ConceptMap/$translate` in the terms every served FHIR version shares.
//!
//! The operation pages: <https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>
//! and <https://hl7.org/fhir/R5/conceptmap-operation-translate.html>.
//!
//! The map is inline, named by `url`, or chosen: every stored map whose
//! scopes fit `source` and `target` and whose groups map the code's system
//! (to `targetsystem` when given). One of `code` with `system`, `coding`, or
//! `codeableConcept` is the input; `reverse` reads the groups the other way.
//! A map applies only to a code its source scope holds. An element
//! without a target, or an `unmapped` rule, still yields a match, so a client
//! sees why nothing translated. `result` is true when a match has
//! a relationship other than `unmatched` or `disjoint`.

mod groups;

use std::sync::Arc;

use super::sources::Sources;
use super::{CodeableConceptRef, CodingRef, OperationError};
use crate::compose::ValueSetResolver;
use crate::provider::MapSelection;
use crate::provider::concept_map::{ConceptMapModel, ModelError, Relationship};
use crate::valueset::store::Resolver;
use crate::versioned::Versioned;

/// What the ecosystem reports beside each R4B `match`: the map the match
/// came from, the source concept, its comment, and whether the element is
/// explicitly unmapped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MatchOrigin {
    /// The canonical of the map (`url|version`).
    pub origin_map: String,
    /// The source concept the match is for.
    pub source_concept: Option<CodingRef>,
    /// The element's comment.
    pub source_comment: Option<String>,
    /// The target's comment.
    pub target_comment: Option<String>,
    /// `noMap`: the element is explicitly not mapped.
    pub no_map: bool,
}

/// The outcome of a translation: the R4B response and, per match, its origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Translation {
    /// Whether some match answers the request.
    pub result: bool,
    /// Why nothing translated, when nothing did.
    pub message: Option<String>,
    /// The matches, in the order the maps and groups were read.
    pub matches: Vec<Match>,
    /// The canonicals of the maps an `other-map` rule chained to
    /// (`used-conceptmap`), beyond the ones the matches name.
    pub used_concept_maps: Vec<String>,
}

/// One match of the translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// The relationship between the source and the target.
    pub relationship: Relationship,
    /// The target concept, absent for an element without a target.
    pub concept: Option<CodingRef>,
    /// The other elements the match depends on (`product`).
    pub products: Vec<Product>,
    /// The canonical of the map the match came from (`source`).
    pub source: Option<String>,
    /// Where the match came from, beyond what every version declares.
    pub origin: MatchOrigin,
}

/// One `product` of a match: an attribute and the concept it depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    /// The attribute (`element`).
    pub element: String,
    /// The concept.
    pub concept: CodingRef,
}

/// The input of `$translate`: the union of the parameters the served versions
/// declare.
#[derive(Debug, Default)]
pub struct TranslateInput {
    /// The concept map URL (`url`).
    pub url: Option<String>,
    /// The concept map version (`conceptMapVersion`).
    pub concept_map_version: Option<String>,
    /// The inline `conceptMap`, converted by the wire layer of its version.
    pub inline_concept_map: Option<Result<ConceptMapModel, ModelError>>,
    /// The code.
    pub code: Option<String>,
    /// The code system URI (`system`).
    pub system: Option<String>,
    /// The code system version.
    pub version: Option<String>,
    /// The coding, instead of `code`.
    pub coding: Option<CodingRef>,
    /// The `codeableConcept`, instead of `code`.
    pub codeable_concept: Option<CodeableConceptRef>,
    /// The source value set (`source`, R4B) or scope (`sourceScope`, R5).
    pub source: Option<String>,
    /// The target value set (`target`, R4B) or scope (`targetScope`, R5).
    pub target: Option<String>,
    /// The target code system (`targetsystem`, R4B; `targetSystem`, R5).
    pub target_system: Option<String>,
    /// Whether the groups are read the other way.
    pub reverse: Option<bool>,
    /// Whether `dependency` was given; not supported.
    pub dependency: bool,
    /// Whether the subject is a target code (the R5 `target*` inputs): the map
    /// is read in reverse and each match is reported source to target.
    pub target_input: bool,
}

/// The code under translation.
#[derive(Debug, Clone)]
struct Subject {
    system: String,
    version: Option<String>,
    code: String,
}

/// Runs `$translate`.
///
/// # Errors
///
/// Returns [`OperationError`] when the translation cannot be performed: no
/// or several code inputs, a code without a system, both `url` and
/// `conceptMap`, an unknown or invalid map, or `dependency`.
pub fn translate(
    sources: &Sources<'_>,
    input: &TranslateInput,
) -> Result<Translation, OperationError> {
    if input.dependency {
        return Err(OperationError::NotSupported(String::from(
            "`dependency` is not supported",
        )));
    }
    let candidates = candidate_maps(sources, input)?;
    let reverse = input.reverse.unwrap_or(false);
    let target_system = input.target_system.as_deref();
    let subjects = subjects(input)?;
    let mut used = Vec::new();
    let (mut matches, consulted) = translated(
        sources,
        &candidates,
        input,
        &subjects,
        target_system,
        reverse,
        &mut used,
    )?;
    if matches.is_empty() && input.url.is_none() {
        matches = successors(sources, &subjects, target_system)?;
    }
    if input.target_input {
        report_source_to_target(&mut matches);
    }
    // NOTE: R4B and R5 `$translate` `result`: true only if a returned match has
    // an equivalence other than unmatched or disjoint (R5: `not-related-to`),
    // <https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>.
    let result = matches.iter().any(translates);
    let message = if !matches.is_empty() {
        None
    } else if let Some(subject) = subjects.first()
        && !consulted.out_of_scope.is_empty()
    {
        Some(out_of_scope_message(subject, &consulted.out_of_scope))
    } else if consulted.mapped {
        Some(String::from("No translations found"))
    } else {
        Some(no_map_message(subjects.first(), target_system, reverse))
    };
    Ok(Translation {
        result,
        message,
        matches,
        used_concept_maps: used,
    })
}

/// Whether a match translates the concept: a target an element names, with
/// a relationship that relates the two.
fn translates(found: &Match) -> bool {
    !found.origin.no_map
        && !matches!(
            found.relationship,
            Relationship::Unmatched | Relationship::Disjoint
        )
}

/// Which maps a translation consulted.
#[derive(Debug, Default)]
struct Consulted {
    /// Whether any map applied to a subject at all.
    mapped: bool,
    /// The canonicals of the maps whose source scope left a subject out.
    out_of_scope: Vec<String>,
}

/// The matches every candidate map holds for every subject, each target
/// reported once, and which maps were consulted.
fn translated(
    sources: &Sources<'_>,
    candidates: &Candidates,
    input: &TranslateInput,
    subjects: &[Subject],
    target_system: Option<&str>,
    reverse: bool,
    used: &mut Vec<String>,
) -> Result<(Vec<Match>, Consulted), OperationError> {
    let mut matches: Vec<Match> = Vec::new();
    let mut consulted = Consulted::default();
    for subject in subjects {
        let mut maps = maps_for(sources, candidates, input, subject, reverse)?;
        // The source scope limits the source codes, so a reverse read, whose
        // subject is a target code, is not held to it.
        if !reverse {
            let mut applicable = Vec::with_capacity(maps.len());
            for map in maps {
                if in_source_scope(sources, &map, subject)? {
                    applicable.push(map);
                } else {
                    consulted.out_of_scope.push(map.canonical());
                }
            }
            maps = applicable;
        }
        consulted.mapped |= !maps.is_empty();
        for map in &maps {
            for found in groups::matches_in(sources, map, subject, target_system, reverse, 0, used)?
            {
                // NOTE: the same target reached through two maps is one match, the
                // ecosystem's shape (<https://hl7.org/fhir/uv/tx-ecosystem/>).
                if !already_matched(&matches, &found) {
                    matches.push(found);
                }
            }
        }
    }
    Ok((matches, consulted))
}

/// Whether `subject` is a member of the source scope of `map`, or `map`
/// states none: R5 `ConceptMap.sourceScope[x]`
/// (<https://hl7.org/fhir/R5/conceptmap-definitions.html#ConceptMap.sourceScope_x_>)
/// and R4 and R4B `ConceptMap.source[x]`
/// (<https://hl7.org/fhir/R4B/conceptmap-definitions.html#ConceptMap.source_x_>).
///
/// # Errors
///
/// Returns [`OperationError`] when the scope cannot be resolved or evaluated,
/// naming the value set.
fn in_source_scope(
    sources: &Sources<'_>,
    map: &ConceptMapModel,
    subject: &Subject,
) -> Result<bool, OperationError> {
    let Some(scope) = map.source_scope.as_deref() else {
        return Ok(true);
    };
    // NOTE: R5 `sourceScope[x]` "Limits the scope of the map to source codes ...
    // that are members of this value set", the `unmapped` rule included (R5
    // conceptmap-definitions.html#ConceptMap.sourceScope_x_).
    let found = Resolver::new(sources.registry, sources.value_sets).contains(
        scope,
        &subject.system,
        &subject.code,
    )?;
    Ok(found.is_some())
}

/// The message when every map that could translate `subject` leaves it out
/// of its source scope (spec-silent wording, our own).
fn out_of_scope_message(subject: &Subject, maps: &[String]) -> String {
    format!(
        "The code '{}' of '{}' is not in the source scope of the ConceptMaps {}",
        subject.code,
        subject.system,
        maps.iter()
            .map(|m| format!("'{m}'"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Whether `found` names a target `matches` already reports.
fn already_matched(matches: &[Match], found: &Match) -> bool {
    found.concept.is_some()
        && matches
            .iter()
            .any(|m| m.concept == found.concept && m.relationship == found.relationship)
}

/// Reports each match source to target, the map's own direction.
fn report_source_to_target(matches: &mut [Match]) {
    // NOTE: a `target*` request reports the source concept as `sourceConcept`
    // and the target as `concept`
    // (<https://hl7.org/fhir/R5/conceptmap-operation-translate.html>).
    for found in matches {
        std::mem::swap(&mut found.concept, &mut found.origin.source_concept);
        found.relationship = found.relationship.inverse();
    }
}

/// The matches an inactive concept's own historical associations give, when no
/// concept map translated it and the request named none.
///
/// No FHIR specification asks a server to do this: `$translate` is defined
/// over the map a request names, and a system's own associations are reached
/// only by naming their reference set. Answering a retired concept with the
/// concept that stands in for it is our own design.
fn successors(
    sources: &Sources<'_>,
    subjects: &[Subject],
    target_system: Option<&str>,
) -> Result<Vec<Match>, OperationError> {
    let mut matches = Vec::new();
    for subject in subjects {
        let Ok(resolved) = sources
            .registry
            .resolve(&subject.system, subject.version.as_deref())
        else {
            continue;
        };
        let Some(located) = resolved.provider.locate(&subject.code)? else {
            continue;
        };
        if resolved.provider.status(located.concept)?.active {
            continue;
        }
        for successor in resolved.provider.successors(located.concept)? {
            if target_system.is_some_and(|wanted| wanted != subject.system) {
                continue;
            }
            matches.push(Match {
                relationship: successor.relationship,
                concept: Some(CodingRef {
                    system: Some(subject.system.clone()),
                    version: None,
                    code: Some(successor.code),
                    display: successor.display,
                }),
                products: Vec::new(),
                source: Some(successor.map.clone()),
                origin: MatchOrigin {
                    origin_map: successor.map,
                    source_concept: Some(CodingRef {
                        system: Some(subject.system.clone()),
                        version: None,
                        code: Some(subject.code.clone()),
                        display: None,
                    }),
                    ..MatchOrigin::default()
                },
            });
        }
    }
    Ok(matches)
}

/// The ecosystem's message when no concept map fits the request.
fn no_map_message(subject: Option<&Subject>, target_system: Option<&str>, reverse: bool) -> String {
    let subject_system = subject.map(|s| s.system.as_str());
    let (from, to) = if reverse {
        (target_system, subject_system)
    } else {
        (subject_system, target_system)
    };
    match (from, to) {
        (Some(from), Some(to)) => {
            format!("No ConceptMap is available to translate from '{from}' to '{to}'")
        }
        (Some(from), None) => format!("No ConceptMap is available to translate from '{from}'"),
        (None, Some(to)) => format!("No ConceptMap is available to translate to '{to}'"),
        (None, None) => String::from("No ConceptMap is available"),
    }
}

/// The codes to translate: the one `code`/`system`, the `coding`, or every
/// `codeableConcept.coding` with a system.
fn subjects(input: &TranslateInput) -> Result<Vec<Subject>, OperationError> {
    let inputs = usize::from(input.code.is_some())
        + usize::from(input.coding.is_some())
        + usize::from(input.codeable_concept.is_some());
    if inputs != 1 {
        return Err(OperationError::Invalid(String::from(
            "provide one and only one of `code`, `coding`, or `codeableConcept`",
        )));
    }
    if let Some(code) = input.code.as_deref() {
        let system = input.system.as_deref().ok_or_else(|| {
            OperationError::Required(String::from("`system` is required with `code`"))
        })?;
        return Ok(vec![Subject {
            system: system.to_owned(),
            version: input.version.clone(),
            code: code.to_owned(),
        }]);
    }
    let codings: Vec<&CodingRef> = match (&input.coding, &input.codeable_concept) {
        (Some(coding), _) => vec![coding],
        (None, Some(concept)) => concept.coding.iter().collect(),
        (None, None) => Vec::new(),
    };
    let mut subjects = Vec::new();
    for coding in codings {
        let (Some(system), Some(code)) = (coding.system.as_deref(), coding.code.as_deref()) else {
            return Err(OperationError::Required(String::from(
                "a coding to translate needs `system` and `code`",
            )));
        };
        subjects.push(Subject {
            system: system.to_owned(),
            version: coding.version.clone(),
            code: code.to_owned(),
        });
    }
    if subjects.is_empty() {
        return Err(OperationError::Required(String::from(
            "`codeableConcept` carries no coding with `system` and `code`",
        )));
    }
    Ok(subjects)
}

/// Where the maps of a translation come from.
enum Candidates {
    /// Maps that stand whole whatever code is translated: the inline one, the
    /// stored one `url` names, or the stored maps the scopes fit.
    Stored(Vec<Arc<ConceptMapModel>>),
    /// The implicit map `url` names, built per code out of the system that
    /// defines it.
    Implicit(String),
}

/// The maps to consult: the inline one, the one `url` names, or every stored
/// map whose scopes fit `source` and `target`.
fn candidate_maps(
    sources: &Sources<'_>,
    input: &TranslateInput,
) -> Result<Candidates, OperationError> {
    let url = input.url.as_deref();
    match (&input.inline_concept_map, url) {
        (Some(_), Some(_)) => Err(OperationError::Invalid(String::from(
            "provide either `url` or an inline `conceptMap`, not both",
        ))),
        (Some(inline), None) => Ok(Candidates::Stored(vec![Arc::new(
            inline
                .clone()
                .map_err(|e| OperationError::Invalid(e.to_string()))?,
        )])),
        (None, Some(url)) => {
            let version = input.concept_map_version.as_deref();
            match sources.concept_maps.resolve(url, version) {
                Some(map) => Ok(Candidates::Stored(vec![map])),
                // NOTE: a system may define concept maps by URI, the way SNOMED CT's
                // `?fhir_cm=[sctid]` names a map reference set
                // (<https://hl7.org/fhir/R4B/snomedct.html>).
                None => Ok(Candidates::Implicit(url.to_owned())),
            }
        }
        (None, None) => {
            let source = input.source.as_deref();
            let target = input.target.as_deref();
            Ok(Candidates::Stored(
                sources
                    .concept_maps
                    .iter()
                    .filter(|map| {
                        source.is_none_or(|s| map.source_scope.as_deref() == Some(s))
                            && target.is_none_or(|t| map.target_scope.as_deref() == Some(t))
                    })
                    .cloned()
                    .collect(),
            ))
        }
    }
}

/// The maps to read for `subject`.
///
/// An implicit map is built for the code under translation, so it carries the
/// mappings that answer this request and no others; a stored map is already
/// whole. Both give the same answer, and the implicit one costs the size of
/// that answer rather than the size of the reference set behind it.
fn maps_for(
    sources: &Sources<'_>,
    candidates: &Candidates,
    input: &TranslateInput,
    subject: &Subject,
    reverse: bool,
) -> Result<Vec<Arc<ConceptMapModel>>, OperationError> {
    let url = match candidates {
        Candidates::Stored(maps) => return Ok(maps.clone()),
        Candidates::Implicit(url) => url,
    };
    let selection = if reverse {
        MapSelection::Target(&subject.code)
    } else {
        MapSelection::Source(&subject.code)
    };
    match sources.registry.implicit_concept_map(url, selection) {
        Some(Ok(map)) => Ok(vec![Arc::new(map)]),
        Some(Err(source)) => Err(source.into()),
        None => Err(OperationError::UnknownConceptMap(
            match input.concept_map_version.as_deref() {
                Some(version) => format!("{url}|{version}"),
                None => url.clone(),
            },
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{Match, MatchOrigin, Relationship, translates};

    fn found(relationship: Relationship, no_map: bool) -> Match {
        Match {
            relationship,
            concept: None,
            products: Vec::new(),
            source: None,
            origin: MatchOrigin {
                no_map,
                ..MatchOrigin::default()
            },
        }
    }

    /// R4B `$translate` `result`: only a match whose equivalence is neither
    /// unmatched nor disjoint translates.
    #[test]
    fn only_a_relating_match_translates() {
        assert!(translates(&found(Relationship::Equivalent, false)));
        assert!(translates(&found(Relationship::RelatedTo, false)));
        assert!(!translates(&found(Relationship::Disjoint, false)));
        assert!(!translates(&found(Relationship::Unmatched, false)));
        assert!(!translates(&found(Relationship::RelatedTo, true)));
    }
}
