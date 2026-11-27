// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The qualifier grammar of a standalone ICF code (the terminology ecosystem
//! IG at `9c2bbce`, `icf.md`, Qualifiers): the category, then `.` or, for an
//! environmental factor only, `+`, then one digit per position.

/// One qualifier position: the property it fills and the scale its digit
/// comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Position {
    /// The property that reports it (`icf.md`, Properties).
    pub(super) property: &'static str,
    /// The scale heading (`q1` to `q6`).
    pub(super) scale: &'static str,
    /// The word the display puts before the value, for the `d` positions that
    /// share one scale.
    pub(super) label: Option<&'static str>,
}

const fn position(
    property: &'static str,
    scale: &'static str,
    label: Option<&'static str>,
) -> Position {
    Position {
        property,
        scale,
        label,
    }
}

/// "one for `b`, up to three for `s`, up to four for `d` ... and one for `e`".
const BODY_FUNCTION: [Position; 1] = [position("extent", "q1", None)];
const BODY_STRUCTURE: [Position; 3] = [
    position("extent", "q1", None),
    position("nature", "q2", None),
    position("localization", "q3", None),
];
const ACTIVITY: [Position; 4] = [
    position("performance", "q4", Some("performance")),
    position("capacity", "q4", Some("capacity")),
    position(
        "capacity-with-assistance",
        "q4",
        Some("capacity with assistance"),
    ),
    position(
        "performance-without-assistance",
        "q4",
        Some("performance without assistance"),
    ),
];
const BARRIER: [Position; 1] = [position("barrier", "q5", None)];
const FACILITATOR: [Position; 1] = [position("facilitator", "q6", None)];

/// Every qualifier property, for the filters that name one.
pub(super) const PROPERTIES: [&str; 9] = [
    "extent",
    "nature",
    "localization",
    "performance",
    "capacity",
    "capacity-with-assistance",
    "performance-without-assistance",
    "barrier",
    "facilitator",
];

/// The digit that says nothing: "A qualifier of `8` (not specified) says
/// nothing".
pub(super) const NOT_SPECIFIED: char = '8';

/// A code split into its category and its qualifiers, before the category or
/// the scales are checked against the code system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Qualified<'a> {
    /// The category code.
    pub(super) category: &'a str,
    /// The positions the digits fill, in order.
    pub(super) positions: &'static [Position],
    /// The digits, one per position.
    pub(super) digits: Vec<char>,
}

impl Qualified<'_> {
    /// The positions with their digits.
    pub(super) fn values(&self) -> impl Iterator<Item = (&Position, char)> {
        self.positions.iter().zip(self.digits.iter().copied())
    }

    /// The scale value concept a position's digit names (`q1-2`).
    pub(super) fn scale_value(position: &Position, digit: char) -> String {
        format!("{}-{digit}", position.scale)
    }

    /// Whether `property` is present with something to say: "A qualifier of
    /// `8` counts as absent".
    pub(super) fn states(&self, property: &str) -> bool {
        self.values()
            .any(|(position, digit)| position.property == property && digit != NOT_SPECIFIED)
    }
}

/// Splits `code` into a category and qualifiers that follow the positional
/// rules of its component, or `None` when it is no qualified code.
pub(super) fn split(code: &str) -> Option<Qualified<'_>> {
    let at = code.find(['.', '+'])?;
    let (category, rest) = code.split_at(at);
    let mut characters = rest.chars();
    let separator = characters.next()?;
    let digits: Vec<char> = characters.collect();
    if digits.is_empty() || !digits.iter().all(char::is_ascii_digit) {
        return None;
    }
    let positions: &'static [Position] = match (category.chars().next()?, separator) {
        ('b', '.') => &BODY_FUNCTION,
        ('s', '.') => &BODY_STRUCTURE,
        ('d', '.') => &ACTIVITY,
        ('e', '.') => &BARRIER,
        ('e', '+') => &FACILITATOR,
        _ => return None,
    };
    if digits.len() > positions.len() {
        return None;
    }
    Some(Qualified {
        category,
        positions,
        digits,
    })
}

#[cfg(test)]
mod tests {
    use super::split;

    // The examples `icf.md` gives under Qualifiers, on synthetic categories.
    #[test]
    fn the_positional_rules_of_each_component_hold() {
        assert!(split("b9900.2").is_some());
        assert!(split("s9900.183").is_some());
        assert!(split("d9900.1234").is_some());
        assert!(split("e9900.2").is_some());
        assert!(split("e9900+3").is_some());
        for refused in [
            "b9900.21",
            "d9900.12345",
            "e9900.23",
            "b9900+2",
            "b9900.",
            "b9900.x",
            "e9900-4",
            "b9900",
        ] {
            assert!(split(refused).is_none(), "{refused}");
        }
    }
}
