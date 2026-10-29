// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The RFC 9457 problem details the server's admin listener and the sync
//! service answer their errors with (<https://www.rfc-editor.org/rfc/rfc9457>),
//! read as far as a screen draws them: the type, the title and the detail.

use serde::Deserialize;

/// A problem document, as a refusal draws it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub(crate) struct Problem {
    /// The problem type, a URI; `about:blank` when the status says it all.
    #[serde(rename = "type", default)]
    pub(crate) kind: Option<String>,
    /// The type's summary, the same for every occurrence.
    #[serde(default)]
    pub(crate) title: Option<String>,
    /// The HTTP status it was answered with.
    #[serde(default)]
    pub(crate) status: Option<u16>,
    /// What happened this time.
    #[serde(default)]
    pub(crate) detail: Option<String>,
}

impl Problem {
    /// The problem `body` holds, or `None` for a body that is no problem
    /// document: one that carries neither a type nor a title.
    #[must_use]
    pub(crate) fn read(body: &str) -> Option<Self> {
        // NOTE: RFC 9457 §3.1: every member is optional, so a document that names
        // neither its type nor its title is read as a plain body instead.
        serde_json::from_str::<Self>(body)
            .ok()
            .filter(|problem| problem.kind.is_some() || problem.title.is_some())
    }

    /// The fragment of a FerroTERM problem type, such as `reload-failed`, or
    /// the whole type for any other.
    #[must_use]
    pub(crate) fn code(&self) -> String {
        let kind = self.kind.as_deref().unwrap_or("about:blank");
        kind.rsplit_once('#')
            .map_or(kind, |(_, slug)| slug)
            .to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::Problem;

    #[test]
    fn a_problem_is_read_by_its_members_and_a_plain_body_is_not_one() {
        let problem = Problem::read(
            r#"{"type":"https://ferroterm.eu/docs/operate/problems.html#reload-failed",
                "title":"The served set could not be rebuilt","status":500,
                "detail":"the index could not be opened"}"#,
        )
        .expect("a problem");
        assert_eq!("reload-failed", problem.code());
        assert_eq!(Some(500), problem.status);
        assert_eq!(
            Some("the index could not be opened"),
            problem.detail.as_deref()
        );
        let blank = Problem::read(r#"{"type":"about:blank","title":"Not Found","status":404}"#)
            .expect("a problem");
        assert_eq!("about:blank", blank.code());
        assert_eq!(None, Problem::read(r#"{"outcome":"failed"}"#));
        assert_eq!(None, Problem::read("not json"));
    }
}
