// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The form model behind the concept map editor.
//!
//! Everything here is plain values and plain functions: what a local
//! `ConceptMap` is while it is being authored, the FHIR JSON a save sends, and
//! the `Parameters` body a preview posts. No component reaches into it, which
//! is what lets the rules it encodes be pinned by ordinary unit tests.
//!
//! One rule shapes the whole module. A concept map is spelled differently in
//! every release this server mounts, so nothing here writes an element name of
//! its own: the served version hands over a [`Dialect`], and the dialect says
//! which element a target's relationship goes in, which element scopes the map,
//! and whether the version defines `element.noMap`. A form that wrote R4B's
//! `equivalence` on an R5 root would be refused, and one that wrote R5's
//! `relationship` on an R4B root would be refused the other way.

use serde_json::Value;

use crate::authoring::code_system::Key;
use crate::authoring::code_system::Keys;
use crate::fhir::release::{Release, Rewritable, rewritten};
use crate::fhir::version::FhirVersion;

mod typed;
use crate::locale::{Msg, tr, tr_with};

/// The resource type this form authors.
/// How one served FHIR version spells a concept map.
///
/// Every name here is read from that version's own definition page, so a save
/// writes what the root it is sent to defines rather than what another release
/// defines (<https://hl7.org/fhir/R4B/conceptmap.html>,
/// <https://hl7.org/fhir/R5/conceptmap.html>).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Dialect {
    /// The value set whose codes that element admits.
    ///
    /// The control offers what the served root expands this to, so a root that
    /// admits another code offers it without a new build.
    pub(crate) relationship_value_set: &'static str,
    /// Whether the version defines `element.noMap`.
    pub(crate) no_map: bool,
    /// The relationship codes whose target the version requires a comment on.
    ///
    /// R4 and R4B make it an error to leave `narrower` or `inexact`
    /// uncommented (`cmd-1`,
    /// <https://hl7.org/fhir/R4B/conceptmap.html>), and R5 makes the same
    /// error of `source-is-broader-than-target` and `not-related-to`
    /// (<https://hl7.org/fhir/R5/conceptmap.html>). The R6 ballot demoted it
    /// to a warning, so this is empty there: the editor refuses what the
    /// version refuses and nothing more
    /// (<https://hl7.org/fhir/6.0.0-ballot5/conceptmap.html>).
    pub(crate) comment_required: &'static [&'static str],
    /// Whether a draft map is excused from that requirement.
    ///
    /// R5 added the `%resource.status = 'draft'` arm to `cmd-1`; R4 and R4B
    /// have no such arm.
    pub(crate) draft_excuses_comment: bool,
}

/// The `ConceptMap.status` code that excuses `cmd-1` on R5.
const DRAFT: &str = "draft";

/// The value set `ConceptMap.group.element.target.equivalence` is bound to.
const EQUIVALENCE_VALUE_SET: &str = "http://hl7.org/fhir/ValueSet/concept-map-equivalence";

/// The value set `ConceptMap.group.element.target.relationship` is bound to.
const RELATIONSHIP_VALUE_SET: &str = "http://hl7.org/fhir/ValueSet/concept-map-relationship";

/// How `version` spells a concept map.
pub(crate) fn dialect(version: FhirVersion) -> Dialect {
    match version {
        // R4 and R4B scope the map with `source[x]`/`target[x]`, state a
        // target's relation in `equivalence`, and define no `noMap`
        // (<https://hl7.org/fhir/R4B/conceptmap.html>).
        FhirVersion::R4 | FhirVersion::R4B => Dialect {
            relationship_value_set: EQUIVALENCE_VALUE_SET,
            no_map: false,
            comment_required: &["narrower", "inexact"],
            draft_excuses_comment: false,
        },
        // R5 renamed the scopes to `sourceScope[x]`/`targetScope[x]`, replaced
        // `equivalence` with `relationship`, made a group's system a canonical,
        // and added `element.noMap` (<https://hl7.org/fhir/R5/conceptmap.html>).
        // The R6 ballot carries the same four
        // (<https://hl7.org/fhir/6.0.0-ballot5/conceptmap.html>).
        FhirVersion::R5 => Dialect {
            relationship_value_set: RELATIONSHIP_VALUE_SET,
            no_map: true,
            comment_required: &["source-is-broader-than-target", "not-related-to"],
            draft_excuses_comment: true,
        },
        FhirVersion::R6 => Dialect {
            relationship_value_set: RELATIONSHIP_VALUE_SET,
            no_map: true,
            comment_required: &[],
            draft_excuses_comment: true,
        },
    }
}

/// One target a source code maps to.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MapTarget {
    /// The row's key.
    pub(crate) key: Key,
    /// `target.code`.
    pub(crate) code: String,
    /// `target.display`, as the server stated it when the code was picked.
    pub(crate) display: String,
    /// The relationship code, written under whichever element the version
    /// defines.
    pub(crate) relationship: String,
    /// `target.comment`.
    pub(crate) comment: String,
}

/// One source code, with what it maps to.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MapElement {
    /// The row's key.
    pub(crate) key: Key,
    /// `element.code`.
    pub(crate) code: String,
    /// `element.display`.
    pub(crate) display: String,
    /// `element.noMap`, on a version that defines it.
    pub(crate) no_map: bool,
    /// The targets this code maps to.
    pub(crate) targets: Vec<MapTarget>,
}

/// One group: a pair of systems, and the codes mapped between them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MapGroup {
    /// The row's key.
    pub(crate) key: Key,
    /// `group.source`, the system the codes on the left belong to.
    pub(crate) source: String,
    /// `group.sourceVersion`, or the version inside the source canonical.
    pub(crate) source_version: String,
    /// `group.target`, the system the codes on the right belong to.
    pub(crate) target: String,
    /// `group.targetVersion`, or the version inside the target canonical.
    pub(crate) target_version: String,
    /// The codes this group maps.
    pub(crate) elements: Vec<MapElement>,
}

/// A local concept map as the editor holds it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MapDraft {
    /// The logical id the server assigned, empty for one never saved.
    pub(crate) id: String,
    /// `meta.versionId`, the version an update states in `If-Match`.
    pub(crate) version_id: String,
    /// `ConceptMap.url`.
    pub(crate) url: String,
    /// `ConceptMap.version`.
    pub(crate) version: String,
    /// `ConceptMap.title`, the name written for a person.
    pub(crate) title: String,
    /// `ConceptMap.name`, the computer-friendly name.
    pub(crate) name: String,
    /// `ConceptMap.status`.
    pub(crate) status: String,
    /// The value set the map's source codes are drawn from.
    pub(crate) source_scope: String,
    /// The value set the map's target codes are drawn from.
    pub(crate) target_scope: String,
    /// The groups it maps between.
    pub(crate) groups: Vec<MapGroup>,
    /// The keys minted for this draft's rows.
    pub(crate) keys: Keys,
    /// The resource this draft was read from, as the server sent it.
    ///
    /// An update replaces the whole resource
    /// (<https://hl7.org/fhir/R4B/http.html#update>), so a save is this
    /// document with the elements the form owns written over it. Everything
    /// else, from `name` and `publisher` to a group's `unmapped`, travels back
    /// untouched rather than being deleted by a form that never drew it.
    base: Value,
}

/// The status a new draft opens in.
///
/// A code of the publication status value set, which every FHIR version binds
/// `ConceptMap.status` to as `required`
/// (<https://hl7.org/fhir/R4B/conceptmap-definitions.html#ConceptMap.status>).
/// The screen replaces the whole list with the codes the served root expands,
/// so this is only where an unsaved draft starts.
const NEW_STATUS: &str = "draft";

/// Why a draft cannot be saved yet.
///
/// The screen branches on the variant and renders its own sentence, so nothing
/// here is a message a caller has to match a substring of.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Problem {
    /// `ConceptMap.url` is what every later request names the map by.
    NoCanonical,
    /// `ConceptMap.status` is mandatory.
    NoStatus,
    /// A group states no source or no target system.
    GrouplessSystems,
    /// A group carries no code, which every version makes 1..* mandatory.
    GroupWithoutCodes,
    /// A target carries no relationship, which every version makes mandatory.
    ///
    /// R4 and R4B make `target.equivalence` 1..1 and R5 and R6 make
    /// `target.relationship` 1..1, so a target without one is refused whichever
    /// root it is sent to.
    RelationshipMissing {
        /// The source code whose target is unstated.
        element: String,
        /// The target code that carries no relationship.
        code: String,
    },
    /// A target carries a relationship the version requires a comment on.
    ///
    /// `cmd-1` is an error on R4, R4B and R5, and the R6 ballot demoted it to
    /// a warning, so this is never raised there.
    CommentRequired {
        /// The source code whose target is uncommented.
        element: String,
        /// The target code that carries no comment.
        code: String,
        /// The relationship the version requires a comment on.
        relationship: String,
    },
    /// A code is marked as mapping to nothing and carries targets as well.
    ///
    /// `element.noMap` says the source code "has no target", so a code that
    /// carries one is saying both things at once
    /// (<https://hl7.org/fhir/R5/conceptmap.html>).
    MappedAndUnmapped {
        /// The source code that says both.
        element: String,
    },
    /// A scope names a code system the map's groups map between.
    ///
    /// R4 and R4B type `ConceptMap.source[x]` and `target[x]` as `uri` or
    /// `canonical(ValueSet)`, the value set that "provides context for the
    /// mappings", and R5 and R6 `sourceScope[x]` and `targetScope[x]` the same
    /// (<https://hl7.org/fhir/R4/conceptmap-definitions.html#ConceptMap.source_x_>),
    /// so `$translate` cannot read such a map's scope.
    ScopeIsCodeSystem {
        /// Which scope names the code system.
        side: Side,
        /// The code system it names.
        scope: String,
    },
}

/// The two scopes of a map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Side {
    /// The scope of the codes on the left.
    Source,
    /// The scope of the codes on the right.
    Target,
}

impl Problem {
    /// The sentence the screen shows for this problem.
    pub(crate) fn text(&self, locale: &ferrohealth_ui::i18n::Locale) -> String {
        match self {
            Self::NoCanonical => tr(locale, Msg::ConceptMapNeedsCanonicalBeforeCan),
            Self::NoStatus => tr(locale, Msg::ConceptMapNeedsPublicationStatus),
            Self::GrouplessSystems => tr(locale, Msg::EveryGroupNamesSystemMapsFrom),
            Self::GroupWithoutCodes => tr(locale, Msg::EveryGroupCarriesLeastOneCode),
            Self::CommentRequired {
                element,
                code,
                relationship,
            } => tr_with(
                locale,
                Msg::TargetFhirVersionRequiresCommentTarget,
                &[
                    ("code", (code).clone().into()),
                    ("element", (element).clone().into()),
                    ("relationship", (relationship).clone().into()),
                ],
            ),
            Self::RelationshipMissing { element, code } => tr_with(
                locale,
                Msg::TargetStatesNoRelationshipSourceWhich,
                &[
                    ("code", (code).clone().into()),
                    ("element", (element).clone().into()),
                ],
            ),
            Self::MappedAndUnmapped { element } => tr_with(
                locale,
                Msg::MarkedMappingNothingCarriesTargetWell,
                &[("element", (element).clone().into())],
            ),
            Self::ScopeIsCodeSystem { side, scope } => tr_with(
                locale,
                match side {
                    Side::Source => Msg::SourceScopeNamesCodeSystem,
                    Side::Target => Msg::TargetScopeNamesCodeSystem,
                },
                &[("scope", scope.clone().into())],
            ),
        }
    }
}

impl MapDraft {
    /// A draft for a concept map that does not exist yet.
    pub(crate) fn new() -> Self {
        Self {
            status: NEW_STATUS.to_owned(),
            ..Self::default()
        }
    }

    /// Whether the server states a version for this resource.
    ///
    /// `meta.versionId` is what an update states in `If-Match`
    /// (<https://hl7.org/fhir/R4B/http.html#concurrency>), and the REST API is
    /// what stamps it, so a resource the server states none for is one this
    /// deployment built or loaded rather than one a person wrote here. The
    /// editor shows it and offers to change nothing.
    pub(crate) fn managed(&self) -> bool {
        !self.version_id.is_empty()
    }

    /// Everything that stops this draft being saved as `version` writes a map.
    pub(crate) fn problems(&self, version: FhirVersion) -> Vec<Problem> {
        let dialect = dialect(version);
        let mut found = Vec::new();
        if self.url.trim().is_empty() {
            found.push(Problem::NoCanonical);
        }
        if self.status.trim().is_empty() {
            found.push(Problem::NoStatus);
        }
        for (side, scope) in [
            (Side::Source, &self.source_scope),
            (Side::Target, &self.target_scope),
        ] {
            if let Some(scope) = self.code_system_scope(scope) {
                found.push(Problem::ScopeIsCodeSystem { side, scope });
            }
        }
        // `cmd-1` excuses a draft map on R5 and R6
        // (<https://hl7.org/fhir/R5/conceptmap.html>).
        let excused = dialect.draft_excuses_comment && self.status.trim() == DRAFT;
        for group in &self.groups {
            if group.source.trim().is_empty() || group.target.trim().is_empty() {
                found.push(Problem::GrouplessSystems);
            }
            let coded: Vec<&MapElement> = group
                .elements
                .iter()
                .filter(|held| !held.code.trim().is_empty())
                .collect();
            // `ConceptMap.group.element` is 1..* in every served version.
            if coded.is_empty() {
                found.push(Problem::GroupWithoutCodes);
            }
            for element in coded {
                let mapped: Vec<&MapTarget> = element
                    .targets
                    .iter()
                    .filter(|target| !target.code.trim().is_empty())
                    .collect();
                if dialect.no_map && element.no_map && !mapped.is_empty() {
                    found.push(Problem::MappedAndUnmapped {
                        element: element.code.clone(),
                    });
                }
                for target in mapped {
                    let relationship = target.relationship.trim();
                    if relationship.is_empty() {
                        found.push(Problem::RelationshipMissing {
                            element: element.code.clone(),
                            code: target.code.clone(),
                        });
                    } else if !excused
                        && target.comment.trim().is_empty()
                        && dialect.comment_required.contains(&relationship)
                    {
                        found.push(Problem::CommentRequired {
                            element: element.code.clone(),
                            code: target.code.clone(),
                            relationship: relationship.to_owned(),
                        });
                    }
                }
            }
        }
        found
    }

    /// The code system `scope` names, when it names one of the systems the
    /// groups map between rather than a value set.
    fn code_system_scope(&self, scope: &str) -> Option<String> {
        let scope = scope.trim();
        let url = systemless(scope);
        let named = self.groups.iter().any(|group| {
            [&group.source, &group.target]
                .iter()
                .any(|s| s.trim() == url)
        });
        (!url.is_empty() && named).then(|| scope.to_owned())
    }

    /// The `ConceptMap` a save sends, written as the `ConceptMap` of
    /// `version`.
    ///
    /// It starts from the resource this draft was read from, so an element the
    /// form does not draw is carried back rather than deleted by an update
    /// that replaces the whole resource
    /// (<https://hl7.org/fhir/R4B/http.html#update>).
    ///
    /// # Errors
    ///
    /// Returns the decoder's error for a stored resource that is not a
    /// `ConceptMap` at `version`.
    pub(crate) fn body(&self, version: FhirVersion) -> Result<String, serde_json::Error> {
        version.rewrite(self)
    }
}

impl Rewritable for MapDraft {
    fn rewrite<R: Release>(&self) -> Result<String, serde_json::Error> {
        rewritten::<R::ConceptMap, Self>(&self.base, self)
    }
}

/// The version part of a canonical, split off the system it qualifies.
///
/// A canonical states its version after a `|`
/// (<https://hl7.org/fhir/R5/references.html#canonical>), and the form holds
/// the two apart so the same two controls draw a group on every version.
fn split_canonical(held: &str) -> (String, String) {
    held.split_once('|').map_or_else(
        || (held.to_owned(), String::new()),
        |(system, version)| (system.to_owned(), version.to_owned()),
    )
}

/// The system half of a canonical that may carry a version.
fn systemless(held: &str) -> &str {
    held.split_once('|').map_or(held, |(system, _)| system)
}

/// A system and its version as one canonical.
fn canonical(system: &str, version: &str) -> String {
    if version.is_empty() {
        system.to_owned()
    } else {
        format!("{system}|{version}")
    }
}

#[cfg(test)]
mod tests;
