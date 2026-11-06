// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The postcoordination expressions of an ICD-11 linearization: binding each
//! value to an axis of its stem, and the display, code and properties of the
//! validated expression.

use std::sync::PoisonError;

use ::icd11::expression::Expression;
use concept_graph::ordinal::Ordinal;

use crate::provider::{
    Concept, Located, Property, PropertyValue, Subproperty, error::ProviderError,
};

use super::{Icd11Provider, Scale};

/// One value of a postcoordination expression, bound to an axis.
#[derive(Debug, Clone)]
struct Bound {
    axis: String,
    value: u32,
    /// The value token as written.
    text: String,
}

/// One syntactic member of an expression: a stem with its bound values.
#[derive(Debug, Clone)]
struct Member {
    stem: u32,
    values: Vec<Bound>,
}

/// A validated postcoordination expression.
#[derive(Debug, Clone)]
pub(super) struct Cluster {
    members: Vec<Member>,
    /// The members that were read as values of an earlier stem, with the
    /// axis they bound to (index into `members`, then the binding).
    joined: Vec<(usize, Bound)>,
}

impl Cluster {
    /// Each member as written: its stem, and the values written after it,
    /// which a member read as a value of an earlier stem has none of.
    fn written(&self) -> impl Iterator<Item = (u32, Vec<u32>)> + '_ {
        self.members.iter().enumerate().map(|(position, member)| {
            let own = !self.joined.iter().any(|(p, _)| *p == position);
            let values = member
                .values
                .iter()
                .filter(|bound| {
                    own && !self
                        .joined
                        .iter()
                        .any(|(_, b)| b.value == bound.value && b.text == bound.text)
                })
                .map(|bound| bound.value)
                .collect();
            (member.stem, values)
        })
    }
}

impl Icd11Provider {
    /// Whether `value` is on the scale: one of its entities or under one.
    fn fits(&self, scale: &Scale, value: u32) -> bool {
        scale.entities.iter().any(|&e| {
            e == value
                || self
                    .stored()
                    .tree
                    .graph
                    .closure
                    .is_ancestor(Ordinal::new(e), Ordinal::new(value))
        })
    }

    /// The axis `value` binds to on `stem`, given the axes already filled:
    /// an unfilled axis first, then a required one, then WHO's order.
    fn bind(&self, stem: u32, value: u32, filled: &[String]) -> Option<String> {
        let scales = self.system().scales.get(&stem)?;
        let candidates: Vec<&Scale> = scales.iter().filter(|s| self.fits(s, value)).collect();
        let pick = |unfilled_only: bool| {
            candidates
                .iter()
                .filter(|s| !unfilled_only || !filled.contains(&s.axis))
                .find(|s| s.required)
                .or_else(|| {
                    candidates
                        .iter()
                        .find(|s| !unfilled_only || !filled.contains(&s.axis))
                })
                .map(|s| s.axis.clone())
        };
        pick(true).or_else(|| pick(false))
    }

    /// Binds `stem` onto an unfilled postcoordination axis of `last`, when one
    /// takes it: the value, recorded on `last` as well.
    fn join(&self, last: &mut Member, stem: u32, text: &str) -> Option<Bound> {
        let filled: Vec<String> = last.values.iter().map(|b| b.axis.clone()).collect();
        let axis = self.bind(last.stem, stem, &filled)?;
        if filled.contains(&axis) {
            return None;
        }
        let bound = Bound {
            axis,
            value: stem,
            text: text.to_owned(),
        };
        last.values.push(bound.clone());
        Some(bound)
    }

    /// The values `member` binds onto the axes of `stem`, in the order written;
    /// `None` when a dotted qualifier names no code.
    fn bindings(
        &self,
        text: &str,
        expression: &Expression,
        member: &::icd11::expression::Member,
        stem: u32,
    ) -> Result<Option<Vec<Bound>>, ProviderError> {
        let invalid = |reason: String| ProviderError::InvalidCode {
            code: text.to_owned(),
            reason,
        };
        let mut values: Vec<Bound> = Vec::new();
        for token in &member.values {
            let Some(value) = self.resolve(token)? else {
                // NOTE: an ICF dotted qualifier that is no code (`d5409.3`) is an unknown
                // code (the ecosystem's `lookup-icf-pc-old`), not a malformed expression.
                if expression.dotted {
                    return Ok(None);
                }
                return Err(invalid(format!("`{}` is not a code", token.text)));
            };
            let filled: Vec<String> = values.iter().map(|b: &Bound| b.axis.clone()).collect();
            let Some(axis) = self.bind(stem, value, &filled) else {
                return Err(invalid(format!(
                    "`{}` is on no postcoordination axis of `{}`",
                    token.text, member.stem.text
                )));
            };
            values.push(Bound {
                axis,
                value,
                text: token.text.clone(),
            });
        }
        Ok(Some(values))
    }

    /// Validates the expression `text`, interning it when it holds.
    pub(super) fn cluster(
        &self,
        text: &str,
        expression: &Expression,
    ) -> Result<Option<Located>, ProviderError> {
        let mut members: Vec<Member> = Vec::new();
        let mut joined: Vec<(usize, Bound)> = Vec::new();
        let mut last_stem: Option<usize> = None;
        for (position, member) in expression.members.iter().enumerate() {
            let Some(stem) = self.resolve(&member.stem)? else {
                return Ok(None);
            };
            // A `/` member without values that fits an unfilled axis of the
            // stem before it is a value of that stem.
            if position > 0
                && member.values.is_empty()
                && let Some(last) = last_stem.and_then(|i| members.get_mut(i))
                && let Some(bound) = self.join(last, stem, &member.stem.text)
            {
                joined.push((position, bound));
                members.push(Member {
                    stem,
                    values: Vec::new(),
                });
                continue;
            }
            last_stem = Some(members.len());
            let Some(values) = self.bindings(text, expression, member, stem)? else {
                return Ok(None);
            };
            members.push(Member { stem, values });
        }
        let index = self.system().expressions.intern(text)?.index();
        let mut clusters = self
            .system()
            .clusters
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = Self::at(index) {
            if slot == clusters.len() {
                clusters.push(Cluster { members, joined });
            } else if let Some(existing) = clusters.get_mut(slot) {
                *existing = Cluster { members, joined };
            }
        }
        Ok(Some(Located {
            concept: Concept::new(self.stored().concepts.saturating_add(index)),
            code: text.to_owned(),
        }))
    }

    pub(super) fn cluster_of(&self, concept: Concept) -> Option<Cluster> {
        let index = concept.index().checked_sub(self.stored().concepts)?;
        self.system()
            .clusters
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(Self::at(index)?)
            .cloned()
    }

    /// The display of an expression: the members joined by ` / `, each stem
    /// title followed by its `&` values' titles in brackets.
    pub(super) fn cluster_display(
        &self,
        cluster: &Cluster,
        language: Option<&str>,
    ) -> Result<String, ProviderError> {
        let mut parts = Vec::new();
        for (stem, values) in cluster.written() {
            let mut part = self.title(stem, language)?.unwrap_or_default();
            for value in values {
                part.push_str(" [");
                part.push_str(&self.title(value, language)?.unwrap_or_default());
                part.push(']');
            }
            parts.push(part);
        }
        Ok(parts.join(" / "))
    }

    /// The short-code form of an expression.
    fn cluster_code(&self, cluster: &Cluster) -> String {
        Self::cluster_text(cluster, |ordinal| self.code_or_uri(ordinal), "&", "/")
    }

    /// The URI form of an expression: the members' URIs, values joined with ` & `.
    fn cluster_uri(&self, cluster: &Cluster) -> String {
        Self::cluster_text(cluster, |ordinal| self.uri(ordinal), " & ", " / ")
    }

    /// An expression written out with `spell`: each stem followed by its own
    /// values after `and`, the members joined by `then`.
    fn cluster_text(
        cluster: &Cluster,
        spell: impl Fn(u32) -> String,
        and: &str,
        then: &str,
    ) -> String {
        cluster
            .written()
            .map(|(stem, values)| {
                let mut text = spell(stem);
                for value in values {
                    text.push_str(and);
                    text.push_str(&spell(value));
                }
                text
            })
            .collect::<Vec<_>>()
            .join(then)
    }

    pub(super) fn cluster_properties(
        &self,
        cluster: &Cluster,
        language: Option<&str>,
    ) -> Result<Vec<Property>, ProviderError> {
        let mut out = vec![
            Property {
                code: String::from("code"),
                value: PropertyValue::Code(self.cluster_code(cluster)),
                ..Property::default()
            },
            Property {
                code: String::from("id"),
                value: PropertyValue::Code(self.cluster_uri(cluster)),
                ..Property::default()
            },
        ];
        for (position, member) in cluster.members.iter().enumerate() {
            if cluster.joined.iter().any(|(p, _)| *p == position) {
                continue;
            }
            out.push(Property {
                code: String::from("stem"),
                value: PropertyValue::Code(self.code_or_uri(member.stem)),
                subproperties: vec![
                    Subproperty {
                        code: String::from("stemLabel"),
                        value: PropertyValue::String(
                            self.title(member.stem, language)?.unwrap_or_default(),
                        ),
                        description: None,
                    },
                    Subproperty {
                        code: String::from("stemUri"),
                        value: PropertyValue::Uri(self.uri(member.stem)),
                        description: None,
                    },
                ],
                ..Property::default()
            });
            let mut bounds: Vec<&Bound> = member.values.iter().collect();
            bounds.sort_by(|a, b| {
                self.code_or_uri(a.value)
                    .cmp(&self.code_or_uri(b.value))
                    .then_with(|| a.axis.cmp(&b.axis))
            });
            // NOTE: the ecosystem's icd-11 `lookup-pc-simple` keys each bound value's
            // subproperty by the value's code, not by an axis property name.
            for bound in bounds {
                out.push(Property {
                    code: String::from("postcoordinationValues"),
                    value: PropertyValue::Code(bound.axis.clone()),
                    subproperties: vec![Subproperty {
                        code: self.code_or_uri(bound.value),
                        value: PropertyValue::Uri(self.uri(bound.value)),
                        description: self.title(bound.value, language)?,
                    }],
                    ..Property::default()
                });
            }
        }
        Ok(out)
    }
}
