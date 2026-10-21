// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The UCUM expression grammar (<https://ucum.org/ucum> §2.2 "Syntax Rules").
//!
//! `term = component | term "." component | term "/" component`, with a
//! leading `/` allowed; `component = annotatable annotation | annotatable |
//! annotation | factor | "(" term ")"`; `annotatable = simple-unit exponent |
//! simple-unit`; `simple-unit = atom | prefix atom`, a prefix only before a
//! metric atom; `exponent = ["+" | "-"] digits`; `factor = digits`;
//! `annotation = "{" text "}"`. Case sensitive.

use super::essence::Essence;

/// A parsed expression: a product of factors, each raised to an exponent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    /// The factors, in the order written, with their signed exponents.
    pub components: Vec<Component>,
}

/// One component of a term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    /// What the component is.
    pub atom: Atom,
    /// The exponent after `/` and `-n` are applied.
    pub exponent: i32,
    /// The `{annotation}`, when written.
    pub annotation: Option<String>,
}

/// The unit-bearing part of a component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Atom {
    /// A unit atom of the essence, with an optional prefix.
    Unit {
        /// The prefix code.
        prefix: Option<String>,
        /// The atom code.
        code: String,
    },
    /// A plain integer factor.
    Factor(u64),
    /// A bare annotation (`{cells}`), the unit `1`.
    Annotation,
    /// A parenthesized term, expanded in place.
    Group(Expression),
}

/// Why a text is not an expression.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GrammarError {
    /// The text is empty.
    #[error("an expression cannot be empty")]
    Empty,
    /// A character or sequence the grammar does not allow, at `at`.
    #[error("unexpected `{found}` at {at}")]
    Unexpected {
        /// The byte offset.
        at: usize,
        /// The offending text.
        found: String,
    },
    /// A symbol no prefix and atom of the essence spell.
    #[error("`{symbol}` is not a unit atom")]
    UnknownAtom {
        /// The symbol.
        symbol: String,
    },
    /// A prefix before an atom that is not metric.
    #[error("`{atom}` is not metric and takes no prefix `{prefix}`")]
    PrefixOnNonMetric {
        /// The prefix.
        prefix: String,
        /// The atom.
        atom: String,
    },
    /// An annotation without its closing brace.
    #[error("an annotation is not closed")]
    UnclosedAnnotation,
    /// A parenthesis without its match.
    #[error("a parenthesis is not closed")]
    UnclosedGroup,
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
    essence: &'a Essence,
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        self.text.get(self.at..).unwrap_or_default()
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += c.len_utf8();
        Some(c)
    }

    fn unexpected(&self) -> GrammarError {
        GrammarError::Unexpected {
            at: self.at,
            found: self.rest().chars().take(8).collect(),
        }
    }

    /// `term`, up to a closing parenthesis or the end.
    fn term(&mut self) -> Result<Expression, GrammarError> {
        let mut components = Vec::new();
        let mut sign = 1;
        if self.peek() == Some('/') {
            self.bump();
            sign = -1;
        }
        loop {
            let mut component = self.component()?;
            component.exponent *= sign;
            components.push(component);
            match self.peek() {
                Some('.') => {
                    self.bump();
                    sign = 1;
                }
                Some('/') => {
                    self.bump();
                    sign = -1;
                }
                Some(')') | None => break,
                Some(_) => return Err(self.unexpected()),
            }
        }
        Ok(Expression { components })
    }

    /// `component`.
    fn component(&mut self) -> Result<Component, GrammarError> {
        match self.peek() {
            None => Err(GrammarError::Empty),
            Some('(') => {
                self.bump();
                let inner = self.term()?;
                if self.bump() != Some(')') {
                    return Err(GrammarError::UnclosedGroup);
                }
                Ok(Component {
                    atom: Atom::Group(inner),
                    exponent: self.exponent()?,
                    annotation: self.annotation()?,
                })
            }
            Some('{') => Ok(Component {
                atom: Atom::Annotation,
                exponent: 1,
                annotation: self.annotation()?,
            }),
            // NOTE: `10*` and `10^` are atoms that begin with digits; any other digit
            // sequence is a factor (<https://ucum.org/ucum> section 2.2).
            Some(c)
                if c.is_ascii_digit()
                    && !self.rest().starts_with("10*")
                    && !self.rest().starts_with("10^") =>
            {
                let start = self.at;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.bump();
                }
                let digits = self.text.get(start..self.at).unwrap_or_default();
                // The diagnostic names the position; a `ParseIntError` adds nothing to it.
                let Ok(value) = digits.parse::<u64>() else {
                    return Err(self.unexpected());
                };
                Ok(Component {
                    atom: Atom::Factor(value),
                    exponent: 1,
                    annotation: self.annotation()?,
                })
            }
            Some(_) => {
                let symbol = self.symbol();
                if symbol.is_empty() {
                    return Err(self.unexpected());
                }
                let atom = self.resolve(symbol)?;
                Ok(Component {
                    atom,
                    exponent: self.exponent()?,
                    annotation: self.annotation()?,
                })
            }
        }
    }

    /// The characters of a unit symbol: anything up to an operator, digit
    /// (an exponent), brace, or parenthesis; brackets enclose freely.
    fn symbol(&mut self) -> &'a str {
        let start = self.at;
        let mut depth = 0;
        while let Some(c) = self.peek() {
            match c {
                '[' => depth += 1,
                ']' => depth -= 1,
                '.' | '/' | '{' | '}' | '(' | ')' | '+' | '-' if depth == 0 => break,
                '0'..='9' if depth == 0 => {
                    // NOTE: `10*` and `10^` are atoms whose symbol holds digits; other
                    // digits after a symbol start the exponent (<https://ucum.org/ucum> §2.2).
                    let so_far = self.text.get(start..self.at).unwrap_or_default();
                    if !(so_far.is_empty() || so_far == "1") {
                        break;
                    }
                }
                _ => {}
            }
            self.bump();
        }
        self.text.get(start..self.at).unwrap_or_default()
    }

    /// The atom, or a prefix and a metric atom, that spell `symbol`.
    fn resolve(&self, symbol: &str) -> Result<Atom, GrammarError> {
        if self.essence.base_units.contains_key(symbol) || self.essence.units.contains_key(symbol) {
            return Ok(Atom::Unit {
                prefix: None,
                code: symbol.to_owned(),
            });
        }
        let mut candidates: Vec<&String> = self.essence.prefixes.keys().collect();
        candidates.sort_by_key(|p| std::cmp::Reverse(p.len()));
        for prefix in candidates {
            if let Some(atom) = symbol.strip_prefix(prefix.as_str()) {
                if self.essence.base_units.contains_key(atom) {
                    return Ok(Atom::Unit {
                        prefix: Some(prefix.clone()),
                        code: atom.to_owned(),
                    });
                }
                if let Some(unit) = self.essence.units.get(atom) {
                    if !unit.is_metric {
                        return Err(GrammarError::PrefixOnNonMetric {
                            prefix: prefix.clone(),
                            atom: atom.to_owned(),
                        });
                    }
                    return Ok(Atom::Unit {
                        prefix: Some(prefix.clone()),
                        code: atom.to_owned(),
                    });
                }
            }
        }
        Err(GrammarError::UnknownAtom {
            symbol: symbol.to_owned(),
        })
    }

    /// An optional signed integer exponent.
    fn exponent(&mut self) -> Result<i32, GrammarError> {
        let start = self.at;
        if matches!(self.peek(), Some('+' | '-')) {
            self.bump();
        }
        let digits_start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        if self.at == digits_start {
            if start != self.at {
                return Err(self.unexpected());
            }
            return Ok(1);
        }
        // The diagnostic names the position; a `ParseIntError` adds nothing to it.
        let Ok(value) = self
            .text
            .get(start..self.at)
            .unwrap_or_default()
            .parse::<i32>()
        else {
            return Err(self.unexpected());
        };
        Ok(value)
    }

    /// An optional `{annotation}`.
    fn annotation(&mut self) -> Result<Option<String>, GrammarError> {
        if self.peek() != Some('{') {
            return Ok(None);
        }
        self.bump();
        let start = self.at;
        while let Some(c) = self.peek() {
            if c == '}' {
                let text = self.text.get(start..self.at).unwrap_or_default().to_owned();
                self.bump();
                return Ok(Some(text));
            }
            if c == '{' {
                return Err(self.unexpected());
            }
            self.bump();
        }
        Err(GrammarError::UnclosedAnnotation)
    }
}

/// Parses `text` against the grammar and the atoms of `essence`.
///
/// # Errors
///
/// Returns [`GrammarError`] for a text the grammar or the essence rejects.
pub fn parse(text: &str, essence: &Essence) -> Result<Expression, GrammarError> {
    if text.is_empty() {
        return Err(GrammarError::Empty);
    }
    if text.bytes().any(|b| !(0x20..=0x7e).contains(&b)) {
        return Err(GrammarError::Unexpected {
            at: 0,
            found: String::from("a character outside printable ASCII"),
        });
    }
    let mut parser = Parser {
        text,
        at: 0,
        essence,
    };
    let expression = parser.term()?;
    if parser.at != text.len() {
        return Err(parser.unexpected());
    }
    Ok(expression)
}
