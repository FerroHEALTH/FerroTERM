// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The token layer of ECL: the grammar's lexer rules folded into the tokens
//! the parser consumes (`ECL.g4`, the lexer rules and `ws`, `comment`,
//! `term`, and the quoted forms).
//!
//! The grammar is written character by character; the tokens here are the
//! maximal runs a parser rule never splits: a pipe-delimited term, a quoted
//! string, an alternate identifier, a run of digits, a word, and each
//! operator. Whitespace and comments are skipped, and each token keeps its
//! byte span so the parser can require the grammar's mandatory whitespace
//! (`mws`) and its adjacency (`sctid`, `numericvalue`, `cardinality`).

use std::ops::Range;

use logos::Logos;

/// A token kind.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t\r\n]+")]
#[logos(skip r"/\*([^*]|\*[^/])*\*/")]
pub enum Kind {
    /// `|term|` (`eclconceptreference`, `altidentifier`).
    #[regex(r"\|[^|]*\|")]
    Term,
    /// A quoted string (`typedsearchterm`, `timevalue`, the quoted
    /// `altidentifier`, and the string concrete value).
    #[regex(r#""([^"\\]|\\.)*""#)]
    String,
    /// `scheme#code` (`altidentifier` without quotes).
    #[regex(r"[A-Za-z][A-Za-z0-9\-]*#[A-Za-z0-9\-._]+")]
    AltIdentifier,
    /// A run of digits (`sctid`, `integervalue`, `nonnegativeintegervalue`).
    #[regex(r"[0-9]+")]
    Integer,
    /// A word: the keywords, `refsetfieldname`, `languagecode`,
    /// `dialectalias`, and the reverse flag `R`.
    #[regex(r"[A-Za-z][A-Za-z0-9_\-]*")]
    Identifier,
    /// `<<!`
    #[token("<<!")]
    ChildOrSelfOf,
    /// `<<`
    #[token("<<")]
    DescendantOrSelfOf,
    /// `<!`
    #[token("<!")]
    ChildOf,
    /// `<=`
    #[token("<=")]
    LessOrEqual,
    /// `<`
    #[token("<")]
    LessThan,
    /// `>>!`
    #[token(">>!")]
    ParentOrSelfOf,
    /// `>>`
    #[token(">>")]
    AncestorOrSelfOf,
    /// `>!`
    #[token(">!")]
    ParentOf,
    /// `>=`
    #[token(">=")]
    GreaterOrEqual,
    /// `>`
    #[token(">")]
    GreaterThan,
    /// `!!>`
    #[token("!!>")]
    Top,
    /// `!!<`
    #[token("!!<")]
    Bottom,
    /// `!=`
    #[token("!=")]
    NotEqual,
    /// `=`
    #[token("=")]
    Equal,
    /// `(`
    #[token("(")]
    LeftParen,
    /// `)`
    #[token(")")]
    RightParen,
    /// `{{`
    #[token("{{")]
    DoubleLeftBrace,
    /// `}}`
    #[token("}}")]
    DoubleRightBrace,
    /// `{`
    #[token("{")]
    LeftBrace,
    /// `}`
    #[token("}")]
    RightBrace,
    /// `[`
    #[token("[")]
    LeftBracket,
    /// `]`
    #[token("]")]
    RightBracket,
    /// `:`
    #[token(":")]
    Colon,
    /// `,`
    #[token(",")]
    Comma,
    /// `^`
    #[token("^")]
    Caret,
    /// `..`
    #[token("..")]
    To,
    /// `.`
    #[token(".")]
    Period,
    /// `*`
    #[token("*")]
    Asterisk,
    /// `#`
    #[token("#")]
    Hash,
    /// `+`
    #[token("+")]
    Plus,
    /// `-`
    #[token("-")]
    Dash,
}

impl Kind {
    /// The token class as an error message names it.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Term => "a term between pipes",
            Self::String => "a quoted string",
            Self::AltIdentifier => "an alternate identifier",
            Self::Integer => "a number",
            Self::Identifier => "a word",
            Self::ChildOrSelfOf => "'<<!'",
            Self::DescendantOrSelfOf => "'<<'",
            Self::ChildOf => "'<!'",
            Self::LessOrEqual => "'<='",
            Self::LessThan => "'<'",
            Self::ParentOrSelfOf => "'>>!'",
            Self::AncestorOrSelfOf => "'>>'",
            Self::ParentOf => "'>!'",
            Self::GreaterOrEqual => "'>='",
            Self::GreaterThan => "'>'",
            Self::Top => "'!!>'",
            Self::Bottom => "'!!<'",
            Self::NotEqual => "'!='",
            Self::Equal => "'='",
            Self::LeftParen => "'('",
            Self::RightParen => "')'",
            Self::DoubleLeftBrace => "'{{'",
            Self::DoubleRightBrace => "'}}'",
            Self::LeftBrace => "'{'",
            Self::RightBrace => "'}'",
            Self::LeftBracket => "'['",
            Self::RightBracket => "']'",
            Self::Colon => "':'",
            Self::Comma => "','",
            Self::Caret => "'^'",
            Self::To => "'..'",
            Self::Period => "'.'",
            Self::Asterisk => "'*'",
            Self::Hash => "'#'",
            Self::Plus => "'+'",
            Self::Dash => "'-'",
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

/// Splits `input` into tokens, skipping whitespace and comments.
///
/// # Errors
///
/// Returns [`LexError`] at the first character no token starts with (an
/// unterminated term, string, or comment among them).
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
    fn operators_take_the_longest_match_and_comments_are_skipped() {
        assert_eq!(
            kinds("<<! /* c */ 123 |a b| {{ D term = \"x\" }} [1..*] LOINC#54-6 !!>"),
            [
                Kind::ChildOrSelfOf,
                Kind::Integer,
                Kind::Term,
                Kind::DoubleLeftBrace,
                Kind::Identifier,
                Kind::Identifier,
                Kind::Equal,
                Kind::String,
                Kind::DoubleRightBrace,
                Kind::LeftBracket,
                Kind::Integer,
                Kind::To,
                Kind::Asterisk,
                Kind::RightBracket,
                Kind::AltIdentifier,
                Kind::Top,
            ]
        );
        assert_eq!(
            kinds("#-5.5"),
            [
                Kind::Hash,
                Kind::Dash,
                Kind::Integer,
                Kind::Period,
                Kind::Integer
            ]
        );
        let error = lex("< 123 |unterminated").expect_err("refused");
        assert_eq!(error.offset, 6);
        assert_eq!(error.found, '|');
        assert_eq!(lex("a /* open").expect_err("refused").offset, 2);
    }
}
