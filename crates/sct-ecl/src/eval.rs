// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The evaluator: an expression constraint to the set of concept ordinals it
//! selects, as set algebra over the materialized closure, the attribute
//! graph, and the reference set tables of one edition.
//!
//! The semantics are the ECL specification's
//! (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-expression-constraint-language>,
//! the quick reference in Appendix D): `<` is the closure's descendants,
//! `^` the active members, a refinement the sources whose attribute rows
//! satisfy the attributes with their cardinalities per role group, and the
//! filters and history supplements restrict or extend the set. The edition
//! is reached through [`Model`]; nothing here walks a graph edge by edge.

mod descriptions;
mod members;
use std::fmt;

use concept_graph::attributes::{Row, ValueRef};
use concept_graph::ordinal::Ordinal;
use roaring::RoaringBitmap;

use self::matching::{
    AttributeTest, CompiledItem, CompiledSet, Kinds, ValueTest, matching_rows, number,
    relationship_groups, set_holds, within,
};
use crate::ast::{
    Acceptability, Attribute, AttributeSet, AttributeValue, Cardinality, Comparison,
    ConstraintOperator, Equality, ExpressionConstraint, FilterConstraint, FocusConcept, Refinement,
    RefsetOperator, Sctid, SubAttributeSet, SubExpressionConstraint, SubRefinement,
    TypedSearchTerm,
};

mod matching;

/// The historical association reference set root
/// (`900000000000522004 |Historical association reference set|`).
pub const HISTORICAL_ASSOCIATION: u64 = 900_000_000_000_522_004;
/// `900000000000527005 |SAME AS association reference set|`.
pub const SAME_AS: u64 = 900_000_000_000_527_005;
/// `900000000000526001 |REPLACED BY association reference set|`.
pub const REPLACED_BY: u64 = 900_000_000_000_526_001;
/// `900000000000528000 |WAS A association reference set|`.
pub const WAS_A: u64 = 900_000_000_000_528_000;
/// `1186924009 |PARTIALLY EQUIVALENT TO association reference set|`.
pub const PARTIALLY_EQUIVALENT_TO: u64 = 1_186_924_009;
/// The reference set field every member carries.
const REFERENCED_COMPONENT: &str = "referencedComponentId";
/// The association reference set field a history supplement follows.
const TARGET_COMPONENT: &str = "targetComponentId";

/// A failure to evaluate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvalError {
    /// The expression names a concept the edition does not have.
    #[error("the edition has no concept {0}")]
    UnknownConcept(Sctid),
    /// A `^` focus is not a reference set with members in the edition.
    #[error("{0} is not a reference set with members in the edition")]
    NotAReferenceSet(Sctid),
    /// An alternate identifier scheme alias resolves to no identifier scheme.
    #[error("`{0}` is not an identifier scheme alias of the edition")]
    UnknownScheme(String),
    /// An alternate identifier names no concept.
    #[error("no concept has the alternate identifier {scheme}#{code}")]
    UnknownIdentifier {
        /// The scheme alias.
        scheme: String,
        /// The code.
        code: String,
    },
    /// A reference set has no field of the name.
    #[error("reference set {refset} has no field `{field}`")]
    UnknownField {
        /// The reference set.
        refset: Sctid,
        /// The field.
        field: String,
    },
    /// A construct the edition's data cannot answer.
    #[error("{0} is not supported")]
    Unsupported(&'static str),
    /// A dialect alias the specification does not list.
    #[error("`{0}` is not a dialect alias")]
    UnknownDialect(String),
    /// The edition's storage failed.
    #[error("the edition could not be read: {0}")]
    Storage(String),
    /// A numeric value of the expression does not convert to a number.
    #[error("the numeric value `{value}` does not convert")]
    InvalidNumber {
        /// The value as written.
        value: String,
        /// Why it does not convert.
        #[source]
        source: std::num::ParseFloatError,
    },
    /// A time of the expression or of the edition's data is no `YYYYMMDD`.
    #[error("the time `{value}` is no YYYYMMDD")]
    InvalidTime {
        /// The value as written or stored.
        value: String,
        /// Why it does not convert.
        #[source]
        source: std::num::ParseIntError,
    },
}

/// One concept filter with its concept sets resolved, for the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConceptPredicate {
    /// The concept is active (or, `false`, inactive).
    Active(bool),
    /// The definition status is one of the allowed ones.
    DefinitionStatus {
        /// Sufficiently defined concepts pass.
        defined: bool,
        /// Primitive concepts pass.
        primitive: bool,
    },
    /// The module is one of `modules` (SCTIDs), or is not when negated.
    Module {
        /// The module SCTIDs.
        modules: Vec<u64>,
        /// `!=`.
        negated: bool,
    },
    /// The effective time compares to one of `values` (`YYYYMMDD`; `!=` means
    /// to none of them).
    EffectiveTime {
        /// The operator.
        operator: Comparison,
        /// The times.
        values: Vec<u32>,
    },
}

/// One description filter with its concept sets and aliases resolved, for
/// the model; every predicate of one `{{ D ... }}` must hold for the same
/// description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescriptionPredicate {
    /// The term matches one of the search terms (or, `!=`, none).
    Term {
        /// The operator.
        operator: Equality,
        /// The search terms.
        terms: Vec<TypedSearchTerm>,
    },
    /// The language code (the primary subtag) is one of `codes`.
    Language {
        /// The operator.
        operator: Equality,
        /// Two-letter codes.
        codes: Vec<String>,
    },
    /// The description type is one of `types` (SCTIDs).
    Type {
        /// The operator.
        operator: Equality,
        /// The description type SCTIDs.
        types: Vec<u64>,
    },
    /// The description is acceptable in one of the language reference sets,
    /// with one of the listed acceptabilities when any are listed.
    Dialect {
        /// The operator.
        operator: Equality,
        /// `(language reference set SCTID, allowed acceptabilities)`.
        dialects: Vec<(u64, Vec<Acceptability>)>,
    },
    /// The description is active (`Some(true)`), inactive (`Some(false)`),
    /// or either (`None`, `active = *`).
    Active(Option<bool>),
    /// The description identifier is one of `ids`.
    Id {
        /// The operator.
        operator: Equality,
        /// The description identifiers.
        ids: Vec<u64>,
    },
}

/// `900000000000548007 |Preferred|`.
const PREFERRED: u64 = 900_000_000_000_548_007;
/// `900000000000549004 |Acceptable|`.
const ACCEPTABLE: u64 = 900_000_000_000_549_004;
/// `900000000000073002 |Defined|`.
const DEFINED: u64 = 900_000_000_000_073_002;
/// `900000000000074008 |Primitive|`.
const PRIMITIVE: u64 = 900_000_000_000_074_008;
/// `900000000000003001 |Fully specified name|`.
const FULLY_SPECIFIED_NAME: u64 = 900_000_000_000_003_001;
/// `900000000000013009 |Synonym|`.
const SYNONYM: u64 = 900_000_000_000_013_009;
/// `900000000000550004 |Definition|`.
const DEFINITION: u64 = 900_000_000_000_550_004;

/// The edition an expression is evaluated against.
pub trait Model {
    /// The ordinal of a concept, `None` when the edition lacks it.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError::Storage`] when the edition cannot be read.
    fn concept(&self, id: Sctid) -> Result<Option<Ordinal>, EvalError>;
    /// The SCTID of a concept.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError::Storage`] when the edition cannot be read.
    fn sctid(&self, concept: Ordinal) -> Result<Option<Sctid>, EvalError>;
    /// Every concept, active and inactive.
    fn all(&self) -> RoaringBitmap;
    /// The concepts with no parent.
    fn roots(&self) -> RoaringBitmap;
    /// The concepts with no child.
    fn leaves(&self) -> RoaringBitmap;
    /// The transitive descendants, self excluded.
    fn descendants(&self, concept: Ordinal) -> &RoaringBitmap;
    /// The transitive ancestors, self excluded.
    fn ancestors(&self, concept: Ordinal) -> &RoaringBitmap;
    /// The direct children.
    fn children(&self, concept: Ordinal) -> RoaringBitmap;
    /// The direct parents.
    fn parents(&self, concept: Ordinal) -> RoaringBitmap;
    /// The attribute relationships.
    fn attributes(&self) -> &concept_graph::attributes::Attributes;
    /// The reference set member tables.
    fn members(&self) -> &concept_graph::refsets::RefsetMembers;
    /// The alternate identifiers.
    fn identifiers(&self) -> &concept_graph::identifiers::Identifiers;
    /// The identifier scheme an alias names (case-insensitively).
    ///
    /// # Errors
    ///
    /// Returns [`EvalError::Storage`] when the edition cannot be read.
    fn scheme(&self, alias: &str) -> Result<Option<u64>, EvalError>;
    /// The concepts of `within` whose concept rows satisfy every predicate.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError`] for a predicate the edition cannot answer.
    fn filter_concepts(
        &self,
        within: &RoaringBitmap,
        predicates: &[ConceptPredicate],
    ) -> Result<RoaringBitmap, EvalError>;
    /// The concepts of `within` that have one description satisfying every
    /// predicate.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError`] for a predicate the edition cannot answer.
    fn filter_descriptions(
        &self,
        within: &RoaringBitmap,
        predicates: &[DescriptionPredicate],
    ) -> Result<RoaringBitmap, EvalError>;
}

/// Evaluates `constraint` against `model`.
///
/// # Errors
///
/// Returns [`EvalError`] when the expression names something the edition
/// does not have, or asks for a construct its data cannot answer.
pub fn evaluate<M: Model>(
    model: &M,
    constraint: &ExpressionConstraint,
) -> Result<RoaringBitmap, EvalError> {
    Evaluator { model }.expression(constraint)
}

struct Evaluator<'m, M: Model> {
    model: &'m M,
}

impl<M: Model> fmt::Debug for Evaluator<'_, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Evaluator")
    }
}

/// `\\` for the literal characters, matches the whole of `text`
/// (case-insensitively, as description matching is).
#[must_use]
pub fn wild_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    wild_at(&pattern, &text)
}

fn wild_at(pattern: &[char], text: &[char]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some(('*', rest)) => (0..=text.len()).any(|skip| {
            text.get(skip..)
                .is_some_and(|remaining| wild_at(rest, remaining))
        }),
        Some(('\\', rest)) => {
            let Some((escaped, after)) = rest.split_first() else {
                return false;
            };
            text.split_first()
                .is_some_and(|(first, remaining)| first == escaped && wild_at(after, remaining))
        }
        Some((expected, rest)) => text
            .split_first()
            .is_some_and(|(first, remaining)| first == expected && wild_at(rest, remaining)),
    }
}

/// Whether every word of a match term is a prefix of some word of `text`,
/// case-insensitively, or a wild pattern matches the whole text.
#[must_use]
pub fn term_matches(term: &TypedSearchTerm, text: &str) -> bool {
    match term {
        TypedSearchTerm::Match(words) => {
            let lower = text.to_lowercase();
            let text_words: Vec<&str> = lower
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .collect();
            words.iter().all(|word| {
                let word = unescape(word).to_lowercase();
                text_words.iter().any(|t| t.starts_with(word.as_str()))
            })
        }
        TypedSearchTerm::Wild(pattern) => wild_matches(pattern, text),
    }
}

/// `\"` and `\\` to the character they escape.
fn unescape(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    let mut chars = word.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl<M: Model> Evaluator<'_, M> {
    fn expression(&self, constraint: &ExpressionConstraint) -> Result<RoaringBitmap, EvalError> {
        match constraint {
            ExpressionConstraint::Sub(sub) => self.sub(sub),
            ExpressionConstraint::Refined { focus, refinement } => {
                let focus = self.sub(focus)?;
                self.refinement(&focus, refinement)
            }
            ExpressionConstraint::Conjunction(operands) => {
                let mut result: Option<RoaringBitmap> = None;
                for operand in operands {
                    let set = self.sub(operand)?;
                    result = Some(match result {
                        None => set,
                        Some(current) => current & set,
                    });
                }
                Ok(result.unwrap_or_default())
            }
            ExpressionConstraint::Disjunction(operands) => {
                let mut result = RoaringBitmap::new();
                for operand in operands {
                    result |= self.sub(operand)?;
                }
                Ok(result)
            }
            ExpressionConstraint::Exclusion { left, right } => {
                Ok(self.sub(left)? - self.sub(right)?)
            }
            ExpressionConstraint::Dotted { focus, attributes } => {
                let mut set = self.sub(focus)?;
                for attribute in attributes {
                    let kinds = self.kinds(attribute)?;
                    set = self.values_of(&set, &kinds);
                }
                Ok(set)
            }
        }
    }

    /// `subexpressionconstraint`: the focus, the member-of, the operator, then
    /// the filters and the history supplement.
    fn sub(&self, sub: &SubExpressionConstraint) -> Result<RoaringBitmap, EvalError> {
        let mut set = self.focus(&sub.focus)?;
        match &sub.refset_operator {
            Some(RefsetOperator::MemberOf(member_of)) => {
                set = self.member_of(&set, member_of.fields.as_ref(), &sub.member_filters)?;
            }
            Some(RefsetOperator::ContainingAny) => {
                set = self.containing_any(&set, &sub.member_filters)?;
            }
            None => {}
        }
        if let Some(operator) = sub.operator {
            set = self.operate(operator, &set, matches!(sub.focus, FocusConcept::Wildcard));
        }
        for filter in &sub.filters {
            set = match filter {
                FilterConstraint::Concept(filters) => match self.concept_predicates(filters)? {
                    Some(predicates) => self.model.filter_concepts(&set, &predicates)?,
                    None => RoaringBitmap::new(),
                },
                FilterConstraint::Description(filters) => {
                    match self.description_predicates(filters)? {
                        Some(predicates) => self.model.filter_descriptions(&set, &predicates)?,
                        None => RoaringBitmap::new(),
                    }
                }
            };
        }
        if let Some(history) = &sub.history {
            set = self.history(set, history)?;
        }
        Ok(set)
    }

    fn focus(&self, focus: &FocusConcept) -> Result<RoaringBitmap, EvalError> {
        match focus {
            FocusConcept::Wildcard => Ok(self.model.all()),
            FocusConcept::Reference(reference) => {
                let ordinal = self
                    .model
                    .concept(reference.id)?
                    .ok_or(EvalError::UnknownConcept(reference.id))?;
                Ok(RoaringBitmap::from_iter([ordinal.index()]))
            }
            FocusConcept::AltIdentifier(alt) => {
                let scheme = self
                    .model
                    .scheme(&alt.scheme)?
                    .ok_or_else(|| EvalError::UnknownScheme(alt.scheme.clone()))?;
                let ordinal = self
                    .model
                    .identifiers()
                    .lookup(scheme, &alt.code)
                    .ok_or_else(|| EvalError::UnknownIdentifier {
                        scheme: alt.scheme.clone(),
                        code: alt.code.clone(),
                    })?;
                Ok(RoaringBitmap::from_iter([ordinal.index()]))
            }
            FocusConcept::Nested(inner) => self.expression(inner),
        }
    }

    /// The constraint operator over a set; the whole edition has the roots
    /// and the leaves as its answers.
    fn operate(
        &self,
        operator: ConstraintOperator,
        set: &RoaringBitmap,
        whole: bool,
    ) -> RoaringBitmap {
        if whole {
            return self.operate_over_edition(operator);
        }
        match operator {
            ConstraintOperator::Top => self.top_of_set(set),
            ConstraintOperator::Bottom => self.bottom_of_set(set),
            _ => {
                let mut out = self.related(operator, set);
                if matches!(
                    operator,
                    ConstraintOperator::DescendantOrSelfOf
                        | ConstraintOperator::AncestorOrSelfOf
                        | ConstraintOperator::ChildOrSelfOf
                        | ConstraintOperator::ParentOrSelfOf
                ) {
                    out |= set;
                }
                out
            }
        }
    }

    /// The constraint operator over `*`: the whole edition, less the roots or
    /// the leaves where the operator excludes them.
    fn operate_over_edition(&self, operator: ConstraintOperator) -> RoaringBitmap {
        let all = self.model.all();
        match operator {
            ConstraintOperator::DescendantOrSelfOf
            | ConstraintOperator::AncestorOrSelfOf
            | ConstraintOperator::ChildOrSelfOf
            | ConstraintOperator::ParentOrSelfOf => all,
            ConstraintOperator::DescendantOf | ConstraintOperator::ChildOf => {
                all - self.model.roots()
            }
            ConstraintOperator::AncestorOf | ConstraintOperator::ParentOf => {
                all - self.model.leaves()
            }
            ConstraintOperator::Top => self.model.roots(),
            ConstraintOperator::Bottom => self.model.leaves(),
        }
    }

    /// `!!>`: the members of the set with no ancestor in the set.
    fn top_of_set(&self, set: &RoaringBitmap) -> RoaringBitmap {
        let mut out = RoaringBitmap::new();
        for concept in set {
            if self.model.ancestors(Ordinal::new(concept)).is_disjoint(set) {
                out.insert(concept);
            }
        }
        out
    }

    /// `!!<`: the members of the set with no descendant in the set.
    fn bottom_of_set(&self, set: &RoaringBitmap) -> RoaringBitmap {
        let mut out = RoaringBitmap::new();
        for concept in set {
            if self
                .model
                .descendants(Ordinal::new(concept))
                .is_disjoint(set)
            {
                out.insert(concept);
            }
        }
        out
    }

    /// The concepts the hierarchy operator reaches from the set, without the
    /// set itself.
    fn related(&self, operator: ConstraintOperator, set: &RoaringBitmap) -> RoaringBitmap {
        let mut out = RoaringBitmap::new();
        for concept in set {
            let ordinal = Ordinal::new(concept);
            match operator {
                ConstraintOperator::DescendantOf | ConstraintOperator::DescendantOrSelfOf => {
                    out |= self.model.descendants(ordinal);
                }
                ConstraintOperator::AncestorOf | ConstraintOperator::AncestorOrSelfOf => {
                    out |= self.model.ancestors(ordinal);
                }
                ConstraintOperator::ChildOf | ConstraintOperator::ChildOrSelfOf => {
                    out |= self.model.children(ordinal);
                }
                ConstraintOperator::ParentOf | ConstraintOperator::ParentOrSelfOf => {
                    out |= self.model.parents(ordinal);
                }
                ConstraintOperator::Top | ConstraintOperator::Bottom => {}
            }
        }
        out
    }

    /// The attribute type kinds an attribute name names.
    fn kinds(&self, name: &SubExpressionConstraint) -> Result<Kinds, EvalError> {
        if matches!(name.focus, FocusConcept::Wildcard)
            && name.operator.is_none()
            && name.filters.is_empty()
        {
            return Ok(Kinds::Any);
        }
        let types = self.sub(name)?;
        let attributes = self.model.attributes();
        let mut kinds = Vec::new();
        for concept in types {
            if let Some(id) = self.model.sctid(Ordinal::new(concept))?
                && let Some(kind) = attributes.kind(id.0)
            {
                kinds.push(kind);
            }
        }
        Ok(Kinds::These(kinds))
    }

    /// The concept values of the attributes of `kinds` on the sources in `set`.
    fn values_of(&self, set: &RoaringBitmap, kinds: &Kinds) -> RoaringBitmap {
        let attributes = self.model.attributes();
        let mut out = RoaringBitmap::new();
        for source in set {
            for row in attributes.rows(Ordinal::new(source)) {
                if let ValueRef::Concept(target) = row.value
                    && kinds.contains(row.kind)
                {
                    out.insert(target.index());
                }
            }
        }
        out
    }

    /// `eclrefinement` over the focus set.
    fn refinement(
        &self,
        focus: &RoaringBitmap,
        refinement: &Refinement,
    ) -> Result<RoaringBitmap, EvalError> {
        match refinement {
            Refinement::Single(one) => self.sub_refinement(focus, one),
            Refinement::Conjunction(items) => {
                let mut result = focus.clone();
                for item in items {
                    result &= self.sub_refinement(&result, item)?;
                }
                Ok(result)
            }
            Refinement::Disjunction(items) => {
                let mut result = RoaringBitmap::new();
                for item in items {
                    result |= self.sub_refinement(focus, item)?;
                }
                Ok(result)
            }
        }
    }

    fn sub_refinement(
        &self,
        focus: &RoaringBitmap,
        item: &SubRefinement,
    ) -> Result<RoaringBitmap, EvalError> {
        match item {
            SubRefinement::AttributeSet(set) => self.attribute_set(focus, set),
            SubRefinement::Nested(inner) => self.refinement(focus, inner),
            SubRefinement::Group {
                cardinality,
                attributes,
            } => self.attribute_group(focus, *cardinality, attributes),
        }
    }

    /// `eclattributegroup`: the concepts whose relationship groups satisfy the
    /// attribute set as often as the cardinality demands.
    fn attribute_group(
        &self,
        focus: &RoaringBitmap,
        cardinality: Option<Cardinality>,
        attributes: &AttributeSet,
    ) -> Result<RoaringBitmap, EvalError> {
        let tests = self.compile_set(attributes)?;
        let graph = self.model.attributes();
        // A concept whose rows satisfy the set within one group satisfies
        // it over all its rows, so the ungrouped answer (the inverted
        // index) narrows the concepts whose groups are counted, unless
        // zero groups may match.
        let candidates = if cardinality.is_none_or(|c| c.min >= 1) {
            self.attribute_set(focus, attributes)?
        } else {
            focus.clone()
        };
        let mut out = RoaringBitmap::new();
        for concept in &candidates {
            let rows: Vec<Row<'_>> = graph.rows(Ordinal::new(concept)).collect();
            let mut matching = 0_u32;
            for group in &relationship_groups(&rows) {
                if set_holds(&tests, group)? {
                    matching = matching.saturating_add(1);
                }
            }
            if within(cardinality, matching) {
                out.insert(concept);
            }
        }
        Ok(out)
    }

    /// An ungrouped attribute set over the focus: each attribute is a set of
    /// sources (a fast path when the default cardinality and a concept value
    /// allow the inverted index, else a row scan), joined by the junction.
    fn attribute_set(
        &self,
        focus: &RoaringBitmap,
        set: &AttributeSet,
    ) -> Result<RoaringBitmap, EvalError> {
        match set {
            AttributeSet::Single(one) => self.sub_attribute_set(focus, one),
            AttributeSet::Conjunction(items) => {
                let mut result = focus.clone();
                for item in items {
                    result &= self.sub_attribute_set(&result, item)?;
                }
                Ok(result)
            }
            AttributeSet::Disjunction(items) => {
                let mut result = RoaringBitmap::new();
                for item in items {
                    result |= self.sub_attribute_set(focus, item)?;
                }
                Ok(result)
            }
        }
    }

    fn sub_attribute_set(
        &self,
        focus: &RoaringBitmap,
        item: &SubAttributeSet,
    ) -> Result<RoaringBitmap, EvalError> {
        match item {
            SubAttributeSet::Attribute(attribute) => self.attribute(focus, attribute),
            SubAttributeSet::Nested(inner) => self.attribute_set(focus, inner),
        }
    }

    /// One attribute over the focus, ungrouped.
    fn attribute(
        &self,
        focus: &RoaringBitmap,
        attribute: &Attribute,
    ) -> Result<RoaringBitmap, EvalError> {
        let test = self.compile(attribute)?;
        if attribute.reverse {
            return Ok(self.reverse(focus, &test));
        }
        let default = attribute.cardinality.is_none();
        if default
            && let ValueTest::Concept {
                set,
                negated: false,
            } = &test.value
        {
            let sources = match set {
                None => self.sources_of_kinds(&test.kinds),
                Some(values) => self.sources_of_values(&test.kinds, values),
            };
            return Ok(sources & focus);
        }
        let graph = self.model.attributes();
        let mut out = RoaringBitmap::new();
        for concept in focus {
            let rows: Vec<Row<'_>> = graph.rows(Ordinal::new(concept)).collect();
            if within(attribute.cardinality, matching_rows(&test, rows.iter())?) {
                out.insert(concept);
            }
        }
        Ok(out)
    }

    /// The concepts carrying an attribute of one of `kinds` with any value,
    /// from the inverted index.
    fn sources_of_kinds(&self, kinds: &Kinds) -> RoaringBitmap {
        let graph = self.model.attributes();
        let mut out = RoaringBitmap::new();
        for kind in kinds.iter(graph.types().len()) {
            if let Some(sources) = graph.sources_of_kind(kind) {
                out |= sources;
            }
        }
        out
    }

    /// The concepts carrying an attribute of one of `kinds` whose value is in
    /// `values`, from the inverted index.
    fn sources_of_values(&self, kinds: &Kinds, values: &RoaringBitmap) -> RoaringBitmap {
        let graph = self.model.attributes();
        let mut out = RoaringBitmap::new();
        for kind in kinds.iter(graph.types().len()) {
            for target in graph.targets_of_kind(kind) {
                if values.contains(*target) {
                    out.extend(graph.sources(kind, Ordinal::new(*target)).iter().copied());
                }
            }
        }
        out
    }

    /// `R attribute = X`: the values of the attribute whose sources are in the
    /// value set, counted per value for the cardinality, within the focus.
    fn reverse(&self, focus: &RoaringBitmap, test: &AttributeTest) -> RoaringBitmap {
        let graph = self.model.attributes();
        let sources = match &test.value {
            ValueTest::Concept {
                set: Some(set),
                negated: false,
            } => Some(set),
            _ => None,
        };
        let mut out = RoaringBitmap::new();
        for target in focus {
            let mut count = 0_u32;
            for kind in test.kinds.iter(graph.types().len()) {
                for source in graph.sources(kind, Ordinal::new(target)) {
                    if sources.is_none_or(|set| set.contains(*source)) {
                        count = count.saturating_add(1);
                    }
                }
            }
            if within(test.cardinality, count) {
                out.insert(target);
            }
        }
        out
    }

    /// The compiled tests of an attribute set, for per-group evaluation.
    fn compile_set(&self, set: &AttributeSet) -> Result<CompiledSet, EvalError> {
        Ok(match set {
            AttributeSet::Single(one) => CompiledSet::Single(Box::new(self.compile_item(one)?)),
            AttributeSet::Conjunction(items) => CompiledSet::Conjunction(
                items
                    .iter()
                    .map(|i| self.compile_item(i))
                    .collect::<Result<_, _>>()?,
            ),
            AttributeSet::Disjunction(items) => CompiledSet::Disjunction(
                items
                    .iter()
                    .map(|i| self.compile_item(i))
                    .collect::<Result<_, _>>()?,
            ),
        })
    }

    fn compile_item(&self, item: &SubAttributeSet) -> Result<CompiledItem, EvalError> {
        Ok(match item {
            SubAttributeSet::Attribute(attribute) => {
                CompiledItem::Attribute(self.compile(attribute)?)
            }
            SubAttributeSet::Nested(inner) => CompiledItem::Nested(self.compile_set(inner)?),
        })
    }

    fn compile(&self, attribute: &Attribute) -> Result<AttributeTest, EvalError> {
        let kinds = self.kinds(&attribute.name)?;
        let value = match &attribute.value {
            AttributeValue::Expression { operator, value } => {
                let set = if matches!(value.focus, FocusConcept::Wildcard)
                    && value.operator.is_none()
                    && value.filters.is_empty()
                    && value.refset_operator.is_none()
                {
                    None
                } else {
                    Some(self.sub(value)?)
                };
                ValueTest::Concept {
                    set,
                    negated: *operator == Equality::NotEqual,
                }
            }
            AttributeValue::Numeric { operator, value } => {
                ValueTest::Number(*operator, number(&value.0)?)
            }
            AttributeValue::String { operator, values } => {
                ValueTest::Text(*operator, values.clone())
            }
            AttributeValue::Boolean { operator, value } => ValueTest::Boolean(*operator, *value),
        };
        Ok(AttributeTest {
            kinds,
            value,
            cardinality: attribute.cardinality,
        })
    }
}
