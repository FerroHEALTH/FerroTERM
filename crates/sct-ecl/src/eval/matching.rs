// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a compiled refinement and a filter test row by row: the attribute
//! value tests, the cardinalities, the role groups and the comparisons.

use concept_graph::attributes::{Row, ValueRef};
use roaring::RoaringBitmap;

use crate::ast::{ActiveValue, Cardinality, Comparison, ConcreteString, Equality, TimeValue};

/// The kinds of the attribute types a set of type concepts names, or every
/// kind for `*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Kinds {
    Any,
    These(Vec<u32>),
}

impl Kinds {
    pub(super) fn contains(&self, kind: u32) -> bool {
        match self {
            Self::Any => true,
            Self::These(kinds) => kinds.contains(&kind),
        }
    }

    pub(super) fn iter(&self, total: usize) -> Box<dyn Iterator<Item = u32> + '_> {
        match self {
            Self::Any => Box::new(0..u32::try_from(total).unwrap_or(u32::MAX)),
            Self::These(kinds) => Box::new(kinds.iter().copied()),
        }
    }
}

/// What an attribute's value must satisfy.
#[derive(Debug)]
pub(super) enum ValueTest {
    /// The value is a concept in (or, negated, not in) the set; `None` is `*`.
    Concept {
        set: Option<RoaringBitmap>,
        negated: bool,
    },
    Number(Comparison, f64),
    Text(Equality, Vec<ConcreteString>),
    Boolean(Equality, bool),
}

impl ValueTest {
    pub(super) fn matches(&self, value: ValueRef<'_>) -> bool {
        match (self, value) {
            // NOTE: `= *` matches any value, a concrete one too; the specification
            // spells the wildcard as any value and the reference servers agree.
            (Self::Concept { set: None, negated }, _) => !*negated,
            (
                Self::Concept {
                    set: Some(set),
                    negated,
                },
                ValueRef::Concept(target),
            ) => set.contains(target.index()) != *negated,
            (Self::Number(operator, expected), ValueRef::Number(text)) => text
                .parse::<f64>()
                .is_ok_and(|actual| compare_numbers(*operator, actual, *expected)),
            // NOTE: ECL 2.3, "Concrete Values": a concrete string must match the
            // string in the expression constraint exactly, case included.
            (Self::Text(operator, values), ValueRef::String(text)) => {
                let hit = values.iter().any(|value| value.0 == text);
                hit == (*operator == Equality::Equal)
            }
            (Self::Boolean(operator, expected), ValueRef::String(text)) => {
                let actual = text.eq_ignore_ascii_case("true");
                let boolean = actual || text.eq_ignore_ascii_case("false");
                boolean && ((actual == *expected) == (*operator == Equality::Equal))
            }
            _ => false,
        }
    }
}

pub(super) fn compare_numbers(operator: Comparison, actual: f64, expected: f64) -> bool {
    match operator {
        Comparison::Equal => (actual - expected).abs() < f64::EPSILON,
        Comparison::NotEqual => (actual - expected).abs() >= f64::EPSILON,
        Comparison::Less => actual < expected,
        Comparison::LessOrEqual => actual <= expected,
        Comparison::Greater => actual > expected,
        Comparison::GreaterOrEqual => actual >= expected,
    }
}

fn compare_times(operator: Comparison, actual: u32, expected: u32) -> bool {
    match operator {
        Comparison::Equal => actual == expected,
        Comparison::NotEqual => actual != expected,
        Comparison::Less => actual < expected,
        Comparison::LessOrEqual => actual <= expected,
        Comparison::Greater => actual > expected,
        Comparison::GreaterOrEqual => actual >= expected,
    }
}

/// Whether `pattern`, with `*` for any run of characters and `\*`, `\"`,
/// Whether `count` satisfies a cardinality; `None` is the default `[1..*]`.
pub(super) fn within(cardinality: Option<Cardinality>, count: u32) -> bool {
    let Cardinality { min, max } = cardinality.unwrap_or(Cardinality { min: 1, max: None });
    count >= min && max.is_none_or(|max| count <= max)
}

/// The statuses an `active` filter admits.
pub(super) enum Activity {
    /// One status: active (`true`) or inactive.
    Is(bool),
    /// Both (`active = *`).
    Either,
    /// None (`active != *`).
    Neither,
}

/// The statuses `active op value` admits.
pub(super) fn activity(operator: Equality, value: ActiveValue) -> Activity {
    let equal = operator == Equality::Equal;
    match value {
        ActiveValue::Active => Activity::Is(equal),
        ActiveValue::Inactive => Activity::Is(!equal),
        ActiveValue::Any if equal => Activity::Either,
        ActiveValue::Any => Activity::Neither,
    }
}

/// Whether `values` (one time or a set) match `actual` under `operator`.
pub(super) fn times_match(operator: Comparison, values: &[TimeValue], actual: u32) -> bool {
    let expected = values
        .iter()
        .map(|v| v.0.parse::<u32>().unwrap_or_default());
    match operator {
        Comparison::NotEqual => expected.clone().all(|e| actual != e),
        _ => expected
            .into_iter()
            .any(|e| compare_times(operator, actual, e)),
    }
}

/// A compiled attribute: the type kinds, the value test, the cardinality.
#[derive(Debug)]
pub(super) struct AttributeTest {
    pub(super) kinds: Kinds,
    pub(super) value: ValueTest,
    pub(super) cardinality: Option<Cardinality>,
}

impl AttributeTest {
    pub(super) fn matches(&self, row: &Row<'_>) -> bool {
        self.kinds.contains(row.kind) && self.value.matches(row.value)
    }
}

#[derive(Debug)]
pub(super) enum CompiledSet {
    Single(Box<CompiledItem>),
    Conjunction(Vec<CompiledItem>),
    Disjunction(Vec<CompiledItem>),
}

#[derive(Debug)]
pub(super) enum CompiledItem {
    Attribute(AttributeTest),
    Nested(CompiledSet),
}

/// The relationship groups of one concept, in row order.
pub(super) fn relationship_groups<'row, 'graph>(
    rows: &'row [Row<'graph>],
) -> Vec<Vec<&'row Row<'graph>>> {
    let mut groups: Vec<Vec<&Row<'_>>> = Vec::new();
    let mut current: Option<u32> = None;
    for row in rows {
        // NOTE: ECL treats each ungrouped relationship (group 0) as
        // its own group; the rows arrive sorted by group.
        if row.group == 0 || current != Some(row.group) {
            groups.push(Vec::new());
            current = Some(row.group);
        }
        if let Some(last) = groups.last_mut() {
            last.push(row);
        }
    }
    groups
}

/// Whether the rows of one group satisfy the set.
pub(super) fn set_holds(set: &CompiledSet, rows: &[&Row<'_>]) -> bool {
    match set {
        CompiledSet::Single(item) => item_holds(item, rows),
        CompiledSet::Conjunction(items) => items.iter().all(|i| item_holds(i, rows)),
        CompiledSet::Disjunction(items) => items.iter().any(|i| item_holds(i, rows)),
    }
}

fn item_holds(item: &CompiledItem, rows: &[&Row<'_>]) -> bool {
    match item {
        CompiledItem::Nested(set) => set_holds(set, rows),
        CompiledItem::Attribute(test) => {
            let count = rows.iter().filter(|row| test.matches(row)).count();
            within(test.cardinality, u32::try_from(count).unwrap_or(u32::MAX))
        }
    }
}

#[cfg(test)]
mod tests {
    use concept_graph::attributes::ValueRef;

    use super::{Cardinality, ValueTest, compare_numbers, within};
    use crate::ast::{Comparison, ConcreteString, Equality, TypedSearchTerm};
    use crate::eval::{term_matches, wild_matches};

    /// ECL 2.3, "Concrete Values": a concrete string matches exactly, with
    /// its case, never as a word prefix or a pattern.
    #[test]
    fn a_concrete_string_matches_the_whole_value_with_its_case() {
        let panadol =
            |operator| ValueTest::Text(operator, vec![ConcreteString(String::from("PANADOL"))]);
        assert!(panadol(Equality::Equal).matches(ValueRef::String("PANADOL")));
        assert!(!panadol(Equality::Equal).matches(ValueRef::String("panadol")));
        assert!(!panadol(Equality::Equal).matches(ValueRef::String("PANADOL EXTRA")));
        assert!(!panadol(Equality::Equal).matches(ValueRef::String("PANA")));
        assert!(panadol(Equality::NotEqual).matches(ValueRef::String("PANADOL EXTRA")));
        assert!(!panadol(Equality::NotEqual).matches(ValueRef::String("PANADOL")));
        let star = ValueTest::Text(Equality::Equal, vec![ConcreteString(String::from("a*"))]);
        assert!(
            star.matches(ValueRef::String("a*")),
            "an asterisk is itself"
        );
        assert!(!star.matches(ValueRef::String("ab")), "not a wildcard");
        let set = ValueTest::Text(
            Equality::Equal,
            vec![
                ConcreteString(String::from("A")),
                ConcreteString(String::from("B")),
            ],
        );
        assert!(set.matches(ValueRef::String("B")), "any string of the set");
        assert!(!panadol(Equality::Equal).matches(ValueRef::Number("5")));
    }

    #[test]
    fn cardinalities_patterns_and_words_match_as_the_specification_says() {
        assert!(within(None, 1) && within(None, 5) && !within(None, 0));
        assert!(within(
            Some(Cardinality {
                min: 0,
                max: Some(0)
            }),
            0
        ));
        assert!(!within(
            Some(Cardinality {
                min: 0,
                max: Some(0)
            }),
            1
        ));
        assert!(within(Some(Cardinality { min: 2, max: None }), 2));
        assert!(wild_matches("cardi*opathy", "Cardiomyopathy"));
        assert!(!wild_matches("cardi*opathy", "Cardiomyopathy X"));
        assert!(wild_matches("*itis", "Bronchitis"));
        assert!(wild_matches("a\\*b", "A*B"));
        assert!(!wild_matches("a\\*b", "AXB"));
        let term = TypedSearchTerm::Match(vec![String::from("hea"), String::from("att")]);
        assert!(term_matches(&term, "Heart attack"));
        assert!(!term_matches(&term, "Heart"));
        assert!(compare_numbers(Comparison::GreaterOrEqual, 500.0, 500.0));
        assert!(!compare_numbers(Comparison::Less, 500.0, 500.0));
    }
}
