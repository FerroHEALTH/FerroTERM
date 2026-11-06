// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Refinements: the sources whose attribute rows satisfy the attributes, with
//! their cardinalities per role group, and the reverse attributes.

use concept_graph::attributes::Row;
use concept_graph::ordinal::Ordinal;
use roaring::RoaringBitmap;

use super::matching::{
    AttributeTest, CompiledItem, CompiledSet, Kinds, ValueTest, matching_rows, number,
    relationship_groups, set_holds, within,
};
use crate::ast::{
    Attribute, AttributeSet, AttributeValue, Cardinality, Equality, FocusConcept, Refinement,
    SubAttributeSet, SubRefinement,
};

use super::{EvalError, Evaluator, Model};

impl<M: Model> Evaluator<'_, M> {
    /// `eclrefinement` over the focus set.
    pub(super) fn refinement(
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
