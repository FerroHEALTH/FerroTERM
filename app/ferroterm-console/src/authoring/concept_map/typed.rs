// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `ConceptMap` the concept map editor opens and the one a save writes,
//! through the `fhir-types` model of the release that serves it.
//!
//! R4 and R4B scope a map with `source[x]` and `target[x]`, state a group's
//! system as a `uri` with its version beside it, state a target's relation in
//! `equivalence`, and define no `noMap` (<https://hl7.org/fhir/R4B/conceptmap.html>).
//! R5 renamed the scopes to `sourceScope[x]` and `targetScope[x]`, made a
//! group's system a `canonical`, replaced `equivalence` with `relationship`,
//! and added `element.noMap` (<https://hl7.org/fhir/R5/conceptmap.html>); the
//! R6 ballot carries the same four. Each difference is a parameter of the
//! macro below, so each release's map is read and written in its own elements.

use super::{MapDraft, MapElement, MapGroup, MapTarget, split_canonical, systemless};
use crate::authoring::code_system::Keys;
use crate::fhir::release::{Reads, Rewrites, document};
use crate::fhir::searchset::Found;

/// The text of an optional primitive, empty where it states none.
fn text<T>(value: Option<&T>, of: fn(&T) -> &Option<String>) -> String {
    value
        .and_then(|value| of(value).clone())
        .unwrap_or_default()
}

/// `text`, or nothing when the reader left it empty.
///
/// An element the reader emptied is removed rather than sent as an empty
/// string, which is not a value any of these elements admits.
fn stated<T: From<String>>(text: &str) -> Option<T> {
    (!text.is_empty()).then(|| T::from(text.to_owned()))
}

/// The row of `stored` whose code is `code`, as the server sent it.
fn held<T: Clone + Default>(stored: &[T], code: &str, code_of: fn(&T) -> Option<&str>) -> T {
    stored
        .iter()
        .find(|row| code_of(row) == Some(code))
        .cloned()
        .unwrap_or_default()
}

/// One stored group as the editor opens it, its systems split from the
/// versions beside them or inside their canonicals.
///
/// A version stated beside the system wins over one inside the canonical:
/// only one of the two is ever present on a given version, and the element
/// is the version's own spelling.
fn opened_group(
    (source, source_version): (&str, &str),
    (target, target_version): (&str, &str),
    elements: Vec<MapElement>,
    keys: &mut Keys,
) -> MapGroup {
    let (source, inside_source) = split_canonical(source);
    let (target, inside_target) = split_canonical(target);
    let beside = |held: &str, inside: String| {
        if held.is_empty() {
            inside
        } else {
            held.to_owned()
        }
    };
    MapGroup {
        key: keys.next(),
        source,
        source_version: beside(source_version, inside_source),
        target,
        target_version: beside(target_version, inside_target),
        elements,
    }
}

/// Reads and writes the `ConceptMap` of the `fhir-types` module `$module`,
/// with the parts that differ by release as the macros below name them.
macro_rules! concept_map {
    ($module:ident, $scope:ident, $group:ident, $relationship:ident, $no_map:ident) => {
        mod $module {
            use fhir_types::$module::concept_map::{
                ConceptMap, ConceptMapGroup, ConceptMapGroupElement, ConceptMapGroupElementTarget,
            };
            use fhir_types::$module::primitives;

            use super::{
                Found, Keys, MapDraft, MapElement, MapTarget, Reads, Rewrites, document, held,
                opened_group, stated, systemless, text,
            };

            /// The text of a `Code`.
            fn code_text(code: Option<&primitives::Code>) -> String {
                text(code, |code| &code.value)
            }

            /// The text of a `String`.
            fn string_text(string: Option<&primitives::String>) -> String {
                text(string, |string| &string.value)
            }

            /// One stored target, as the editor opens it.
            fn opened_target(target: &ConceptMapGroupElementTarget, keys: &mut Keys) -> MapTarget {
                MapTarget {
                    key: keys.next(),
                    code: code_text(target.code.as_ref()),
                    display: string_text(target.display.as_ref()),
                    relationship: code_text(Some(&target.$relationship)),
                    comment: string_text(target.comment.as_ref()),
                }
            }

            /// One stored element, as the editor opens it.
            fn opened_element(element: &ConceptMapGroupElement, keys: &mut Keys) -> MapElement {
                MapElement {
                    key: keys.next(),
                    code: code_text(element.code.as_ref()),
                    display: string_text(element.display.as_ref()),
                    no_map: $no_map!(read, element),
                    targets: element
                        .target
                        .iter()
                        .map(|target| opened_target(target, keys))
                        .collect(),
                }
            }

            impl Reads<MapDraft> for ConceptMap {
                fn read(&self) -> MapDraft {
                    let mut keys = Keys::default();
                    let groups = self
                        .group
                        .iter()
                        .map(|group| {
                            let elements = group
                                .element
                                .iter()
                                .map(|element| opened_element(element, &mut keys))
                                .collect();
                            let (source, target) = $group!(read, group);
                            opened_group(
                                (&source.0, &source.1),
                                (&target.0, &target.1),
                                elements,
                                &mut keys,
                            )
                        })
                        .collect();
                    let (source_scope, target_scope) = $scope!(read, $module, self);
                    MapDraft {
                        id: self.id.clone().unwrap_or_default(),
                        version_id: text(
                            self.meta.as_ref().and_then(|meta| meta.version_id.as_ref()),
                            |id| &id.value,
                        ),
                        url: text(self.url.as_ref(), |url| &url.value),
                        version: string_text(self.version.as_ref()),
                        title: string_text(self.title.as_ref()),
                        name: string_text(self.name.as_ref()),
                        status: code_text(Some(&self.status)),
                        source_scope,
                        target_scope,
                        groups,
                        keys,
                        base: document(self),
                    }
                }
            }

            crate::fhir::searchset::carries!($module, ConceptMap, MapDraft);

            /// One target, written over the stored one of the same code.
            fn written_target(
                stored: &[ConceptMapGroupElementTarget],
                target: &MapTarget,
            ) -> ConceptMapGroupElementTarget {
                let code = target.code.trim();
                let mut written = held(stored, code, |held| {
                    held.code.as_ref().and_then(|code| code.value.as_deref())
                });
                written.code = Some(code.into());
                written.display = stated(target.display.trim());
                written.$relationship = target.relationship.trim().into();
                written.comment = stated(target.comment.trim());
                written
            }

            /// One element, written over the stored one of the same code.
            fn written_element(
                stored: &[ConceptMapGroupElement],
                element: &MapElement,
            ) -> ConceptMapGroupElement {
                let code = element.code.trim();
                let mut written = held(stored, code, |held| {
                    held.code.as_ref().and_then(|code| code.value.as_deref())
                });
                written.code = Some(code.into());
                written.display = stated(element.display.trim());
                let unmapped = $no_map!(write, &mut written, element.no_map);
                let targets = std::mem::take(&mut written.target);
                // `noMap` says the code maps to nothing, so a target beside it
                // would say both things at once; `problems` refuses the draft
                // before a save reaches here, and this keeps the map honest.
                written.target = if unmapped {
                    Vec::new()
                } else {
                    element
                        .targets
                        .iter()
                        .filter(|target| !target.code.trim().is_empty())
                        .map(|target| written_target(&targets, target))
                        .collect()
                };
                written
            }

            impl Rewrites<MapDraft> for ConceptMap {
                fn rewritten(mut self, draft: &MapDraft) -> Self {
                    self.id = (!draft.id.is_empty()).then(|| draft.id.clone());
                    self.url = Some(draft.url.trim().into());
                    self.version = stated(draft.version.trim());
                    self.title = stated(draft.title.trim());
                    self.name = stated(draft.name.trim());
                    self.status = draft.status.as_str().into();
                    $scope!(write, $module, self, draft);
                    let stored = std::mem::take(&mut self.group);
                    self.group = draft
                        .groups
                        .iter()
                        .filter(|group| {
                            !group.source.trim().is_empty() && !group.target.trim().is_empty()
                        })
                        .map(|group| {
                            let source = group.source.trim();
                            let target = group.target.trim();
                            let mut written: ConceptMapGroup = stored
                                .iter()
                                .find(|held| {
                                    let (held_source, held_target) = $group!(read, held);
                                    systemless(&held_source.0) == source
                                        && systemless(&held_target.0) == target
                                })
                                .cloned()
                                .unwrap_or_default();
                            $group!(write, &mut written, group);
                            let elements = std::mem::take(&mut written.element);
                            written.element = group
                                .elements
                                .iter()
                                .filter(|element| !element.code.trim().is_empty())
                                .map(|element| written_element(&elements, element))
                                .collect();
                            written
                        })
                        .collect();
                    self
                }
            }
        }
    };
}

/// The scopes of an R4 or R4B map, `source[x]` and `target[x]`.
macro_rules! r4_scope {
    (read, $module:ident, $map:expr) => {{
        use fhir_types::$module::concept_map::{ConceptMapSource, ConceptMapTarget};
        (
            match &$map.source {
                Some(ConceptMapSource::Uri(uri)) => uri.value.clone().unwrap_or_default(),
                Some(ConceptMapSource::Canonical(canonical)) => {
                    canonical.value.clone().unwrap_or_default()
                }
                None => String::new(),
            },
            match &$map.target {
                Some(ConceptMapTarget::Uri(uri)) => uri.value.clone().unwrap_or_default(),
                Some(ConceptMapTarget::Canonical(canonical)) => {
                    canonical.value.clone().unwrap_or_default()
                }
                None => String::new(),
            },
        )
    }};
    (write, $module:ident, $map:expr, $draft:expr) => {{
        use fhir_types::$module::concept_map::{ConceptMapSource, ConceptMapTarget};
        $map.source = stated($draft.source_scope.trim()).map(ConceptMapSource::Canonical);
        $map.target = stated($draft.target_scope.trim()).map(ConceptMapTarget::Canonical);
    }};
}

/// The scopes of an R5 or R6 map, `sourceScope[x]` and `targetScope[x]`.
macro_rules! r5_scope {
    (read, $module:ident, $map:expr) => {{
        use fhir_types::$module::concept_map::{ConceptMapSourceScope, ConceptMapTargetScope};
        (
            match &$map.source_scope {
                Some(ConceptMapSourceScope::Uri(uri)) => uri.value.clone().unwrap_or_default(),
                Some(ConceptMapSourceScope::Canonical(canonical)) => {
                    canonical.value.clone().unwrap_or_default()
                }
                None => String::new(),
            },
            match &$map.target_scope {
                Some(ConceptMapTargetScope::Uri(uri)) => uri.value.clone().unwrap_or_default(),
                Some(ConceptMapTargetScope::Canonical(canonical)) => {
                    canonical.value.clone().unwrap_or_default()
                }
                None => String::new(),
            },
        )
    }};
    (write, $module:ident, $map:expr, $draft:expr) => {{
        use fhir_types::$module::concept_map::{ConceptMapSourceScope, ConceptMapTargetScope};
        $map.source_scope =
            stated($draft.source_scope.trim()).map(ConceptMapSourceScope::Canonical);
        $map.target_scope =
            stated($draft.target_scope.trim()).map(ConceptMapTargetScope::Canonical);
    }};
}

/// A group's systems on R4 and R4B: a `uri` with its version beside it.
macro_rules! r4_group {
    (read, $group:expr) => {
        (
            (
                text($group.source.as_ref(), |uri| &uri.value),
                string_text($group.source_version.as_ref()),
            ),
            (
                text($group.target.as_ref(), |uri| &uri.value),
                string_text($group.target_version.as_ref()),
            ),
        )
    };
    (write, $written:expr, $group:expr) => {{
        $written.source = Some($group.source.trim().into());
        $written.target = Some($group.target.trim().into());
        $written.source_version = stated($group.source_version.trim());
        $written.target_version = stated($group.target_version.trim());
    }};
}

/// A group's systems on R5 and R6: a `canonical` carrying its own version
/// after a `|`, so the two controls write one element.
// NOTE: no FHIR text says the dropped version moved into the canonical, so
// writing it after a `|` is our own design over the general canonical rule
// (<https://hl7.org/fhir/R5/references.html#canonical>).
macro_rules! r5_group {
    (read, $group:expr) => {
        (
            (
                text($group.source.as_ref(), |canonical| &canonical.value),
                String::new(),
            ),
            (
                text($group.target.as_ref(), |canonical| &canonical.value),
                String::new(),
            ),
        )
    };
    (write, $written:expr, $group:expr) => {{
        $written.source = Some(
            super::super::canonical($group.source.trim(), $group.source_version.trim()).into(),
        );
        $written.target = Some(
            super::super::canonical($group.target.trim(), $group.target_version.trim()).into(),
        );
    }};
}

/// `element.noMap`, which R4 and R4B do not define.
macro_rules! no_no_map {
    (read, $element:expr) => {
        false
    };
    (write, $element:expr, $wanted:expr) => {
        false
    };
}

/// `element.noMap`, which R5 and R6 define.
macro_rules! no_map {
    (read, $element:expr) => {
        $element
            .no_map
            .as_ref()
            .and_then(|flag| flag.value)
            .unwrap_or_default()
    };
    (write, $element:expr, $wanted:expr) => {{
        $element.no_map = $wanted.then(|| primitives::Boolean {
            value: Some(true),
            ..primitives::Boolean::default()
        });
        $wanted
    }};
}

concept_map!(r4, r4_scope, r4_group, equivalence, no_no_map);
concept_map!(r4b, r4_scope, r4_group, equivalence, no_no_map);
concept_map!(r5, r5_scope, r5_group, relationship, no_map);
concept_map!(r6, r5_scope, r5_group, relationship, no_map);
