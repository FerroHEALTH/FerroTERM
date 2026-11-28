// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The postcoordination expressions of an ICD-11 linearization: each
//! qualifier bound to an axis of the code it qualifies, the normal form, and
//! the display, code and properties of the validated expression.
//!
//! The contract is the HL7 terminology ecosystem IG's ICD-11 page
//! (`HL7/fhir-tx-ecosystem-ig` at `9c2bbce`, `input/pagecontent/icd-11.md`,
//! Post Coordination Expressions and Properties). The axis a code binds to is
//! the suite's rule (`tests/icd-11/doco.txt` at `9c2bbce`, items 12b and "How
//! an expression is read"): an axis with no value first, a required one
//! before WHO's order, the codes taken in entity id order.

use std::sync::PoisonError;

use ::icd11::SCHEMA;
use ::icd11::expression::{Expression, Group as Written};
use concept_graph::ordinal::Ordinal;

use crate::provider::{
    CodeSystemProvider, Concept, Located, Property, PropertyValue, Subproperty,
    error::ProviderError,
};

use super::{Icd11Provider, Scale, axis_short};

/// The `allowMultipleValues` of an axis that takes one value.
const NOT_ALLOWED: &str = "NotAllowed";

/// One code with its qualifiers bound to its axes, in normal order.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Group {
    code: u32,
    qualifiers: Vec<Bound>,
}

/// A code on an axis of the code it qualifies, the axis an index into that
/// code's scales; a `/` member on no axis of the root has none.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Bound {
    axis: Option<usize>,
    group: Group,
}

/// A code and its qualifiers as written.
type AsWritten = (u32, Vec<u32>);

/// A validated postcoordination expression.
#[derive(Debug, Clone)]
pub(super) struct Cluster {
    /// The root with its `&` qualifiers.
    root: Group,
    /// The `/` members, bound to the root's axes.
    members: Vec<Bound>,
    /// Each code with its qualifiers as written, the root first.
    written: Vec<AsWritten>,
    /// Whether the qualifiers are written with ICF's `.`.
    dotted: bool,
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

    /// The axis of `code` that `value` binds to, given the axes already
    /// carrying a value: an axis with none first, a required one before
    /// WHO's order.
    fn bind(&self, code: u32, value: u32, filled: &[usize]) -> Option<usize> {
        let scales = self.system().scales.get(&code)?;
        let candidates: Vec<(usize, &Scale)> = scales
            .iter()
            .enumerate()
            .filter(|(_, scale)| self.fits(scale, value))
            .collect();
        let first = |open: bool, required: bool| {
            candidates
                .iter()
                .find(|(axis, scale)| {
                    (!open || !filled.contains(axis)) && (!required || scale.required)
                })
                .map(|(axis, _)| *axis)
        };
        first(true, true)
            .or_else(|| first(true, false))
            .or_else(|| first(false, true))
            .or_else(|| first(false, false))
    }

    /// The entity id of a group and the group in entity ids: the order a
    /// group sorts in after its axis, so the same member with other
    /// qualifiers stays apart.
    fn sort_key(&self, group: &Group) -> (String, String) {
        (
            self.code_of(group.code),
            spell_group(group, '&', &|o| Some(self.code_of(o))).unwrap_or_default(),
        )
    }

    /// Binds each of `values` to an axis of `code` in entity id order, the
    /// axes in `filled` already carrying a value, then puts them in the
    /// normal order; a duplicate is deleted and a value on no axis has none.
    fn bind_all(&self, code: u32, mut values: Vec<Group>, mut filled: Vec<usize>) -> Vec<Bound> {
        values.sort_by_cached_key(|group| self.sort_key(group));
        values.dedup();
        let mut bound = Vec::with_capacity(values.len());
        for group in values {
            let axis = self.bind(code, group.code, &filled);
            filled.extend(axis);
            bound.push(Bound { axis, group });
        }
        bound.sort_by_cached_key(|b| (b.axis.unwrap_or(usize::MAX), self.sort_key(&b.group)));
        bound
    }

    /// One written code and its qualifiers, resolved, with each qualifier
    /// bound to an axis of the code; `None` when the code, or an ICF dotted
    /// qualifier, names no concept.
    fn group(
        &self,
        text: &str,
        written: &Written,
        dotted: bool,
    ) -> Result<Option<(Group, AsWritten)>, ProviderError> {
        let invalid = |reason: String| ProviderError::InvalidCode {
            code: text.to_owned(),
            reason,
        };
        let Some(code) = self.resolve(&written.code)? else {
            return Ok(None);
        };
        let mut values = Vec::with_capacity(written.qualifiers.len());
        for token in &written.qualifiers {
            let Some(value) = self.resolve(token)? else {
                // NOTE: an ICF dotted qualifier that is no code (`d5409.3`) is an unknown
                // code (the ecosystem's `lookup-icf-pc-old`), not a malformed expression.
                if dotted {
                    return Ok(None);
                }
                return Err(invalid(format!("`{token}` is not a code")));
            };
            values.push(value);
        }
        let single = |&value: &u32| Group {
            code: value,
            qualifiers: Vec::new(),
        };
        let qualifiers = self.bind_all(code, values.iter().map(single).collect(), Vec::new());
        if let Some(stray) = qualifiers.iter().find(|bound| bound.axis.is_none()) {
            let token = values
                .iter()
                .position(|&value| value == stray.group.code)
                .and_then(|at| written.qualifiers.get(at))
                .map_or("", String::as_str);
            return Err(invalid(format!(
                "`{token}` is on no postcoordination axis of `{}`",
                written.code
            )));
        }
        Ok(Some((Group { code, qualifiers }, (code, values))))
    }

    /// Validates the expression `text`, interning its normal form when it holds.
    pub(super) fn cluster(
        &self,
        text: &str,
        expression: &Expression,
    ) -> Result<Option<Located>, ProviderError> {
        let Some((root, root_written)) = self.group(text, &expression.root, expression.dotted)?
        else {
            return Ok(None);
        };
        let mut written = vec![root_written];
        let mut members = Vec::with_capacity(expression.members.len());
        for member in &expression.members {
            let Some((group, as_written)) = self.group(text, member, expression.dotted)? else {
                return Ok(None);
            };
            members.push(group);
            written.push(as_written);
        }
        let filled = root.qualifiers.iter().filter_map(|b| b.axis).collect();
        let members = self.bind_all(root.code, members, filled);
        let cluster = Cluster {
            root,
            members,
            written,
            dotted: expression.dotted,
        };
        let code = self.cluster_code(&cluster);
        let index = self.system().expressions.intern(&code)?.index();
        let mut clusters = self
            .system()
            .clusters
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = Self::at(index) {
            if slot == clusters.len() {
                clusters.push(cluster);
            } else if let Some(existing) = clusters.get_mut(slot) {
                *existing = cluster;
            }
        }
        Ok(Some(Located {
            concept: Concept::new(self.stored().concepts.saturating_add(index)),
            code,
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

    /// The display of an expression as written, duplicates kept: each code's
    /// title with its qualifiers' titles in brackets, the members joined by
    /// ` / ` (`icd-11.md`, Display Terms).
    pub(super) fn cluster_display(
        &self,
        cluster: &Cluster,
        language: Option<&str>,
    ) -> Result<String, ProviderError> {
        let mut parts = Vec::with_capacity(cluster.written.len());
        for (code, qualifiers) in &cluster.written {
            let mut part = self.title(*code, language)?.unwrap_or_default();
            for &qualifier in qualifiers {
                part.push_str(" [");
                part.push_str(&self.title(qualifier, language)?.unwrap_or_default());
                part.push(']');
            }
            parts.push(part);
        }
        Ok(parts.join(" / "))
    }

    /// The normal form of an expression, in entity ids.
    fn cluster_code(&self, cluster: &Cluster) -> String {
        spell(cluster, &|ordinal| Some(self.code_of(ordinal))).unwrap_or_default()
    }

    /// The normal form in short codes, when every code has one.
    fn cluster_classification_code(&self, cluster: &Cluster) -> Option<String> {
        spell(cluster, &|ordinal| self.classification_code(ordinal))
    }

    /// A code of this system as a `Coding`, with its title.
    fn coding(&self, ordinal: u32, language: Option<&str>) -> Result<PropertyValue, ProviderError> {
        Ok(PropertyValue::Coding {
            system: self.identity().url.clone(),
            code: self.code_of(ordinal),
            display: self.title(ordinal, language)?,
        })
    }

    /// The `value` subproperties of `bounds`.
    fn values<'a>(
        &self,
        bounds: impl Iterator<Item = &'a Bound>,
        language: Option<&str>,
    ) -> Result<Vec<Subproperty>, ProviderError> {
        // TODO(#1391): a member's own qualifiers as `postcoordination` subproperties inside
        // its `value`, once fhir-types carries a subproperty inside a subproperty.
        bounds
            .map(|bound| {
                Ok(Subproperty {
                    code: String::from("value"),
                    value: self.coding(bound.group.code, language)?,
                    description: None,
                })
            })
            .collect()
    }

    /// `code`, `classificationCode`, `stem`, and one `postcoordination`
    /// property per axis of the root that has a value, then one for the
    /// members on no axis of the root (`icd-11.md`, Properties).
    pub(super) fn cluster_properties(
        &self,
        cluster: &Cluster,
        language: Option<&str>,
    ) -> Result<Vec<Property>, ProviderError> {
        let mut out = vec![Property {
            code: String::from("code"),
            value: Some(PropertyValue::Code(self.cluster_code(cluster))),
            ..Property::default()
        }];
        if let Some(code) = self.cluster_classification_code(cluster) {
            out.push(Property {
                code: String::from("classificationCode"),
                value: Some(PropertyValue::Code(code)),
                ..Property::default()
            });
        }
        out.push(Property {
            code: String::from("stem"),
            value: Some(self.coding(cluster.root.code, language)?),
            ..Property::default()
        });
        let on_root = |axis: Option<usize>| {
            cluster
                .root
                .qualifiers
                .iter()
                .chain(&cluster.members)
                .filter(move |bound| bound.axis == axis)
        };
        let scales = self.system().scales.get(&cluster.root.code);
        for (index, scale) in scales.into_iter().flatten().enumerate() {
            if on_root(Some(index)).next().is_none() {
                continue;
            }
            let mut subproperties = vec![Subproperty {
                code: String::from("axis"),
                value: axis_coding(&scale.axis),
                description: None,
            }];
            subproperties.extend(self.values(on_root(Some(index)), language)?);
            out.push(Property {
                code: String::from("postcoordination"),
                subproperties,
                ..Property::default()
            });
        }
        if on_root(None).next().is_some() {
            out.push(Property {
                code: String::from("postcoordination"),
                subproperties: self.values(on_root(None), language)?,
                ..Property::default()
            });
        }
        Ok(out)
    }

    /// The axes that take one value and are given several, each a warning.
    // NOTE: tx-ecosystem `cs-validate-nf-axis-not-multiple` at 9c2bbce: several values on an
    // axis that takes one is still valid and in normal form, with a warning naming the axis.
    pub(super) fn cluster_advisories(&self, cluster: &Cluster) -> Vec<String> {
        let mut out = Vec::new();
        let root: Vec<&Bound> = cluster
            .root
            .qualifiers
            .iter()
            .chain(&cluster.members)
            .collect();
        self.crowded(cluster.root.code, &root, &mut out);
        for member in &cluster.members {
            let qualifiers: Vec<&Bound> = member.group.qualifiers.iter().collect();
            self.crowded(member.group.code, &qualifiers, &mut out);
        }
        out
    }

    /// Pushes a warning for each axis of `code` that takes one value and
    /// carries more than one of `bounds`.
    fn crowded(&self, code: u32, bounds: &[&Bound], out: &mut Vec<String>) {
        let scales = self.system().scales.get(&code);
        for (index, scale) in scales.into_iter().flatten().enumerate() {
            let count = bounds.iter().filter(|b| b.axis == Some(index)).count();
            if scale.multiple == NOT_ALLOWED && count > 1 {
                out.push(format!(
                    "The postcoordination axis '{}' of '{}' takes one value, and is given {count}",
                    axis_short(&scale.axis),
                    self.code_of(code)
                ));
            }
        }
    }
}

/// The normal form of `cluster` written with `name`, `None` when a code has
/// no name.
fn spell(cluster: &Cluster, name: &dyn Fn(u32) -> Option<String>) -> Option<String> {
    let qualifier = if cluster.dotted { '.' } else { '&' };
    let mut text = spell_group(&cluster.root, qualifier, name)?;
    for member in &cluster.members {
        text.push('/');
        text.push_str(&spell_group(&member.group, qualifier, name)?);
    }
    Some(text)
}

/// One group written with `name`, each qualifier after `qualifier`.
fn spell_group(
    group: &Group,
    qualifier: char,
    name: &dyn Fn(u32) -> Option<String>,
) -> Option<String> {
    let mut text = name(group.code)?;
    for bound in &group.qualifiers {
        text.push(qualifier);
        text.push_str(&name(bound.group.code)?);
    }
    Some(text)
}

/// An axis as a `Coding` in the ICD schema, its display read from its name.
fn axis_coding(axis: &str) -> PropertyValue {
    let short = axis_short(axis);
    PropertyValue::Coding {
        system: SCHEMA.trim_end_matches('/').to_owned(),
        code: short.to_owned(),
        display: Some(axis_display(short)),
    }
}

/// An axis name in sentence case: `hasAlternativeSeverity1` is "Has
/// alternative severity 1".
// NOTE: no specification governs this: our own design; the ICD-API names an axis by its
// schema URI only, and the ecosystem's `icd-11` cases expect this display.
fn axis_display(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len().saturating_add(4));
    for (at, &c) in chars.iter().enumerate() {
        let before = at.checked_sub(1).and_then(|b| chars.get(b));
        let after = chars.get(at.saturating_add(1));
        let upper_before = before.is_some_and(|b| b.is_uppercase());
        let lower_after = after.is_some_and(|a| a.is_lowercase());
        let acronym = c.is_uppercase()
            && (after.is_some_and(|a| a.is_uppercase()) || (upper_before && !lower_after));
        let word = before.is_some_and(|&b| {
            (c.is_uppercase() && !b.is_uppercase())
                || (c.is_ascii_digit() != b.is_ascii_digit())
                || (c.is_uppercase() && upper_before && lower_after)
        });
        if word {
            out.push(' ');
        }
        if out.is_empty() {
            out.extend(c.to_uppercase());
        } else if c.is_uppercase() && !acronym {
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::axis_display;

    #[test]
    fn an_axis_name_reads_in_sentence_case() {
        assert_eq!(axis_display("laterality"), "Laterality");
        assert_eq!(axis_display("specificAnatomy"), "Specific anatomy");
        assert_eq!(
            axis_display("objectOrSubstanceProducingInjury"),
            "Object or substance producing injury"
        );
        assert_eq!(
            axis_display("hasAlternativeSeverity1"),
            "Has alternative severity 1"
        );
        assert_eq!(axis_display("typeOfICDCode"), "Type of ICD code");
    }
}
