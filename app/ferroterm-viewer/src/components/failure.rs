// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Rendering a refused request, in the server's own words.

use ferrohealth_ui::view::refusal::{Issue, Refusal, Severity, refusal};
use leptos::prelude::*;

use crate::fhir::error::FhirError;
use crate::fhir::outcome::OperationOutcome;
use crate::locale::{Msg, tr};

/// The severity a FHIR `issue.severity` code names (the `IssueSeverity` value
/// set, <https://hl7.org/fhir/R4B/valueset-issue-severity.html>); a code it
/// does not define is read as an error, never dropped.
fn severity(code: &str) -> Severity {
    match code {
        "fatal" => Severity::Fatal,
        "warning" => Severity::Warning,
        "information" => Severity::Information,
        _ => Severity::Error,
    }
}

/// The issues `outcome` carries, each with the text the server wrote and the
/// codes that classify it.
fn issues(outcome: &OperationOutcome) -> Vec<Issue> {
    outcome
        .lines()
        .into_iter()
        .map(|line| Issue {
            severity: severity(&line.severity),
            code: line.code,
            text: (!line.text.is_empty()).then_some(line.text),
            place: String::new(),
            details: line.details,
        })
        .collect()
}

/// The refusal `error` is, as FerroUI draws one.
pub(crate) fn refusal_of(error: &FhirError) -> Refusal {
    Refusal {
        status: error.status().map(|status| status.as_u16()),
        summary: match error {
            FhirError::Refused { .. } | FhirError::Status { .. } => tr(Msg::ServerRefusedRequest),
            FhirError::Transport { .. } => tr(Msg::RequestGotNoAnswer),
            FhirError::Decode { .. } => tr(Msg::AnswerDidNotRead),
        },
        issues: error.outcome().map(issues).unwrap_or_default(),
        request: Some(error.url().to_owned()),
        evidence: match error {
            FhirError::Status { body, .. } if !body.is_empty() => Some(body.clone()),
            FhirError::Transport { message, .. } | FhirError::Decode { message, .. } => {
                Some(message.clone())
            }
            FhirError::Status { .. } | FhirError::Refused { .. } => None,
        },
    }
}

/// Explains why a request failed, in the server's own words.
///
/// The server's `OperationOutcome` is shown issue by issue, never
/// paraphrased; a failure that carried none keeps the start of its body as
/// the evidence, and the address asked is shown so a reader can repeat the
/// request themselves.
#[component]
#[expect(
    unreachable_pub,
    reason = "the leptos component macro emits a pub props type, and the module tree is private"
)]
pub(crate) fn Failure(
    /// The failure to explain.
    #[prop(into)]
    error: Signal<FhirError>,
) -> impl IntoView {
    move || error.with(|error| refusal(&refusal_of(error)).into_any())
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::view::refusal::Severity;
    use http::StatusCode;

    use super::refusal_of;
    use crate::fhir::error::FhirError;
    use crate::fhir::outcome::OperationOutcome;

    #[test]
    fn a_refusal_keeps_the_status_and_every_issue_in_the_server_s_words() {
        let outcome: OperationOutcome = serde_json::from_str(
            r#"{"resourceType":"OperationOutcome","issue":[
                {"severity":"error","code":"not-found","diagnostics":"no such value set"},
                {"severity":"warning","code":"informational","details":{"text":"a hint"}}]}"#,
        )
        .expect("an outcome");
        let refused = refusal_of(&FhirError::Refused {
            url: String::from("http://x/r4/ValueSet/$expand"),
            status: StatusCode::NOT_FOUND,
            outcome,
        });
        assert_eq!(Some(404), refused.status);
        assert_eq!(2, refused.issues.len());
        assert_eq!(Severity::Error, refused.issues[0].severity);
        assert_eq!(Some("no such value set"), refused.issues[0].text.as_deref());
        assert_eq!(Severity::Warning, refused.issues[1].severity);
    }

    #[test]
    fn a_request_with_no_answer_has_no_status() {
        let refused = refusal_of(&FhirError::Transport {
            url: String::from("http://x/health"),
            message: String::from("connection refused"),
        });
        assert_eq!(None, refused.status);
        assert!(refused.issues.is_empty());
        assert_eq!(Some("http://x/health"), refused.request.as_deref());
        assert_eq!(Some("connection refused"), refused.evidence.as_deref());
    }
}
