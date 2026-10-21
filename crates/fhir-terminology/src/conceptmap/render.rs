// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A model back to a generated `ConceptMap`, for a read of a loaded instance.
//!
//! One macro produces a module per served version so they cannot drift:
//! `render::r4b::concept_map(&model)`. It is the inverse of
//! [`crate::conceptmap::convert`]: R4 and R4B are written with `source[x]`, a
//! `uri` `group.source` beside `sourceVersion`, `equivalence`, and
//! `unmapped.url`; R5 and R6 with `sourceScope[x]`, a canonical
//! `group.source`, `relationship`, `noMap`, and `unmapped.otherMap`
//! (<https://hl7.org/fhir/R4B/conceptmap.html>,
//! <https://hl7.org/fhir/R5/conceptmap.html>).

/// The cross-version extension carrying R6's `ConceptMap.group.element.comment`
/// (<https://hl7.org/fhir/R5/versions.html#extensions>).
const ELEMENT_COMMENT: &str =
    "http://hl7.org/fhir/6.0/StructureDefinition/extension-ConceptMap.group.element.comment";

/// The `ConceptMap.status` to write for `model`.
///
/// `status` is 1..1, so a map read from a resource that stated none still has
/// to state one; `unknown` is the `PublicationStatus` code for exactly that
/// ("The resource status is undetermined",
/// <https://hl7.org/fhir/R4B/codesystem-publication-status.html>).
fn status(model: &crate::conceptmap::model::ConceptMapModel) -> &str {
    match model.status.as_str() {
        "" => "unknown",
        stated => stated,
    }
}

/// `url` and `version` as the one canonical R5 and R6 spell `group.source` and
/// `group.target` as (<https://hl7.org/fhir/R5/datatypes.html#canonical>).
fn canonical(url: Option<&str>, version: Option<&str>) -> Option<String> {
    let url = url?;
    Some(match version {
        Some(version) => format!("{url}|{version}"),
        None => url.to_owned(),
    })
}

macro_rules! render_concept_map {
    // R6 defines `group.element.comment`; R4, R4B, and R5 carry it as the
    // cross-version extension.
    (@comment native, $module:ident) => {
        /// The element comment, as this version spells it.
        fn element_comment(
            element: &mut fhir_types::$module::concept_map::ConceptMapGroupElement,
            comment: Option<&str>,
        ) {
            element.comment = comment.map(Into::into);
        }
    };
    (@comment extension, $module:ident) => {
        /// The element comment, as this version spells it.
        fn element_comment(
            element: &mut fhir_types::$module::concept_map::ConceptMapGroupElement,
            comment: Option<&str>,
        ) {
            element
                .extension
                .extend(comment.map(|text| fhir_types::$module::extension::Extension {
                    url: super::ELEMENT_COMMENT.to_owned(),
                    value: Some(fhir_types::$module::extension::ExtensionValue::String(
                        text.into(),
                    )),
                    ..Default::default()
                }));
        }
    };
    // The R4 family: `equivalence`, `sourceVersion`, `dependsOn.property`, `unmapped.url`.
    ($module:ident, r4, $comment:ident) => {
        /// The `ConceptMap` renders of one FHIR version.
        pub mod $module {
            use fhir_types::$module::concept_map::{
                ConceptMap, ConceptMapGroup, ConceptMapGroupElement, ConceptMapGroupElementTarget,
                ConceptMapGroupElementTargetDependsOn, ConceptMapGroupUnmapped, ConceptMapSource,
                ConceptMapTarget,
            };

            use super::super::model::{
                ConceptMapModel, DependsOn, Element, Group, Target, Unmapped, UnmappedMode,
            };
            use super::status;

            render_concept_map!(@comment $comment, $module);

            /// The `ConceptMap` of `model`.
            ///
            /// `source[x]` and `target[x]` are written in their `uri` form: the
            /// model holds the scope as the one string both forms reduce to
            /// (<https://hl7.org/fhir/R4B/conceptmap-definitions.html#ConceptMap.source_x_>).
            #[must_use]
            pub fn concept_map(model: &ConceptMapModel) -> ConceptMap {
                ConceptMap {
                    url: Some(model.url.as_str().into()),
                    version: model.version.as_deref().map(Into::into),
                    name: model.name.as_deref().map(Into::into),
                    title: model.title.as_deref().map(Into::into),
                    status: status(model).into(),
                    experimental: model.experimental.map(Into::into),
                    date: model.date.as_deref().map(Into::into),
                    publisher: model.publisher.as_deref().map(Into::into),
                    description: model.description.as_deref().map(Into::into),
                    source: model
                        .source_scope
                        .as_deref()
                        .map(|scope| ConceptMapSource::Uri(scope.into())),
                    target: model
                        .target_scope
                        .as_deref()
                        .map(|scope| ConceptMapTarget::Uri(scope.into())),
                    group: model.groups.iter().map(group).collect(),
                    ..Default::default()
                }
            }

            /// One `ConceptMap.group`.
            fn group(held: &Group) -> ConceptMapGroup {
                ConceptMapGroup {
                    source: held.source.as_deref().map(Into::into),
                    source_version: held.source_version.as_deref().map(Into::into),
                    target: held.target.as_deref().map(Into::into),
                    target_version: held.target_version.as_deref().map(Into::into),
                    element: held.elements.iter().map(element).collect(),
                    unmapped: held.unmapped.as_ref().map(unmapped),
                    ..Default::default()
                }
            }

            /// One `ConceptMap.group.element`.
            ///
            /// R4 spells an element that maps to nothing as a target with
            /// `equivalence = unmatched` and no code
            /// (<https://hl7.org/fhir/R4B/conceptmap.html#unmapped>).
            fn element(held: &Element) -> ConceptMapGroupElement {
                let mut target: Vec<ConceptMapGroupElementTarget> =
                    held.targets.iter().map(mapped).collect();
                if held.no_map && target.is_empty() {
                    target.push(ConceptMapGroupElementTarget {
                        equivalence: "unmatched".into(),
                        ..Default::default()
                    });
                }
                let mut out = ConceptMapGroupElement {
                    code: held.code.as_deref().map(Into::into),
                    display: held.display.as_deref().map(Into::into),
                    target,
                    ..Default::default()
                };
                element_comment(&mut out, held.comment.as_deref());
                out
            }

            /// One `ConceptMap.group.element.target`.
            fn mapped(held: &Target) -> ConceptMapGroupElementTarget {
                ConceptMapGroupElementTarget {
                    code: held.code.as_deref().map(Into::into),
                    display: held.display.as_deref().map(Into::into),
                    equivalence: held.relationship.equivalence().into(),
                    comment: held.comment.as_deref().map(Into::into),
                    depends_on: held.depends_on.iter().map(depends_on).collect(),
                    product: held.product.iter().map(depends_on).collect(),
                    ..Default::default()
                }
            }

            /// One `dependsOn` or `product`.
            fn depends_on(held: &DependsOn) -> ConceptMapGroupElementTargetDependsOn {
                ConceptMapGroupElementTargetDependsOn {
                    property: held.attribute.as_str().into(),
                    system: held.system.as_deref().map(Into::into),
                    value: held.value.as_str().into(),
                    display: held.display.as_deref().map(Into::into),
                    ..Default::default()
                }
            }

            /// One `ConceptMap.group.unmapped`; the R4 family names the
            /// source-code mode `provided`
            /// (<https://hl7.org/fhir/R4B/codesystem-conceptmap-unmapped-mode.html>).
            fn unmapped(held: &Unmapped) -> ConceptMapGroupUnmapped {
                ConceptMapGroupUnmapped {
                    mode: match held.mode {
                        UnmappedMode::Provided => "provided",
                        UnmappedMode::Fixed => "fixed",
                        UnmappedMode::OtherMap => "other-map",
                    }
                    .into(),
                    code: held.code.as_deref().map(Into::into),
                    display: held.display.as_deref().map(Into::into),
                    url: held.other_map.as_deref().map(Into::into),
                    ..Default::default()
                }
            }
        }
    };
    // The R5 family: `relationship`, canonical `source`, `dependsOn.attribute`, `noMap`, `otherMap`.
    ($module:ident, r5, $comment:ident) => {
        /// The `ConceptMap` renders of one FHIR version.
        pub mod $module {
            use fhir_types::$module::coding::Coding;
            use fhir_types::$module::concept_map::{
                ConceptMap, ConceptMapGroup, ConceptMapGroupElement, ConceptMapGroupElementTarget,
                ConceptMapGroupElementTargetDependsOn,
                ConceptMapGroupElementTargetDependsOnValue, ConceptMapGroupUnmapped,
                ConceptMapSourceScope, ConceptMapTargetScope,
            };

            use super::super::model::{
                ConceptMapModel, DependsOn, Element, Group, Target, Unmapped, UnmappedMode,
            };
            use super::{canonical, status};

            render_concept_map!(@comment $comment, $module);

            /// The `ConceptMap` of `model`.
            ///
            /// `sourceScope[x]` and `targetScope[x]` are written in their `uri`
            /// form: the model holds the scope as the one string both forms
            /// reduce to
            /// (<https://hl7.org/fhir/R5/conceptmap-definitions.html#ConceptMap.sourceScope_x_>).
            #[must_use]
            pub fn concept_map(model: &ConceptMapModel) -> ConceptMap {
                ConceptMap {
                    url: Some(model.url.as_str().into()),
                    version: model.version.as_deref().map(Into::into),
                    name: model.name.as_deref().map(Into::into),
                    title: model.title.as_deref().map(Into::into),
                    status: status(model).into(),
                    experimental: model.experimental.map(Into::into),
                    date: model.date.as_deref().map(Into::into),
                    publisher: model.publisher.as_deref().map(Into::into),
                    description: model.description.as_deref().map(Into::into),
                    source_scope: model
                        .source_scope
                        .as_deref()
                        .map(|scope| ConceptMapSourceScope::Uri(scope.into())),
                    target_scope: model
                        .target_scope
                        .as_deref()
                        .map(|scope| ConceptMapTargetScope::Uri(scope.into())),
                    group: model.groups.iter().map(group).collect(),
                    ..Default::default()
                }
            }

            /// One `ConceptMap.group`.
            fn group(held: &Group) -> ConceptMapGroup {
                ConceptMapGroup {
                    source: canonical(held.source.as_deref(), held.source_version.as_deref())
                        .map(|c| c.as_str().into()),
                    target: canonical(held.target.as_deref(), held.target_version.as_deref())
                        .map(|c| c.as_str().into()),
                    element: held.elements.iter().map(element).collect(),
                    unmapped: held.unmapped.as_ref().map(unmapped),
                    ..Default::default()
                }
            }

            /// One `ConceptMap.group.element`; `noMap` and `target` exclude
            /// each other (`cmd-4`,
            /// <https://hl7.org/fhir/R5/conceptmap.html#invs>).
            fn element(held: &Element) -> ConceptMapGroupElement {
                let mut out = ConceptMapGroupElement {
                    code: held.code.as_deref().map(Into::into),
                    display: held.display.as_deref().map(Into::into),
                    no_map: (held.no_map && held.targets.is_empty()).then(|| true.into()),
                    target: held.targets.iter().map(mapped).collect(),
                    ..Default::default()
                };
                element_comment(&mut out, held.comment.as_deref());
                out
            }

            /// One `ConceptMap.group.element.target`.
            fn mapped(held: &Target) -> ConceptMapGroupElementTarget {
                ConceptMapGroupElementTarget {
                    code: held.code.as_deref().map(Into::into),
                    display: held.display.as_deref().map(Into::into),
                    relationship: held.relationship.relationship().into(),
                    comment: held.comment.as_deref().map(Into::into),
                    depends_on: held.depends_on.iter().map(depends_on).collect(),
                    product: held.product.iter().map(depends_on).collect(),
                    ..Default::default()
                }
            }

            /// One `dependsOn` or `product`; a value that named a code system
            /// is written back as the `Coding` form the R5 family defines
            /// (<https://hl7.org/fhir/R5/conceptmap-definitions.html#ConceptMap.group.element.target.dependsOn.value_x_>).
            fn depends_on(held: &DependsOn) -> ConceptMapGroupElementTargetDependsOn {
                let value = match held.system.as_deref() {
                    Some(system) => {
                        ConceptMapGroupElementTargetDependsOnValue::Coding(Box::new(Coding {
                            system: Some(system.into()),
                            code: Some(held.value.as_str().into()),
                            display: held.display.as_deref().map(Into::into),
                            ..Default::default()
                        }))
                    }
                    None => {
                        ConceptMapGroupElementTargetDependsOnValue::Code(held.value.as_str().into())
                    }
                };
                ConceptMapGroupElementTargetDependsOn {
                    attribute: held.attribute.as_str().into(),
                    value: Some(value),
                    ..Default::default()
                }
            }

            /// One `ConceptMap.group.unmapped`; the R5 family names the
            /// source-code mode `use-source-code`
            /// (<https://hl7.org/fhir/R5/codesystem-conceptmap-unmapped-mode.html>).
            fn unmapped(held: &Unmapped) -> ConceptMapGroupUnmapped {
                ConceptMapGroupUnmapped {
                    mode: match held.mode {
                        UnmappedMode::Provided => "use-source-code",
                        UnmappedMode::Fixed => "fixed",
                        UnmappedMode::OtherMap => "other-map",
                    }
                    .into(),
                    code: held.code.as_deref().map(Into::into),
                    display: held.display.as_deref().map(Into::into),
                    relationship: held
                        .relationship
                        .map(|relationship| relationship.relationship().into()),
                    other_map: held.other_map.as_deref().map(Into::into),
                    ..Default::default()
                }
            }
        }
    };
}

render_concept_map!(r4, r4, extension);
render_concept_map!(r4b, r4, extension);
render_concept_map!(r5, r5, extension);
render_concept_map!(r6, r5, native);
