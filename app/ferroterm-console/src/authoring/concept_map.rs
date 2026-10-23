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

use serde_json::Map;
use serde_json::Value;
use serde_json::json;

use crate::authoring::code_system::Key;
use crate::authoring::code_system::Keys;
use crate::fhir::authoring::StoredConceptMap;
use crate::fhir::authoring::StoredMapElement;
use crate::fhir::authoring::StoredMapGroup;
use crate::fhir::authoring::StoredMapTarget;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr, tr_with};

/// The resource type this form authors.
const CONCEPT_MAP: &str = "ConceptMap";

/// How one served FHIR version spells a concept map.
///
/// Every name here is read from that version's own definition page, so a save
/// writes what the root it is sent to defines rather than what another release
/// defines (<https://hl7.org/fhir/R4B/conceptmap.html>,
/// <https://hl7.org/fhir/R5/conceptmap.html>).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Dialect {
    /// The element one target's relationship to its source is written under.
    pub(crate) relationship: &'static str,
    /// The value set whose codes that element admits.
    ///
    /// The control offers what the served root expands this to, so a root that
    /// admits another code offers it without a new build.
    pub(crate) relationship_value_set: &'static str,
    /// The element the map's source scope is written under.
    pub(crate) source_scope: &'static str,
    /// The element the map's target scope is written under.
    pub(crate) target_scope: &'static str,
    /// Whether the version carries a version element beside a group's system.
    ///
    /// R4 and R4B state the system as a `uri` with `group.sourceVersion`
    /// beside it; R5 made it a `canonical(CodeSystem)` and dropped the version
    /// element (<https://hl7.org/fhir/R5/conceptmap.html>).
    // NOTE: no FHIR text says the dropped version moved into the canonical, so
    // writing it after a `|` is our own design over the general canonical rule
    // (<https://hl7.org/fhir/R5/references.html#canonical>).
    pub(crate) group_version_element: bool,
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

/// Every element name a scope is spelled with, in any served version.
///
/// A save removes all of them before writing the one the served version
/// defines, so a resource read on one root and saved on another does not reach
/// the server carrying both spellings of the same fact.
const SCOPE_ELEMENTS: [&str; 8] = [
    "sourceUri",
    "sourceCanonical",
    "targetUri",
    "targetCanonical",
    "sourceScopeUri",
    "sourceScopeCanonical",
    "targetScopeUri",
    "targetScopeCanonical",
];

/// Every element name a target's relationship is spelled with.
const RELATIONSHIP_ELEMENTS: [&str; 2] = ["equivalence", "relationship"];

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
            relationship: "equivalence",
            relationship_value_set: EQUIVALENCE_VALUE_SET,
            source_scope: "sourceCanonical",
            target_scope: "targetCanonical",
            group_version_element: true,
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
            relationship: "relationship",
            relationship_value_set: RELATIONSHIP_VALUE_SET,
            source_scope: "sourceScopeCanonical",
            target_scope: "targetScopeCanonical",
            group_version_element: false,
            no_map: true,
            comment_required: &["source-is-broader-than-target", "not-related-to"],
            draft_excuses_comment: true,
        },
        FhirVersion::R6 => Dialect {
            relationship: "relationship",
            relationship_value_set: RELATIONSHIP_VALUE_SET,
            source_scope: "sourceScopeCanonical",
            target_scope: "targetScopeCanonical",
            group_version_element: false,
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
}

impl Problem {
    /// The sentence the screen shows for this problem.
    pub(crate) fn text(&self) -> String {
        match self {
            Self::NoCanonical => tr(Msg::ConceptMapNeedsCanonicalBeforeCan),
            Self::NoStatus => tr(Msg::ConceptMapNeedsPublicationStatus),
            Self::GrouplessSystems => tr(Msg::EveryGroupNamesSystemMapsFrom),
            Self::GroupWithoutCodes => tr(Msg::EveryGroupCarriesLeastOneCode),
            Self::CommentRequired {
                element,
                code,
                relationship,
            } => tr_with(
                Msg::TargetFhirVersionRequiresCommentTarget,
                &[
                    ("code", (code).clone().into()),
                    ("element", (element).clone().into()),
                    ("relationship", (relationship).clone().into()),
                ],
            ),
            Self::RelationshipMissing { element, code } => tr_with(
                Msg::TargetStatesNoRelationshipSourceWhich,
                &[
                    ("code", (code).clone().into()),
                    ("element", (element).clone().into()),
                ],
            ),
            Self::MappedAndUnmapped { element } => tr_with(
                Msg::MarkedMappingNothingCarriesTargetWell,
                &[("element", (element).clone().into())],
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

    /// The draft for a concept map the server already holds.
    pub(crate) fn of(resource: &Value) -> Self {
        let stored = StoredConceptMap::of(resource);
        let mut keys = Keys::default();
        let groups = stored
            .group
            .iter()
            .map(|group| group_of(group, &mut keys))
            .collect();
        Self {
            id: stored.id.clone().unwrap_or_default(),
            version_id: stored.version_id().unwrap_or_default(),
            url: stored.url.clone().unwrap_or_default(),
            version: stored.version.clone().unwrap_or_default(),
            title: resource
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            name: resource
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            status: stored.status.clone().unwrap_or_default(),
            source_scope: stored.source_scope().unwrap_or_default().to_owned(),
            target_scope: stored.target_scope().unwrap_or_default().to_owned(),
            groups,
            keys,
            base: resource.clone(),
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

    /// The resource a save sends, as the version `version` defines it.
    pub(crate) fn resource(&self, version: FhirVersion) -> Value {
        let dialect = dialect(version);
        let mut resource = object(&self.base);
        resource.insert("resourceType".to_owned(), json!(CONCEPT_MAP));
        if self.id.is_empty() {
            resource.remove("id");
        } else {
            resource.insert("id".to_owned(), json!(self.id));
        }
        resource.insert("url".to_owned(), json!(self.url.trim()));
        set(&mut resource, "version", self.version.trim());
        set(&mut resource, "title", self.title.trim());
        set(&mut resource, "name", self.name.trim());
        resource.insert("status".to_owned(), json!(self.status));
        // A resource read on one root and saved on another would otherwise
        // reach the server carrying two spellings of the same scope.
        for element in SCOPE_ELEMENTS {
            resource.remove(element);
        }
        set(
            &mut resource,
            dialect.source_scope,
            self.source_scope.trim(),
        );
        set(
            &mut resource,
            dialect.target_scope,
            self.target_scope.trim(),
        );

        let groups: Vec<Value> = self
            .groups
            .iter()
            .filter(|group| !group.source.trim().is_empty() && !group.target.trim().is_empty())
            .map(|group| self.group_body(group, dialect))
            .collect();
        if groups.is_empty() {
            resource.remove("group");
        } else {
            resource.insert("group".to_owned(), Value::Array(groups));
        }
        Value::Object(resource)
    }

    /// The FHIR JSON a save sends.
    pub(crate) fn body(&self, version: FhirVersion) -> String {
        self.resource(version).to_string()
    }

    /// The group of the stored resource that maps the same pair of systems.
    ///
    /// A group is identified by the two systems it maps between
    /// (<https://hl7.org/fhir/R4B/conceptmap.html>), so that is what an
    /// authored group is matched to the stored one by, and everything the form
    /// does not draw on it survives the save.
    fn held_group(&self, source: &str, target: &str) -> Map<String, Value> {
        self.base
            .get("group")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .find(|held| {
                systemless(
                    held.get("source")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ) == source
                    && systemless(
                        held.get("target")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    ) == target
            })
            .cloned()
            .unwrap_or_default()
    }

    /// One group, as the resource carries it.
    fn group_body(&self, group: &MapGroup, dialect: Dialect) -> Value {
        let source = group.source.trim();
        let target = group.target.trim();
        let mut written = self.held_group(source, target);
        if dialect.group_version_element {
            written.insert("source".to_owned(), json!(source));
            written.insert("target".to_owned(), json!(target));
            set(&mut written, "sourceVersion", group.source_version.trim());
            set(&mut written, "targetVersion", group.target_version.trim());
        } else {
            // The system is a canonical here, which carries its own version
            // after a `|`, so the two controls write one element
            // (<https://hl7.org/fhir/R5/references.html#canonical>).
            written.remove("sourceVersion");
            written.remove("targetVersion");
            written.insert(
                "source".to_owned(),
                json!(canonical(source, group.source_version.trim())),
            );
            written.insert(
                "target".to_owned(),
                json!(canonical(target, group.target_version.trim())),
            );
        }
        let elements: Vec<Value> = group
            .elements
            .iter()
            .filter(|element| !element.code.trim().is_empty())
            .map(|element| element_body(&written, element, dialect))
            .collect();
        if elements.is_empty() {
            written.remove("element");
        } else {
            written.insert("element".to_owned(), Value::Array(elements));
        }
        Value::Object(written)
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

/// One group, read off the resource the server sent.
fn group_of(group: &StoredMapGroup, keys: &mut Keys) -> MapGroup {
    let (source, source_version) = split_canonical(group.source.as_deref().unwrap_or_default());
    let (target, target_version) = split_canonical(group.target.as_deref().unwrap_or_default());
    MapGroup {
        key: keys.next(),
        source,
        // A version stated beside the system wins over one inside the
        // canonical: only one of the two is ever present on a given version,
        // and the element is the version's own spelling.
        source_version: group
            .source_version
            .clone()
            .filter(|held| !held.is_empty())
            .unwrap_or(source_version),
        target,
        target_version: group
            .target_version
            .clone()
            .filter(|held| !held.is_empty())
            .unwrap_or(target_version),
        elements: group
            .element
            .iter()
            .map(|element| element_of(element, keys))
            .collect(),
    }
}

/// One source code, read off the resource the server sent.
fn element_of(element: &StoredMapElement, keys: &mut Keys) -> MapElement {
    MapElement {
        key: keys.next(),
        code: element.code.clone().unwrap_or_default(),
        display: element.display.clone().unwrap_or_default(),
        no_map: element.no_map.unwrap_or_default(),
        targets: element
            .target
            .iter()
            .map(|target| target_of(target, keys))
            .collect(),
    }
}

/// One target, read off the resource the server sent.
///
/// Whichever element the resource stated the relationship in is what the form
/// holds: a map written on an R4B root and opened on an R5 one shows the value
/// it carries, and the save then writes it under R5's own element.
fn target_of(target: &StoredMapTarget, keys: &mut Keys) -> MapTarget {
    MapTarget {
        key: keys.next(),
        code: target.code.clone().unwrap_or_default(),
        display: target.display.clone().unwrap_or_default(),
        relationship: target
            .relationship
            .clone()
            .or_else(|| target.equivalence.clone())
            .unwrap_or_default(),
        comment: target.comment.clone().unwrap_or_default(),
    }
}

/// One source code, as the resource carries it.
fn element_body(group: &Map<String, Value>, element: &MapElement, dialect: Dialect) -> Value {
    let code = element.code.trim();
    let mut written = held_element(group, code);
    written.insert("code".to_owned(), json!(code));
    set(&mut written, "display", element.display.trim());
    let unmapped = dialect.no_map && element.no_map;
    if unmapped {
        written.insert("noMap".to_owned(), json!(true));
    } else {
        written.remove("noMap");
    }
    let targets: Vec<Value> = if unmapped {
        // `noMap` says the code maps to nothing, so a target beside it would
        // say both things at once; `problems` refuses the draft before a save
        // reaches here, and this keeps the JSON honest either way.
        Vec::new()
    } else {
        element
            .targets
            .iter()
            .filter(|target| !target.code.trim().is_empty())
            .map(|target| target_body(&written, target, dialect))
            .collect()
    };
    if targets.is_empty() {
        written.remove("target");
    } else {
        written.insert("target".to_owned(), Value::Array(targets));
    }
    Value::Object(written)
}

/// The stored element of `group` that `code` names.
fn held_element(group: &Map<String, Value>, code: &str) -> Map<String, Value> {
    held(group, "element", code)
}

/// One target, as the resource carries it.
fn target_body(element: &Map<String, Value>, target: &MapTarget, dialect: Dialect) -> Value {
    let code = target.code.trim();
    let mut written = held(element, "target", code);
    written.insert("code".to_owned(), json!(code));
    set(&mut written, "display", target.display.trim());
    for spelling in RELATIONSHIP_ELEMENTS {
        written.remove(spelling);
    }
    set(
        &mut written,
        dialect.relationship,
        target.relationship.trim(),
    );
    set(&mut written, "comment", target.comment.trim());
    Value::Object(written)
}

/// The member of `list` inside `held` whose `code` is `code`.
fn held(held: &Map<String, Value>, list: &str, code: &str) -> Map<String, Value> {
    held.get(list)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .find(|member| member.get("code").and_then(Value::as_str) == Some(code))
        .cloned()
        .unwrap_or_default()
}

/// The object `resource` carries, or an empty one when it is not an object.
fn object(resource: &Value) -> Map<String, Value> {
    resource.as_object().cloned().unwrap_or_default()
}

/// Writes `value` under `name`, or removes the element when nothing was typed.
fn set(written: &mut Map<String, Value>, name: &str, value: &str) {
    if value.is_empty() {
        written.remove(name);
    } else {
        written.insert(name.to_owned(), json!(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A draft with one group and one mapped code.
    fn one_mapping(relationship: &str) -> MapDraft {
        let mut draft = MapDraft::new();
        let mut keys = Keys::default();
        let group = keys.next();
        let element = keys.next();
        let target = keys.next();
        draft.url = String::from("https://terminology.example/colour-map");
        draft.source_scope = String::from("https://terminology.example/colours-vs");
        draft.target_scope = String::from("https://terminology.example/paints-vs");
        draft.groups = vec![MapGroup {
            key: group,
            source: String::from("https://terminology.example/colours"),
            source_version: String::from("1.0.0"),
            target: String::from("https://terminology.example/paints"),
            target_version: String::new(),
            elements: vec![MapElement {
                key: element,
                code: String::from("red"),
                display: String::from("Red"),
                no_map: false,
                targets: vec![MapTarget {
                    key: target,
                    code: String::from("vermilion"),
                    display: String::from("Vermilion"),
                    relationship: relationship.to_owned(),
                    comment: String::new(),
                }],
            }],
        }];
        draft.keys = keys;
        draft
    }

    #[test]
    fn a_target_without_a_relationship_is_refused() {
        let draft = one_mapping("");
        assert_eq!(
            draft.problems(FhirVersion::R4B),
            vec![Problem::RelationshipMissing {
                element: String::from("red"),
                code: String::from("vermilion"),
            }],
            "every version makes the element 1..1, so a target without one is refused"
        );
        assert_ne!(draft.problems(FhirVersion::R4B), Vec::<Problem>::new());
        assert!(
            !draft.problems(FhirVersion::R5).is_empty(),
            "R5 makes `relationship` mandatory the same way R4B makes `equivalence` mandatory"
        );
    }

    #[test]
    fn a_target_that_states_its_relationship_is_savable() {
        let draft = one_mapping("equivalent");
        for version in [
            FhirVersion::R4,
            FhirVersion::R4B,
            FhirVersion::R5,
            FhirVersion::R6,
        ] {
            assert!(
                draft.problems(version).is_empty(),
                "{version:?} takes a target that states its relationship: {:?}",
                draft.problems(version)
            );
        }
    }

    #[test]
    fn a_draft_with_no_canonical_or_status_says_both() {
        let mut draft = MapDraft::default();
        assert_eq!(
            draft.problems(FhirVersion::R5),
            vec![Problem::NoCanonical, Problem::NoStatus]
        );
        draft.url = String::from("https://terminology.example/m");
        draft.status = String::from("draft");
        assert_eq!(draft.problems(FhirVersion::R5), Vec::<Problem>::new());
    }

    #[test]
    fn a_group_that_names_only_one_system_is_refused() {
        let mut draft = one_mapping("equivalent");
        draft
            .groups
            .first_mut()
            .expect("the draft carries one group")
            .target = String::new();
        assert!(
            draft
                .problems(FhirVersion::R4B)
                .contains(&Problem::GrouplessSystems),
            "a group maps between two systems, so one of them is not enough"
        );
    }

    #[test]
    fn no_map_and_a_target_are_exclusive_on_a_version_that_defines_no_map() {
        let mut draft = one_mapping("equivalent");
        draft
            .groups
            .first_mut()
            .expect("the draft carries one group")
            .elements
            .first_mut()
            .expect("the group carries one element")
            .no_map = true;
        assert_eq!(
            draft.problems(FhirVersion::R5),
            vec![Problem::MappedAndUnmapped {
                element: String::from("red"),
            }],
            "a code cannot both map to nothing and map to something"
        );
        assert!(
            draft.problems(FhirVersion::R4B).is_empty(),
            "R4B defines no `noMap`, so the flag is not a fact it can state"
        );
    }

    #[test]
    fn the_version_decides_which_element_a_relationship_is_written_under() {
        let draft = one_mapping("equivalent");
        let r4b = draft.body(FhirVersion::R4B);
        assert!(
            r4b.contains(r#""equivalence":"equivalent""#),
            "R4B states a target's relation in `equivalence`: {r4b}"
        );
        assert!(
            !r4b.contains(r#""relationship""#),
            "and never in R5's element: {r4b}"
        );
        let r5 = draft.body(FhirVersion::R5);
        assert!(
            r5.contains(r#""relationship":"equivalent""#),
            "R5 states it in `relationship`: {r5}"
        );
        assert!(
            !r5.contains(r#""equivalence""#),
            "and never in R4B's element: {r5}"
        );
    }

    #[test]
    fn the_version_decides_which_element_scopes_the_map() {
        let draft = one_mapping("equivalent");
        let r4b = draft.body(FhirVersion::R4B);
        assert!(
            r4b.contains(r#""sourceCanonical":"https://terminology.example/colours-vs""#),
            "R4B scopes a map with `source[x]`: {r4b}"
        );
        assert!(!r4b.contains("sourceScope"), "{r4b}");
        let r6 = draft.body(FhirVersion::R6);
        assert!(
            r6.contains(r#""sourceScopeCanonical":"https://terminology.example/colours-vs""#),
            "R6 scopes it with `sourceScope[x]`: {r6}"
        );
        assert!(!r6.contains(r#""sourceCanonical""#), "{r6}");
    }

    #[test]
    fn a_group_s_system_version_is_an_element_on_r4b_and_a_canonical_on_r5() {
        let draft = one_mapping("equivalent");
        let r4b = draft.body(FhirVersion::R4B);
        assert!(
            r4b.contains(r#""source":"https://terminology.example/colours""#)
                && r4b.contains(r#""sourceVersion":"1.0.0""#),
            "R4B states the system as a uri with its version beside it: {r4b}"
        );
        let r5 = draft.body(FhirVersion::R5);
        assert!(
            r5.contains(r#""source":"https://terminology.example/colours|1.0.0""#),
            "R5 states it as a canonical carrying its own version: {r5}"
        );
        assert!(!r5.contains("sourceVersion"), "{r5}");
    }

    #[test]
    fn no_map_is_written_only_on_a_version_that_defines_it() {
        let mut draft = one_mapping("equivalent");
        let element = draft
            .groups
            .first_mut()
            .expect("the draft carries one group")
            .elements
            .first_mut()
            .expect("the group carries one element");
        element.no_map = true;
        element.targets.clear();
        let r5 = draft.body(FhirVersion::R5);
        assert!(r5.contains(r#""noMap":true"#), "{r5}");
        let r4b = draft.body(FhirVersion::R4B);
        assert!(
            !r4b.contains("noMap"),
            "R4B defines no such element, so writing one would be refused: {r4b}"
        );
    }

    #[test]
    fn an_element_the_form_never_drew_survives_a_save() {
        let held: Value = serde_json::from_str(
            r#"{"resourceType":"ConceptMap","id":"m","meta":{"versionId":"3"},
                "url":"https://terminology.example/colour-map","status":"active",
                "name":"ColourMap","publisher":"Someone",
                "sourceUri":"https://terminology.example/colours-vs",
                "group":[{"source":"https://terminology.example/colours",
                  "target":"https://terminology.example/paints",
                  "unmapped":{"mode":"fixed","code":"unknown"},
                  "element":[{"code":"red","target":[
                    {"code":"vermilion","equivalence":"equivalent","comment":"kept"}]}]}]}"#,
        )
        .expect("the server's own answer parses");
        let draft = MapDraft::of(&held);
        assert_eq!(draft.version_id, "3");
        assert_eq!(draft.source_scope, "https://terminology.example/colours-vs");
        assert!(draft.managed());
        let saved = draft.body(FhirVersion::R4B);
        assert!(saved.contains(r#""publisher":"Someone""#), "{saved}");
        assert!(saved.contains(r#""name":"ColourMap""#), "{saved}");
        assert!(saved.contains(r#""unmapped""#), "{saved}");
        assert!(saved.contains(r#""comment":"kept""#), "{saved}");
    }

    #[test]
    fn a_map_read_on_one_root_writes_the_other_root_s_own_elements() {
        let held: Value = serde_json::from_str(
            r#"{"resourceType":"ConceptMap","url":"https://terminology.example/m",
                "status":"active","sourceUri":"https://terminology.example/vs",
                "group":[{"source":"https://terminology.example/a",
                  "target":"https://terminology.example/b",
                  "element":[{"code":"x","target":[
                    {"code":"y","equivalence":"wider"}]}]}]}"#,
        )
        .expect("the server's own answer parses");
        let draft = MapDraft::of(&held);
        let r5 = draft.body(FhirVersion::R5);
        assert!(r5.contains(r#""relationship":"wider""#), "{r5}");
        assert!(
            !r5.contains("equivalence") && !r5.contains(r#""sourceUri""#),
            "no element of the root it was read from survives into the one it is sent to: {r5}"
        );
    }

    #[test]
    fn an_unmanaged_resource_states_no_version_to_update_from() {
        let held: Value = serde_json::from_str(
            r#"{"resourceType":"ConceptMap","url":"https://terminology.example/built","status":"active"}"#,
        )
        .expect("the server's own answer parses");
        assert!(
            !MapDraft::of(&held).managed(),
            "a resource the REST API never stamped has no version an update could state"
        );
    }

    #[test]
    fn a_canonical_splits_into_the_system_and_the_version_it_carries() {
        assert_eq!(
            split_canonical("https://x.example/s|2.0"),
            (String::from("https://x.example/s"), String::from("2.0"))
        );
        assert_eq!(
            split_canonical("https://x.example/s"),
            (String::from("https://x.example/s"), String::new())
        );
        assert_eq!(canonical("https://x.example/s", ""), "https://x.example/s");
        assert_eq!(
            canonical("https://x.example/s", "2.0"),
            "https://x.example/s|2.0"
        );
    }

    /// `cmd-1` is an error on R4, R4B and R5 and a warning on the R6 ballot,
    /// and each version names its own codes
    /// (<https://hl7.org/fhir/R4B/conceptmap.html>,
    /// <https://hl7.org/fhir/R5/conceptmap.html>).
    #[test]
    fn a_relationship_the_version_requires_a_comment_on_is_refused_uncommented() {
        let mut draft = one_mapping("narrower");
        draft.status = String::from("active");
        assert_eq!(
            draft.problems(FhirVersion::R4B),
            vec![Problem::CommentRequired {
                element: String::from("red"),
                code: String::from("vermilion"),
                relationship: String::from("narrower"),
            }]
        );
        let mut broader = one_mapping("source-is-broader-than-target");
        broader.status = String::from("active");
        assert!(
            broader
                .problems(FhirVersion::R5)
                .iter()
                .any(|problem| matches!(problem, Problem::CommentRequired { .. })),
            "R5 names its own two codes"
        );
        assert!(
            broader.problems(FhirVersion::R6).is_empty(),
            "the R6 ballot demoted cmd-1 to a warning, so the editor refuses nothing for it"
        );
    }

    #[test]
    fn a_draft_map_is_excused_the_comment_on_the_versions_that_excuse_it() {
        let broader = one_mapping("source-is-broader-than-target");
        assert_eq!(broader.status, "draft");
        assert!(
            broader.problems(FhirVersion::R5).is_empty(),
            "R5 cmd-1 carries a `status = draft` arm"
        );
        let narrower = one_mapping("narrower");
        assert!(
            !narrower.problems(FhirVersion::R4B).is_empty(),
            "R4B cmd-1 carries no such arm, so a draft is refused too"
        );
    }

    #[test]
    fn a_target_that_carries_a_comment_satisfies_the_invariant() {
        let mut draft = one_mapping("narrower");
        draft.status = String::from("active");
        draft
            .groups
            .first_mut()
            .expect("the draft carries one group")
            .elements
            .first_mut()
            .expect("the group carries one element")
            .targets
            .first_mut()
            .expect("the element carries one target")
            .comment = String::from("Vermilion is one shade of red.");
        assert_eq!(draft.problems(FhirVersion::R4B), Vec::<Problem>::new());
    }

    #[test]
    fn a_group_with_no_code_is_refused() {
        let mut draft = one_mapping("equivalent");
        draft
            .groups
            .first_mut()
            .expect("the draft carries one group")
            .elements
            .clear();
        assert!(
            draft
                .problems(FhirVersion::R5)
                .contains(&Problem::GroupWithoutCodes),
            "`ConceptMap.group.element` is 1..* in every served version"
        );
    }

    #[test]
    fn every_problem_reads_as_a_sentence() {
        for problem in [
            Problem::NoCanonical,
            Problem::NoStatus,
            Problem::GrouplessSystems,
            Problem::GroupWithoutCodes,
            Problem::CommentRequired {
                element: String::from("red"),
                code: String::from("vermilion"),
                relationship: String::from("narrower"),
            },
            Problem::RelationshipMissing {
                element: String::from("red"),
                code: String::from("vermilion"),
            },
            Problem::MappedAndUnmapped {
                element: String::from("red"),
            },
        ] {
            assert!(!problem.text().is_empty(), "{problem:?} renders as nothing");
        }
    }

    #[test]
    fn each_version_offers_the_value_set_its_own_element_is_bound_to() {
        for version in [FhirVersion::R4, FhirVersion::R4B] {
            assert_eq!(
                dialect(version).relationship_value_set,
                EQUIVALENCE_VALUE_SET
            );
        }
        for version in [FhirVersion::R5, FhirVersion::R6] {
            assert_eq!(
                dialect(version).relationship_value_set,
                RELATIONSHIP_VALUE_SET
            );
        }
    }
}
