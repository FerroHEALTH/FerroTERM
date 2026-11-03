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

impl sct_scg::front::Lexeme for Kind {
    fn describe(self) -> &'static str {
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

    fn nesting(self) -> sct_scg::front::Nesting {
        match self {
            Self::LeftParen | Self::LeftBrace | Self::DoubleLeftBrace | Self::LeftBracket => {
                sct_scg::front::Nesting::Opens
            }
            Self::RightParen | Self::RightBrace | Self::DoubleRightBrace | Self::RightBracket => {
                sct_scg::front::Nesting::Closes
            }
            _ => sct_scg::front::Nesting::Neither,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Kind;
    use sct_scg::front::lex;

    fn kinds(input: &str) -> Vec<Kind> {
        lex::<Kind>(input)
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
        let error = lex::<Kind>("< 123 |unterminated").expect_err("refused");
        assert_eq!(error.offset, 6);
        assert_eq!(error.found, '|');
        assert_eq!(lex::<Kind>("a /* open").expect_err("refused").offset, 2);
    }
}
