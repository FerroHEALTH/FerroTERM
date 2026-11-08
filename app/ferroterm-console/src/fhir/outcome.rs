// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `OperationOutcome` the server answers a refusal with.
//!
//! The viewer reads it through the `fhir-types` module of the version that
//! answered, and keeps of it what it renders.

use crate::fhir::release::Reads;
use crate::locale::{Msg, tr};

/// What a page draws of the `OperationOutcome` a FHIR server refused with,
/// read from the one its release decoded.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Outcome {
    /// Each `issue`, in the server's order.
    issues: Vec<OutcomeIssue>,
}

/// What a page draws of one `OperationOutcome.issue`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct OutcomeIssue {
    /// `severity`: `fatal`, `error`, `warning`, or `information`.
    pub(crate) severity: Option<String>,
    /// `code`, the `IssueType` code, for example `not-found` or `invalid`.
    pub(crate) code: Option<String>,
    /// `details.text`, the message written for a person.
    pub(crate) details_text: Option<String>,
    /// `diagnostics`, the server's own diagnostic sentence.
    pub(crate) diagnostics: Option<String>,
    /// The codes of `details.coding`, in the order they arrived.
    pub(crate) classifications: Vec<Classification>,
    /// `expression`, the `FHIRPath` of the input the issue is about.
    pub(crate) expressions: Vec<String>,
}

/// One `issue.details.coding`: the classification, and what defines it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Classification {
    /// The code, as the server wrote it.
    pub(crate) code: String,
    /// The system that defines the code.
    pub(crate) system: String,
}

/// One issue flattened into the three strings a reader is shown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IssueLine {
    /// The severity word, so the meaning is never carried by colour alone.
    pub(crate) severity: String,
    /// The `IssueType` code.
    pub(crate) code: String,
    /// The server's own wording, never a paraphrase.
    pub(crate) text: String,
    /// The codes of `issue.details`, each as `system#code`.
    pub(crate) details: Vec<String>,
}

impl Outcome {
    /// Each `issue`, in the server's order.
    pub(crate) fn issues(&self) -> &[OutcomeIssue] {
        &self.issues
    }

    /// Flattens the outcome into the lines a reader is shown.
    pub(crate) fn lines(&self, locale: &ferrohealth_ui::i18n::Locale) -> Vec<IssueLine> {
        self.issues
            .iter()
            .map(|issue| IssueLine {
                severity: issue.severity_word(),
                code: issue.code.clone().unwrap_or_else(|| "unknown".to_owned()),
                text: issue.text(locale),
                details: issue
                    .classifications
                    .iter()
                    .map(Classification::rendered)
                    .collect(),
            })
            .collect()
    }
}

impl OutcomeIssue {
    /// The severity word, `error` where the server stated none.
    pub(crate) fn severity_word(&self) -> String {
        self.severity.clone().unwrap_or_else(|| "error".to_owned())
    }

    /// The server's own wording of the issue.
    ///
    /// `details.text` wins over `diagnostics` because the specification calls
    /// `details` "additional details about the error" and `diagnostics`
    /// "additional diagnostic information"
    /// (<https://hl7.org/fhir/R4B/operationoutcome.html>): the first is the
    /// message written for a person. An issue carrying neither still produces
    /// a line, so a refusal never renders as nothing.
    pub(crate) fn text(&self, locale: &ferrohealth_ui::i18n::Locale) -> String {
        self.details_text
            .clone()
            .or_else(|| self.diagnostics.clone())
            .unwrap_or_else(|| tr(locale, Msg::ServerSentNoDiagnostic).clone())
    }
}

impl Classification {
    /// The coding as `system#code`, the form the specification writes one in.
    ///
    /// The separator is the one FHIR uses for a code in a system
    /// (<https://hl7.org/fhir/R4B/datatypes.html#Coding>). A coding that names
    /// no system renders as the bare code, which is what arrived.
    fn rendered(&self) -> String {
        if self.system.is_empty() {
            self.code.clone()
        } else {
            format!("{}#{}", self.system, self.code)
        }
    }
}

/// The `issues` outcome of a `$validate-code` answer, as `ferrobridge-term`
/// read it.
impl From<&ferrobridge_term::error::Outcome> for Outcome {
    fn from(outcome: &ferrobridge_term::error::Outcome) -> Self {
        Self {
            issues: outcome
                .issues
                .iter()
                .map(|issue| OutcomeIssue {
                    severity: Some(issue.severity.clone()),
                    code: Some(issue.code.clone()),
                    details_text: issue.details.clone(),
                    diagnostics: issue.diagnostics.clone(),
                    classifications: issue
                        .codings
                        .iter()
                        .map(|coding| Classification {
                            code: coding.code.clone().unwrap_or_default(),
                            system: coding.system.clone().unwrap_or_default(),
                        })
                        .collect(),
                    expressions: issue.expression.clone(),
                })
                .collect(),
        }
    }
}

/// Reads the `OperationOutcome` of the `fhir-types` module `$module`.
macro_rules! reads_outcome {
    ($module:ident) => {
        mod $module {
            use fhir_types::$module::operation_outcome::{OperationOutcome, OperationOutcomeIssue};

            use super::{Classification, Outcome, OutcomeIssue, Reads};

            /// One `issue`, as a page draws it.
            fn issue(issue: &OperationOutcomeIssue) -> OutcomeIssue {
                OutcomeIssue {
                    severity: issue.severity.value.clone(),
                    code: issue.code.value.clone(),
                    details_text: issue
                        .details
                        .as_ref()
                        .and_then(|details| details.text.as_ref())
                        .and_then(|text| text.value.clone()),
                    diagnostics: issue
                        .diagnostics
                        .as_ref()
                        .and_then(|diagnostics| diagnostics.value.clone()),
                    classifications: issue
                        .details
                        .iter()
                        .flat_map(|details| &details.coding)
                        .map(|coding| Classification {
                            code: coding
                                .code
                                .as_ref()
                                .and_then(|code| code.value.clone())
                                .unwrap_or_default(),
                            system: coding
                                .system
                                .as_ref()
                                .and_then(|system| system.value.clone())
                                .unwrap_or_default(),
                        })
                        .collect(),
                    expressions: issue
                        .expression
                        .iter()
                        .filter_map(|expression| expression.value.clone())
                        .collect(),
                }
            }

            impl Reads<Outcome> for OperationOutcome {
                fn read(&self) -> Outcome {
                    Outcome {
                        issues: self.issue.iter().map(issue).collect(),
                    }
                }
            }
        }
    };
}

reads_outcome!(r4);
reads_outcome!(r4b);
reads_outcome!(r5);
reads_outcome!(r6);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fhir::version::FhirVersion;

    /// `json`, decoded as the R4B `OperationOutcome` it completes with its
    /// `resourceType`.
    fn parse(json: &str) -> Outcome {
        let mut document: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(json).expect("the fixture is a JSON object");
        document
            .entry("resourceType")
            .or_insert_with(|| serde_json::Value::from("OperationOutcome"));
        FhirVersion::R4B
            .decode(&serde_json::Value::Object(document).to_string())
            .expect("the fixture is a valid OperationOutcome at R4B")
    }

    #[test]
    fn the_details_text_is_rendered_verbatim() {
        let outcome = parse(
            r#"{"resourceType":"OperationOutcome","issue":[
                {"severity":"error","code":"not-found",
                 "details":{"text":"Unknown code system http://example.org/cs"},
                 "diagnostics":"lookup failed"}]}"#,
        );
        assert_eq!(
            outcome.lines(&ferrohealth_ui::i18n::Locale::default()),
            vec![IssueLine {
                severity: "error".to_owned(),
                code: "not-found".to_owned(),
                text: "Unknown code system http://example.org/cs".to_owned(),
                details: Vec::new(),
            }],
            "details.text is the message written for a person"
        );
    }

    #[test]
    fn the_codes_of_the_details_are_rendered_beside_the_text() {
        let outcome = parse(
            r#"{"issue":[{"severity":"error","code":"too-costly",
                "details":{"coding":[
                   {"system":"http://hl7.org/fhir/tools/CodeSystem/tx-issue-type",
                    "code":"too-costly"},
                   {"code":"bare"}],
                 "text":"the expansion is too large"}}]}"#,
        );
        assert_eq!(
            outcome
                .lines(&ferrohealth_ui::i18n::Locale::default())
                .first()
                .map(|line| line.details.clone()),
            Some(vec![
                "http://hl7.org/fhir/tools/CodeSystem/tx-issue-type#too-costly".to_owned(),
                "bare".to_owned(),
            ]),
            "the classification the server stated is shown, not only its prose"
        );
    }

    #[test]
    fn diagnostics_carry_the_line_when_there_are_no_details() {
        let outcome = parse(
            r#"{"resourceType":"OperationOutcome","issue":[
                {"severity":"warning","code":"invalid","diagnostics":"count must be >= 0"}]}"#,
        );
        assert_eq!(
            outcome
                .lines(&ferrohealth_ui::i18n::Locale::default())
                .first()
                .map(|line| line.text.clone()),
            Some("count must be >= 0".to_owned()),
            "the fallback is the server's diagnostic, not a paraphrase"
        );
    }

    #[test]
    fn an_issue_with_no_wording_still_produces_a_line() {
        let outcome = parse(r#"{"issue":[{"severity":"fatal","code":"exception"}]}"#);
        assert_eq!(
            outcome
                .lines(&ferrohealth_ui::i18n::Locale::default())
                .len(),
            1,
            "a refusal never renders as nothing"
        );
    }

    #[test]
    fn every_issue_is_kept_in_order() {
        let outcome = parse(
            r#"{"issue":[{"severity":"error","code":"invalid","diagnostics":"first"},
                         {"severity":"error","code":"not-found","diagnostics":"second"}]}"#,
        );
        let texts: Vec<String> = outcome
            .lines(&ferrohealth_ui::i18n::Locale::default())
            .into_iter()
            .map(|line| line.text)
            .collect();
        assert_eq!(
            texts,
            vec!["first".to_owned(), "second".to_owned()],
            "no issue is dropped and the server's order is kept"
        );
    }

    #[test]
    fn a_missing_severity_reads_as_an_error_rather_than_as_nothing() {
        let issue = OutcomeIssue {
            code: Some("processing".to_owned()),
            ..OutcomeIssue::default()
        };
        assert_eq!(
            issue.severity_word(),
            "error",
            "severity is 1..1 in the specification; a missing one is still a refusal"
        );
    }

    #[test]
    fn an_outcome_that_does_not_decode_at_its_release_is_refused_by_its_path() {
        let error = FhirVersion::R4B
            .decode::<Outcome>(
                r#"{"resourceType":"OperationOutcome","issue":[{"code":"processing"}]}"#,
            )
            .expect_err("severity is 1..1");
        assert!(
            error.to_string().contains("severity"),
            "the decoder names the element the issue lacks: {error}"
        );
    }
}
