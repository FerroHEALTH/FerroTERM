// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Postcoordination expressions: `1A00&XN8P1`, `NC53.Y&XK8G/PA71&XE5UF`,
//! `1D01.0Y/1G41/1G40`, the ICF form `d5409.qp3`, and the URI form
//! `<uri> & <uri>`.
//!
//! `&` says that what follows is a value on an axis of the stem before it;
//! `/` says only that another member of the cluster follows. ICF spells the
//! value operator as `.`, which is also the character inside MMS codes
//! (`1D01.0Y`), so the ICF reading is asked for by the caller. The syntax is
//! WHO's (the ICD-11 reference guide, postcoordination); the tokens keep
//! their spelling and the provider resolves them.

/// One member of an expression: a code or an entity URI as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The text as written.
    pub text: String,
    /// Whether it is a URI (`http://id.who.int/...`) rather than a short code.
    pub uri: bool,
}

/// One cluster member: a stem with its `&` values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The stem token.
    pub stem: Token,
    /// The values joined to the stem with `&`.
    pub values: Vec<Token>,
}

/// A parsed expression: one or more cluster members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    /// The members, in order.
    pub members: Vec<Member>,
    /// Whether every token was a URI (the URI form).
    pub uri_form: bool,
    /// Whether the values were written in the ICF dotted qualifier form
    /// (`d5409.qp3`), which admits qualifier codes only.
    pub dotted: bool,
}

fn token(text: &str) -> Option<Token> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(Token {
        text: text.to_owned(),
        uri: text.starts_with("http://") || text.starts_with("https://"),
    })
}

/// Splits `text` on `separator`, but never inside a URI's own path.
fn split_outside_uris(text: &str, separator: char) -> Vec<&str> {
    if separator == '/' && text.contains("://") {
        // A URI form uses ` / ` with spaces around the cluster separator, and
        // `/` inside the URIs; only the spaced form separates.
        return text.split(" / ").collect();
    }
    text.split(separator).collect()
}

impl Expression {
    /// Parses `text`; `icf` reads `.` as the value operator.
    ///
    /// Returns `None` for an empty or malformed expression (an empty member,
    /// a dangling operator).
    #[must_use]
    pub fn parse(text: &str, icf: bool) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let value_separator = if icf && !text.contains('&') { '.' } else { '&' };
        let mut members = Vec::new();
        let mut uri_form = true;
        for part in split_outside_uris(text, '/') {
            let mut tokens = part.split(value_separator);
            let stem = token(tokens.next()?)?;
            let mut values = Vec::new();
            for value in tokens {
                values.push(token(value)?);
            }
            uri_form &= stem.uri && values.iter().all(|v| v.uri);
            members.push(Member { stem, values });
        }
        let dotted = value_separator == '.' && members.iter().any(|m| !m.values.is_empty());
        Some(Self {
            members,
            uri_form,
            dotted,
        })
    }

    /// Whether the expression is a single stem with no values.
    #[must_use]
    pub fn is_simple(&self) -> bool {
        self.members.len() == 1 && self.members.iter().all(|m| m.values.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::Expression;

    #[test]
    fn expressions_split_into_members_and_values() {
        let e = Expression::parse("1A00&XN8P1", false).expect("parses");
        assert_eq!(e.members.len(), 1);
        assert_eq!(e.members[0].stem.text, "1A00");
        assert_eq!(e.members[0].values[0].text, "XN8P1");
        assert!(!e.uri_form);
        let e = Expression::parse("NC53.Y&XK8G&XJ778/PA71&XE5UF&XE4TZ", false).expect("parses");
        assert_eq!(e.members.len(), 2);
        assert_eq!(e.members[0].stem.text, "NC53.Y");
        assert_eq!(e.members[0].values.len(), 2);
        assert_eq!(e.members[1].stem.text, "PA71");
        let e = Expression::parse("1D01.0Y/1G41/1G40", false).expect("parses");
        assert_eq!(e.members.len(), 3);
        assert!(e.members.iter().all(|m| m.values.is_empty()));
        let e = Expression::parse("d5409.qp3", true).expect("parses");
        assert_eq!(e.members[0].stem.text, "d5409");
        assert_eq!(e.members[0].values[0].text, "qp3");
        let e = Expression::parse(
            "http://id.who.int/icd/release/11/mms/257068234 & http://id.who.int/icd/release/11/mms/194483911",
            false,
        )
        .expect("parses");
        assert!(e.uri_form);
        assert_eq!(
            e.members[0].values[0].text,
            "http://id.who.int/icd/release/11/mms/194483911"
        );
        assert!(Expression::parse("1A00&", false).is_none());
        assert!(Expression::parse("", false).is_none());
        assert!(
            Expression::parse("1A00", false)
                .expect("parses")
                .is_simple()
        );
    }
}
