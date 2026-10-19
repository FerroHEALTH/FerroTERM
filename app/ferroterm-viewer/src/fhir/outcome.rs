//! The `OperationOutcome` the server answers a refusal with.
//!
//! The viewer carries the fields it renders and nothing more. It never mirrors
//! a whole FHIR resource, because a viewer that re-models the specification
//! stops being a demonstration that the public API is complete.

use serde::{Deserialize, Serialize};

/// The refusal a FHIR server returns, as the viewer reads it.
///
/// `issue` is 1..* in the specification
/// (<https://hl7.org/fhir/R4B/operationoutcome.html>), and a server that sends
/// none still parses here, so a malformed refusal renders as an empty issue
/// list rather than a decode error that hides the status.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct OperationOutcome {
    /// The issues the server reported.
    #[serde(default)]
    pub(crate) issue: Vec<Issue>,
}

/// One `OperationOutcome.issue`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct Issue {
    /// `fatal`, `error`, `warning`, or `information`.
    pub(crate) severity: Option<String>,
    /// The `IssueType` code, for example `not-found` or `invalid`.
    pub(crate) code: Option<String>,
    /// The human text of `issue.details`, when the server sent one.
    pub(crate) details: Option<CodeableConcept>,
    /// `issue.diagnostics`, the server's own diagnostic sentence.
    pub(crate) diagnostics: Option<String>,
    /// `issue.expression`, the `FHIRPath` of the input the issue is about.
    #[serde(default)]
    pub(crate) expression: Vec<String>,
    /// The extensions on the issue, read for the position of the fault.
    #[serde(default)]
    pub(crate) extension: Vec<Extension>,
}

/// One extension on an issue, with the two value types a position uses.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct Extension {
    /// The extension's canonical URL.
    pub(crate) url: String,
    /// `valueString`, the type `operationoutcome-issue-col` takes.
    #[serde(rename = "valueString")]
    pub(crate) value_string: Option<String>,
    /// `valueInteger`, the type `operationoutcome-issue-line` takes.
    #[serde(rename = "valueInteger")]
    pub(crate) value_integer: Option<i64>,
}

/// The extension stating the line of a fault inside the value
/// `issue.expression` names
/// (<https://hl7.org/fhir/extensions/StructureDefinition-operationoutcome-issue-line.html>).
const ISSUE_LINE: &str = "http://hl7.org/fhir/StructureDefinition/operationoutcome-issue-line";

/// The extension stating the column of that fault
/// (<https://hl7.org/fhir/extensions/StructureDefinition-operationoutcome-issue-col.html>).
const ISSUE_COL: &str = "http://hl7.org/fhir/StructureDefinition/operationoutcome-issue-col";

impl Issue {
    /// The input the issue points into and the 1-based column of the fault
    /// in it, when the server stated both.
    ///
    /// The column is read on the first line only: a filter value the composer
    /// sends is one line, and a fault the server places on another line is
    /// one this screen has no line to mark on.
    pub(crate) fn position(&self) -> Option<(&str, u32)> {
        let value = |url: &str| self.extension.iter().find(|x| x.url == url);
        let line = value(ISSUE_LINE).and_then(|x| x.value_integer);
        if line.is_some_and(|line| line != 1) {
            return None;
        }
        // NOTE: the extension's value is a free `string` (see `ISSUE_COL`), so one that is not a
        // positive number states no column this screen can mark, and the outcome still renders whole.
        let column = value(ISSUE_COL)
            .and_then(|x| x.value_string.as_deref())
            .and_then(|text| text.trim().parse().ok())
            .filter(|column: &u32| *column > 0)?;
        Some((self.expression.first()?.as_str(), column))
    }
}

/// The parts of a `CodeableConcept` the viewer renders.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct CodeableConcept {
    /// The codes classifying the issue, which a server states beside the text.
    #[serde(default)]
    pub(crate) coding: Vec<Coding>,
    /// The human-readable text for the concept.
    pub(crate) text: Option<String>,
}

/// One `Coding` of `issue.details`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct Coding {
    /// The system the code is from.
    pub(crate) system: Option<String>,
    /// The code itself.
    pub(crate) code: Option<String>,
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

impl OperationOutcome {
    /// Flattens the outcome into the lines a reader is shown.
    ///
    /// `details.text` wins over `diagnostics` because the specification calls
    /// `details` "additional details about the error" and `diagnostics`
    /// "additional diagnostic information"
    /// (<https://hl7.org/fhir/R4B/operationoutcome.html>): the first is the
    /// message written for a person. An issue carrying neither still produces
    /// a line, so a refusal never renders as nothing.
    pub(crate) fn lines(&self) -> Vec<IssueLine> {
        self.issue
            .iter()
            .map(|issue| IssueLine {
                severity: issue.severity.clone().unwrap_or_else(|| "error".to_owned()),
                code: issue.code.clone().unwrap_or_else(|| "unknown".to_owned()),
                text: issue
                    .details
                    .as_ref()
                    .and_then(|details| details.text.clone())
                    .or_else(|| issue.diagnostics.clone())
                    .unwrap_or_else(|| "the server sent no diagnostic".to_owned()),
                details: issue
                    .details
                    .as_ref()
                    .map(|details| details.coding.iter().map(Coding::rendered).collect())
                    .unwrap_or_default(),
            })
            .collect()
    }

    /// Whether any issue is classified as `code`.
    ///
    /// The classification is read from `issue.code` and from the codes of
    /// `issue.details`, because a server states the class in either place and
    /// the reader is owed the same answer whichever it used.
    pub(crate) fn carries_code(&self, code: &str) -> bool {
        self.issue.iter().any(|issue| {
            issue.code.as_deref() == Some(code)
                || issue.details.iter().any(|details| {
                    details
                        .coding
                        .iter()
                        .any(|coding| coding.code.as_deref() == Some(code))
                })
        })
    }
}

impl Coding {
    /// The coding as `system#code`, the form the specification writes one in.
    ///
    /// The separator is the one FHIR uses for a code in a system
    /// (<https://hl7.org/fhir/R4B/datatypes.html#Coding>). A coding that names
    /// no system renders as the bare code, which is what arrived.
    fn rendered(&self) -> String {
        let code = self.code.clone().unwrap_or_default();
        match self.system.as_deref() {
            Some(system) if !system.is_empty() => format!("{system}#{code}"),
            Some(_) | None => code,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> OperationOutcome {
        serde_json::from_str(json).expect("the fixture is valid JSON")
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
            outcome.lines(),
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
            outcome.lines().first().map(|line| line.details.clone()),
            Some(vec![
                "http://hl7.org/fhir/tools/CodeSystem/tx-issue-type#too-costly".to_owned(),
                "bare".to_owned(),
            ]),
            "the classification the server stated is shown, not only its prose"
        );
    }

    #[test]
    fn a_refusal_is_recognised_by_the_class_the_server_stated() {
        let by_issue_code = parse(r#"{"issue":[{"code":"too-costly","diagnostics":"too large"}]}"#);
        assert!(
            by_issue_code.carries_code("too-costly"),
            "the IssueType code classifies the refusal"
        );
        let by_detail_coding = parse(
            r#"{"issue":[{"code":"processing",
                "details":{"coding":[{"code":"too-costly"}]}}]}"#,
        );
        assert!(
            by_detail_coding.carries_code("too-costly"),
            "a server that classifies in the details is read the same way"
        );
        assert!(
            !by_detail_coding.carries_code("not-found"),
            "a class the server did not state is not claimed"
        );
    }

    #[test]
    fn diagnostics_carry_the_line_when_there_are_no_details() {
        let outcome = parse(
            r#"{"resourceType":"OperationOutcome","issue":[
                {"severity":"warning","code":"invalid","diagnostics":"count must be >= 0"}]}"#,
        );
        assert_eq!(
            outcome.lines().first().map(|line| line.text.clone()),
            Some("count must be >= 0".to_owned()),
            "the fallback is the server's diagnostic, not a paraphrase"
        );
    }

    #[test]
    fn an_issue_with_no_wording_still_produces_a_line() {
        let outcome = parse(r#"{"issue":[{"severity":"fatal","code":"exception"}]}"#);
        assert_eq!(
            outcome.lines().len(),
            1,
            "a refusal never renders as nothing"
        );
    }

    #[test]
    fn every_issue_is_kept_in_order() {
        let outcome = parse(
            r#"{"issue":[{"code":"invalid","diagnostics":"first"},
                         {"code":"not-found","diagnostics":"second"}]}"#,
        );
        let texts: Vec<String> = outcome.lines().into_iter().map(|line| line.text).collect();
        assert_eq!(
            texts,
            vec!["first".to_owned(), "second".to_owned()],
            "no issue is dropped and the server's order is kept"
        );
    }

    #[test]
    fn a_missing_severity_reads_as_an_error_rather_than_as_nothing() {
        let outcome = parse(r#"{"issue":[{"code":"processing"}]}"#);
        assert_eq!(
            outcome.lines().first().map(|line| line.severity.clone()),
            Some("error".to_owned()),
            "severity is 1..1 in the specification; a missing one is still a refusal"
        );
    }

    #[test]
    fn the_position_is_read_from_the_expression_and_the_column_extension() {
        let outcome = parse(
            r#"{"issue":[{"code":"invalid","diagnostics":"expected a focus concept",
                "expression":["Parameters.parameter[0].resource.compose.include[0].filter[0].value"],
                "extension":[
                  {"url":"http://hl7.org/fhir/StructureDefinition/operationoutcome-issue-line","valueInteger":1},
                  {"url":"http://hl7.org/fhir/StructureDefinition/operationoutcome-issue-col","valueString":"14"}]}]}"#,
        );
        assert_eq!(
            outcome.issue.first().and_then(Issue::position),
            Some((
                "Parameters.parameter[0].resource.compose.include[0].filter[0].value",
                14
            )),
            "the server states the input and the column as data"
        );
    }

    #[test]
    fn an_issue_without_a_readable_column_states_no_position() {
        let no_column = parse(
            r#"{"issue":[{"code":"invalid","diagnostics":"at byte 3","expression":["http.url"]}]}"#,
        );
        assert_eq!(
            no_column.issue.first().and_then(Issue::position),
            None,
            "the diagnostic's words are never read for a position"
        );
        let other_line = parse(
            r#"{"issue":[{"code":"invalid","expression":["http.url"],"extension":[
                {"url":"http://hl7.org/fhir/StructureDefinition/operationoutcome-issue-line","valueInteger":2},
                {"url":"http://hl7.org/fhir/StructureDefinition/operationoutcome-issue-col","valueString":"3"}]}]}"#,
        );
        assert_eq!(
            other_line.issue.first().and_then(Issue::position),
            None,
            "a fault on a later line has no mark on a one-line value"
        );
        let not_a_number = parse(
            r#"{"issue":[{"code":"invalid","expression":["http.url"],"extension":[
                {"url":"http://hl7.org/fhir/StructureDefinition/operationoutcome-issue-col","valueString":"x"}]}]}"#,
        );
        assert_eq!(not_a_number.issue.first().and_then(Issue::position), None);
    }

    #[test]
    fn unknown_fields_and_a_missing_issue_array_still_parse() {
        let outcome =
            parse(r#"{"resourceType":"OperationOutcome","id":"x","text":{"div":"<p/>"}}"#);
        assert_eq!(
            outcome,
            OperationOutcome::default(),
            "a malformed refusal must not hide the status behind a decode error"
        );
    }
}
