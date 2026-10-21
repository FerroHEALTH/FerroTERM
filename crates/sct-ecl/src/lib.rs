// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Expression Constraint Language.
//!
//! A `logos` lexer and a `winnow` parser faithful to the official ECL ANTLR
//! grammar (`vendor/syntax/ECL.g4`, the pinned tag in `docs/VERSIONS.md`;
//! <https://docs.snomed.org/snomed-ct-specifications/snomed-ct-expression-constraint-language>),
//! a syntax tree named after the grammar's rules, and a printer whose output
//! parses back to the same tree.
#![doc(test(attr(deny(warnings))))]

pub mod ast;
pub mod dialects;
pub mod eval;
pub mod lexer;
pub mod parser;
mod print;

use winnow::prelude::*;
use winnow::stream::TokenSlice;

use crate::ast::ExpressionConstraint;
use crate::lexer::LexError;

/// A malformed expression constraint.
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
    /// The tokens do not form an expression constraint.
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
    pub fn offset(&self) -> usize {
        match self {
            Self::Lex(error) => error.offset,
            Self::TooDeep { offset, .. } | Self::Syntax { offset, .. } => *offset,
        }
    }
}

/// The deepest nesting of `(`, `{`, `{{`, or `[` an expression may carry.
///
/// No specification governs this: our own design. The grammar is recursive and
/// the parser descends with it, so nesting costs stack; an expression a client
/// sends is refused here rather than run until the process aborts, which no
/// `OperationOutcome` survives. Real expressions nest a handful deep.
pub const NESTING_LIMIT: usize = 64;

/// The byte offset and depth of the first bracket past [`NESTING_LIMIT`].
fn too_deep(tokens: &[lexer::Token<'_>]) -> Option<(usize, usize)> {
    let mut depth = 0_usize;
    for token in tokens {
        match token.kind {
            lexer::Kind::LeftParen
            | lexer::Kind::LeftBrace
            | lexer::Kind::DoubleLeftBrace
            | lexer::Kind::LeftBracket => {
                depth = depth.saturating_add(1);
                if depth > NESTING_LIMIT {
                    return Some((token.span.start, depth));
                }
            }
            lexer::Kind::RightParen
            | lexer::Kind::RightBrace
            | lexer::Kind::DoubleRightBrace
            | lexer::Kind::RightBracket => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

/// Parses an expression constraint.
///
/// # Errors
///
/// Returns [`ParseError`] with the byte offset of the first character or
/// token the grammar does not admit. An identifier that names no concept
/// still parses; the evaluator refuses it.
///
/// # Examples
///
/// ```
/// let tree = sct_ecl::parse("<< 73211009 |Diabetes mellitus|")?;
/// assert_eq!(tree.to_string(), "<< 73211009 |Diabetes mellitus|");
/// # Ok::<(), sct_ecl::ParseError>(())
/// ```
pub fn parse(input: &str) -> Result<ExpressionConstraint, ParseError> {
    let tokens = lexer::lex(input)?;
    if let Some((offset, depth)) = too_deep(&tokens) {
        return Err(ParseError::TooDeep {
            offset,
            depth,
            limit: NESTING_LIMIT,
        });
    }
    parser::whole
        .parse(TokenSlice::new(&tokens))
        .map_err(|error| {
            let index = error.offset();
            let (offset, found) = tokens.get(index).map_or_else(
                || (input.len(), String::from("the end of the expression")),
                |token| (token.span.start, format!("{:?}", token.text)),
            );
            ParseError::Syntax {
                offset,
                expected: String::from(
                    error
                        .inner()
                        .expected
                        .unwrap_or("a valid expression constraint"),
                ),
                found,
            }
        })
}

impl std::str::FromStr for ExpressionConstraint {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse(s)
    }
}
