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

use sct_scg::front::{self, ParseError};

use crate::ast::ExpressionConstraint;

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
/// # Ok::<(), sct_scg::front::ParseError>(())
/// ```
pub fn parse(input: &str) -> Result<ExpressionConstraint, ParseError> {
    front::parse::<lexer::Kind, _>(input, "a valid expression constraint", parser::whole)
}

impl std::str::FromStr for ExpressionConstraint {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse(s)
    }
}
