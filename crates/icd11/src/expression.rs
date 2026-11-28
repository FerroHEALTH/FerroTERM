// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Postcoordination expressions: `257068234&194483911`,
//! `2110065613&271422288/1847586521&1021954298`, and ICF's
//! `1860600370/unspecified.992904195`.
//!
//! The grammar is the HL7 terminology ecosystem IG's ICD-11 page
//! (`HL7/fhir-tx-ecosystem-ig` at `9c2bbce`, `input/pagecontent/icd-11.md`,
//! Post Coordination Expressions): the first code is the root; `&` qualifies
//! the code before it; `/` starts a cluster member, which qualifies the root
//! and may carry `&` qualifiers of its own; there is no deeper nesting and no
//! space. ICF also writes a qualifier with `.`, which is the character inside
//! MMS codes (`1D01.0Y`), so the ICF reading is asked for by the caller. A `/`
//! before `other` or `unspecified` is part of a residual's id. The tokens keep
//! their spelling; the provider resolves them.

/// The last segments of a residual category's id (`1363559646/other`).
const RESIDUALS: [&str; 2] = ["other", "unspecified"];

/// One code with the qualifiers written after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The code as written.
    pub code: String,
    /// The qualifiers as written, in order.
    pub qualifiers: Vec<String>,
}

/// A parsed expression: the root group and the cluster members after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    /// The root and its qualifiers.
    pub root: Group,
    /// The `/` members, in the order written.
    pub members: Vec<Group>,
    /// Whether the qualifiers were written with ICF's `.`.
    pub dotted: bool,
}

impl Expression {
    /// Parses `text`; `icf` reads `.` as the qualifier operator when the text
    /// holds no `&`.
    ///
    /// Returns `None` for an empty or malformed expression: an empty code, a
    /// dangling operator, or a space.
    #[must_use]
    pub fn parse(text: &str, icf: bool) -> Option<Self> {
        if text.is_empty() || text.chars().any(char::is_whitespace) {
            return None;
        }
        let qualifier = if icf && !text.contains('&') { '.' } else { '&' };
        let mut tokens: Vec<(Option<char>, String)> = Vec::new();
        let mut separator = None;
        let mut current = String::new();
        for c in text.chars() {
            if c == '/' || c == qualifier {
                push(&mut tokens, separator, std::mem::take(&mut current));
                separator = Some(c);
            } else {
                current.push(c);
            }
        }
        push(&mut tokens, separator, current);
        let mut tokens = tokens.into_iter();
        let (_, code) = tokens.next()?;
        let mut root = Group::new(code)?;
        let mut members: Vec<Group> = Vec::new();
        for (separator, code) in tokens {
            if separator == Some('/') {
                members.push(Group::new(code)?);
            } else {
                let group = members.last_mut().unwrap_or(&mut root);
                group.qualifiers.push(Some(code).filter(|c| !c.is_empty())?);
            }
        }
        let dotted = qualifier == '.'
            && std::iter::once(&root)
                .chain(&members)
                .any(|g| !g.qualifiers.is_empty());
        Some(Self {
            root,
            members,
            dotted,
        })
    }

    /// Whether the expression is a single code with no qualifier or member.
    #[must_use]
    pub fn is_simple(&self) -> bool {
        self.members.is_empty() && self.root.qualifiers.is_empty()
    }
}

impl Group {
    fn new(code: String) -> Option<Self> {
        (!code.is_empty()).then_some(Self {
            code,
            qualifiers: Vec::new(),
        })
    }
}

/// Appends `token`, or joins it to the token before when it ends a residual's id.
fn push(tokens: &mut Vec<(Option<char>, String)>, separator: Option<char>, token: String) {
    if separator == Some('/')
        && RESIDUALS.contains(&token.as_str())
        && let Some((_, last)) = tokens.last_mut()
        && !last.is_empty()
    {
        last.push('/');
        last.push_str(&token);
        return;
    }
    tokens.push((separator, token));
}

#[cfg(test)]
mod tests {
    use super::Expression;

    #[test]
    fn qualifiers_belong_to_the_code_before_them_and_members_to_the_root() {
        let e = Expression::parse("1A00&XN8P1", false).expect("parses");
        assert_eq!(e.root.code, "1A00");
        assert_eq!(e.root.qualifiers, ["XN8P1"]);
        assert_eq!(e.members, []);
        let e = Expression::parse("NC53.Y&XK8G&XJ778/PA71&XE5UF&XE4TZ", false).expect("parses");
        assert_eq!(e.root.code, "NC53.Y");
        assert_eq!(e.root.qualifiers.len(), 2);
        assert_eq!(e.members.len(), 1);
        assert_eq!(e.members[0].code, "PA71");
        assert_eq!(e.members[0].qualifiers, ["XE5UF", "XE4TZ"]);
        let e = Expression::parse("1D01.0Y/1G41/1G40", false).expect("parses");
        assert_eq!(e.members.len(), 2);
        assert!(!e.dotted);
        assert!(Expression::parse("1A00&", false).is_none());
        assert!(Expression::parse("1A00//1G40", false).is_none());
        assert!(Expression::parse("", false).is_none());
        assert!(Expression::parse("1A00 & XN8P1", false).is_none());
        assert!(
            Expression::parse("1A00", false)
                .expect("parses")
                .is_simple()
        );
    }

    #[test]
    fn a_slash_before_other_or_unspecified_is_part_of_the_id() {
        let e = Expression::parse("344162786/other&1088044739/916407020", false).expect("parses");
        assert_eq!(e.root.code, "344162786/other");
        assert_eq!(e.root.qualifiers, ["1088044739"]);
        assert_eq!(e.members[0].code, "916407020");
        let e = Expression::parse("684930313/other/622600769", false).expect("parses");
        assert_eq!(e.root.code, "684930313/other");
        assert_eq!(e.members[0].code, "622600769");
        let e = Expression::parse("1A00&1435254666/unspecified", false).expect("parses");
        assert_eq!(e.root.qualifiers, ["1435254666/unspecified"]);
        assert!(
            Expression::parse("1001/other", false)
                .expect("parses")
                .is_simple()
        );
        assert!(Expression::parse("/other", false).is_none());
    }

    #[test]
    fn icf_writes_a_qualifier_with_a_dot() {
        let e = Expression::parse("d5409.qp3", true).expect("parses");
        assert_eq!(e.root.code, "d5409");
        assert_eq!(e.root.qualifiers, ["qp3"]);
        assert!(e.dotted);
        let e = Expression::parse("1860600370/unspecified.992904195", true).expect("parses");
        assert_eq!(e.root.code, "1860600370/unspecified");
        assert_eq!(e.root.qualifiers, ["992904195"]);
        assert!(e.dotted);
        let e = Expression::parse("d5409&qp3", true).expect("parses");
        assert_eq!(e.root.qualifiers, ["qp3"]);
        assert!(!e.dotted);
        let e = Expression::parse("1D01.0Y", false).expect("parses");
        assert!(e.is_simple());
    }
}
