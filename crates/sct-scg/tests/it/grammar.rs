//! What the ABNF admits and what it refuses, rule by rule
//! (`vendor/syntax/Compositional Grammar v2 - ABNF (Normative).txt`).
//!
//! The identifiers are this project's own: a test ships no SNOMED CT content.

use sct_scg::ParseError;
use sct_scg::ast::{AttributeValue, DefinitionStatus};

fn parsed(text: &str) -> sct_scg::ast::Expression {
    sct_scg::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn refused(text: &str) -> ParseError {
    sct_scg::parse(text).expect_err(text)
}

#[test]
fn a_bare_concept_id_is_an_expression() {
    // `expression = ws [definitionStatus ws] subExpression ws`
    let tree = parsed("100000001");
    assert_eq!(tree.definition_status, None);
    assert_eq!(tree.sub_expression.focus.len(), 1);
    assert_eq!(tree.sub_expression.focus[0].concept_id, "100000001");
    assert!(tree.sub_expression.refinement.is_none());
}

#[test]
fn equivalent_to_is_the_status_assumed_when_none_is_written() {
    // "'equivalent to' is the assumed definition status when it is not
    // explicitly stated" (the Compositional Grammar specification, §6.7).
    assert_eq!(parsed("100000001").status(), DefinitionStatus::EquivalentTo);
    assert_eq!(
        parsed("=== 100000001").status(),
        DefinitionStatus::EquivalentTo
    );
    assert_eq!(
        parsed("<<< 100000001").status(),
        DefinitionStatus::SubtypeOf
    );
    assert_eq!(
        parsed("<<< 100000001").definition_status,
        Some(DefinitionStatus::SubtypeOf),
        "the tree keeps what was written, so the printer writes it back"
    );
}

#[test]
fn an_sctid_is_six_to_eighteen_digits_with_a_non_zero_first_digit() {
    // `sctId = digitNonZero 5*17( digit )`
    assert!(sct_scg::parse("123456").is_ok());
    assert!(sct_scg::parse("123456789012345678").is_ok());
    assert_eq!(refused("12345").offset(), 0, "five digits");
    assert_eq!(
        refused("1234567890123456789").offset(),
        0,
        "nineteen digits"
    );
    assert_eq!(refused("012345").offset(), 0, "a leading zero");
}

#[test]
fn focus_concepts_are_joined_by_a_plus() {
    // `focusConcept = conceptReference *(ws "+" ws conceptReference)`
    let tree = parsed("100000001 + 100000002 |A term|");
    assert_eq!(tree.sub_expression.focus.len(), 2);
    assert_eq!(
        tree.sub_expression.focus[1].term.as_deref(),
        Some("A term"),
        "a term is kept as written, without its surrounding whitespace"
    );
}

#[test]
fn a_term_carries_internal_spaces_and_no_other_whitespace() {
    // `term = nonwsNonPipe *( *SP nonwsNonPipe )`, with `ws` around it in
    // `conceptReference` absorbing the leading and trailing whitespace.
    assert_eq!(
        parsed("100000001 |  a shaped term  |").sub_expression.focus[0]
            .term
            .as_deref(),
        Some("a shaped term")
    );
    assert!(sct_scg::parse("100000001 |a\tterm|").is_err(), "a tab");
    assert!(sct_scg::parse("100000001 ||").is_err(), "an empty term");
}

#[test]
fn a_refinement_may_open_with_a_set_or_with_a_group() {
    // `refinement = (attributeSet / attributeGroup) *( ws ["," ws] attributeGroup )`
    let set = parsed("100000001 : 200000001 = 300000001");
    let refinement = set.sub_expression.refinement.expect("a refinement");
    assert_eq!(refinement.ungrouped.len(), 1);
    assert_eq!(
        refinement.groups,
        Vec::<sct_scg::ast::AttributeGroup>::new()
    );

    let grouped = parsed("100000001 : { 200000001 = 300000001 }");
    let refinement = grouped.sub_expression.refinement.expect("a refinement");
    assert_eq!(refinement.ungrouped, Vec::<sct_scg::ast::Attribute>::new());
    assert_eq!(refinement.groups.len(), 1);
}

#[test]
fn the_comma_between_two_adjacent_groups_is_optional() {
    // The `["," ws]` of `refinement`.
    let with = parsed("100000001 : { 200000001 = 300000001 }, { 200000002 = 300000002 }");
    let without = parsed("100000001 : { 200000001 = 300000001 } { 200000002 = 300000002 }");
    assert_eq!(with, without);
    assert_eq!(
        with.sub_expression
            .refinement
            .expect("a refinement")
            .groups
            .len(),
        2
    );
}

#[test]
fn an_ungrouped_set_may_precede_a_group_with_or_without_a_comma() {
    let with = parsed("100000001 : 200000001 = 300000001, { 200000002 = 300000002 }");
    let without = parsed("100000001 : 200000001 = 300000001 { 200000002 = 300000002 }");
    assert_eq!(with, without);
    let refinement = with.sub_expression.refinement.expect("a refinement");
    assert_eq!(refinement.ungrouped.len(), 1);
    assert_eq!(refinement.groups.len(), 1);
}

#[test]
fn an_attribute_value_may_be_a_nested_sub_expression() {
    // `expressionValue = conceptReference / "(" ws subExpression ws ")"`
    let tree = parsed("100000001 : 200000001 = (300000001 : 200000002 = 300000002)");
    let refinement = tree.sub_expression.refinement.expect("a refinement");
    let AttributeValue::Nested(nested) = &refinement.ungrouped[0].value else {
        panic!("a nested sub-expression");
    };
    assert_eq!(nested.focus[0].concept_id, "300000001");
    assert!(
        nested.refinement.is_some(),
        "a nested sub-expression carries its own refinement"
    );
}

#[test]
fn a_nested_sub_expression_carries_no_definition_status() {
    // `expressionValue` nests `subExpression`, which has no `definitionStatus`.
    assert!(sct_scg::parse("100000001 : 200000001 = (=== 300000001)").is_err());
}

#[test]
fn a_concrete_number_touches_its_hash_and_keeps_its_lexical_form() {
    // `attributeValue = … / "#" numericValue`, with no `ws` between them.
    let tree = parsed("100000001 : 200000001 = #-5.5, 200000002 = #0");
    let refinement = tree.sub_expression.refinement.expect("a refinement");
    assert_eq!(
        refinement.ungrouped[0].value,
        AttributeValue::Number(String::from("-5.5"))
    );
    assert_eq!(
        refinement.ungrouped[1].value,
        AttributeValue::Number(String::from("0"))
    );
    assert!(sct_scg::parse("100000001 : 200000001 = # 5").is_err());
}

#[test]
fn the_sign_of_an_integer_sits_before_a_non_zero_first_digit() {
    // `integerValue = (["-"/"+"] digitNonZero *digit ) / zero`, so the ABNF
    // admits neither `-0` nor a leading zero, and `decimalValue` builds on it.
    assert!(sct_scg::parse("100000001 : 200000001 = #-0").is_err());
    assert!(sct_scg::parse("100000001 : 200000001 = #01").is_err());
    assert!(sct_scg::parse("100000001 : 200000001 = #+5").is_ok());
    assert!(sct_scg::parse("100000001 : 200000001 = #0.5").is_ok());
}

#[test]
fn a_decimal_point_touches_both_of_its_sides() {
    // `decimalValue = integerValue "." 1*digit` concatenates with no `ws`.
    assert!(sct_scg::parse("100000001 : 200000001 = #5 .5").is_err());
    assert!(sct_scg::parse("100000001 : 200000001 = #5. 5").is_err());
    assert!(sct_scg::parse("100000001 : 200000001 = #- 5.5").is_err());
    assert!(sct_scg::parse("100000001 : 200000001 = #5.5").is_ok());
}

#[test]
fn a_string_value_is_non_empty_and_escapes_only_the_quote_and_the_backslash() {
    // `stringValue = 1*(anyNonEscapedChar / escapedChar)`,
    // `escapedChar = BS QM / BS BS`.
    let tree = parsed(r#"100000001 : 200000001 = "a \"quoted\" \\ value""#);
    let refinement = tree.sub_expression.refinement.expect("a refinement");
    assert_eq!(
        refinement.ungrouped[0].value,
        AttributeValue::Text(String::from(r#"a "quoted" \ value"#))
    );
    assert!(sct_scg::parse(r#"100000001 : 200000001 = """#).is_err());
    assert!(sct_scg::parse(r#"100000001 : 200000001 = "a \n b""#).is_err());
}

#[test]
fn a_syntax_error_points_at_the_byte_it_stopped_on() {
    let error = refused("100000001 : 200000001 =");
    assert!(
        matches!(error, ParseError::Syntax { .. }),
        "a syntax failure: {error}"
    );
    assert_eq!(error.offset(), "100000001 : 200000001 =".len());
    assert_eq!(refused("100000001 : 200000001 = ?").offset(), 24);
}

#[test]
fn nesting_past_the_limit_is_refused_rather_than_descended() {
    let deep = format!(
        "100000001 : 200000001 = {}300000001{}",
        "(".repeat(sct_scg::NESTING_LIMIT + 1),
        ")".repeat(sct_scg::NESTING_LIMIT + 1)
    );
    let error = sct_scg::parse(&deep).expect_err("refused");
    assert!(
        matches!(error, ParseError::TooDeep { .. }),
        "a depth failure: {error}"
    );
}

#[test]
fn the_printer_writes_a_tree_back_in_the_grammar() {
    let text = "<<< 100000001 |A| + 100000002 : 200000001 = #4, \
                { 200000002 = (300000001 : 200000003 = \"t\") }";
    assert_eq!(parsed(text).to_string(), text);
}
