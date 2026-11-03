// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SNOMED CT Compositional Grammar: the syntax of post-coordinated
//! expressions.
//!
//! A `logos` lexer and a `winnow` parser faithful to the normative ABNF
//! (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-compositional-grammar-specification/design/5-syntax-specification>),
//! a syntax tree named after the grammar's rules, and a printer whose output
//! parses back to the same tree.
//!
//! The crate is syntax only: it resolves no concept identifier, checks no
//! concept model, and decides no subsumption.
#![doc(test(attr(deny(warnings))))]

pub mod ast;
pub mod front;
pub mod lexer;
pub mod parser;
mod print;

use crate::ast::Expression;
use crate::front::ParseError;

/// Parses an expression in Compositional Grammar.
///
/// # Errors
///
/// Returns [`ParseError`] with the byte offset of the first character or token
/// the grammar does not admit. An identifier that names no concept still
/// parses; only the syntax is checked here.
///
/// # Examples
///
/// ```
/// let tree = sct_scg::parse("73211009 |Diabetes mellitus|")?;
/// assert_eq!(tree.to_string(), "73211009 |Diabetes mellitus|");
/// # Ok::<(), sct_scg::front::ParseError>(())
/// ```
pub fn parse(input: &str) -> Result<Expression, ParseError> {
    front::parse::<lexer::Kind, _>(input, "a valid expression", parser::whole)
}

impl std::str::FromStr for Expression {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse(s)
    }
}
