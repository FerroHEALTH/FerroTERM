// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The filter rules of `ECL.g4`: the concept, description and member
//! filters with the values they compare.

use winnow::combinator::{alt, cut_err, fail, opt, preceded, separated, terminated};
use winnow::prelude::*;

use super::{
    bare_or_set, boolean, comparison, concept_reference, equality, is_scheme_alias, keyword,
    numeric_value, refset_field_name, sctid, spaced_set, string_content, sub_expression_constraint,
    typed_search_terms,
};
use crate::ast::{
    Acceptability, AcceptabilitySet, ActiveValue, ConceptFilter, ConceptReference, ConceptSet,
    DefinitionStatus, DescriptionFilter, DialectAlias, DialectIdValue, FieldValue,
    FilterConstraint, MemberFilter, TimeValue, TypeToken,
};
use crate::lexer::Kind;
use sct_scg::front::{Expecting, PResult, Token, Tokens, backtrack, kind, refuse};

/// `timevalue`: `""` or `"YYYYMMDD"`.
fn time_value(i: &mut Tokens<'_, Kind>) -> PResult<TimeValue> {
    let start = i.checkpoint();
    let text = string_content.parse_next(i)?;
    let in_range = |range: std::ops::Range<usize>, max: u8| {
        text.get(range)
            .and_then(|part| part.parse::<u8>().ok())
            .is_some_and(|value| (1..=max).contains(&value))
    };
    let valid = text.is_empty()
        || (text.len() == 8
            && text.bytes().all(|b| b.is_ascii_digit())
            && !text.starts_with('0')
            && in_range(4..6, 12)
            && in_range(6..8, 31));
    if !valid {
        return refuse(i, &start, "a time as \"YYYYMMDD\" or \"\"");
    }
    Ok(TimeValue(text.to_owned()))
}

/// `activevalue`: `1`, `0`, `true`, `false`, or `*` (ECL 2.3).
// NOTE: ECL 2.3 `activevalue` (ABNF and `ECL.g4`) admits a bare `*`; the
// specification's filter examples quote it, which neither grammar admits.
fn active_value(i: &mut Tokens<'_, Kind>) -> PResult<ActiveValue> {
    alt((
        kind(Kind::Integer).verify_map(|t: &Token<'_, Kind>| match t.text {
            "1" => Some(ActiveValue::Active),
            "0" => Some(ActiveValue::Inactive),
            _ => None,
        }),
        boolean.map(|active| {
            if active {
                ActiveValue::Active
            } else {
                ActiveValue::Inactive
            }
        }),
        kind(Kind::Asterisk).value(ActiveValue::Any),
    ))
    .expecting("1, 0, true, false, or *")
    .parse_next(i)
}

/// `eclconceptreferenceset` (two or more) or a sub-expression constraint.
///
/// The grammar lists the constraint first; a set of two or more references
/// never parses as one, so trying the set first reads every valid input the
/// same way and keeps the constraint's errors for the invalid ones.
fn concept_set(i: &mut Tokens<'_, Kind>) -> PResult<ConceptSet> {
    alt((
        spaced_set(concept_reference, 2, "two or more concept references").map(ConceptSet::Set),
        sub_expression_constraint.map(ConceptSet::Expression),
    ))
    .parse_next(i)
}

/// `acceptabilityset`.
fn acceptability_set(i: &mut Tokens<'_, Kind>) -> PResult<AcceptabilitySet> {
    alt((
        spaced_set(concept_reference, 1, "acceptability concepts").map(AcceptabilitySet::Concepts),
        spaced_set(
            alt((
                keyword("accept").value(Acceptability::Acceptable),
                keyword("prefer").value(Acceptability::Preferred),
            )),
            1,
            "accept or prefer",
        )
        .map(AcceptabilitySet::Tokens),
    ))
    .parse_next(i)
}

/// `dialectidset`, kept only when it cannot also be read as a nested
/// constraint (the grammar's first alternative for `( reference )`).
fn dialect_id_set(
    i: &mut Tokens<'_, Kind>,
) -> PResult<Vec<(ConceptReference, Option<AcceptabilitySet>)>> {
    spaced_set(
        (concept_reference, opt(acceptability_set)),
        1,
        "dialect reference sets",
    )
    .verify(
        |items: &Vec<(ConceptReference, Option<AcceptabilitySet>)>| {
            items.len() >= 2 || items.iter().any(|(_, a)| a.is_some())
        },
    )
    .parse_next(i)
}

/// `dialectalias`: a letter, then letters, digits, and dashes.
fn dialect_alias(i: &mut Tokens<'_, Kind>) -> PResult<String> {
    kind(Kind::Identifier)
        .verify(|t: &Token<'_, Kind>| is_scheme_alias(t.text))
        .map(|t| t.text.to_owned())
        .expecting("a dialect alias such as en-gb")
        .parse_next(i)
}

/// `languagecode`: two letters.
fn language_code(i: &mut Tokens<'_, Kind>) -> PResult<String> {
    kind(Kind::Identifier)
        .verify(|t: &Token<'_, Kind>| {
            t.text.len() == 2 && t.text.bytes().all(|b| b.is_ascii_alphabetic())
        })
        .map(|t| t.text.to_owned())
        .expecting("a two-letter language code")
        .parse_next(i)
}

/// `typetoken`.
fn type_token(i: &mut Tokens<'_, Kind>) -> PResult<TypeToken> {
    alt((
        keyword("syn").value(TypeToken::Synonym),
        keyword("fsn").value(TypeToken::FullySpecifiedName),
        keyword("def").value(TypeToken::Definition),
    ))
    .parse_next(i)
}

/// `definitionstatustoken`.
fn definition_status(i: &mut Tokens<'_, Kind>) -> PResult<DefinitionStatus> {
    alt((
        keyword("primitive").value(DefinitionStatus::Primitive),
        keyword("defined").value(DefinitionStatus::Defined),
    ))
    .parse_next(i)
}

/// `dialectfilter`.
fn dialect_filter(i: &mut Tokens<'_, Kind>) -> PResult<DescriptionFilter> {
    if opt(keyword("dialectId")).parse_next(i)?.is_some() {
        let (operator, value, acceptability) = cut_err((
            equality,
            alt((
                dialect_id_set.map(DialectIdValue::Set),
                sub_expression_constraint.map(DialectIdValue::Expression),
            )),
            opt(acceptability_set),
        ))
        .parse_next(i)?;
        return Ok(DescriptionFilter::DialectId {
            operator,
            value,
            acceptability,
        });
    }
    keyword("dialect").parse_next(i)?;
    let (operator, aliases, acceptability) = cut_err((
        equality,
        alt((
            spaced_set(
                (dialect_alias, opt(acceptability_set)).map(|(alias, acceptability)| {
                    DialectAlias {
                        alias,
                        acceptability,
                    }
                }),
                1,
                "dialect aliases",
            ),
            dialect_alias.map(|alias| {
                vec![DialectAlias {
                    alias,
                    acceptability: None,
                }]
            }),
        )),
        opt(acceptability_set),
    ))
    .parse_next(i)?;
    Ok(DescriptionFilter::Dialect {
        operator,
        aliases,
        acceptability,
    })
}

/// `descriptionfilter`.
fn description_filter(i: &mut Tokens<'_, Kind>) -> PResult<DescriptionFilter> {
    alt((
        preceded(keyword("term"), cut_err((equality, typed_search_terms)))
            .map(|(operator, terms)| DescriptionFilter::Term { operator, terms }),
        preceded(
            keyword("language"),
            cut_err((equality, bare_or_set(language_code, "language codes"))),
        )
        .map(|(operator, codes)| DescriptionFilter::Language { operator, codes }),
        preceded(keyword("typeId"), cut_err((equality, concept_set)))
            .map(|(operator, value)| DescriptionFilter::TypeId { operator, value }),
        preceded(
            keyword("type"),
            cut_err((equality, bare_or_set(type_token, "type tokens"))),
        )
        .map(|(operator, tokens)| DescriptionFilter::Type { operator, tokens }),
        dialect_filter,
        preceded(keyword("moduleId"), cut_err((equality, concept_set)))
            .map(|(operator, value)| DescriptionFilter::Module { operator, value }),
        preceded(
            keyword("effectiveTime"),
            cut_err((comparison, bare_or_set(time_value, "times"))),
        )
        .map(|(operator, values)| DescriptionFilter::EffectiveTime { operator, values }),
        preceded(keyword("active"), cut_err((equality, active_value)))
            .map(|(operator, value)| DescriptionFilter::Active { operator, value }),
        preceded(
            keyword("id"),
            cut_err((equality, bare_or_set(sctid, "description identifiers"))),
        )
        .map(|(operator, ids)| DescriptionFilter::Id { operator, ids }),
    ))
    .expecting("a description filter")
    .parse_next(i)
}

/// `conceptfilter`.
fn concept_filter(i: &mut Tokens<'_, Kind>) -> PResult<ConceptFilter> {
    alt((
        preceded(
            keyword("definitionStatusId"),
            cut_err((equality, concept_set)),
        )
        .map(|(operator, value)| ConceptFilter::DefinitionStatusId { operator, value }),
        preceded(
            keyword("definitionStatus"),
            cut_err((
                equality,
                bare_or_set(definition_status, "definition status tokens"),
            )),
        )
        .map(|(operator, tokens)| ConceptFilter::DefinitionStatus { operator, tokens }),
        preceded(keyword("moduleId"), cut_err((equality, concept_set)))
            .map(|(operator, value)| ConceptFilter::Module { operator, value }),
        preceded(
            keyword("effectiveTime"),
            cut_err((comparison, bare_or_set(time_value, "times"))),
        )
        .map(|(operator, values)| ConceptFilter::EffectiveTime { operator, values }),
        preceded(keyword("active"), cut_err((equality, active_value)))
            .map(|(operator, value)| ConceptFilter::Active { operator, value }),
    ))
    .expecting("a concept filter")
    .parse_next(i)
}

/// The value part of `memberfieldfilter`, in the grammar's alternative order.
fn field_value(i: &mut Tokens<'_, Kind>) -> PResult<FieldValue> {
    alt((
        (equality, sub_expression_constraint)
            .map(|(operator, value)| FieldValue::Expression { operator, value }),
        (comparison, numeric_value)
            .map(|(operator, value)| FieldValue::Numeric { operator, value }),
        (equality, typed_search_terms)
            .map(|(operator, terms)| FieldValue::String { operator, terms }),
        (equality, boolean).map(|(operator, value)| FieldValue::Boolean { operator, value }),
        (comparison, bare_or_set(time_value, "times"))
            .map(|(operator, values)| FieldValue::Time { operator, values }),
        preceded(
            alt((equality.void(), comparison.void())),
            cut_err(fail.expecting("a member field value")),
        ),
    ))
    .expecting("a comparison operator")
    .parse_next(i)
}

/// `memberfilter`.
fn member_filter(i: &mut Tokens<'_, Kind>) -> PResult<MemberFilter> {
    alt((
        preceded(keyword("moduleId"), cut_err((equality, concept_set)))
            .map(|(operator, value)| MemberFilter::Module { operator, value }),
        preceded(
            keyword("effectiveTime"),
            cut_err((comparison, bare_or_set(time_value, "times"))),
        )
        .map(|(operator, values)| MemberFilter::EffectiveTime { operator, values }),
        preceded(keyword("active"), cut_err((equality, active_value)))
            .map(|(operator, value)| MemberFilter::Active { operator, value }),
        (refset_field_name, cut_err(field_value))
            .map(|(name, value)| MemberFilter::Field { name, value }),
    ))
    .expecting("a member filter")
    .parse_next(i)
}

/// `{{ M filter, filter }}`.
pub(super) fn member_filter_constraint(i: &mut Tokens<'_, Kind>) -> PResult<Vec<MemberFilter>> {
    preceded(
        (kind(Kind::DoubleLeftBrace), keyword("M")),
        cut_err(terminated(
            separated(1.., member_filter, kind(Kind::Comma)),
            kind(Kind::DoubleRightBrace),
        )),
    )
    .parse_next(i)
}

/// `{{ C filter, filter }}`.
pub(super) fn concept_filter_constraint(i: &mut Tokens<'_, Kind>) -> PResult<FilterConstraint> {
    preceded(
        (kind(Kind::DoubleLeftBrace), keyword("C")),
        cut_err(terminated(
            separated(1.., concept_filter, kind(Kind::Comma)),
            kind(Kind::DoubleRightBrace),
        )),
    )
    .map(FilterConstraint::Concept)
    .parse_next(i)
}

/// `{{ [D] filter, filter }}`; a `{{` that opens a member filter, a concept
/// filter, or a history supplement is left to those rules.
pub(super) fn description_filter_constraint(i: &mut Tokens<'_, Kind>) -> PResult<FilterConstraint> {
    kind(Kind::DoubleLeftBrace).parse_next(i)?;
    let starts_other = i.peek_token().is_some_and(|t| {
        t.kind == Kind::Plus
            || (t.kind == Kind::Identifier
                && (t.text.eq_ignore_ascii_case("M") || t.text.eq_ignore_ascii_case("C")))
    });
    if starts_other {
        return backtrack();
    }
    opt(keyword("D")).parse_next(i)?;
    cut_err(terminated(
        separated(1.., description_filter, kind(Kind::Comma)),
        kind(Kind::DoubleRightBrace),
    ))
    .map(FilterConstraint::Description)
    .parse_next(i)
}
