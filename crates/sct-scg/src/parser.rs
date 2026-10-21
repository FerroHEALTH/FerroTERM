// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The parser: one function per rule of the normative ABNF, over the token
//! stream of [`crate::lexer`], with `winnow`.
//!
//! Every rule keeps the grammar's alternative order, so an input two
//! alternatives admit is read the way the reference grammar reads it. The
//! ABNF's adjacency (`sctId`, `numericValue`, and the `"#"` before one) is
//! checked from the token spans, since the lexer skips whitespace, and the two
//! rules whose content the lexer cannot bound (`term`, `stringValue`) are
//! checked here against the ABNF's character classes.

use winnow::combinator::{alt, cut_err, eof, opt, repeat};
use winnow::error::{ErrMode, ParserError};
use winnow::prelude::*;
use winnow::stream::{ContainsToken, Stream, TokenSlice};
use winnow::token::one_of;

use crate::ast::{
    Attribute, AttributeGroup, AttributeValue, ConceptReference, DefinitionStatus, Expression,
    Refinement, SubExpression,
};
use crate::lexer::{Kind, Token};

/// The token stream.
pub type Tokens<'i> = TokenSlice<'i, Token<'i>>;
/// A parser result.
type PResult<T> = ModalResult<T, Failure>;

/// Why a rule did not match: the token class it expected, when a rule said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Failure {
    /// The token class expected where the parser stopped.
    pub expected: Option<&'static str>,
}

impl<I: Stream> ParserError<I> for Failure {
    type Inner = Self;

    fn from_input(_: &I) -> Self {
        Self::default()
    }

    fn into_inner(self) -> Result<Self::Inner, Self> {
        Ok(self)
    }
}

impl ContainsToken<&'_ Token<'_>> for Kind {
    fn contains_token(&self, token: &Token<'_>) -> bool {
        *self == token.kind
    }
}

impl<const LEN: usize> ContainsToken<&'_ Token<'_>> for [Kind; LEN] {
    fn contains_token(&self, token: &Token<'_>) -> bool {
        self.contains(&token.kind)
    }
}

/// Names what a parser expected when it backtracks.
trait Expecting<'i, O>: Parser<Tokens<'i>, O, ErrMode<Failure>> + Sized {
    fn expecting(self, what: &'static str) -> impl Parser<Tokens<'i>, O, ErrMode<Failure>> {
        let mut parser = self;
        move |i: &mut Tokens<'i>| {
            parser.parse_next(i).map_err(|error| match error {
                ErrMode::Backtrack(_) => ErrMode::Backtrack(Failure {
                    expected: Some(what),
                }),
                other => other,
            })
        }
    }
}

impl<'i, O, P: Parser<Tokens<'i>, O, ErrMode<Failure>>> Expecting<'i, O> for P {}

fn backtrack<T>() -> PResult<T> {
    Err(ErrMode::Backtrack(Failure::default()))
}

/// Rewinds to `start` and refuses with `what`, so the error points at the
/// token the rule read, not past it.
fn refuse<'i, T>(
    i: &mut Tokens<'i>,
    start: &<Tokens<'i> as Stream>::Checkpoint,
    what: &'static str,
) -> PResult<T> {
    i.reset(start);
    Err(ErrMode::Cut(Failure {
        expected: Some(what),
    }))
}

/// One token of `kind`.
fn kind<'i>(kind: Kind) -> impl Parser<Tokens<'i>, &'i Token<'i>, ErrMode<Failure>> {
    one_of(kind).expecting(kind.describe())
}

/// The text inside a delimited token.
fn inner<'i>(token: &Token<'i>, delimiter: char) -> PResult<&'i str> {
    token
        .text
        .strip_prefix(delimiter)
        .and_then(|t| t.strip_suffix(delimiter))
        .map_or_else(backtrack, Ok)
}

/// Whether two tokens touch (no whitespace between them).
fn adjacent(previous: &Token<'_>, next: &Token<'_>) -> bool {
    previous.span.end == next.span.start
}

/// `nonwsNonPipe = %x21-7B / %x7D-7E / UTF8-2 / UTF8-3 / UTF8-4`.
///
/// Every character above `%x7E` is one of the UTF-8 sequences the ABNF spells
/// out, which `char` already guarantees, so the class is the printable ASCII
/// range without the space and the pipe, plus every non-ASCII character.
const fn non_ws_non_pipe(c: char) -> bool {
    matches!(c, '\u{21}'..='\u{7b}' | '\u{7d}' | '\u{7e}') || !c.is_ascii()
}

/// `term = nonwsNonPipe *( *SP nonwsNonPipe )`.
///
/// The `ws` around `term` in `conceptReference` absorbs any leading and
/// trailing whitespace, so the trimmed text must be a non-empty run of
/// `nonwsNonPipe` separated by spaces alone: a tab or a newline inside the
/// term is not a character the rule admits.
fn is_term(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c == ' ' || non_ws_non_pipe(c))
}

/// `anyNonEscapedChar` and the two `escapedChar` forms, unescaped.
///
/// `anyNonEscapedChar = HTAB / CR / LF / %x20-21 / %x23-5B / %x5D-7E / UTF8-*`,
/// so the quotation mark and the backslash appear only as `BS QM` and `BS BS`,
/// and no other control character appears at all. `stringValue = 1*(…)`, so an
/// empty string is not one.
fn string_value(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(next @ ('"' | '\\')) => out.push(next),
                _ => return None,
            },
            '\t' | '\r' | '\n' => out.push(c),
            '"' => return None,
            c if c.is_ascii_control() => return None,
            c => out.push(c),
        }
    }
    (!out.is_empty()).then_some(out)
}

/// `sctId = digitNonZero 5*17( digit )`: 6 to 18 digits, the first non-zero.
fn sctid(i: &mut Tokens<'_>) -> PResult<String> {
    kind(Kind::Integer)
        .verify(|t: &Token<'_>| (6..=18).contains(&t.text.len()) && !t.text.starts_with('0'))
        .map(|t: &Token<'_>| t.text.to_owned())
        .expecting("a SNOMED CT identifier")
        .parse_next(i)
}

/// `conceptReference = conceptId [ws "|" ws term ws "|"]`.
fn concept_reference(i: &mut Tokens<'_>) -> PResult<ConceptReference> {
    let concept_id = sctid.parse_next(i)?;
    let start = i.checkpoint();
    let Some(token) = opt(kind(Kind::Term)).parse_next(i)? else {
        return Ok(ConceptReference {
            concept_id,
            term: None,
        });
    };
    let text = inner(token, '|')?.trim_matches([' ', '\t', '\r', '\n']);
    if !is_term(text) {
        return refuse(i, &start, "a term");
    }
    Ok(ConceptReference {
        concept_id,
        term: Some(text.to_owned()),
    })
}

/// `integerValue = (["-"/"+"] digitNonZero *digit ) / zero`.
///
/// The sign sits inside `integerValue` and before `digitNonZero`, so the ABNF
/// admits `-5` and `0` and admits neither `-0` nor a leading zero.
fn integer_value(i: &mut Tokens<'_>) -> PResult<(String, usize)> {
    let start = i.checkpoint();
    let sign = opt(one_of([Kind::Dash, Kind::Plus])).parse_next(i)?;
    let digits = kind(Kind::Integer).parse_next(i)?;
    if let Some(sign) = sign {
        if !adjacent(sign, digits) {
            return refuse(i, &start, "a number with no space after its sign");
        }
        if digits.text.starts_with('0') {
            return refuse(i, &start, "a signed number with a non-zero first digit");
        }
    } else if digits.text.starts_with('0') && digits.text != "0" {
        return refuse(i, &start, "a number with no leading zero");
    }
    let mut out = String::new();
    if let Some(sign) = sign {
        out.push_str(sign.text);
    }
    out.push_str(digits.text);
    Ok((out, digits.span.end))
}

/// `numericValue = decimalValue / integerValue`, with
/// `decimalValue = integerValue "." 1*digit`.
fn numeric_value(i: &mut Tokens<'_>) -> PResult<String> {
    let start = i.checkpoint();
    let (integer, end) = integer_value.parse_next(i)?;
    let Some(period) = opt(kind(Kind::Period)).parse_next(i)? else {
        return Ok(integer);
    };
    // `decimalValue = integerValue "." 1*digit` concatenates with no `ws`, so
    // neither side of the point may be separated from it.
    if end != period.span.start {
        return refuse(i, &start, "a decimal with no space around its point");
    }
    let fraction = kind(Kind::Integer).parse_next(i)?;
    if !adjacent(period, fraction) {
        return refuse(i, &start, "a decimal with no space around its point");
    }
    Ok(format!("{integer}.{}", fraction.text))
}

/// `expressionValue = conceptReference / "(" ws subExpression ws ")"`.
fn expression_value(i: &mut Tokens<'_>) -> PResult<AttributeValue> {
    alt((
        concept_reference.map(AttributeValue::Concept),
        (
            kind(Kind::LeftParen),
            cut_err(sub_expression).expecting("a nested sub-expression"),
            cut_err(kind(Kind::RightParen)),
        )
            .map(|(_, sub, _)| AttributeValue::Nested(Box::new(sub))),
    ))
    .parse_next(i)
}

/// `attributeValue = expressionValue / QM stringValue QM / "#" numericValue`.
fn attribute_value(i: &mut Tokens<'_>) -> PResult<AttributeValue> {
    let start = i.checkpoint();
    if let Some(token) = opt(kind(Kind::String)).parse_next(i)? {
        let text = inner(token, '"')?;
        return match string_value(text) {
            Some(text) => Ok(AttributeValue::Text(text)),
            None => refuse(i, &start, "a string value"),
        };
    }
    if let Some(hash) = opt(kind(Kind::Hash)).parse_next(i)? {
        let next = i.peek_token().ok_or(ErrMode::Cut(Failure {
            expected: Some("a number after '#'"),
        }))?;
        if !adjacent(hash, next) {
            return refuse(i, &start, "a number touching its '#'");
        }
        return numeric_value.parse_next(i).map(AttributeValue::Number);
    }
    expression_value
        .expecting("a concept reference, a nested expression, a string, or a number")
        .parse_next(i)
}

/// `attribute = attributeName ws "=" ws attributeValue`, with
/// `attributeName = conceptReference`.
fn attribute(i: &mut Tokens<'_>) -> PResult<Attribute> {
    let name = concept_reference.parse_next(i)?;
    kind(Kind::Equal).parse_next(i)?;
    let value = cut_err(attribute_value).parse_next(i)?;
    Ok(Attribute { name, value })
}

/// `attributeSet = attribute *(ws "," ws attribute)`.
fn attribute_set(i: &mut Tokens<'_>) -> PResult<Vec<Attribute>> {
    let first = attribute.parse_next(i)?;
    let mut attributes = vec![first];
    loop {
        let start = i.checkpoint();
        if opt(kind(Kind::Comma)).parse_next(i)?.is_none() {
            break;
        }
        // NOTE: `refinement` puts an optional comma before a group too, so a
        // comma followed by `{` belongs to the refinement, not to this set.
        let Some(next) = opt(attribute).parse_next(i)? else {
            i.reset(&start);
            break;
        };
        attributes.push(next);
    }
    Ok(attributes)
}

/// `attributeGroup = "{" ws attributeSet ws "}"`.
fn attribute_group(i: &mut Tokens<'_>) -> PResult<AttributeGroup> {
    kind(Kind::LeftBrace).parse_next(i)?;
    let attributes = cut_err(attribute_set)
        .expecting("an attribute")
        .parse_next(i)?;
    cut_err(kind(Kind::RightBrace)).parse_next(i)?;
    Ok(AttributeGroup { attributes })
}

/// `refinement = (attributeSet / attributeGroup) *( ws ["," ws] attributeGroup )`.
fn refinement(i: &mut Tokens<'_>) -> PResult<Refinement> {
    let (ungrouped, first) = match opt(attribute_set).parse_next(i)? {
        Some(set) => (set, None),
        None => (Vec::new(), Some(attribute_group.parse_next(i)?)),
    };
    let rest: Vec<AttributeGroup> = repeat(0.., group_after_optional_comma).parse_next(i)?;
    let mut groups = Vec::with_capacity(rest.len().saturating_add(1));
    groups.extend(first);
    groups.extend(rest);
    Ok(Refinement { ungrouped, groups })
}

/// One `*( ws ["," ws] attributeGroup )` repetition.
fn group_after_optional_comma(i: &mut Tokens<'_>) -> PResult<AttributeGroup> {
    let start = i.checkpoint();
    opt(kind(Kind::Comma)).parse_next(i)?;
    let Some(group) = opt(attribute_group).parse_next(i)? else {
        i.reset(&start);
        return backtrack();
    };
    Ok(group)
}

/// `focusConcept = conceptReference *(ws "+" ws conceptReference)`.
fn focus_concept(i: &mut Tokens<'_>) -> PResult<Vec<ConceptReference>> {
    let first = concept_reference.parse_next(i)?;
    let mut focus = vec![first];
    while opt(kind(Kind::Plus)).parse_next(i)?.is_some() {
        focus.push(cut_err(concept_reference).parse_next(i)?);
    }
    Ok(focus)
}

/// `subExpression = focusConcept [ws ":" ws refinement]`.
fn sub_expression(i: &mut Tokens<'_>) -> PResult<SubExpression> {
    let focus = focus_concept.parse_next(i)?;
    let refinement = match opt(kind(Kind::Colon)).parse_next(i)? {
        Some(_) => Some(
            cut_err(refinement)
                .expecting("a refinement")
                .parse_next(i)?,
        ),
        None => None,
    };
    Ok(SubExpression { focus, refinement })
}

/// `definitionStatus = equivalentTo / subtypeOf`.
fn definition_status(i: &mut Tokens<'_>) -> PResult<DefinitionStatus> {
    alt((
        kind(Kind::EquivalentTo).value(DefinitionStatus::EquivalentTo),
        kind(Kind::SubtypeOf).value(DefinitionStatus::SubtypeOf),
    ))
    .parse_next(i)
}

/// `expression = ws [definitionStatus ws] subExpression ws`, to the end.
///
/// # Errors
///
/// Returns the rule's failure with the token the grammar does not admit.
pub fn whole(i: &mut Tokens<'_>) -> PResult<Expression> {
    let definition_status = opt(definition_status).parse_next(i)?;
    let sub_expression = sub_expression.expecting("an expression").parse_next(i)?;
    eof.expecting("the end of the expression").parse_next(i)?;
    Ok(Expression {
        definition_status,
        sub_expression,
    })
}
