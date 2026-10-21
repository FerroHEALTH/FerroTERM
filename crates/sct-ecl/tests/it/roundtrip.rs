// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `parse(print(tree)) == tree` for generated trees (`proptest`).

use proptest::prelude::*;
use sct_ecl::ast::{
    Acceptability, AcceptabilitySet, AltIdentifier, Attribute, AttributeSet, AttributeValue,
    Cardinality, Comparison, ConceptFilter, ConceptReference, ConceptSet, ConstraintOperator,
    DefinitionStatus, DescriptionFilter, DialectAlias, DialectIdValue, Equality,
    ExpressionConstraint, FieldValue, FilterConstraint, FocusConcept, HistorySupplement,
    MemberFilter, MemberOf, NumericValue, Refinement, RefsetFields, Sctid, SubAttributeSet,
    SubExpressionConstraint, SubRefinement, TimeValue, TypeToken, TypedSearchTerm,
};

fn sctid() -> impl Strategy<Value = Sctid> {
    (100_000_u64..1_000_000_000_000_000_000).prop_map(Sctid)
}

fn term() -> impl Strategy<Value = Option<String>> {
    prop::option::of(
        "[A-Za-z][A-Za-z0-9 ,.()-]{0,20}[A-Za-z0-9)]".prop_map(|s| s.trim().to_owned()),
    )
}

fn concept_reference() -> impl Strategy<Value = ConceptReference> {
    (sctid(), term()).prop_map(|(id, term)| ConceptReference { id, term })
}

fn alias() -> impl Strategy<Value = String> {
    "[a-z]{2}(-[a-z0-9]{1,6}){0,2}"
}

fn equality() -> impl Strategy<Value = Equality> {
    prop_oneof![Just(Equality::Equal), Just(Equality::NotEqual)]
}

fn comparison() -> impl Strategy<Value = Comparison> {
    prop_oneof![
        Just(Comparison::Equal),
        Just(Comparison::NotEqual),
        Just(Comparison::Less),
        Just(Comparison::LessOrEqual),
        Just(Comparison::Greater),
        Just(Comparison::GreaterOrEqual),
    ]
}

fn ordering() -> impl Strategy<Value = Comparison> {
    prop_oneof![
        Just(Comparison::Less),
        Just(Comparison::LessOrEqual),
        Just(Comparison::Greater),
        Just(Comparison::GreaterOrEqual),
    ]
}

fn operator() -> impl Strategy<Value = ConstraintOperator> {
    prop_oneof![
        Just(ConstraintOperator::DescendantOf),
        Just(ConstraintOperator::DescendantOrSelfOf),
        Just(ConstraintOperator::ChildOf),
        Just(ConstraintOperator::ChildOrSelfOf),
        Just(ConstraintOperator::AncestorOf),
        Just(ConstraintOperator::AncestorOrSelfOf),
        Just(ConstraintOperator::ParentOf),
        Just(ConstraintOperator::ParentOrSelfOf),
        Just(ConstraintOperator::Top),
        Just(ConstraintOperator::Bottom),
    ]
}

fn typed_search_term() -> impl Strategy<Value = TypedSearchTerm> {
    prop_oneof![
        prop::collection::vec("[A-Za-z0-9][A-Za-z0-9é-]{0,6}", 1..3)
            .prop_map(TypedSearchTerm::Match),
        "[A-Za-z*][A-Za-z0-9*]{0,8}".prop_map(TypedSearchTerm::Wild),
    ]
}

fn time_value() -> impl Strategy<Value = TimeValue> {
    prop_oneof![
        Just(TimeValue(String::new())),
        (1000_u32..10_000, 1_u32..13, 1_u32..32)
            .prop_map(|(y, m, d)| TimeValue(format!("{y}{m:02}{d:02}"))),
    ]
}

fn numeric_value() -> impl Strategy<Value = NumericValue> {
    prop_oneof![
        (0_u32..100_000).prop_map(|n| NumericValue(n.to_string())),
        (1_u32..1000, 0_u32..100).prop_map(|(n, f)| NumericValue(format!("-{n}.{f:02}"))),
        (1_u32..1000).prop_map(|n| NumericValue(format!("+{n}"))),
    ]
}

fn cardinality() -> impl Strategy<Value = Option<Cardinality>> {
    prop::option::of(
        (0_u32..3, prop::option::of(0_u32..5)).prop_map(|(min, max)| Cardinality { min, max }),
    )
}

fn acceptability_set() -> impl Strategy<Value = AcceptabilitySet> {
    prop_oneof![
        prop::collection::vec(concept_reference(), 1..3).prop_map(AcceptabilitySet::Concepts),
        prop::collection::vec(
            prop_oneof![
                Just(Acceptability::Acceptable),
                Just(Acceptability::Preferred)
            ],
            1..3
        )
        .prop_map(AcceptabilitySet::Tokens),
    ]
}

fn concept_set(sub: BoxedStrategy<SubExpressionConstraint>) -> impl Strategy<Value = ConceptSet> {
    prop_oneof![
        sub.prop_map(ConceptSet::Expression),
        prop::collection::vec(concept_reference(), 2..4).prop_map(ConceptSet::Set),
    ]
}

fn description_filter(
    sub: BoxedStrategy<SubExpressionConstraint>,
) -> impl Strategy<Value = DescriptionFilter> {
    prop_oneof![
        (equality(), prop::collection::vec(typed_search_term(), 1..3))
            .prop_map(|(operator, terms)| DescriptionFilter::Term { operator, terms }),
        (equality(), prop::collection::vec("[a-z]{2}", 1..3))
            .prop_map(|(operator, codes)| DescriptionFilter::Language { operator, codes }),
        (equality(), concept_set(sub.clone()))
            .prop_map(|(operator, value)| DescriptionFilter::TypeId { operator, value }),
        (
            equality(),
            prop::collection::vec(
                prop_oneof![
                    Just(TypeToken::Synonym),
                    Just(TypeToken::FullySpecifiedName),
                    Just(TypeToken::Definition)
                ],
                1..3
            )
        )
            .prop_map(|(operator, tokens)| DescriptionFilter::Type { operator, tokens }),
        (
            equality(),
            prop_oneof![
                sub.clone().prop_map(DialectIdValue::Expression),
                prop::collection::vec(
                    (concept_reference(), prop::option::of(acceptability_set())),
                    1..3
                )
                .prop_filter("a bare set of one reads as a constraint", |items| {
                    items.len() >= 2 || items.iter().any(|(_, a)| a.is_some())
                })
                .prop_map(DialectIdValue::Set),
            ],
            prop::option::of(acceptability_set())
        )
            .prop_map(
                |(operator, value, acceptability)| DescriptionFilter::DialectId {
                    operator,
                    value,
                    acceptability
                }
            ),
        (
            equality(),
            prop::collection::vec(
                (alias(), prop::option::of(acceptability_set())).prop_map(
                    |(alias, acceptability)| DialectAlias {
                        alias,
                        acceptability
                    }
                ),
                1..3
            ),
            prop::option::of(acceptability_set())
        )
            .prop_map(
                |(operator, aliases, acceptability)| DescriptionFilter::Dialect {
                    operator,
                    aliases,
                    acceptability
                }
            ),
        (equality(), concept_set(sub))
            .prop_map(|(operator, value)| DescriptionFilter::Module { operator, value }),
        (comparison(), prop::collection::vec(time_value(), 1..3))
            .prop_map(|(operator, values)| DescriptionFilter::EffectiveTime { operator, values }),
        (equality(), any::<bool>())
            .prop_map(|(operator, value)| DescriptionFilter::Active { operator, value }),
        (equality(), prop::collection::vec(sctid(), 1..3))
            .prop_map(|(operator, ids)| DescriptionFilter::Id { operator, ids }),
    ]
}

fn concept_filter(
    sub: BoxedStrategy<SubExpressionConstraint>,
) -> impl Strategy<Value = ConceptFilter> {
    prop_oneof![
        (equality(), concept_set(sub.clone()))
            .prop_map(|(operator, value)| ConceptFilter::DefinitionStatusId { operator, value }),
        (
            equality(),
            prop::collection::vec(
                prop_oneof![
                    Just(DefinitionStatus::Primitive),
                    Just(DefinitionStatus::Defined)
                ],
                1..3
            )
        )
            .prop_map(|(operator, tokens)| ConceptFilter::DefinitionStatus { operator, tokens }),
        (equality(), concept_set(sub))
            .prop_map(|(operator, value)| ConceptFilter::Module { operator, value }),
        (comparison(), prop::collection::vec(time_value(), 1..3))
            .prop_map(|(operator, values)| ConceptFilter::EffectiveTime { operator, values }),
        (equality(), any::<bool>())
            .prop_map(|(operator, value)| ConceptFilter::Active { operator, value }),
    ]
}

fn member_filter(
    sub: BoxedStrategy<SubExpressionConstraint>,
) -> impl Strategy<Value = MemberFilter> {
    let field_name = "[a-zA-Z]{3,12}".prop_filter("not a member filter keyword", |n| {
        !["moduleId", "effectiveTime", "active"]
            .iter()
            .any(|k| k.eq_ignore_ascii_case(n))
    });
    prop_oneof![
        (equality(), concept_set(sub.clone()))
            .prop_map(|(operator, value)| MemberFilter::Module { operator, value }),
        (comparison(), prop::collection::vec(time_value(), 1..3))
            .prop_map(|(operator, values)| MemberFilter::EffectiveTime { operator, values }),
        (equality(), any::<bool>())
            .prop_map(|(operator, value)| MemberFilter::Active { operator, value }),
        (
            field_name,
            prop_oneof![
                (equality(), sub)
                    .prop_map(|(operator, value)| FieldValue::Expression { operator, value }),
                (comparison(), numeric_value())
                    .prop_map(|(operator, value)| FieldValue::Numeric { operator, value }),
                (equality(), prop::collection::vec(typed_search_term(), 1..3))
                    .prop_map(|(operator, terms)| FieldValue::String { operator, terms }),
                (equality(), any::<bool>())
                    .prop_map(|(operator, value)| FieldValue::Boolean { operator, value }),
                (ordering(), prop::collection::vec(time_value(), 1..3))
                    .prop_map(|(operator, values)| FieldValue::Time { operator, values }),
            ]
        )
            .prop_map(|(name, value)| MemberFilter::Field { name, value }),
    ]
}

fn attribute_value(
    sub: BoxedStrategy<SubExpressionConstraint>,
) -> impl Strategy<Value = AttributeValue> {
    prop_oneof![
        (equality(), sub)
            .prop_map(|(operator, value)| AttributeValue::Expression { operator, value }),
        (comparison(), numeric_value())
            .prop_map(|(operator, value)| AttributeValue::Numeric { operator, value }),
        (equality(), prop::collection::vec(typed_search_term(), 1..3))
            .prop_map(|(operator, terms)| AttributeValue::String { operator, terms }),
        (equality(), any::<bool>())
            .prop_map(|(operator, value)| AttributeValue::Boolean { operator, value }),
    ]
}

fn attribute_set(sub: BoxedStrategy<SubExpressionConstraint>) -> BoxedStrategy<AttributeSet> {
    let attribute = (
        cardinality(),
        any::<bool>(),
        sub.clone(),
        attribute_value(sub),
    )
        .prop_map(|(cardinality, reverse, name, value)| Attribute {
            cardinality,
            reverse,
            name,
            value,
        });
    let leaf = attribute
        .prop_map(|a| AttributeSet::Single(Box::new(SubAttributeSet::Attribute(Box::new(a)))));
    leaf.prop_recursive(2, 6, 3, |inner| {
        let item = prop_oneof![
            inner
                .clone()
                .prop_filter_map("a single attribute is not nested", |set| match set {
                    AttributeSet::Single(one) => Some(*one),
                    _ => None,
                }),
            inner.prop_map(|set| SubAttributeSet::Nested(Box::new(set))),
        ];
        prop_oneof![
            prop::collection::vec(item.clone(), 2..4).prop_map(AttributeSet::Conjunction),
            prop::collection::vec(item, 2..4).prop_map(AttributeSet::Disjunction),
        ]
    })
    .boxed()
}

/// Whether a refinement-level item list parses back as written: the
/// grammar's `eclattributeset` is greedy, so two attribute sets in a row
/// would merge into one.
fn keeps_items_apart(items: &[SubRefinement]) -> bool {
    !items.windows(2).any(|pair| {
        matches!(
            pair,
            [
                SubRefinement::AttributeSet(_),
                SubRefinement::AttributeSet(_)
            ]
        )
    })
}

/// Whether a parenthesized refinement stays one: without a group the grammar
/// reads `( ... )` as a nested attribute set.
fn has_group(items: &[SubRefinement]) -> bool {
    items
        .iter()
        .any(|item| matches!(item, SubRefinement::Group { .. }))
}

fn refinement(sub: BoxedStrategy<SubExpressionConstraint>) -> BoxedStrategy<Refinement> {
    let set = attribute_set(sub);
    let leaf = prop_oneof![
        set.clone().prop_map(SubRefinement::AttributeSet),
        (cardinality(), set).prop_map(|(cardinality, attributes)| SubRefinement::Group {
            cardinality,
            attributes
        }),
    ];
    let sub_refinement = leaf.prop_recursive(2, 4, 2, |inner| {
        let items = prop::collection::vec(inner, 2..4).prop_filter(
            "a nested refinement holds a group and keeps its items apart",
            |items| has_group(items) && keeps_items_apart(items),
        );
        prop_oneof![
            items
                .clone()
                .prop_map(|items| SubRefinement::Nested(Box::new(Refinement::Conjunction(items)))),
            items.prop_map(|items| SubRefinement::Nested(Box::new(Refinement::Disjunction(items)))),
        ]
    });
    let items = prop::collection::vec(sub_refinement.clone(), 2..4)
        .prop_filter("items stay apart", |items| keeps_items_apart(items));
    prop_oneof![
        sub_refinement.prop_map(|one| Refinement::Single(Box::new(one))),
        items.clone().prop_map(Refinement::Conjunction),
        items.prop_map(Refinement::Disjunction),
    ]
    .boxed()
}

fn plain_sub() -> BoxedStrategy<SubExpressionConstraint> {
    (
        prop::option::of(operator()),
        prop::option::of(prop::option::of(prop_oneof![
            Just(RefsetFields::Any),
            prop::collection::vec("[a-zA-Z]{2,10}", 1..3).prop_map(RefsetFields::Names),
        ])),
        prop_oneof![
            concept_reference().prop_map(FocusConcept::Reference),
            Just(FocusConcept::Wildcard),
            (
                "[A-Za-z][A-Za-z0-9-]{0,5}",
                "[A-Za-z0-9._-]{1,8}|[A-Za-z0-9 ]{1,8}",
                term()
            )
                .prop_map(|(scheme, code, term)| FocusConcept::AltIdentifier(
                    AltIdentifier { scheme, code, term }
                )),
        ],
    )
        .prop_map(|(operator, member_of, focus)| SubExpressionConstraint {
            operator,
            member_of: member_of.map(|fields| MemberOf { fields }),
            focus,
            member_filters: Vec::new(),
            filters: Vec::new(),
            history: None,
        })
        .boxed()
}

fn expression(depth: u32) -> BoxedStrategy<ExpressionConstraint> {
    let sub = sub_expression(depth);
    prop_oneof![
        sub.clone().prop_map(ExpressionConstraint::Sub),
        (sub.clone(), refinement(sub.clone())).prop_map(|(focus, refinement)| {
            ExpressionConstraint::Refined {
                focus,
                refinement: Box::new(refinement),
            }
        }),
        prop::collection::vec(sub.clone(), 2..4).prop_map(ExpressionConstraint::Conjunction),
        prop::collection::vec(sub.clone(), 2..4).prop_map(ExpressionConstraint::Disjunction),
        (sub.clone(), sub.clone())
            .prop_map(|(left, right)| ExpressionConstraint::Exclusion { left, right }),
        (sub.clone(), prop::collection::vec(sub, 1..3))
            .prop_map(|(focus, attributes)| ExpressionConstraint::Dotted { focus, attributes }),
    ]
    .boxed()
}

fn sub_expression(depth: u32) -> BoxedStrategy<SubExpressionConstraint> {
    if depth == 0 {
        return plain_sub();
    }
    let inner = sub_expression(depth - 1);
    let focus = prop_oneof![
        3 => plain_sub().prop_map(|s| s.focus),
        1 => expression(depth - 1).prop_map(|e| FocusConcept::Nested(Box::new(e))),
    ];
    let history = prop::option::of(prop_oneof![
        Just(HistorySupplement::Default),
        Just(HistorySupplement::Minimum),
        Just(HistorySupplement::Moderate),
        Just(HistorySupplement::Maximum),
        expression(depth - 1).prop_map(|e| HistorySupplement::Subset(Box::new(e))),
    ]);
    (
        plain_sub(),
        focus,
        prop::collection::vec(
            prop::collection::vec(member_filter(inner.clone()), 1..3),
            0..2,
        ),
        prop::collection::vec(
            prop_oneof![
                prop::collection::vec(description_filter(inner.clone()), 1..3)
                    .prop_map(FilterConstraint::Description),
                prop::collection::vec(concept_filter(inner), 1..3)
                    .prop_map(FilterConstraint::Concept),
            ],
            0..3,
        ),
        history,
    )
        .prop_map(
            |(plain, focus, member_filters, filters, history)| SubExpressionConstraint {
                operator: plain.operator,
                member_of: plain.member_of,
                focus,
                member_filters,
                filters,
                history,
            },
        )
        .boxed()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn printing_a_tree_and_parsing_it_back_gives_the_same_tree(tree in expression(2)) {
        let printed = tree.to_string();
        let parsed = sct_ecl::parse(&printed)
            .unwrap_or_else(|e| panic!("{e}\n{printed}\n{tree:#?}"));
        prop_assert_eq!(parsed, tree, "{}", printed);
    }
}
