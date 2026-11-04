// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `message` output of `ValueSet/$validate-code`, joined from its issues.

use super::super::Issue;

/// The `message`: the issues that speak for the outcome, joined with `; `.
///
/// Every error and the warnings about the data, the display, or the concept's
/// standing join, in the ecosystem's order (the missing system first, the
/// membership verdict late); an information about the display speaks alone
/// when nothing else does. Status notes and code warnings stay issues.
pub(super) fn message_of(issues: &[Issue]) -> Option<String> {
    const ORDER: [&str; 11] = [
        "not-found",
        "invalid-data",
        "vs-invalid",
        "version-error",
        "cannot-infer",
        "code-comment",
        "code-rule",
        "not-in-vs",
        "invalid-code",
        "display-comment",
        "invalid-display",
    ];
    const WARNINGS: [&str; 5] = [
        "not-found",
        "invalid-data",
        "code-comment",
        "display-comment",
        "invalid-display",
    ];
    let speaks = |issue: &&Issue| {
        issue.severity == "error" || (issue.severity == "warning" && WARNINGS.contains(&issue.kind))
    };
    let rank = |kind: &str| ORDER.iter().position(|k| *k == kind).unwrap_or(ORDER.len());
    let mut speaking: Vec<&Issue> = issues.iter().filter(speaks).collect();
    speaking.sort_by_key(|issue| rank(issue.kind));
    if speaking.is_empty() {
        return issues
            .iter()
            .find(|i| i.severity == "information" && i.kind == "invalid-display")
            .map(|i| i.text.clone());
    }
    let texts: Vec<&str> = speaking.iter().map(|i| i.text.as_str()).collect();
    Some(texts.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::MessageId;

    fn issue(severity: &'static str, kind: &'static str, text: &str) -> Issue {
        Issue {
            severity,
            code: "invalid",
            kind,
            message: MessageId::TxGeneralError,
            text: text.to_owned(),
            expression: None,
        }
    }

    #[test]
    fn the_message_orders_the_issues_the_ecosystems_way() {
        let issues = [
            issue("error", "not-in-vs", "not in vs"),
            issue("error", "invalid-code", "unknown code"),
        ];
        assert_eq!(
            message_of(&issues).as_deref(),
            Some("not in vs; unknown code")
        );
        let issues = [
            issue("error", "not-in-vs", "not in vs"),
            issue("error", "invalid-data", "bad data"),
            issue("error", "not-found", "missing"),
            issue("warning", "vs-invalid", "quiet warning"),
            issue("information", "status-check", "draft"),
        ];
        assert_eq!(
            message_of(&issues).as_deref(),
            Some("missing; bad data; not in vs")
        );
        let issues = [issue("warning", "invalid-code", "fragment")];
        assert_eq!(message_of(&issues), None);
    }

    #[test]
    fn the_message_falls_back_to_the_first_information_issue() {
        let issues = [Issue {
            severity: "information",
            code: "invalid",
            kind: "invalid-display",
            message: MessageId::NoValidDisplayFoundNoneForLangOk,
            text: String::from("There are no valid display names found"),
            expression: None,
        }];
        assert_eq!(
            message_of(&issues).as_deref(),
            Some("There are no valid display names found")
        );
    }
}
