// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! From a generated `ConceptMap` of any served FHIR version to the model.
//!
//! R4 and R4B carry `equivalence`, `sourceVersion`, `dependsOn.property`, and
//! `unmapped.url`; R5 and R6 carry `relationship`, canonical `source`,
//! `dependsOn.attribute`, `noMap`, and `unmapped.otherMap`. One macro with a
//! family arm per version keeps the four conversions aligned:
//! `convert::r4b::convert(&resource)`.

use super::model::{ModelError, Relationship};

fn text<T: AsRef<str>>(value: Option<T>) -> Option<String> {
    value.map(|v| v.as_ref().to_owned())
}

/// `url|version` split into its parts.
fn split_canonical(canonical: Option<String>) -> (Option<String>, Option<String>) {
    match canonical {
        Some(canonical) => match canonical.split_once('|') {
            Some((url, version)) => (Some(url.to_owned()), Some(version.to_owned())),
            None => (Some(canonical), None),
        },
        None => (None, None),
    }
}

/// The relationship of a target, from the family's vocabulary.
fn relationship(
    parse: fn(&str) -> Option<Relationship>,
    source: &str,
    target: &str,
    code: &str,
) -> Result<Relationship, ModelError> {
    parse(code).ok_or_else(|| ModelError::Relationship {
        element: source.to_owned(),
        target: target.to_owned(),
        code: code.to_owned(),
    })
}

/// The R6 element comment extension R5 resources carry.
const ELEMENT_COMMENT: &str =
    "http://hl7.org/fhir/6.0/StructureDefinition/extension-ConceptMap.group.element.comment";

macro_rules! convert_concept_map {
    // The R4 family: `equivalence`, `sourceVersion`, `dependsOn.property`, `unmapped.url`.
    ($module:ident, r4) => {
        /// The conversion for this FHIR version's `ConceptMap`.
        pub mod $module {
            use fhir_types::$module::concept_map::{
                ConceptMap, ConceptMapGroup, ConceptMapGroupElement,
                ConceptMapGroupElementTargetDependsOn, ConceptMapGroupUnmapped, ConceptMapSource,
                ConceptMapTarget,
            };

            use super::super::model::{
                ConceptMapModel, DependsOn, Element, Group, ModelError, Relationship, Target,
                Unmapped, UnmappedMode,
            };
            use fhir_types::$module::extension::ExtensionValue;

            use super::{ELEMENT_COMMENT, text};

            fn depends_on(d: &ConceptMapGroupElementTargetDependsOn) -> DependsOn {
                DependsOn {
                    attribute: text(d.property.value.as_deref()).unwrap_or_default(),
                    system: text(d.system.as_ref().and_then(|s| s.value.as_deref())),
                    value: text(d.value.value.as_deref()).unwrap_or_default(),
                    display: text(d.display.as_ref().and_then(|s| s.value.as_deref())),
                }
            }

            fn element(e: &ConceptMapGroupElement) -> Result<Element, ModelError> {
                let code = text(e.code.as_ref().and_then(|c| c.value.as_deref()));
                let mut targets = Vec::with_capacity(e.target.len());
                let mut no_map = false;
                for t in &e.target {
                    let target_code = text(t.code.as_ref().and_then(|c| c.value.as_deref()));
                    // NOTE: R4 spells an explicitly unmapped element as a target with
                    // `equivalence = unmatched` and no code
                    // (<https://hl7.org/fhir/R4B/conceptmap.html#unmapped>).
                    if target_code.is_none() && t.equivalence.value.as_deref() == Some("unmatched")
                    {
                        no_map = true;
                        continue;
                    }
                    targets.push(Target {
                        relationship: super::relationship(
                            Relationship::from_equivalence,
                            code.as_deref().unwrap_or_default(),
                            target_code.as_deref().unwrap_or_default(),
                            t.equivalence.value.as_deref().unwrap_or_default(),
                        )?,
                        code: target_code,
                        display: text(t.display.as_ref().and_then(|s| s.value.as_deref())),
                        comment: text(t.comment.as_ref().and_then(|s| s.value.as_deref())),
                        depends_on: t.depends_on.iter().map(depends_on).collect(),
                        product: t.product.iter().map(depends_on).collect(),
                    });
                }
                Ok(Element {
                    no_map: no_map || targets.is_empty(),
                    code,
                    display: text(e.display.as_ref().and_then(|s| s.value.as_deref())),
                    comment: e
                        .extension
                        .iter()
                        .find(|x| x.url == ELEMENT_COMMENT)
                        .and_then(|x| match &x.value {
                            Some(ExtensionValue::String(s)) => text(s.value.as_deref()),
                            _ => None,
                        }),
                    targets,
                })
            }

            fn unmapped(u: &ConceptMapGroupUnmapped) -> Result<Unmapped, ModelError> {
                let mode = u.mode.value.as_deref().unwrap_or_default();
                Ok(Unmapped {
                    mode: UnmappedMode::parse(mode)
                        .ok_or_else(|| ModelError::UnmappedMode(mode.to_owned()))?,
                    code: text(u.code.as_ref().and_then(|c| c.value.as_deref())),
                    display: text(u.display.as_ref().and_then(|s| s.value.as_deref())),
                    relationship: None,
                    other_map: text(u.url.as_ref().and_then(|c| c.value.as_deref())),
                })
            }

            fn group(g: &ConceptMapGroup) -> Result<Group, ModelError> {
                let mut elements = Vec::with_capacity(g.element.len());
                for e in &g.element {
                    elements.push(element(e)?);
                }
                Ok(Group {
                    source: text(g.source.as_ref().and_then(|s| s.value.as_deref())),
                    source_version: text(
                        g.source_version.as_ref().and_then(|s| s.value.as_deref()),
                    ),
                    target: text(g.target.as_ref().and_then(|s| s.value.as_deref())),
                    target_version: text(
                        g.target_version.as_ref().and_then(|s| s.value.as_deref()),
                    ),
                    elements,
                    unmapped: g.unmapped.as_ref().map(unmapped).transpose()?,
                })
            }

            /// Reduces a `ConceptMap` of this FHIR version to the model.
            ///
            /// # Errors
            ///
            /// Returns [`ModelError`] for a resource without a `url`, an unknown
            /// `equivalence`, or an unknown `unmapped.mode`.
            pub fn convert(resource: &ConceptMap) -> Result<ConceptMapModel, ModelError> {
                let mut groups = Vec::with_capacity(resource.group.len());
                for g in &resource.group {
                    groups.push(group(g)?);
                }
                let scope_uri = |s: Option<&str>| text(s);
                Ok(ConceptMapModel {
                    id: resource.id.clone(),
                    url: text(resource.url.as_ref().and_then(|u| u.value.as_deref()))
                        .ok_or(ModelError::NoUrl)?,
                    version: text(resource.version.as_ref().and_then(|v| v.value.as_deref())),
                    name: text(resource.name.as_ref().and_then(|v| v.value.as_deref())),
                    title: text(resource.title.as_ref().and_then(|v| v.value.as_deref())),
                    status: text(resource.status.value.as_deref()).unwrap_or_default(),
                    experimental: resource.experimental.as_ref().and_then(|b| b.value),
                    date: text(resource.date.as_ref().and_then(|d| d.value.as_deref())),
                    publisher: text(resource.publisher.as_ref().and_then(|p| p.value.as_deref())),
                    description: text(
                        resource
                            .description
                            .as_ref()
                            .and_then(|d| d.value.as_deref()),
                    ),
                    source_scope: resource.source.as_ref().and_then(|s| match s {
                        ConceptMapSource::Uri(u) => scope_uri(u.value.as_deref()),
                        ConceptMapSource::Canonical(c) => scope_uri(c.value.as_deref()),
                    }),
                    target_scope: resource.target.as_ref().and_then(|t| match t {
                        ConceptMapTarget::Uri(u) => scope_uri(u.value.as_deref()),
                        ConceptMapTarget::Canonical(c) => scope_uri(c.value.as_deref()),
                    }),
                    groups,
                })
            }
        }
    };
    // The R5 family: `relationship`, canonical `source`, `dependsOn.attribute`, `noMap`, `otherMap`.
    ($module:ident, r5, $native_comment:expr) => {
        /// The conversion for this FHIR version's `ConceptMap`.
        pub mod $module {
            use fhir_types::$module::concept_map::{
                ConceptMap, ConceptMapGroup, ConceptMapGroupElement,
                ConceptMapGroupElementTargetDependsOn, ConceptMapGroupElementTargetDependsOnValue,
                ConceptMapGroupUnmapped, ConceptMapSourceScope, ConceptMapTargetScope,
            };
            use fhir_types::$module::extension::ExtensionValue;

            use super::super::model::{
                ConceptMapModel, DependsOn, Element, Group, ModelError, Relationship, Target,
                Unmapped, UnmappedMode,
            };
            use super::{ELEMENT_COMMENT, split_canonical, text};

            fn depends_on(d: &ConceptMapGroupElementTargetDependsOn) -> DependsOn {
                let (system, value, display) = match &d.value {
                    Some(ConceptMapGroupElementTargetDependsOnValue::Code(c)) => {
                        (None, text(c.value.as_deref()), None)
                    }
                    Some(ConceptMapGroupElementTargetDependsOnValue::Coding(c)) => (
                        text(c.system.as_ref().and_then(|s| s.value.as_deref())),
                        text(c.code.as_ref().and_then(|s| s.value.as_deref())),
                        text(c.display.as_ref().and_then(|s| s.value.as_deref())),
                    ),
                    Some(ConceptMapGroupElementTargetDependsOnValue::String(s)) => {
                        (None, text(s.value.as_deref()), None)
                    }
                    Some(ConceptMapGroupElementTargetDependsOnValue::Boolean(b)) => {
                        (None, b.value.map(|b| b.to_string()), None)
                    }
                    Some(ConceptMapGroupElementTargetDependsOnValue::Quantity(q)) => (
                        None,
                        text(q.value.as_ref().and_then(|v| v.value.as_deref())),
                        None,
                    ),
                    None => (
                        None,
                        text(d.value_set.as_ref().and_then(|v| v.value.as_deref())),
                        None,
                    ),
                };
                DependsOn {
                    attribute: text(d.attribute.value.as_deref()).unwrap_or_default(),
                    system,
                    value: value.unwrap_or_default(),
                    display,
                }
            }

            fn element(e: &ConceptMapGroupElement) -> Result<Element, ModelError> {
                let code = text(e.code.as_ref().and_then(|c| c.value.as_deref()));
                let mut targets = Vec::with_capacity(e.target.len());
                for t in &e.target {
                    let target_code = text(t.code.as_ref().and_then(|c| c.value.as_deref()));
                    targets.push(Target {
                        relationship: super::relationship(
                            Relationship::from_relationship,
                            code.as_deref().unwrap_or_default(),
                            target_code.as_deref().unwrap_or_default(),
                            t.relationship.value.as_deref().unwrap_or_default(),
                        )?,
                        code: target_code,
                        display: text(t.display.as_ref().and_then(|s| s.value.as_deref())),
                        comment: text(t.comment.as_ref().and_then(|s| s.value.as_deref())),
                        depends_on: t.depends_on.iter().map(depends_on).collect(),
                        product: t.product.iter().map(depends_on).collect(),
                    });
                }
                Ok(Element {
                    no_map: e.no_map.as_ref().and_then(|b| b.value).unwrap_or(false)
                        || targets.is_empty(),
                    code,
                    display: text(e.display.as_ref().and_then(|s| s.value.as_deref())),
                    // A version that declares the field carries it there; one
                    // that does not carries the extension, so a document
                    // written either way reads the same
                    // (<https://hl7.org/fhir/6.0.0-ballot5/conceptmap-definitions.html>).
                    comment: ($native_comment)(e).or_else(|| {
                        e.extension
                            .iter()
                            .find(|x| x.url == ELEMENT_COMMENT)
                            .and_then(|x| match &x.value {
                                Some(ExtensionValue::String(s)) => text(s.value.as_deref()),
                                _ => None,
                            })
                    }),
                    targets,
                })
            }

            fn unmapped(u: &ConceptMapGroupUnmapped) -> Result<Unmapped, ModelError> {
                let mode = u.mode.value.as_deref().unwrap_or_default();
                Ok(Unmapped {
                    mode: UnmappedMode::parse(mode)
                        .ok_or_else(|| ModelError::UnmappedMode(mode.to_owned()))?,
                    code: text(u.code.as_ref().and_then(|c| c.value.as_deref())),
                    display: text(u.display.as_ref().and_then(|s| s.value.as_deref())),
                    relationship: u
                        .relationship
                        .as_ref()
                        .and_then(|r| r.value.as_deref())
                        .and_then(Relationship::from_relationship),
                    other_map: text(u.other_map.as_ref().and_then(|c| c.value.as_deref())),
                })
            }

            fn group(g: &ConceptMapGroup) -> Result<Group, ModelError> {
                let mut elements = Vec::with_capacity(g.element.len());
                for e in &g.element {
                    elements.push(element(e)?);
                }
                let (source, source_version) =
                    split_canonical(text(g.source.as_ref().and_then(|s| s.value.as_deref())));
                let (target, target_version) =
                    split_canonical(text(g.target.as_ref().and_then(|s| s.value.as_deref())));
                Ok(Group {
                    source,
                    source_version,
                    target,
                    target_version,
                    elements,
                    unmapped: g.unmapped.as_ref().map(unmapped).transpose()?,
                })
            }

            /// Reduces a `ConceptMap` of this FHIR version to the model.
            ///
            /// # Errors
            ///
            /// Returns [`ModelError`] for a resource without a `url`, an unknown
            /// `relationship`, or an unknown `unmapped.mode`.
            pub fn convert(resource: &ConceptMap) -> Result<ConceptMapModel, ModelError> {
                let mut groups = Vec::with_capacity(resource.group.len());
                for g in &resource.group {
                    groups.push(group(g)?);
                }
                Ok(ConceptMapModel {
                    id: resource.id.clone(),
                    url: text(resource.url.as_ref().and_then(|u| u.value.as_deref()))
                        .ok_or(ModelError::NoUrl)?,
                    version: text(resource.version.as_ref().and_then(|v| v.value.as_deref())),
                    name: text(resource.name.as_ref().and_then(|v| v.value.as_deref())),
                    title: text(resource.title.as_ref().and_then(|v| v.value.as_deref())),
                    status: text(resource.status.value.as_deref()).unwrap_or_default(),
                    experimental: resource.experimental.as_ref().and_then(|b| b.value),
                    date: text(resource.date.as_ref().and_then(|d| d.value.as_deref())),
                    publisher: text(resource.publisher.as_ref().and_then(|p| p.value.as_deref())),
                    description: text(
                        resource
                            .description
                            .as_ref()
                            .and_then(|d| d.value.as_deref()),
                    ),
                    source_scope: resource.source_scope.as_ref().and_then(|s| match s {
                        ConceptMapSourceScope::Uri(u) => text(u.value.as_deref()),
                        ConceptMapSourceScope::Canonical(c) => text(c.value.as_deref()),
                    }),
                    target_scope: resource.target_scope.as_ref().and_then(|t| match t {
                        ConceptMapTargetScope::Uri(u) => text(u.value.as_deref()),
                        ConceptMapTargetScope::Canonical(c) => text(c.value.as_deref()),
                    }),
                    groups,
                })
            }
        }
    };
}

convert_concept_map!(r4, r4);
convert_concept_map!(r4b, r4);
convert_concept_map!(r5, r5, |_: &ConceptMapGroupElement| None);
convert_concept_map!(r6, r5, |e: &ConceptMapGroupElement| text(
    e.comment.as_ref().and_then(|c| c.value.as_deref()),
));
