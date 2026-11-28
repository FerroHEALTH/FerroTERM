// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The concrete ranges of the MRCM attribute range reference set, written in
//! the `concreteValueReplacement` rule of the SNOMED CT Template Syntax
//! (§5.1 Normative Specification): `int(>#0..)`, `dec(#0..#10)`,
//! `str("a" "b")`, `bool(true)`.

use std::cmp::Ordering;

/// The type of a concrete domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Integer,
    Decimal,
    String,
    Boolean,
}

/// One bound of a numeric range.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Bound {
    value: String,
    exclusive: bool,
}

/// One member of a numeric set: a value, or a range with optional ends.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    Value(String),
    Range(Option<Bound>, Option<Bound>),
}

/// A parsed concrete range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Concrete {
    kind: Kind,
    /// The numeric values and ranges, empty for "any value of the type".
    items: Vec<Item>,
    /// The allowed strings or booleans, empty for any.
    tokens: Vec<String>,
}

impl Concrete {
    /// The range `text` states, or `None` when it is not a concrete range.
    pub(super) fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (kind, rest) = [
            ("int", Kind::Integer),
            ("dec", Kind::Decimal),
            ("str", Kind::String),
            ("bool", Kind::Boolean),
        ]
        .into_iter()
        .find_map(|(word, kind)| text.strip_prefix(word).map(|rest| (kind, rest.trim())))?;
        let inner = if rest.is_empty() {
            ""
        } else {
            rest.strip_prefix('(')?.strip_suffix(')')?.trim()
        };
        let mut concrete = Self {
            kind,
            items: Vec::new(),
            tokens: Vec::new(),
        };
        match kind {
            Kind::Integer | Kind::Decimal => {
                for part in inner.split_whitespace() {
                    concrete.items.push(item(part)?);
                }
            }
            Kind::String => {
                for token in inner.split('"').skip(1).step_by(2) {
                    concrete.tokens.push(token.to_owned());
                }
            }
            Kind::Boolean => {
                concrete.tokens = inner.split_whitespace().map(str::to_owned).collect();
            }
        }
        Some(concrete)
    }

    /// Whether the range is over integers (`Some(true)`) or decimals
    /// (`Some(false)`); `None` for a string or boolean range.
    pub(super) fn integral(&self) -> Option<bool> {
        match self.kind {
            Kind::Integer => Some(true),
            Kind::Decimal => Some(false),
            Kind::String | Kind::Boolean => None,
        }
    }

    /// Whether the concrete number `number` (`#4000`, `#2.5`) is in the range.
    pub(super) fn holds_number(&self, number: &str) -> bool {
        let number = number.trim_start_matches('#');
        let integral = !number.contains('.');
        match self.kind {
            Kind::Integer if !integral => false,
            Kind::Integer | Kind::Decimal => {
                self.items.is_empty() || self.items.iter().any(|item| contains(item, number))
            }
            Kind::String | Kind::Boolean => false,
        }
    }

    /// Whether the concrete string or boolean `text` is in the range.
    pub(super) fn holds_text(&self, text: &str) -> bool {
        match self.kind {
            Kind::String | Kind::Boolean => {
                self.tokens.is_empty() || self.tokens.iter().any(|token| token == text)
            }
            Kind::Integer | Kind::Decimal => false,
        }
    }
}

/// One `slotIntegerValue`, `slotDecimalValue` or range of them.
fn item(part: &str) -> Option<Item> {
    let Some((low, high)) = part.split_once("..") else {
        return value(part).map(Item::Value);
    };
    let low = if low.is_empty() {
        None
    } else {
        Some(bound(low, '>')?)
    };
    let high = if high.is_empty() {
        None
    } else {
        Some(bound(high, '<')?)
    };
    (low.is_some() || high.is_some()).then_some(Item::Range(low, high))
}

/// A written bound with its exclusive marker.
fn bound(text: &str, exclusive: char) -> Option<Bound> {
    let (exclusive, text) = match text.strip_prefix(exclusive) {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    Some(Bound {
        value: value(text)?,
        exclusive,
    })
}

/// `#` and a signed number, as the number.
fn value(text: &str) -> Option<String> {
    let number = text.strip_prefix('#')?;
    let digits = number.trim_start_matches(['-', '+']);
    let valid = !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
        && digits.matches('.').count() <= 1;
    valid.then(|| number.to_owned())
}

fn contains(item: &Item, number: &str) -> bool {
    match item {
        Item::Value(value) => compare(number, value) == Some(Ordering::Equal),
        Item::Range(low, high) => {
            let above = low
                .as_ref()
                .is_none_or(|low| match compare(number, &low.value) {
                    Some(Ordering::Greater) => true,
                    Some(Ordering::Equal) => !low.exclusive,
                    _ => false,
                });
            let below = high
                .as_ref()
                .is_none_or(|high| match compare(number, &high.value) {
                    Some(Ordering::Less) => true,
                    Some(Ordering::Equal) => !high.exclusive,
                    _ => false,
                });
            above && below
        }
    }
}

/// Compares two decimal numbers exactly, by their lexical forms; `None` when
/// either is not a number.
fn compare(a: &str, b: &str) -> Option<Ordering> {
    let (a_negative, a) = sign(a);
    let (b_negative, b) = sign(b);
    let (a_whole, a_fraction) = parts(a)?;
    let (b_whole, b_fraction) = parts(b)?;
    let zero = |whole: &str, fraction: &str| whole.is_empty() && fraction.is_empty();
    let a_negative = a_negative && !zero(a_whole, a_fraction);
    let b_negative = b_negative && !zero(b_whole, b_fraction);
    let magnitude = a_whole
        .len()
        .cmp(&b_whole.len())
        .then_with(|| a_whole.cmp(b_whole))
        .then_with(|| {
            let width = a_fraction.len().max(b_fraction.len());
            format!("{a_fraction:0<width$}").cmp(&format!("{b_fraction:0<width$}"))
        });
    Some(match (a_negative, b_negative) {
        (false, false) => magnitude,
        (true, true) => magnitude.reverse(),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
    })
}

fn sign(text: &str) -> (bool, &str) {
    match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    }
}

/// The whole part without leading zeros and the fraction without trailing
/// zeros.
fn parts(text: &str) -> Option<(&str, &str)> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if !digits(whole) || !digits(fraction) || (whole.is_empty() && fraction.is_empty()) {
        return None;
    }
    Some((
        whole.trim_start_matches('0'),
        fraction.trim_end_matches('0'),
    ))
}

#[cfg(test)]
mod tests {
    use super::Concrete;

    #[test]
    fn a_positive_range_refuses_zero_and_holds_a_positive_number() {
        let positive = Concrete::parse("dec(>#0..)").expect("parses");
        assert!(!positive.holds_number("#0"));
        assert!(!positive.holds_number("#-1"));
        assert!(positive.holds_number("#0.5"));
        assert!(positive.holds_number("#4000"));
    }

    #[test]
    fn an_integer_range_refuses_a_decimal() {
        let count = Concrete::parse("int(>#0..)").expect("parses");
        assert!(count.holds_number("#2"));
        assert!(!count.holds_number("#2.5"));
    }

    #[test]
    fn closed_ranges_values_and_open_types() {
        let range = Concrete::parse("int(#1..<#10 #20)").expect("parses");
        assert!(range.holds_number("#1"));
        assert!(!range.holds_number("#10"));
        assert!(range.holds_number("#20"));
        assert!(
            Concrete::parse("dec")
                .expect("parses")
                .holds_number("#-3.25")
        );
        let strings = Concrete::parse("str(\"a\" \"b\")").expect("parses");
        assert!(strings.holds_text("b"));
        assert!(!strings.holds_text("c"));
        assert!(!strings.holds_number("#1"));
        assert_eq!(Concrete::parse("<< 123456"), None);
    }
}
