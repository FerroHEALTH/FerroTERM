// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The parser: one function per rule of `ECL.g4`, over the token stream of
//! [`crate::lexer`], with `winnow`.
//!
//! Every rule keeps the grammar's alternative order, so an input that two
//! alternatives admit is read the way the reference grammar reads it. The
//! grammar's mandatory whitespace (`mws`) and its adjacency (a number, a
//! cardinality) are checked from the token spans, since the lexer skips
//! whitespace.

use winnow::combinator::{
    alt, cut_err, delimited, eof, fail, opt, peek, preceded, repeat, separated, terminated,
};
use winnow::error::{ErrMode, ParserError};
use winnow::prelude::*;
use winnow::stream::{ContainsToken, Stream, TokenSlice};
use winnow::token::{any, one_of};

use crate::ast::{
    Acceptability, AcceptabilitySet, AltIdentifier, Attribute, AttributeSet, AttributeValue,
    Cardinality, Comparison, ConceptFilter, ConceptReference, ConceptSet, ConstraintOperator,
    DefinitionStatus, DescriptionFilter, DialectAlias, DialectIdValue, Equality,
    ExpressionConstraint, FieldValue, FilterConstraint, FocusConcept, HistorySupplement,
    MemberFilter, MemberOf, NumericValue, Refinement, RefsetFields, Sctid, SubAttributeSet,
    SubExpressionConstraint, SubRefinement, TimeValue, TypeToken, TypedSearchTerm,
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

/// Names what a parser expected when it backtracks; a cut error keeps the
/// deeper rule's own expectation.
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

/// A word, compared case-insensitively (every keyword of the grammar spells
/// each letter as `(CAP_X | X)`).
fn keyword<'i>(word: &'static str) -> impl Parser<Tokens<'i>, &'i Token<'i>, ErrMode<Failure>> {
    any.verify(move |t: &Token<'i>| t.kind == Kind::Identifier && t.text.eq_ignore_ascii_case(word))
        .expecting(word)
}

/// Whether whitespace or a comment separates two tokens (`mws`).
fn separated_by_space(previous: &Token<'_>, next: &Token<'_>) -> bool {
    previous.span.end < next.span.start
}

/// Whether two tokens touch (no whitespace between them).
fn adjacent(previous: &Token<'_>, next: &Token<'_>) -> bool {
    previous.span.end == next.span.start
}

/// A junction word that the grammar follows with mandatory whitespace
/// (`conjunction`, `disjunction`, `exclusion`).
fn junction_word<'i>(word: &'static str) -> impl Parser<Tokens<'i>, (), ErrMode<Failure>> {
    move |i: &mut Tokens<'i>| {
        let word = keyword(word).parse_next(i)?;
        let next = cut_err(peek(any).expecting("whitespace after the junction")).parse_next(i)?;
        if !separated_by_space(word, next) {
            return Err(ErrMode::Cut(Failure {
                expected: Some("whitespace after the junction"),
            }));
        }
        Ok(())
    }
}

/// `conjunction`: `AND` or `,`.
fn conjunction(i: &mut Tokens<'_>) -> PResult<()> {
    alt((junction_word("AND"), kind(Kind::Comma).void())).parse_next(i)
}

/// `disjunction`: `OR`.
fn disjunction(i: &mut Tokens<'_>) -> PResult<()> {
    junction_word("OR").parse_next(i)
}

/// `exclusion`: `MINUS`.
fn exclusion(i: &mut Tokens<'_>) -> PResult<()> {
    junction_word("MINUS").parse_next(i)
}

/// The text between a token's delimiters (the pipes of a term, the quotes
/// of a string).
fn inner<'i>(token: &Token<'i>, delimiter: char) -> PResult<&'i str> {
    token
        .text
        .strip_prefix(delimiter)
        .and_then(|t| t.strip_suffix(delimiter))
        .map_or_else(backtrack, Ok)
}

/// `( item (mws item)* )`, at least `min` items, each separated from the
/// previous one by whitespace.
fn spaced_set<'i, T>(
    mut item: impl Parser<Tokens<'i>, T, ErrMode<Failure>>,
    min: usize,
    what: &'static str,
) -> impl Parser<Tokens<'i>, Vec<T>, ErrMode<Failure>> {
    move |i: &mut Tokens<'i>| {
        kind(Kind::LeftParen).parse_next(i)?;
        let mut items = vec![item.parse_next(i)?];
        loop {
            let start = i.checkpoint();
            let Some(next) = i.peek_token() else { break };
            let Some(previous) = i.previous_tokens().next() else {
                break;
            };
            if !separated_by_space(previous, next) {
                break;
            }
            match item.parse_next(i) {
                Ok(value) => items.push(value),
                Err(ErrMode::Backtrack(_)) => {
                    i.reset(&start);
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        if items.len() < min {
            return Err(ErrMode::Backtrack(Failure {
                expected: Some(what),
            }));
        }
        kind(Kind::RightParen).parse_next(i)?;
        Ok(items)
    }
}

/// One item bare, or a set of them.
fn bare_or_set<'i, T>(
    mut item: impl Parser<Tokens<'i>, T, ErrMode<Failure>>,
    what: &'static str,
) -> impl Parser<Tokens<'i>, Vec<T>, ErrMode<Failure>> {
    move |i: &mut Tokens<'i>| {
        if i.peek_token().is_some_and(|t| t.kind == Kind::LeftParen) {
            return spaced_set(item.by_ref(), 1, what).parse_next(i);
        }
        item.parse_next(i).map(|one| vec![one])
    }
}

/// `sctid`: 6 to 18 digits, the first not zero.
fn sctid(i: &mut Tokens<'_>) -> PResult<Sctid> {
    kind(Kind::Integer)
        .verify_map(|t: &Token<'_>| {
            let digits = t.text.len();
            ((6..=18).contains(&digits) && !t.text.starts_with('0'))
                .then(|| t.text.parse().ok().map(Sctid))
                .flatten()
        })
        .expecting("a SNOMED CT identifier")
        .parse_next(i)
}

/// `term`: the text between pipes, without its surrounding whitespace; one
/// line, not empty.
fn term(i: &mut Tokens<'_>) -> PResult<String> {
    let start = i.checkpoint();
    let token = kind(Kind::Term).parse_next(i)?;
    let text = inner(token, '|')?.trim();
    if text.is_empty() || text.contains(['\t', '\r', '\n']) {
        return refuse(i, &start, "a term on one line");
    }
    Ok(text.to_owned())
}

/// `eclconceptreference`.
fn concept_reference(i: &mut Tokens<'_>) -> PResult<ConceptReference> {
    let id = sctid.parse_next(i)?;
    let term = opt(term).parse_next(i)?;
    Ok(ConceptReference { id, term })
}

/// `altidentifierschemealias`: `alpha (dash | alpha | integervalue)*`.
fn is_scheme_alias(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// `altidentifier`: `scheme#code` bare or in quotes, with an optional term.
fn alt_identifier(i: &mut Tokens<'_>) -> PResult<AltIdentifier> {
    let token = one_of([Kind::AltIdentifier, Kind::String]).parse_next(i)?;
    let text = if token.kind == Kind::String {
        inner(token, '"')?
    } else {
        token.text
    };
    let Some((scheme, code)) = text.split_once('#') else {
        return backtrack();
    };
    if !is_scheme_alias(scheme) || code.is_empty() || code.contains('\\') {
        return backtrack();
    }
    let term = opt(term).parse_next(i)?;
    Ok(AltIdentifier {
        scheme: scheme.to_owned(),
        code: code.to_owned(),
        term,
    })
}

/// `eclfocusconcept`, or the parenthesized nested constraint.
fn focus(i: &mut Tokens<'_>) -> PResult<FocusConcept> {
    alt((
        kind(Kind::Asterisk).value(FocusConcept::Wildcard),
        concept_reference.map(FocusConcept::Reference),
        alt_identifier.map(FocusConcept::AltIdentifier),
        delimited(
            kind(Kind::LeftParen),
            expression_constraint,
            kind(Kind::RightParen),
        )
        .map(|inner| FocusConcept::Nested(Box::new(inner))),
    ))
    .expecting("a concept reference, '*', an alternate identifier, or '('")
    .parse_next(i)
}

/// `constraintoperator`.
fn constraint_operator(i: &mut Tokens<'_>) -> PResult<ConstraintOperator> {
    any.verify_map(|t: &Token<'_>| match t.kind {
        Kind::LessThan => Some(ConstraintOperator::DescendantOf),
        Kind::DescendantOrSelfOf => Some(ConstraintOperator::DescendantOrSelfOf),
        Kind::ChildOf => Some(ConstraintOperator::ChildOf),
        Kind::ChildOrSelfOf => Some(ConstraintOperator::ChildOrSelfOf),
        Kind::GreaterThan => Some(ConstraintOperator::AncestorOf),
        Kind::AncestorOrSelfOf => Some(ConstraintOperator::AncestorOrSelfOf),
        Kind::ParentOf => Some(ConstraintOperator::ParentOf),
        Kind::ParentOrSelfOf => Some(ConstraintOperator::ParentOrSelfOf),
        Kind::Top => Some(ConstraintOperator::Top),
        Kind::Bottom => Some(ConstraintOperator::Bottom),
        _ => None,
    })
    .expecting("a constraint operator")
    .parse_next(i)
}

/// `refsetfieldname`: letters only.
fn refset_field_name(i: &mut Tokens<'_>) -> PResult<String> {
    kind(Kind::Identifier)
        .verify(|t: &Token<'_>| t.text.bytes().all(|b| b.is_ascii_alphabetic()))
        .map(|t| t.text.to_owned())
        .expecting("a reference set field name")
        .parse_next(i)
}

/// `memberof`: `^`, optionally `[ fields ]`.
fn member_of(i: &mut Tokens<'_>) -> PResult<MemberOf> {
    kind(Kind::Caret).parse_next(i)?;
    let fields = opt(delimited(
        kind(Kind::LeftBracket),
        cut_err(alt((
            kind(Kind::Asterisk).value(RefsetFields::Any),
            separated(1.., refset_field_name, kind(Kind::Comma)).map(RefsetFields::Names),
        ))),
        cut_err(kind(Kind::RightBracket)),
    ))
    .parse_next(i)?;
    Ok(MemberOf { fields })
}

/// `subexpressionconstraint`.
fn sub_expression_constraint(i: &mut Tokens<'_>) -> PResult<SubExpressionConstraint> {
    let operator = opt(constraint_operator).parse_next(i)?;
    let member_of = opt(member_of).parse_next(i)?;
    let focus = if operator.is_some() || member_of.is_some() {
        cut_err(focus).parse_next(i)?
    } else {
        focus.parse_next(i)?
    };
    let member_filters = repeat(0.., member_filter_constraint).parse_next(i)?;
    let filters = repeat(
        0..,
        alt((concept_filter_constraint, description_filter_constraint)),
    )
    .parse_next(i)?;
    let history = opt(history_supplement).parse_next(i)?;
    Ok(SubExpressionConstraint {
        operator,
        member_of,
        focus,
        member_filters,
        filters,
        history,
    })
}

/// The operands after the first of a compound constraint, joined by
/// `junction`.
fn operands<'i>(
    first: SubExpressionConstraint,
    junction: impl Parser<Tokens<'i>, (), ErrMode<Failure>>,
    i: &mut Tokens<'i>,
) -> PResult<Option<Vec<SubExpressionConstraint>>> {
    let rest: Option<Vec<SubExpressionConstraint>> = opt(repeat(
        1..,
        preceded(junction, cut_err(sub_expression_constraint)),
    ))
    .parse_next(i)?;
    Ok(rest.map(|rest| {
        let mut all = vec![first];
        all.extend(rest);
        all
    }))
}

/// `expressionconstraint`: a sub-expression, then what follows it decides
/// the form (`:` refined, `.` dotted, a junction compound, else bare).
fn expression_constraint(i: &mut Tokens<'_>) -> PResult<ExpressionConstraint> {
    let first = sub_expression_constraint.parse_next(i)?;
    if opt(kind(Kind::Colon)).parse_next(i)?.is_some() {
        let refinement = cut_err(refinement).parse_next(i)?;
        return Ok(ExpressionConstraint::Refined {
            focus: first,
            refinement: Box::new(refinement),
        });
    }
    if i.peek_token().is_some_and(|t| t.kind == Kind::Period) {
        let attributes = repeat(
            1..,
            preceded(kind(Kind::Period), cut_err(sub_expression_constraint)),
        )
        .parse_next(i)?;
        return Ok(ExpressionConstraint::Dotted {
            focus: first,
            attributes,
        });
    }
    if let Some(all) = operands(first.clone(), conjunction, i)? {
        return Ok(ExpressionConstraint::Conjunction(all));
    }
    if let Some(all) = operands(first.clone(), disjunction, i)? {
        return Ok(ExpressionConstraint::Disjunction(all));
    }
    if let Some(right) =
        opt(preceded(exclusion, cut_err(sub_expression_constraint))).parse_next(i)?
    {
        return Ok(ExpressionConstraint::Exclusion { left: first, right });
    }
    Ok(ExpressionConstraint::Sub(first))
}

/// `nonnegativeintegervalue`: digits without a leading zero, or `0`.
fn non_negative_integer(token: &Token<'_>) -> Option<u32> {
    (token.text == "0" || !token.text.starts_with('0'))
        .then(|| token.text.parse().ok())
        .flatten()
}

/// `cardinality` between brackets: `[min..max]`, written without spaces.
fn cardinality(i: &mut Tokens<'_>) -> PResult<Cardinality> {
    let start = i.checkpoint();
    let open = kind(Kind::LeftBracket).parse_next(i)?;
    let (min, to, max, close) = cut_err(
        (
            kind(Kind::Integer),
            kind(Kind::To),
            one_of([Kind::Integer, Kind::Asterisk]),
            kind(Kind::RightBracket),
        )
            .expecting("a cardinality such as [1..*]"),
    )
    .parse_next(i)?;
    let tight =
        adjacent(open, min) && adjacent(min, to) && adjacent(to, max) && adjacent(max, close);
    let min_value = non_negative_integer(min);
    let max_value = if max.kind == Kind::Asterisk {
        Some(None)
    } else {
        non_negative_integer(max).map(Some)
    };
    match (tight, min_value, max_value) {
        (true, Some(min), Some(max)) => Ok(Cardinality { min, max }),
        _ => refuse(i, &start, "a cardinality such as [1..*]"),
    }
}

/// `expressioncomparisonoperator` and the other `=` / `!=` operators.
fn equality(i: &mut Tokens<'_>) -> PResult<Equality> {
    alt((
        kind(Kind::Equal).value(Equality::Equal),
        kind(Kind::NotEqual).value(Equality::NotEqual),
    ))
    .parse_next(i)
}

/// `numericcomparisonoperator` and `timecomparisonoperator`.
fn comparison(i: &mut Tokens<'_>) -> PResult<Comparison> {
    alt((
        kind(Kind::Equal).value(Comparison::Equal),
        kind(Kind::NotEqual).value(Comparison::NotEqual),
        kind(Kind::LessOrEqual).value(Comparison::LessOrEqual),
        kind(Kind::LessThan).value(Comparison::Less),
        kind(Kind::GreaterOrEqual).value(Comparison::GreaterOrEqual),
        kind(Kind::GreaterThan).value(Comparison::Greater),
    ))
    .parse_next(i)
}

/// `HASH numericvalue`: `#`, an optional sign, digits, an optional fraction,
/// all touching.
fn numeric_value(i: &mut Tokens<'_>) -> PResult<NumericValue> {
    let start = i.checkpoint();
    let hash = kind(Kind::Hash).parse_next(i)?;
    let sign = opt(one_of([Kind::Dash, Kind::Plus])).parse_next(i)?;
    let integer = cut_err(kind(Kind::Integer)).parse_next(i)?;
    let fraction = opt((kind(Kind::Period), kind(Kind::Integer))).parse_next(i)?;
    let mut previous = hash;
    let mut text = String::new();
    for token in sign
        .into_iter()
        .chain(std::iter::once(integer))
        .chain(fraction.iter().flat_map(|(dot, digits)| [*dot, *digits]))
    {
        if !adjacent(previous, token) {
            return refuse(i, &start, "a number written without spaces");
        }
        text.push_str(token.text);
        previous = token;
    }
    if integer.text != "0" && integer.text.starts_with('0') {
        return refuse(i, &start, "a number without a leading zero");
    }
    Ok(NumericValue(text))
}

/// `booleanvalue`.
fn boolean(i: &mut Tokens<'_>) -> PResult<bool> {
    alt((keyword("true").value(true), keyword("false").value(false))).parse_next(i)
}

/// A quoted string's content.
fn string_content<'i>(i: &mut Tokens<'i>) -> PResult<&'i str> {
    let token = kind(Kind::String).parse_next(i)?;
    inner(token, '"')
}

/// Whether every backslash in `text` escapes one of `allowed`.
fn escapes_only(text: &str, allowed: &[char]) -> bool {
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' && !chars.next().is_some_and(|e| allowed.contains(&e)) {
            return false;
        }
    }
    true
}

/// `matchsearchtermset` content: words separated by whitespace or comments,
/// each with `\"` and `\\` as its only escapes.
fn match_words(text: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix("/*") {
            let end = after.find("*/")?;
            rest = after.get(end.saturating_add(2)..)?;
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            continue;
        }
        rest = rest.get(c.len_utf8()..)?;
        if matches!(c, ' ' | '\t' | '\r' | '\n') {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else if c == '\\' {
            let escaped = rest.chars().next().filter(|e| matches!(e, '"' | '\\'))?;
            rest = rest.get(escaped.len_utf8()..)?;
            word.push('\\');
            word.push(escaped);
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    (!words.is_empty()).then_some(words)
}

/// `typedsearchterm`: `wild:"pattern"`, or `[match:]"words"`.
fn typed_search_term(i: &mut Tokens<'_>) -> PResult<TypedSearchTerm> {
    if opt((keyword("wild"), kind(Kind::Colon)))
        .parse_next(i)?
        .is_some()
    {
        let start = i.checkpoint();
        let pattern = cut_err(string_content).parse_next(i)?;
        if pattern.is_empty() || !escapes_only(pattern, &['"', '\\', '*']) {
            return refuse(i, &start, "a wildcard search term");
        }
        return Ok(TypedSearchTerm::Wild(pattern.to_owned()));
    }
    let explicit = opt((keyword("match"), kind(Kind::Colon)))
        .parse_next(i)?
        .is_some();
    let start = i.checkpoint();
    let content = if explicit {
        cut_err(string_content).parse_next(i)?
    } else {
        string_content.parse_next(i)?
    };
    match match_words(content) {
        Some(words) => Ok(TypedSearchTerm::Match(words)),
        None => refuse(i, &start, "one or more search words"),
    }
}

/// `typedsearchterm | typedsearchtermset`.
fn typed_search_terms(i: &mut Tokens<'_>) -> PResult<Vec<TypedSearchTerm>> {
    bare_or_set(typed_search_term, "a search term").parse_next(i)
}

/// `timevalue`: `""` or `"YYYYMMDD"`.
fn time_value(i: &mut Tokens<'_>) -> PResult<TimeValue> {
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

/// `activevalue`: `1`, `0`, `true`, or `false`.
fn active_value(i: &mut Tokens<'_>) -> PResult<bool> {
    alt((
        kind(Kind::Integer).verify_map(|t: &Token<'_>| match t.text {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        }),
        boolean,
    ))
    .expecting("1, 0, true, or false")
    .parse_next(i)
}

/// `eclconceptreferenceset` (two or more) or a sub-expression constraint.
///
/// The grammar lists the constraint first; a set of two or more references
/// never parses as one, so trying the set first reads every valid input the
/// same way and keeps the constraint's errors for the invalid ones.
fn concept_set(i: &mut Tokens<'_>) -> PResult<ConceptSet> {
    alt((
        spaced_set(concept_reference, 2, "two or more concept references").map(ConceptSet::Set),
        sub_expression_constraint.map(ConceptSet::Expression),
    ))
    .parse_next(i)
}

/// `acceptabilityset`.
fn acceptability_set(i: &mut Tokens<'_>) -> PResult<AcceptabilitySet> {
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
    i: &mut Tokens<'_>,
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
fn dialect_alias(i: &mut Tokens<'_>) -> PResult<String> {
    kind(Kind::Identifier)
        .verify(|t: &Token<'_>| is_scheme_alias(t.text))
        .map(|t| t.text.to_owned())
        .expecting("a dialect alias such as en-gb")
        .parse_next(i)
}

/// `languagecode`: two letters.
fn language_code(i: &mut Tokens<'_>) -> PResult<String> {
    kind(Kind::Identifier)
        .verify(|t: &Token<'_>| {
            t.text.len() == 2 && t.text.bytes().all(|b| b.is_ascii_alphabetic())
        })
        .map(|t| t.text.to_owned())
        .expecting("a two-letter language code")
        .parse_next(i)
}

/// `typetoken`.
fn type_token(i: &mut Tokens<'_>) -> PResult<TypeToken> {
    alt((
        keyword("syn").value(TypeToken::Synonym),
        keyword("fsn").value(TypeToken::FullySpecifiedName),
        keyword("def").value(TypeToken::Definition),
    ))
    .parse_next(i)
}

/// `definitionstatustoken`.
fn definition_status(i: &mut Tokens<'_>) -> PResult<DefinitionStatus> {
    alt((
        keyword("primitive").value(DefinitionStatus::Primitive),
        keyword("defined").value(DefinitionStatus::Defined),
    ))
    .parse_next(i)
}

/// `dialectfilter`.
fn dialect_filter(i: &mut Tokens<'_>) -> PResult<DescriptionFilter> {
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
fn description_filter(i: &mut Tokens<'_>) -> PResult<DescriptionFilter> {
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
fn concept_filter(i: &mut Tokens<'_>) -> PResult<ConceptFilter> {
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
fn field_value(i: &mut Tokens<'_>) -> PResult<FieldValue> {
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
fn member_filter(i: &mut Tokens<'_>) -> PResult<MemberFilter> {
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
fn member_filter_constraint(i: &mut Tokens<'_>) -> PResult<Vec<MemberFilter>> {
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
fn concept_filter_constraint(i: &mut Tokens<'_>) -> PResult<FilterConstraint> {
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
fn description_filter_constraint(i: &mut Tokens<'_>) -> PResult<FilterConstraint> {
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

/// `historysupplement`.
fn history_supplement(i: &mut Tokens<'_>) -> PResult<HistorySupplement> {
    (kind(Kind::DoubleLeftBrace), kind(Kind::Plus)).parse_next(i)?;
    let start = i.checkpoint();
    let word = cut_err(kind(Kind::Identifier).expecting("HISTORY")).parse_next(i)?;
    let profile = match word.text.to_ascii_uppercase().as_str() {
        "HISTORY" => None,
        "HISTORY-MIN" | "HISTORY_MIN" => Some(HistorySupplement::Minimum),
        "HISTORY-MOD" | "HISTORY_MOD" => Some(HistorySupplement::Moderate),
        "HISTORY-MAX" | "HISTORY_MAX" => Some(HistorySupplement::Maximum),
        _ => {
            return refuse(
                i,
                &start,
                "HISTORY, HISTORY-MIN, HISTORY-MOD, or HISTORY-MAX",
            );
        }
    };
    let supplement = match profile {
        Some(profile) => profile,
        None => match opt(delimited(
            kind(Kind::LeftParen),
            cut_err(expression_constraint),
            cut_err(kind(Kind::RightParen)),
        ))
        .parse_next(i)?
        {
            Some(subset) => HistorySupplement::Subset(Box::new(subset)),
            None => HistorySupplement::Default,
        },
    };
    cut_err(kind(Kind::DoubleRightBrace)).parse_next(i)?;
    Ok(supplement)
}

/// `eclattribute`. The cardinality is not committed to: a group starts the
/// same way.
fn attribute(i: &mut Tokens<'_>) -> PResult<Attribute> {
    let cardinality = opt(cardinality).parse_next(i)?;
    let reverse = opt(keyword("R")).parse_next(i)?.is_some();
    let name = if reverse {
        cut_err(sub_expression_constraint).parse_next(i)?
    } else {
        sub_expression_constraint.parse_next(i)?
    };
    let value = cut_err(attribute_value).parse_next(i)?;
    Ok(Attribute {
        cardinality,
        reverse,
        name,
        value,
    })
}

/// The comparison and value of `eclattribute`, in the grammar's order.
fn attribute_value(i: &mut Tokens<'_>) -> PResult<AttributeValue> {
    alt((
        (equality, sub_expression_constraint)
            .map(|(operator, value)| AttributeValue::Expression { operator, value }),
        (comparison, numeric_value)
            .map(|(operator, value)| AttributeValue::Numeric { operator, value }),
        (equality, typed_search_terms)
            .map(|(operator, terms)| AttributeValue::String { operator, terms }),
        (equality, boolean).map(|(operator, value)| AttributeValue::Boolean { operator, value }),
        preceded(
            alt((equality.void(), comparison.void())),
            cut_err(fail.expecting("an attribute value")),
        ),
    ))
    .expecting("a comparison operator")
    .parse_next(i)
}

/// `subattributeset`.
fn sub_attribute_set(i: &mut Tokens<'_>) -> PResult<SubAttributeSet> {
    alt((
        attribute.map(|attribute| SubAttributeSet::Attribute(Box::new(attribute))),
        delimited(kind(Kind::LeftParen), attribute_set, kind(Kind::RightParen))
            .map(|set| SubAttributeSet::Nested(Box::new(set))),
    ))
    .parse_next(i)
}

/// The items after the first of a junction-joined set.
fn joined<'i, T: Clone>(
    first: &T,
    junction: impl Parser<Tokens<'i>, (), ErrMode<Failure>>,
    item: impl Parser<Tokens<'i>, T, ErrMode<Failure>>,
    i: &mut Tokens<'i>,
) -> PResult<Option<Vec<T>>> {
    let rest: Option<Vec<T>> = opt(repeat(1.., preceded(junction, item))).parse_next(i)?;
    Ok(rest.map(|rest| {
        let mut all = vec![first.clone()];
        all.extend(rest);
        all
    }))
}

/// `eclattributeset`.
fn attribute_set(i: &mut Tokens<'_>) -> PResult<AttributeSet> {
    let first = sub_attribute_set.parse_next(i)?;
    if let Some(all) = joined(&first, conjunction, sub_attribute_set, i)? {
        return Ok(AttributeSet::Conjunction(all));
    }
    if let Some(all) = joined(&first, disjunction, sub_attribute_set, i)? {
        return Ok(AttributeSet::Disjunction(all));
    }
    Ok(AttributeSet::Single(Box::new(first)))
}

/// `eclattributegroup`.
fn attribute_group(i: &mut Tokens<'_>) -> PResult<SubRefinement> {
    let cardinality = opt(cardinality).parse_next(i)?;
    kind(Kind::LeftBrace).parse_next(i)?;
    let attributes = cut_err(attribute_set).parse_next(i)?;
    cut_err(kind(Kind::RightBrace)).parse_next(i)?;
    Ok(SubRefinement::Group {
        cardinality,
        attributes,
    })
}

/// `subrefinement`, in the grammar's order: an attribute set, a group, a
/// parenthesized refinement.
fn sub_refinement(i: &mut Tokens<'_>) -> PResult<SubRefinement> {
    alt((
        attribute_set.map(SubRefinement::AttributeSet),
        attribute_group,
        delimited(
            kind(Kind::LeftParen),
            cut_err(refinement),
            cut_err(kind(Kind::RightParen)),
        )
        .map(|inner| SubRefinement::Nested(Box::new(inner))),
    ))
    .expecting("an attribute, an attribute group, or '('")
    .parse_next(i)
}

/// `eclrefinement`.
fn refinement(i: &mut Tokens<'_>) -> PResult<Refinement> {
    let first = sub_refinement.parse_next(i)?;
    if let Some(all) = joined(&first, conjunction, cut_err(sub_refinement), i)? {
        return Ok(Refinement::Conjunction(all));
    }
    if let Some(all) = joined(&first, disjunction, cut_err(sub_refinement), i)? {
        return Ok(Refinement::Disjunction(all));
    }
    Ok(Refinement::Single(Box::new(first)))
}

/// A whole input: one expression constraint and nothing after it.
///
/// # Errors
///
/// Returns the failure at the first token the grammar does not admit, with
/// the token class expected there when a rule names one.
pub fn whole(i: &mut Tokens<'_>) -> PResult<ExpressionConstraint> {
    let constraint = expression_constraint
        .expecting("an expression constraint")
        .parse_next(i)?;
    eof.expecting("the end of the expression").parse_next(i)?;
    Ok(constraint)
}
