// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the grammar admits and refuses, one behaviour per case
//! (`vendor/syntax/ECL.g4`; the rule named in each test).

use sct_ecl::ast::{
    AttributeSet, AttributeValue, Cardinality, Comparison, ConstraintOperator, Equality,
    ExpressionConstraint, FieldValue, FocusConcept, HistorySupplement, MemberFilter, Refinement,
    RefsetFields, Sctid, SubAttributeSet, SubRefinement, TypedSearchTerm,
};
use sct_ecl::{ParseError, parse};

fn refused(input: &str) -> (usize, String) {
    match parse(input) {
        Err(ParseError::Syntax {
            offset, expected, ..
        }) => (offset, expected),
        Err(ParseError::Lex(error)) => (error.offset, String::from("lex")),
        Err(ParseError::TooDeep { offset, .. }) => (offset, String::from("nesting")),
        Ok(tree) => panic!("{input:?} parsed: {tree}"),
    }
}

fn sub(input: &str) -> sct_ecl::ast::SubExpressionConstraint {
    match parse(input).expect("parses") {
        ExpressionConstraint::Sub(sub) => sub,
        other => panic!("{input:?} is {other:?}"),
    }
}

#[test]
fn constraint_operators_and_the_focus_forms() {
    for (text, operator) in [
        ("<", ConstraintOperator::DescendantOf),
        ("<<", ConstraintOperator::DescendantOrSelfOf),
        ("<!", ConstraintOperator::ChildOf),
        ("<<!", ConstraintOperator::ChildOrSelfOf),
        (">", ConstraintOperator::AncestorOf),
        (">>", ConstraintOperator::AncestorOrSelfOf),
        (">!", ConstraintOperator::ParentOf),
        (">>!", ConstraintOperator::ParentOrSelfOf),
        ("!!>", ConstraintOperator::Top),
        ("!!<", ConstraintOperator::Bottom),
    ] {
        assert_eq!(
            sub(&format!("{text} 404684003")).operator,
            Some(operator),
            "{text}"
        );
        assert_eq!(
            sub(&format!("{text}404684003")).operator,
            Some(operator),
            "no space"
        );
    }
    assert_eq!(sub("*").focus, FocusConcept::Wildcard);
    let reference = sub("404684003 |  Clinical finding  |");
    let FocusConcept::Reference(reference) = reference.focus else {
        panic!("a reference");
    };
    assert_eq!(reference.id, Sctid(404_684_003));
    assert_eq!(reference.term.as_deref(), Some("Clinical finding"));
    let FocusConcept::AltIdentifier(alt) = sub("\"LOINC#54486 6\" |x|").focus else {
        panic!("an alternate identifier");
    };
    assert_eq!(
        (alt.scheme.as_str(), alt.code.as_str()),
        ("LOINC", "54486 6")
    );
    assert!(matches!(sub("(< 123456)").focus, FocusConcept::Nested(_)));
}

#[test]
fn sctids_are_six_to_eighteen_digits_without_a_leading_zero() {
    assert!(parse("123456").is_ok());
    assert!(parse("123456789012345678").is_ok());
    assert_eq!(refused("12345").0, 0, "five digits");
    assert_eq!(refused("1234567890123456789").0, 0, "nineteen digits");
    assert_eq!(refused("0123456").0, 0, "leading zero");
    assert_eq!(refused("123456 ||").0, 7, "an empty term");
    assert_eq!(refused("123456 |a\tb|").0, 7, "a tab in a term");
}

#[test]
fn member_of_with_its_field_selection() {
    let fields = |input: &str| sub(input).member_of.expect("member of").fields;
    assert_eq!(fields("^ 700043003"), None);
    assert_eq!(fields("^ [*] 700043003"), Some(RefsetFields::Any));
    assert_eq!(
        fields("^ [targetComponentId, mapTarget] 700043003"),
        Some(RefsetFields::Names(vec![
            String::from("targetComponentId"),
            String::from("mapTarget")
        ]))
    );
    assert_eq!(
        refused("^ [map2] 700043003").0,
        3,
        "a field name is letters only"
    );
    assert_eq!(refused("^").0, 1, "nothing after the caret");
}

#[test]
fn compound_constraints_keep_one_junction_and_need_whitespace_after_it() {
    assert!(matches!(
        parse("< 19829001 AND < 301867009, ^ 700043003").expect("parses"),
        ExpressionConstraint::Conjunction(operands) if operands.len() == 3
    ));
    assert!(matches!(
        parse("< 19829001 or < 301867009 OR < 111115").expect("parses"),
        ExpressionConstraint::Disjunction(operands) if operands.len() == 3
    ));
    assert!(matches!(
        parse("<< 19829001 minus << 301867009").expect("parses"),
        ExpressionConstraint::Exclusion { .. }
    ));
    assert_eq!(
        refused("< 19829001 AND < 301867009 OR < 111115").0,
        27,
        "mixed junctions"
    );
    assert_eq!(
        refused("< 19829001 MINUS < 301867009 MINUS < 111115").0,
        29,
        "one exclusion"
    );
    assert_eq!(
        refused("< 19829001 AND<301867009").0,
        14,
        "no whitespace after AND"
    );
    assert_eq!(refused("< 19829001 AND").0, 14, "a dangling junction");
}

#[test]
fn dotted_constraints_walk_attribute_names() {
    let ExpressionConstraint::Dotted { attributes, .. } =
        parse("< 19829001 . < 47429007 . 363698007").expect("parses")
    else {
        panic!("dotted");
    };
    assert_eq!(attributes.len(), 2);
    assert_eq!(
        attributes[0].operator,
        Some(ConstraintOperator::DescendantOf),
        "an attribute name is a sub-expression"
    );
    assert_eq!(
        refused("< 19829001 . 363698007 AND < 111115").0,
        23,
        "a dotted form is not an operand"
    );
    assert_eq!(refused("< 19829001 .").0, 12);
}

#[test]
fn refinements_attributes_cardinalities_and_reverse_flags() {
    let mixed = "< 404684003 : [1..3] R 363698007 = < 91723000, { [0..0] 116676008 != << 26036001 } OR 42752001 = *";
    assert_eq!(
        refused(mixed).0,
        mixed.find(" OR ").expect("OR") + 1,
        "a refinement has one kind of junction"
    );
    let ExpressionConstraint::Refined { refinement, .. } =
        parse("< 404684003 : [1..3] R 363698007 = < 91723000, { [0..0] 116676008 != << 26036001 }")
            .expect("parses")
    else {
        panic!("refined");
    };
    let Refinement::Conjunction(items) = *refinement else {
        panic!("attribute set, group: {refinement:?}");
    };
    assert_eq!(items.len(), 2);
    let SubRefinement::Group { cardinality, .. } = &items[1] else {
        panic!("the group is the second item: {items:?}");
    };
    assert_eq!(*cardinality, None);
    let ExpressionConstraint::Refined { refinement, .. } =
        parse("< 404684003 : [1..3] R 363698007 = < 91723000").expect("parses")
    else {
        panic!("refined");
    };
    let Refinement::Single(single) = *refinement else {
        panic!("one attribute: {refinement:?}");
    };
    let SubRefinement::AttributeSet(AttributeSet::Single(single)) = *single else {
        panic!("one attribute: {single:?}");
    };
    let SubAttributeSet::Attribute(attribute) = *single else {
        panic!("one attribute: {single:?}");
    };
    assert_eq!(
        attribute.cardinality,
        Some(Cardinality {
            min: 1,
            max: Some(3)
        })
    );
    assert!(attribute.reverse);
    assert!(matches!(
        attribute.value,
        AttributeValue::Expression {
            operator: Equality::Equal,
            ..
        }
    ));
    assert_eq!(
        refused("< 404684003 : [1 .. 3] 363698007 = *").0,
        14,
        "a cardinality has no spaces"
    );
    assert_eq!(
        refused("< 404684003 : [3..1x] 363698007 = *").0,
        19,
        "the bracket must close after the maximum"
    );
    assert_eq!(refused("< 404684003 :").0, 13, "an empty refinement");
    assert_eq!(
        refused("< 404684003 : 363698007").0,
        23,
        "an attribute without a value"
    );
    assert_eq!(
        refused("< 404684003 : 363698007 = ").0,
        26,
        "a value missing"
    );
}

#[test]
fn refinement_junctions_bind_attribute_sets_before_groups() {
    let ExpressionConstraint::Refined { refinement, .. } =
        parse("< 404684003 : 111115 = * , 111116 = * OR 111117 = *").expect("parses")
    else {
        panic!("refined");
    };
    let Refinement::Disjunction(items) = *refinement else {
        panic!("(a, b) OR c: {refinement:?}");
    };
    assert!(
        matches!(&items[0], SubRefinement::AttributeSet(AttributeSet::Conjunction(pair)) if pair.len() == 2)
    );
    let ExpressionConstraint::Refined { refinement, .. } =
        parse("< 404684003 : 111115 = * AND 111116 = * OR 111117 = * AND 111118 = *")
            .expect("parses")
    else {
        panic!("refined");
    };
    assert!(
        matches!(&*refinement, Refinement::Disjunction(items) if items.len() == 2),
        "attribute-set junctions bind before refinement junctions: {refinement:?}"
    );
    let groups = "< 404684003 : { 111115 = * } AND { 111116 = * } OR { 111117 = * }";
    assert_eq!(
        refused(groups).0,
        groups.find(" OR ").expect("OR") + 1,
        "groups join at one level only"
    );
}

#[test]
fn concrete_values_follow_the_grammar_order() {
    let value = |input: &str| {
        let ExpressionConstraint::Refined { refinement, .. } = parse(input).expect("parses") else {
            panic!("refined");
        };
        let Refinement::Single(single) = *refinement else {
            panic!("one attribute");
        };
        let SubRefinement::AttributeSet(AttributeSet::Single(single)) = *single else {
            panic!("one attribute");
        };
        let SubAttributeSet::Attribute(attribute) = *single else {
            panic!("one attribute");
        };
        attribute.value
    };
    assert!(matches!(
        value("< 111115 : 111116 >= #500"),
        AttributeValue::Numeric { operator: Comparison::GreaterOrEqual, value } if value.0 == "500"
    ));
    assert!(matches!(
        value("< 111115 : 111116 = #-5.25"),
        AttributeValue::Numeric { value, .. } if value.0 == "-5.25"
    ));
    assert!(matches!(
        value("< 111115 : 111116 = \"PANADOL\""),
        AttributeValue::String { terms, .. } if terms == [TypedSearchTerm::Match(vec![String::from("PANADOL")])]
    ));
    assert!(matches!(
        value("< 111115 : 111116 = (\"a\" wild:\"b*\")"),
        AttributeValue::String { terms, .. } if terms.len() == 2
    ));
    assert!(matches!(
        value("< 111115 : 111116 != TRUE"),
        AttributeValue::Boolean {
            operator: Equality::NotEqual,
            value: true
        }
    ));
    assert!(
        matches!(
            value("< 111115 : 111116 = LOINC#1234-5"),
            AttributeValue::Expression { .. }
        ),
        "an alternate identifier is a constraint before a string"
    );
    assert_eq!(
        refused("< 111115 : 111116 = # 500").0,
        20,
        "no space after #"
    );
    assert_eq!(refused("< 111115 : 111116 = #05").0, 20, "no leading zero");
    assert_eq!(
        refused("< 111115 : 111116 = 5").0,
        20,
        "a bare number is not a concrete value"
    );
    assert_eq!(
        refused("< 111115 : 111116 > 111117").0,
        20,
        "ordering needs a number"
    );
    assert_eq!(
        refused("< 111115 : 111116 = \"\"").0,
        20,
        "a search needs a word"
    );
    assert_eq!(
        refused("< 111115 : 111116 = (\"a\"\"b\")").0,
        20,
        "set items need whitespace"
    );
}

#[test]
fn description_filters_each_keyword_and_its_value_forms() {
    let filters = |input: &str| sub(input).filters;
    assert_eq!(filters("* {{ term = \"heart att\" }}").len(), 1);
    assert_eq!(
        filters("* {{ D term = \"heart\", term = \"att\" }} {{ d term = wild:\"*itis\" }}").len(),
        2
    );
    assert!(parse("* {{ term = match:\"gas\" }}").is_ok());
    assert!(parse("* {{ term = (match:\"gas\" wild:\"*itis\") }}").is_ok());
    assert!(parse("* {{ language = sv }} {{ language = (sv EN) }}").is_ok());
    assert!(parse("* {{ typeId = 900000000000013009 }} {{ typeId = ( 900000000000013009 900000000000003001 ) }} {{ typeId = < 900000000000446008 }}").is_ok());
    assert!(parse("* {{ type = fsn }} {{ type = (syn fsn def) }}").is_ok());
    assert!(parse("* {{ dialect = en-gb }} {{ dialect = (en-nhs-clinical en-nhs-pharmacy) }} {{ dialect = en-us (prefer) }} {{ dialect = ( en-gb (accept) en-us ) (prefer accept) }}").is_ok());
    assert!(parse("* {{ dialectId = 900000000000508004 }} {{ dialectId = ( 900000000000508004 (prefer) ) }} {{ dialectId = ( 900000000000508004 900000000000509007 ) ( 900000000000548007 ) }}").is_ok());
    assert!(parse("* {{ moduleId = 900000000000207008 }} {{ effectiveTime >= \"20190731\" }} {{ effectiveTime = (\"\" \"20200131\") }} {{ active = 1 }} {{ active != false }} {{ id = 670169018 }} {{ id = (670169018 670169019) }}").is_ok());
    assert_eq!(refused("* {{ language = sve }}").0, 16, "two letters");
    assert_eq!(refused("* {{ type = synonym }}").0, 12);
    assert_eq!(
        refused("* {{ effectiveTime = \"2019073\" }}").0,
        21,
        "eight digits"
    );
    assert_eq!(
        refused("* {{ effectiveTime = \"20191331\" }}").0,
        21,
        "a month"
    );
    assert_eq!(refused("* {{ active = 2 }}").0, 14);
    assert_eq!(refused("* {{ D }}").0, 7, "an empty filter list");
    assert_eq!(refused("* {{ colour = red }}").0, 5, "an unknown filter");
    assert_eq!(refused("* {{ term = \"x\" }").0, 16, "one closing brace");
}

#[test]
fn concept_member_filters_and_history_in_their_order() {
    let constraint = sub(
        "^ 447562003 {{ M mapGroup != #2, mapPriority < #2, mapTarget = wild:\"J*\", active = 1, effectiveTime < \"20190101\" }} {{ C definitionStatus = primitive, moduleId = 900000000000207008 }} {{ D term = \"x\" }} {{ + HISTORY-MOD }}",
    );
    assert_eq!(constraint.member_filters.len(), 1);
    assert_eq!(constraint.member_filters[0].len(), 5);
    assert!(
        matches!(&constraint.member_filters[0][2], MemberFilter::Field { name, value: FieldValue::String { .. } } if name == "mapTarget")
    );
    assert!(
        matches!(
            &constraint.member_filters[0][4],
            MemberFilter::EffectiveTime {
                operator: Comparison::Less,
                ..
            }
        ),
        "the keyword filter comes before a field of that name"
    );
    let field = sub("^ 447562003 {{ M sourceEffectiveTime >= \"20190101\" }}");
    assert!(
        matches!(
            &field.member_filters[0][0],
            MemberFilter::Field {
                name,
                value: FieldValue::Time {
                    operator: Comparison::GreaterOrEqual,
                    ..
                }
            } if name == "sourceEffectiveTime"
        ),
        "an ordering with a quoted value is a time"
    );
    assert_eq!(constraint.filters.len(), 2);
    assert_eq!(constraint.history, Some(HistorySupplement::Moderate));
    assert!(matches!(
        sub("<< 195967001 {{ + HISTORY }}").history,
        Some(HistorySupplement::Default)
    ));
    assert!(matches!(
        sub("<< 195967001 {{ + HISTORY_MAX }}").history,
        Some(HistorySupplement::Maximum)
    ));
    assert!(matches!(
        sub("<< 195967001 {{ + history-min }}").history,
        Some(HistorySupplement::Minimum)
    ));
    assert!(matches!(
        sub("<< 195967001 {{ + HISTORY ( 900000000000527005 OR 900000000000526001 ) }}").history,
        Some(HistorySupplement::Subset(_))
    ));
    assert!(
        matches!(
            sub("^ 447562003 {{ M mapTarget = \"J45.9\" }}").member_filters[0][0],
            MemberFilter::Field {
                value: FieldValue::String { .. },
                ..
            }
        ),
        "= with a quoted value is a string, as the grammar orders"
    );
    assert_eq!(
        refused("<< 195967001 {{ D term = \"x\" }} {{ M mapTarget = \"J\" }}").0,
        32,
        "member filters come first"
    );
    assert_eq!(
        refused("<< 195967001 {{ + HISTORY }} {{ C active = 1 }}").0,
        29,
        "history comes last"
    );
    assert_eq!(
        refused("<< 195967001 {{ + HISTORY -MIN }}").0,
        26,
        "the profile suffix touches the keyword"
    );
    assert_eq!(refused("<< 195967001 {{ + PAST }}").0, 18);
    assert_eq!(refused("<< 195967001 {{ M }}").0, 18);
}

#[test]
fn comments_are_whitespace_and_unterminated_forms_are_lexer_errors() {
    assert!(
        parse("/* lead */ < 19829001 /* mid */ : /* x */ 116676008 = << 79654002 /* tail */")
            .is_ok()
    );
    assert!(
        parse("* {{ term = \"heart /* c */ att\" }}").is_ok(),
        "a comment inside a search splits words"
    );
    let unterminated = parse("< 19829001 /* open").expect_err("refused");
    assert_eq!(unterminated.offset(), 11);
    assert!(matches!(unterminated, ParseError::Lex(_)));
    assert_eq!(
        parse("< 19829001 \"open").expect_err("refused").offset(),
        11
    );
    assert_eq!(parse("< 19829001 ?").expect_err("refused").offset(), 11);
    assert_eq!(refused("").0, 0);
    assert_eq!(refused("< 19829001 )").0, 11, "an unbalanced parenthesis");
    assert_eq!(refused("( < 19829001").0, 12);
}

#[test]
fn errors_name_the_offset_and_what_was_expected() {
    let (offset, expected) = refused("<< ");
    assert_eq!(offset, 3);
    assert!(expected.contains("concept reference"), "{expected}");
    let error = parse("< 404684003 : 363698007 = ").expect_err("refused");
    assert_eq!(error.offset(), 26);
    assert_eq!(
        error.to_string(),
        "expected an attribute value at byte 26, found the end of the expression"
    );
    let (offset, expected) = refused("< 404684003 )");
    assert_eq!(offset, 12);
    assert!(expected.contains("end of the expression"), "{expected}");
}

/// An expression nests as deep as the parser will descend, and no deeper.
///
/// The grammar is recursive and so is the parser, so an unbounded nesting from
/// a client would exhaust the stack and abort the process, which no
/// `OperationOutcome` survives. No specification governs the limit: our own
/// design, refused as a parse error like any other malformed expression.
#[test]
fn nesting_past_the_limit_is_refused_rather_than_descended() {
    let nested = |depth: usize| format!("{}138875005{}", "(".repeat(depth), ")".repeat(depth));
    let at_limit = nested(sct_ecl::NESTING_LIMIT);
    assert!(
        parse(&at_limit).is_ok(),
        "an expression at the limit still parses"
    );
    match parse(&nested(sct_ecl::NESTING_LIMIT + 1)) {
        Err(ParseError::TooDeep {
            offset,
            depth,
            limit,
        }) => {
            assert_eq!(limit, sct_ecl::NESTING_LIMIT, "the limit is reported");
            assert_eq!(depth, sct_ecl::NESTING_LIMIT + 1, "the depth reached");
            assert_eq!(
                offset,
                sct_ecl::NESTING_LIMIT,
                "the bracket that crossed it"
            );
        }
        other => panic!("nesting past the limit gave {other:?}"),
    }
    let unbalanced = "(".repeat(10_000);
    assert!(
        matches!(parse(&unbalanced), Err(ParseError::TooDeep { .. })),
        "an unclosed run of brackets is refused before the descent"
    );
}

/// Closing a bracket makes room again: a wide expression is not a deep one.
#[test]
fn nesting_depth_counts_open_brackets_not_all_of_them() {
    let wide = format!("{} 138875005", "(138875005) OR ".repeat(200));
    assert!(parse(&wide).is_ok(), "200 closed groups nest one deep");
}
