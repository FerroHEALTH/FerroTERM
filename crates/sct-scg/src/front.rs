// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The front both SNOMED CT languages read through.
//!
//! It holds the token with its span, the lexing loop, the parser's failure
//! and its expectation, the nesting guard, and the parse driver that turns a
//! `winnow` error into a [`ParseError`] with a byte offset.
//!
//! No production lives here. The Compositional Grammar and the Expression
//! Constraint Language each define their own token set and rules, and each
//! crate keeps its own; this module is the machinery the two parsers run
//! them on. No specification governs it: our own design.

use std::ops::Range;

use logos::Logos;
use winnow::error::{ErrMode, ParserError};
use winnow::prelude::*;
use winnow::stream::{Stream, TokenSlice};
use winnow::token::any;

/// What a bracket does to the nesting depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nesting {
    /// The token opens a nesting level.
    Opens,
    /// The token closes one.
    Closes,
    /// The token does neither.
    Neither,
}

/// A token kind of one of the languages.
pub trait Lexeme: Copy + Eq + std::fmt::Debug {
    /// The token as an error names what was expected.
    fn describe(self) -> &'static str;

    /// What the token does to the nesting depth.
    fn nesting(self) -> Nesting;
}

/// A token with its source text and byte span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token<'s, K> {
    /// The kind.
    pub kind: K,
    /// The source text.
    pub text: &'s str,
    /// The byte span in the input.
    pub span: Range<usize>,
}

impl<K: PartialEq> PartialEq<K> for Token<'_, K> {
    fn eq(&self, other: &K) -> bool {
        self.kind == *other
    }
}

/// The token stream of the kind `K`.
pub type Tokens<'i, K> = TokenSlice<'i, Token<'i, K>>;

/// A character no token starts with.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unexpected character {found:?} at byte {offset}")]
pub struct LexError {
    /// The byte offset of the character.
    pub offset: usize,
    /// The character.
    pub found: char,
}

/// Splits `input` into tokens of `K`, skipping what `K` skips.
///
/// # Errors
///
/// Returns [`LexError`] at the first character no token starts with (an
/// unterminated term, string, or comment among them).
pub fn lex<'s, K>(input: &'s str) -> Result<Vec<Token<'s, K>>, LexError>
where
    K: Logos<'s, Source = str, Error = ()>,
    K::Extras: Default,
{
    let mut lexer = K::lexer(input);
    let mut tokens = Vec::new();
    while let Some(kind) = lexer.next() {
        let span = lexer.span();
        let kind = kind.map_err(|()| LexError {
            offset: span.start,
            found: lexer.slice().chars().next().unwrap_or('\u{0}'),
        })?;
        tokens.push(Token {
            kind,
            text: lexer.slice(),
            span,
        });
    }
    Ok(tokens)
}

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

/// A parser result.
pub type PResult<T> = ModalResult<T, Failure>;

/// Names what a parser expected when it backtracks.
pub trait Expecting<'i, K: Lexeme + 'i, O>:
    Parser<Tokens<'i, K>, O, ErrMode<Failure>> + Sized
{
    /// This parser, its backtrack naming `what`.
    fn expecting(self, what: &'static str) -> impl Parser<Tokens<'i, K>, O, ErrMode<Failure>> {
        let mut parser = self;
        move |i: &mut Tokens<'i, K>| {
            parser.parse_next(i).map_err(|error| match error {
                ErrMode::Backtrack(_) => ErrMode::Backtrack(Failure {
                    expected: Some(what),
                }),
                other => other,
            })
        }
    }
}

impl<'i, K: Lexeme + 'i, O, P: Parser<Tokens<'i, K>, O, ErrMode<Failure>>> Expecting<'i, K, O>
    for P
{
}

/// A backtrack that names nothing.
///
/// # Errors
///
/// Always: it is the refusal.
pub fn backtrack<T>() -> PResult<T> {
    Err(ErrMode::Backtrack(Failure::default()))
}

/// Rewinds to `start` and refuses with `what`, so the error points at the
/// token the rule read, not past it.
///
/// # Errors
///
/// Always: it is the refusal.
pub fn refuse<'i, K: Lexeme, T>(
    i: &mut Tokens<'i, K>,
    start: &<Tokens<'i, K> as Stream>::Checkpoint,
    what: &'static str,
) -> PResult<T> {
    i.reset(start);
    Err(ErrMode::Cut(Failure {
        expected: Some(what),
    }))
}

/// One token of `wanted`.
pub fn kind<'i, K: Lexeme + 'i>(
    wanted: K,
) -> impl Parser<Tokens<'i, K>, &'i Token<'i, K>, ErrMode<Failure>> {
    any.verify(move |t: &Token<'i, K>| t.kind == wanted)
        .expecting(wanted.describe())
}

/// One token of any of `wanted`.
pub fn one_of_kinds<'i, K: Lexeme + 'i, const N: usize>(
    wanted: [K; N],
) -> impl Parser<Tokens<'i, K>, &'i Token<'i, K>, ErrMode<Failure>> {
    any.verify(move |t: &Token<'i, K>| wanted.contains(&t.kind))
}

/// The deepest nesting of brackets an expression may carry.
///
/// No specification governs this: our own design. Both grammars are recursive
/// and the parsers descend with them, so nesting costs stack; an expression a
/// client sends is refused here rather than run until the process aborts,
/// which no `OperationOutcome` survives. Real expressions nest a handful deep.
pub const NESTING_LIMIT: usize = 64;

/// A malformed expression.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// A character no token starts with.
    #[error(transparent)]
    Lex(#[from] LexError),
    /// The expression nests deeper than the parser will descend.
    #[error("the expression nests {depth} deep at byte {offset}; the limit is {limit}")]
    TooDeep {
        /// The byte offset of the bracket that crossed the limit.
        offset: usize,
        /// The nesting depth reached there.
        depth: usize,
        /// The deepest nesting the parser admits.
        limit: usize,
    },
    /// The tokens do not form an expression.
    #[error("expected {expected} at byte {offset}, found {found}")]
    Syntax {
        /// The byte offset where the parser stopped.
        offset: usize,
        /// The token class the grammar admits there.
        expected: String,
        /// The text found there, or "the end of the expression".
        found: String,
    },
}

impl ParseError {
    /// The byte offset the error points at.
    #[must_use]
    pub const fn offset(&self) -> usize {
        match self {
            Self::Lex(error) => error.offset,
            Self::TooDeep { offset, .. } | Self::Syntax { offset, .. } => *offset,
        }
    }
}

/// The byte offset and depth of the first bracket past [`NESTING_LIMIT`].
fn too_deep<K: Lexeme>(tokens: &[Token<'_, K>]) -> Option<(usize, usize)> {
    let mut depth = 0_usize;
    for token in tokens {
        match token.kind.nesting() {
            Nesting::Opens => {
                depth = depth.saturating_add(1);
                if depth > NESTING_LIMIT {
                    return Some((token.span.start, depth));
                }
            }
            Nesting::Closes => depth = depth.saturating_sub(1),
            Nesting::Neither => {}
        }
    }
    None
}

/// Lexes `input` into tokens of `K`, refuses a nesting past
/// [`NESTING_LIMIT`], and runs `whole` over the tokens; `what` names the
/// whole input when the failing rule names nothing.
///
/// # Errors
///
/// Returns [`ParseError`] with the byte offset of the first character or
/// token the grammar does not admit.
pub fn parse<'s, K, O>(
    input: &'s str,
    what: &'static str,
    mut whole: impl for<'i> FnMut(&mut Tokens<'i, K>) -> PResult<O>,
) -> Result<O, ParseError>
where
    K: Lexeme + Logos<'s, Source = str, Error = ()>,
    K::Extras: Default,
{
    let tokens = lex::<K>(input)?;
    if let Some((offset, depth)) = too_deep(&tokens) {
        return Err(ParseError::TooDeep {
            offset,
            depth,
            limit: NESTING_LIMIT,
        });
    }
    whole.parse(TokenSlice::new(&tokens)).map_err(|error| {
        let index = error.offset();
        let (offset, found) = tokens.get(index).map_or_else(
            || (input.len(), String::from("the end of the expression")),
            |token| (token.span.start, format!("{:?}", token.text)),
        );
        ParseError::Syntax {
            offset,
            expected: String::from(error.inner().expected.unwrap_or(what)),
            found,
        }
    })
}
