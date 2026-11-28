// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The concept model check of a post-coordinated expression, over the MRCM
//! reference sets the edition carries.
//!
//! The process is the one the MRCM specification gives for postcoordinated
//! expressions (SNOMED CT Machine Readable Concept Model, §6 Considerations,
//! "Expressions, Constraints and Queries"): find the domains of the focus
//! concepts, check each attribute's domain, grouping and cardinality against
//! the MRCM attribute domain reference set, and its value against the range of
//! the MRCM attribute range reference set. Only rules whose content type is
//! `<< 723595009 |All postcoordinated SNOMED CT content|` apply, a mandatory
//! rule's violation is an error and an optional rule's a warning.

mod range;

use std::collections::BTreeMap;

use concept_graph::ordinal::Ordinal;
use concept_graph::refsets::{Table, ValueRef};

use sct_scg::ast;

use super::{Attribute, Term, Value};
use crate::provider::error::ProviderError;
use crate::snomed::SnomedProvider;
use crate::stored::storage;

/// `723604009 |MRCM attribute domain reference set|`.
const ATTRIBUTE_DOMAIN: u64 = 723_604_009;
/// `723592007 |MRCM attribute range reference set|`.
const ATTRIBUTE_RANGE: u64 = 723_592_007;
/// `723589008 |MRCM domain reference set|`.
const DOMAIN: u64 = 723_589_008;
/// `723595009 |All postcoordinated SNOMED CT content|`.
const POSTCOORDINATED: u64 = 723_595_009;
/// `723597001 |Mandatory concept model rule|`.
const MANDATORY: u64 = 723_597_001;
/// `410662002 |Concept model attribute|`.
const CONCEPT_MODEL_ATTRIBUTE: u64 = 410_662_002;

/// Whether breaking a rule is an error or a warning (MRCM §5.2, `ruleStrengthId`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strength {
    Mandatory,
    Optional,
}

/// The concepts that decide whether a rule applies, resolved once per compile.
#[derive(Debug, Clone, Copy)]
struct Scope {
    postcoordinated: Option<Ordinal>,
    mandatory: Option<Ordinal>,
}

/// One row of the attribute domain reference set that applies to
/// postcoordinated content.
#[derive(Debug, Clone)]
struct DomainRule {
    /// The domain concept, `domainId`.
    domain: Ordinal,
    /// Whether the attribute must be in a relationship group, `grouped`.
    grouped: bool,
    /// The most times the attribute may appear, `attributeCardinality`.
    most: Option<u32>,
    /// The most times it may appear in one group, `attributeInGroupCardinality`.
    most_in_group: Option<u32>,
    strength: Strength,
}

/// One row of the attribute range reference set that applies to
/// postcoordinated content.
#[derive(Debug, Clone)]
struct RangeRule {
    /// The `rangeConstraint` as written.
    constraint: String,
    strength: Strength,
}

/// The rules of one attribute.
#[derive(Debug, Clone, Default)]
struct Rules {
    domains: Vec<DomainRule>,
    ranges: Vec<RangeRule>,
}

/// The concept model of one edition, compiled once from its MRCM reference
/// sets; empty when the edition carries none, and then nothing is checked.
#[derive(Debug, Default)]
pub(in crate::snomed) struct Mrcm {
    /// The rules by attribute concept.
    attributes: BTreeMap<u32, Rules>,
    /// The `domainConstraint` of each domain the MRCM domain reference set lists.
    domains: BTreeMap<u32, String>,
}

/// An expression's breach of the concept model.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Violation {
    /// The attribute is not a concept model attribute (RF2 §4.2.3, `typeId`).
    #[error("`{attribute}` is not a concept model attribute (`<< {CONCEPT_MODEL_ATTRIBUTE}`)")]
    NotAnAttribute {
        /// The attribute as written.
        attribute: String,
    },
    /// No domain of the attribute holds a focus concept.
    #[error("attribute `{attribute}` is not valid on `{focus}`; its domains are {domains}")]
    Domain {
        /// The attribute.
        attribute: String,
        /// The focus concepts, `+`-joined.
        focus: String,
        /// The domains, `|`-joined.
        domains: String,
    },
    /// The value is outside the attribute's range.
    #[error("`{value}` is outside the range `{range}` of attribute `{attribute}`")]
    Range {
        /// The attribute.
        attribute: String,
        /// The value as written.
        value: String,
        /// The range constraints, ` OR `-joined.
        range: String,
    },
    /// The attribute appears more often than its cardinality allows.
    #[error("attribute `{attribute}` appears {count} times where its cardinality allows {most}")]
    Cardinality {
        /// The attribute.
        attribute: String,
        /// How often the expression states it, in total or in one group.
        count: usize,
        /// The most the concept model allows there.
        most: u32,
    },
    /// An ungrouped attribute is written inside a relationship group.
    #[error(
        "attribute `{attribute}` is ungrouped in the concept model and is written in a relationship group"
    )]
    Grouped {
        /// The attribute.
        attribute: String,
    },
}

/// What the check found: the mandatory rules broken, which make the expression
/// invalid, and the optional ones, which are warnings.
#[derive(Debug, Default)]
pub(in crate::snomed) struct Findings {
    pub(in crate::snomed) errors: Vec<Violation>,
    pub(in crate::snomed) warnings: Vec<Violation>,
}

impl Findings {
    fn add(&mut self, strength: Strength, violation: Violation) {
        match strength {
            Strength::Mandatory => self.errors.push(violation),
            Strength::Optional => self.warnings.push(violation),
        }
    }
}

/// The violations as one sentence, `; `-separated.
pub(super) fn listed(violations: &[Violation]) -> String {
    violations
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

/// The upper bound of an MRCM cardinality (`0..1`, `1..*`); `None` for `*`.
fn most(cardinality: &str) -> Option<u32> {
    cardinality.split_once("..")?.1.trim().parse().ok()
}

impl SnomedProvider {
    /// The edition's concept model, compiled on first use.
    fn mrcm(&self) -> Result<&Mrcm, ProviderError> {
        if let Some(mrcm) = self.system().mrcm.get() {
            return Ok(mrcm);
        }
        let compiled = self.compile_mrcm()?;
        Ok(self.system().mrcm.get_or_init(|| compiled))
    }

    /// Whether `concept` is `ancestor` or below it.
    fn is_a(&self, concept: Ordinal, ancestor: Ordinal) -> bool {
        concept == ancestor
            || self
                .stored()
                .tree
                .graph
                .closure
                .ancestors(concept)
                .contains(ancestor.index())
    }

    /// The ordinal of a concept by its SCTID.
    fn ordinal_of(&self, sctid: u64) -> Result<Option<Ordinal>, ProviderError> {
        self.stored()
            .store
            .ordinal(&sctid.to_string())
            .map_err(storage)
    }

    /// Whether the concept SCTID `sctid` is `<< ancestor`.
    fn sctid_is_a(&self, sctid: u64, ancestor: Option<Ordinal>) -> Result<bool, ProviderError> {
        let (Some(ancestor), Some(concept)) = (ancestor, self.ordinal_of(sctid)?) else {
            return Ok(false);
        };
        Ok(self.is_a(concept, ancestor))
    }

    fn compile_mrcm(&self) -> Result<Mrcm, ProviderError> {
        let attribute_domain = self.ordinal_of(ATTRIBUTE_DOMAIN)?;
        let attribute_range = self.ordinal_of(ATTRIBUTE_RANGE)?;
        let domain = self.ordinal_of(DOMAIN)?;
        let scope = Scope {
            postcoordinated: self.ordinal_of(POSTCOORDINATED)?,
            mandatory: self.ordinal_of(MANDATORY)?,
        };
        let mut mrcm = Mrcm::default();
        let tables = &self.system().member_tables;
        for refset in tables.refsets() {
            let Some(table) = tables.table(refset) else {
                continue;
            };
            if self.sctid_is_a(refset, attribute_domain)? {
                self.read_domain_rules(table, scope, &mut mrcm);
            } else if self.sctid_is_a(refset, attribute_range)? {
                self.read_range_rules(table, scope, &mut mrcm);
            } else if self.sctid_is_a(refset, domain)? {
                read_domains(table, &mut mrcm);
            }
        }
        Ok(mrcm)
    }

    /// The strength of `row` when its content type is postcoordinated content.
    fn applicable(&self, table: &Table, row: usize, scope: Scope) -> Option<Strength> {
        let content = table.field("contentTypeId")?;
        let strength = table.field("ruleStrengthId")?;
        let Some(ValueRef::Concept(content)) = table.value(row, content) else {
            return None;
        };
        if !self.is_a(content, scope.postcoordinated?) {
            return None;
        }
        let Some(ValueRef::Concept(strength)) = table.value(row, strength) else {
            return None;
        };
        Some(if Some(strength) == scope.mandatory {
            Strength::Mandatory
        } else {
            Strength::Optional
        })
    }

    fn read_domain_rules(&self, table: &Table, scope: Scope, mrcm: &mut Mrcm) {
        let (Some(domain), Some(grouped), Some(total), Some(in_group)) = (
            table.field("domainId"),
            table.field("grouped"),
            table.field("attributeCardinality"),
            table.field("attributeInGroupCardinality"),
        ) else {
            return;
        };
        for row in 0..table.len() {
            let (Some(attribute), Some(strength)) =
                (table.concept(row), self.applicable(table, row, scope))
            else {
                continue;
            };
            let Some(ValueRef::Concept(domain)) = table.value(row, domain) else {
                continue;
            };
            let text = |field| match table.value(row, field) {
                Some(ValueRef::String(text)) => most(text),
                _ => None,
            };
            mrcm.attributes
                .entry(attribute.index())
                .or_default()
                .domains
                .push(DomainRule {
                    domain,
                    grouped: table.value(row, grouped) == Some(ValueRef::Integer(1)),
                    most: text(total),
                    most_in_group: text(in_group),
                    strength,
                });
        }
    }

    fn read_range_rules(&self, table: &Table, scope: Scope, mrcm: &mut Mrcm) {
        let Some(constraint) = table.field("rangeConstraint") else {
            return;
        };
        for row in 0..table.len() {
            let (Some(attribute), Some(strength)) =
                (table.concept(row), self.applicable(table, row, scope))
            else {
                continue;
            };
            let Some(ValueRef::String(text)) = table.value(row, constraint) else {
                continue;
            };
            mrcm.attributes
                .entry(attribute.index())
                .or_default()
                .ranges
                .push(RangeRule {
                    constraint: text.to_owned(),
                    strength,
                });
        }
    }

    /// The code of an ordinal, for a message.
    fn code_text(&self, ordinal: u32) -> Result<String, ProviderError> {
        Ok(self
            .stored()
            .store
            .code(Ordinal::new(ordinal))
            .map_err(storage)?
            .map_or_else(|| ordinal.to_string(), ToOwned::to_owned))
    }

    /// Moves each loose attribute the concept model groups in the domain of
    /// `sub`'s focus concepts into a relationship group of its own, nested
    /// sub-expressions included.
    ///
    /// This is the "Adding a Self-grouped Attribute" transformation (SNOMED CT
    /// Postcoordination Guide, Transformation Level 1), which the tx-ecosystem
    /// IG requires for every attribute the MRCM marks as grouped, so that
    /// `40468003:363698007=10200004` is `40468003:{363698007=10200004}` "for
    /// validation, rendering and subsumption" (requirements, SNOMED CT). The
    /// guide applies it to an attribute stated once, so one stated more often
    /// stays as written.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] when the substrate fails.
    pub(super) fn self_group(&self, sub: &mut ast::SubExpression) -> Result<(), ProviderError> {
        let Some(refinement) = sub.refinement.as_mut() else {
            return Ok(());
        };
        for attribute in refinement.ungrouped.iter_mut().chain(
            refinement
                .groups
                .iter_mut()
                .flat_map(|g| g.attributes.iter_mut()),
        ) {
            if let ast::AttributeValue::Nested(nested) = &mut attribute.value {
                self.self_group(nested)?;
            }
        }
        if self.mrcm()?.attributes.is_empty() {
            return Ok(());
        }
        let store = &self.stored().store;
        let mut focus = Vec::with_capacity(sub.focus.len());
        for concept in &sub.focus {
            if let Some(ordinal) = store.ordinal(&concept.concept_id).map_err(storage)? {
                focus.push(ordinal.index());
            }
        }
        let term = Term {
            focus,
            ..Term::default()
        };
        let stated = std::mem::take(&mut refinement.ungrouped);
        let named = |name: &str| {
            stated
                .iter()
                .filter(|held| held.name.concept_id == name)
                .count()
        };
        let once: Vec<bool> = stated
            .iter()
            .map(|held| named(&held.name.concept_id) == 1)
            .collect();
        let mut loose = Vec::with_capacity(stated.len());
        let mut grouped = Vec::new();
        for (attribute, once) in stated.into_iter().zip(once) {
            let kind = store.ordinal(&attribute.name.concept_id).map_err(storage)?;
            let self_grouped = match kind {
                Some(kind) => self
                    .matching_domains(&term, kind.index())?
                    .iter()
                    .any(|rule| rule.grouped),
                None => false,
            };
            if self_grouped && once {
                grouped.push(attribute);
            } else {
                loose.push(attribute);
            }
        }
        refinement.ungrouped = loose;
        refinement
            .groups
            .extend(grouped.into_iter().map(|attribute| ast::AttributeGroup {
                attributes: vec![attribute],
            }));
        Ok(())
    }

    /// The concept model findings of `term`, nested values included; none when
    /// the edition carries no MRCM.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] when the substrate fails or a range
    /// constraint of the edition does not parse.
    pub(super) fn concept_model(&self, term: &Term) -> Result<Findings, ProviderError> {
        let mut findings = Findings::default();
        if !self.mrcm()?.attributes.is_empty() {
            self.check_term(term, &mut findings)?;
        }
        Ok(findings)
    }

    fn check_term(&self, term: &Term, findings: &mut Findings) -> Result<(), ProviderError> {
        let attributes = term.ungrouped.iter().map(|a| (a, false));
        let grouped = term.groups.iter().flatten().map(|a| (a, true));
        let mut counted: BTreeMap<u32, usize> = BTreeMap::new();
        for (attribute, in_group) in attributes.chain(grouped) {
            *counted.entry(attribute.kind).or_default() += 1;
            self.check_attribute(term, attribute, in_group, findings)?;
            if let Value::Concept(nested) = &attribute.value {
                self.check_term(nested, findings)?;
            }
        }
        for (kind, count) in counted {
            for rule in self.matching_domains(term, kind)? {
                if let Some(most) = rule
                    .most
                    .filter(|most| usize::try_from(*most).is_ok_and(|m| count > m))
                {
                    findings.add(
                        rule.strength,
                        Violation::Cardinality {
                            attribute: self.code_text(kind)?,
                            count,
                            most,
                        },
                    );
                }
                for group in &term.groups {
                    let in_group = group.iter().filter(|a| a.kind == kind).count();
                    if let Some(most) = rule.most_in_group.filter(|most| {
                        *most > 0 && usize::try_from(*most).is_ok_and(|m| in_group > m)
                    }) {
                        findings.add(
                            rule.strength,
                            Violation::Cardinality {
                                attribute: self.code_text(kind)?,
                                count: in_group,
                                most,
                            },
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// The domain rules of attribute `kind` whose domain holds a focus
    /// concept of `term`.
    fn matching_domains(&self, term: &Term, kind: u32) -> Result<Vec<DomainRule>, ProviderError> {
        let Some(rules) = self.mrcm()?.attributes.get(&kind) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for rule in &rules.domains {
            if self.in_domain(term, rule.domain)? {
                out.push(rule.clone());
            }
        }
        Ok(out)
    }

    /// Whether a focus concept of `term` is in `domain`: by the domain's
    /// `domainConstraint` when the MRCM domain reference set lists it.
    fn in_domain(&self, term: &Term, domain: Ordinal) -> Result<bool, ProviderError> {
        if let Some(constraint) = self.mrcm()?.domains.get(&domain.index()) {
            let set = self.constraint(constraint)?;
            return Ok(term.focus.iter().any(|focus| set.contains(*focus)));
        }
        // NOTE: the domain reference set is absent, so `domainId` stands for its
        // own constraint: a reference set's members, otherwise `<<` the concept
        // (MRCM §6, "tested for subsumption with each domain concept"): our own design.
        let code = self.code_text(domain.index())?;
        if let Some(members) = code
            .parse::<u64>()
            .ok()
            .and_then(|sctid| self.system().memberships.members(sctid))
        {
            return Ok(term.focus.iter().any(|focus| members.contains(*focus)));
        }
        Ok(term
            .focus
            .iter()
            .any(|focus| self.is_a(Ordinal::new(*focus), domain)))
    }

    fn check_attribute(
        &self,
        term: &Term,
        attribute: &Attribute,
        in_group: bool,
        findings: &mut Findings,
    ) -> Result<(), ProviderError> {
        let name = self.code_text(attribute.kind)?;
        let model_attribute = self.ordinal_of(CONCEPT_MODEL_ATTRIBUTE)?;
        if model_attribute.is_some_and(|a| !self.is_a(Ordinal::new(attribute.kind), a)) {
            findings
                .errors
                .push(Violation::NotAnAttribute { attribute: name });
            return Ok(());
        }
        let rules = self
            .mrcm()?
            .attributes
            .get(&attribute.kind)
            .cloned()
            .unwrap_or_default();
        let matching = self.matching_domains(term, attribute.kind)?;
        for strength in [Strength::Mandatory, Strength::Optional] {
            let stated: Vec<&DomainRule> = rules
                .domains
                .iter()
                .filter(|r| r.strength == strength)
                .collect();
            let met = matching.iter().any(|r| r.strength == strength);
            // NOTE: an attribute the MRCM does not list has no domain, so no
            // expression may use it (MRCM §6, step 2).
            let unlisted = strength == Strength::Mandatory && rules.domains.is_empty();
            if (!stated.is_empty() && !met) || unlisted {
                let mut domains = Vec::with_capacity(stated.len());
                for rule in &stated {
                    domains.push(self.code_text(rule.domain.index())?);
                }
                let mut focus = Vec::with_capacity(term.focus.len());
                for concept in &term.focus {
                    focus.push(self.code_text(*concept)?);
                }
                findings.add(
                    strength,
                    Violation::Domain {
                        attribute: name.clone(),
                        focus: focus.join("+"),
                        domains: domains.join("|"),
                    },
                );
            }
        }
        if in_group
            && matching
                .iter()
                .any(|r| r.strength == Strength::Mandatory && !r.grouped)
            && !matching.iter().any(|r| r.grouped)
        {
            findings.errors.push(Violation::Grouped {
                attribute: name.clone(),
            });
        }
        self.check_range(&name, attribute, &rules.ranges, findings)
    }

    fn check_range(
        &self,
        name: &str,
        attribute: &Attribute,
        ranges: &[RangeRule],
        findings: &mut Findings,
    ) -> Result<(), ProviderError> {
        for strength in [Strength::Mandatory, Strength::Optional] {
            let stated: Vec<&RangeRule> =
                ranges.iter().filter(|r| r.strength == strength).collect();
            if stated.is_empty() {
                continue;
            }
            let mut held = false;
            for rule in &stated {
                if self.in_range(&rule.constraint, &attribute.value)? {
                    held = true;
                    break;
                }
            }
            if !held {
                let range: Vec<&str> = stated.iter().map(|r| r.constraint.as_str()).collect();
                findings.add(
                    strength,
                    Violation::Range {
                        attribute: name.to_owned(),
                        value: self.value_text(&attribute.value)?,
                        range: range.join(" OR "),
                    },
                );
            }
        }
        Ok(())
    }

    /// Whether `value` is in the range `constraint`: a concrete domain of the
    /// Template Syntax (`int(...)`, `dec(...)`, `str(...)`, `bool(...)`) or an
    /// expression constraint.
    fn in_range(&self, constraint: &str, value: &Value) -> Result<bool, ProviderError> {
        if let Some(concrete) = range::Concrete::parse(constraint) {
            return Ok(match value {
                Value::Number(number) => concrete.holds_number(number),
                Value::Text(text) => concrete.holds_text(text),
                Value::Concept(_) => false,
            });
        }
        let Value::Concept(term) = value else {
            return Ok(false);
        };
        let set = self.constraint(constraint)?;
        // NOTE: a nested value is a subtype of each of its focus concepts
        // (Compositional Grammar §5), so one focus concept in the range holds it.
        Ok(term.focus.iter().any(|focus| set.contains(*focus)))
    }

    fn value_text(&self, value: &Value) -> Result<String, ProviderError> {
        Ok(match value {
            Value::Number(number) => format!("#{number}"),
            Value::Text(text) => format!("\"{text}\""),
            Value::Concept(term) => {
                let mut focus = Vec::with_capacity(term.focus.len());
                for concept in &term.focus {
                    focus.push(self.code_text(*concept)?);
                }
                focus.join("+")
            }
        })
    }
}

/// The `domainConstraint` of each row of an MRCM domain reference set.
fn read_domains(table: &Table, mrcm: &mut Mrcm) {
    let Some(constraint) = table.field("domainConstraint") else {
        return;
    };
    for row in 0..table.len() {
        if let (Some(domain), Some(ValueRef::String(text))) =
            (table.concept(row), table.value(row, constraint))
        {
            mrcm.domains.insert(domain.index(), text.to_owned());
        }
    }
}
