// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The token layer of the Compositional Grammar: the ABNF's character rules
//! folded into the tokens the parser consumes (`ws`, `term`, `stringValue`,
//! `digit`, and each literal the grammar spells).
//!
//! The ABNF is written character by character; the tokens here are the maximal
//! runs a parser rule never splits: a pipe-delimited term, a quoted string, a
//! run of digits, and each operator. Whitespace is skipped, and each token
//! keeps its byte span so the parser can require the grammar's adjacency
//! (`sctId`, `numericValue`, and the `"#"` before one).

use std::ops::Range;

use logos::Logos;

/// A token kind.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t\r\n]+")]
pub enum Kind {
    /// `"|" ws term ws "|"` (`conceptReference`), with the pipes.
    ///
    /// `term` admits every character except the pipe, so the closing pipe is
    /// unambiguous and the parser checks the content against the rule.
    #[regex(r"\|[^|]*\|")]
    Term,
    /// `QM stringValue QM`, with the quotation marks.
    ///
    /// `escapedChar = BS QM / BS BS` is the whole escape set, so a backslash
    /// before anything else does not continue the string.
    #[regex(r#""([^"\\]|\\"|\\\\)*""#)]
    String,
    /// A run of digits (`sctId`, `integerValue`, `decimalValue`).
    #[regex(r"[0-9]+")]
    Integer,
    /// `equivalentTo = "==="`
    #[token("===")]
    EquivalentTo,
    /// `subtypeOf = "<<<"`
    #[token("<<<")]
    SubtypeOf,
    /// `"="` of an `attribute`.
    #[token("=")]
    Equal,
    /// `"+"` between two focus concepts, and the plus sign of a signed
    /// `integerValue`.
    #[token("+")]
    Plus,
    /// `":"` before a `refinement`.
    #[token(":")]
    Colon,
    /// `","` between two attributes or two groups.
    #[token(",")]
    Comma,
    /// `"{"` of an `attributeGroup`.
    #[token("{")]
    LeftBrace,
    /// `"}"` of an `attributeGroup`.
    #[token("}")]
    RightBrace,
    /// `"("` of a nested `subExpression`.
    #[token("(")]
    LeftParen,
    /// `")"` of a nested `subExpression`.
    #[token(")")]
    RightParen,
    /// `"#"` before a `numericValue`.
    #[token("#")]
    Hash,
    /// `"-"` of a signed `integerValue`.
    #[token("-")]
    Dash,
    /// `"."` of a `decimalValue`.
    #[token(".")]
    Period,
}

impl Kind {
    /// The token class as an error message names it.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Term => "a term between pipes",
            Self::String => "a quoted string",
            Self::Integer => "a number",
            Self::EquivalentTo => "'==='",
            Self::SubtypeOf => "'<<<'",
            Self::Equal => "'='",
            Self::Plus => "'+'",
            Self::Colon => "':'",
            Self::Comma => "','",
            Self::LeftBrace => "'{'",
            Self::RightBrace => "'}'",
            Self::LeftParen => "'('",
            Self::RightParen => "')'",
            Self::Hash => "'#'",
            Self::Dash => "'-'",
            Self::Period => "'.'",
        }
    }
}

/// A token with its source text and byte span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token<'s> {
    /// The kind.
    pub kind: Kind,
    /// The source text.
    pub text: &'s str,
    /// The byte span in the input.
    pub span: Range<usize>,
}

impl PartialEq<Kind> for Token<'_> {
    fn eq(&self, other: &Kind) -> bool {
        self.kind == *other
    }
}

/// A character no token starts with.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unexpected character {found:?} at byte {offset}")]
pub struct LexError {
    /// The byte offset of the character.
    pub offset: usize,
    /// The character.
    pub found: char,
}

/// Splits `input` into tokens, skipping whitespace.
///
/// # Errors
///
/// Returns [`LexError`] at the first character no token starts with (an
/// unterminated term or string among them).
pub fn lex(input: &str) -> Result<Vec<Token<'_>>, LexError> {
    let mut lexer = Kind::lexer(input);
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

#[cfg(test)]
mod tests {
    use super::{Kind, lex};

    fn kinds(input: &str) -> Vec<Kind> {
        lex(input)
            .expect("lexes")
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn the_definition_status_prefixes_take_the_longest_match() {
        assert_eq!(kinds("=== 123456"), [Kind::EquivalentTo, Kind::Integer]);
        assert_eq!(kinds("<<< 123456"), [Kind::SubtypeOf, Kind::Integer]);
    }

    #[test]
    fn a_refinement_lexes_into_its_literals() {
        assert_eq!(
            kinds("1 |a b| : { 2 = (3 : 4 = #-5.5) , 6 = \"t\" }"),
            [
                Kind::Integer,
                Kind::Term,
                Kind::Colon,
                Kind::LeftBrace,
                Kind::Integer,
                Kind::Equal,
                Kind::LeftParen,
                Kind::Integer,
                Kind::Colon,
                Kind::Integer,
                Kind::Equal,
                Kind::Hash,
                Kind::Dash,
                Kind::Integer,
                Kind::Period,
                Kind::Integer,
                Kind::RightParen,
                Kind::Comma,
                Kind::Integer,
                Kind::Equal,
                Kind::String,
                Kind::RightBrace,
            ]
        );
    }

    #[test]
    fn an_escape_the_grammar_does_not_define_does_not_continue_a_string() {
        // `escapedChar = BS QM / BS BS`, so a backslash before anything else
        // ends no escape and the quoted run is not a string token at all.
        let error = lex(r#""a\nb" "#).expect_err("refused");
        assert_eq!(error.offset, 0);
        assert_eq!(error.found, '"');
    }

    #[test]
    fn an_unterminated_term_or_string_is_refused_at_its_opening() {
        assert_eq!(lex("123456 |unterminated").expect_err("refused").offset, 7);
        assert_eq!(lex("123456 = \"open").expect_err("refused").offset, 9);
    }
}
