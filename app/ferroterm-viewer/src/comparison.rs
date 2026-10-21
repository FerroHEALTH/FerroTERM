// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The four served roots, read side by side.
//!
//! The comparison is a plain value built from the four `CapabilityStatement`
//! documents, so what the screen claims is pinned by unit tests over recorded
//! answers rather than by a browser. Nothing here knows which version declares
//! what: every cell comes from a document, and a root that answers something
//! new grows a row with no change to this file.

use http::StatusCode;

use crate::fhir::capability::CapabilityStatement;
use crate::fhir::capability::OperationDeclaration;
use crate::fhir::error::FhirError;
use crate::fhir::version::FhirVersion;
use crate::locale::{Msg, tr, tr_with};

/// What one root answered when the screen asked for its capability statement.
#[derive(Clone, Debug)]
pub(crate) enum RootAnswer {
    /// The root answered, and this is what it declares.
    Declared(Box<CapabilityStatement>),
    /// The deployment does not serve this root.
    Absent,
    /// The root answered a failure, which the screen renders as it arrived.
    Refused(Box<FhirError>),
    /// The read has not settled yet.
    Unread,
}

impl RootAnswer {
    /// Reads one root's answer, telling an unserved root from a failed one.
    ///
    /// A root the deployment does not mount answers `404`, which is the
    /// RESTful API's "unknown resource" answer
    /// (<https://hl7.org/fhir/R4B/http.html#read>). That is an ordinary state
    /// of a deployment rather than a failure, so it reads as absent and the
    /// other three roots still compare.
    pub(crate) fn read(answered: Result<CapabilityStatement, FhirError>) -> Self {
        match answered {
            Ok(statement) => Self::Declared(Box::new(statement)),
            Err(error) if error.status() == Some(StatusCode::NOT_FOUND) => Self::Absent,
            Err(error) => Self::Refused(Box::new(error)),
        }
    }

    /// The statement this root sent, when it sent one.
    fn statement(&self) -> Option<&CapabilityStatement> {
        match self {
            Self::Declared(statement) => Some(statement),
            Self::Absent | Self::Refused(_) | Self::Unread => None,
        }
    }

    /// The failure this root answered, when it answered one.
    pub(crate) fn failure(&self) -> Option<&FhirError> {
        match self {
            Self::Refused(error) => Some(error),
            Self::Declared(_) | Self::Absent | Self::Unread => None,
        }
    }

    /// The one word a column header carries about this root's state.
    fn state(&self) -> String {
        match self {
            Self::Declared(_) => tr(Msg::StateAnswered),
            Self::Absent => tr(Msg::NotServed),
            Self::Refused(_) => tr(Msg::StateRefusedWord),
            Self::Unread => tr(Msg::NotRead),
        }
    }

    /// Whether the root answered.
    const fn answered(&self) -> bool {
        matches!(self, Self::Declared(_))
    }
}

/// One cell of the comparison, in the words the reader sees.
///
/// Absence is a word rather than an empty cell, and the three states are told
/// apart in text, so nothing here is carried by colour alone
/// (<https://www.w3.org/TR/WCAG22/#use-of-color>).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Cell {
    /// The root stated this.
    Stated(String),
    /// The root answered and stated nothing here.
    NotDeclared,
    /// The root did not answer, so the comparison states nothing about it.
    Unread,
}

/// One column of the comparison: the root it reads and how that read went.
#[derive(Clone, Debug)]
pub(crate) struct RootColumn {
    /// The FHIR version this root serves.
    pub(crate) version: FhirVersion,
    /// The release the root declared, absent when it declared none.
    pub(crate) fhir_version: Option<String>,
    /// The one word the header carries about the read.
    pub(crate) state: String,
}

/// One row of the comparison, with one cell per root, in column order.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    /// The row's heading, which is also what identifies it.
    ///
    /// It is derived from what the row describes, never from a position, so a
    /// row keeps its identity when a root stops declaring what it names.
    pub(crate) label: String,
    /// One cell per root, in the same order as the columns.
    pub(crate) cells: Vec<Cell>,
}

/// The four roots, read side by side.
#[derive(Clone, Debug)]
pub(crate) struct Comparison {
    /// One column per root, in release order.
    pub(crate) columns: Vec<RootColumn>,
    /// What each root says about itself.
    pub(crate) facts: Vec<Row>,
    /// The operations each root declares, and at which levels.
    pub(crate) operations: Vec<Row>,
}

/// Builds the comparison from what the four roots answered.
pub(crate) fn compare(answers: &[(FhirVersion, RootAnswer)]) -> Comparison {
    Comparison {
        columns: answers
            .iter()
            .map(|(version, answer)| RootColumn {
                version: *version,
                fhir_version: answer
                    .statement()
                    .and_then(|statement| statement.fhir_version.clone()),
                state: answer.state(),
            })
            .collect(),
        facts: facts(answers),
        operations: operations(answers),
    }
}

/// What each root says about itself, one row per fact.
fn facts(answers: &[(FhirVersion, RootAnswer)]) -> Vec<Row> {
    let row = |label: &str, of: &dyn Fn(&CapabilityStatement) -> Option<String>| Row {
        label: label.to_owned(),
        cells: answers
            .iter()
            .map(|(_, answer)| match answer.statement() {
                Some(statement) => of(statement).map_or(Cell::NotDeclared, Cell::Stated),
                None => Cell::Unread,
            })
            .collect(),
    };
    vec![
        row(&tr(Msg::Software), &CapabilityStatement::software_line),
        row(&tr(Msg::WhatRootSaysServes), &|statement| {
            statement.description.clone()
        }),
    ]
}

/// The operations the roots declare, one row per operation any root names.
///
/// The rows are the union of what the four documents declare, in the order
/// they were first seen, so an operation one root drops keeps its row and the
/// difference is visible instead of vanishing.
fn operations(answers: &[(FhirVersion, RootAnswer)]) -> Vec<Row> {
    let declared: Vec<Vec<OperationDeclaration>> = answers
        .iter()
        .map(|(_, answer)| {
            answer
                .statement()
                .map(CapabilityStatement::declarations)
                .unwrap_or_default()
        })
        .collect();
    let mut keys: Vec<(String, String)> = Vec::new();
    for operations in &declared {
        for operation in operations {
            let key = (operation.resource.clone(), operation.code.clone());
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys.into_iter()
        .map(|(resource, code)| Row {
            label: operation_label(&resource, &code),
            cells: answers
                .iter()
                .zip(&declared)
                .map(|((_, answer), operations)| {
                    if answer.statement().is_none() {
                        return Cell::Unread;
                    }
                    operations
                        .iter()
                        .find(|operation| operation.resource == resource && operation.code == code)
                        .map_or(Cell::NotDeclared, |operation| {
                            Cell::Stated(levels_of(operation))
                        })
                })
                .collect(),
        })
        .collect()
}

/// How one operation reads, as the URL that invokes it is written.
///
/// An operation a root declares on itself rather than on a resource type is
/// invoked at the root (<https://hl7.org/fhir/R4B/operations.html#executing>),
/// so it carries no resource type in front of the `$`.
fn operation_label(resource: &str, code: &str) -> String {
    if resource.is_empty() {
        format!("${code}")
    } else {
        format!("{resource}/${code}")
    }
}

/// The levels one declared operation is answered at, as a reader reads them.
fn levels_of(operation: &OperationDeclaration) -> String {
    if operation.levels.is_empty() {
        return tr(Msg::StateDeclared);
    }
    operation.levels.join(", ")
}

/// How the reads went, as the one sentence the live region announces.
pub(crate) fn state_sentence(answers: &[(FhirVersion, RootAnswer)]) -> String {
    let answered = answers
        .iter()
        .filter(|(_, answer)| answer.answered())
        .count();
    let sentence = tr_with(
        Msg::OfRootsAnswered,
        &[
            ("answered", (answered).to_string().into()),
            ("answers", (answers.len()).to_string().into()),
        ],
    );
    let rest: Vec<String> = answers
        .iter()
        .filter(|(_, answer)| !answer.answered())
        .map(|(version, answer)| format!("{} {}", version.label(), answer.state()))
        .collect();
    if rest.is_empty() {
        format!("{sentence}.")
    } else {
        format!("{sentence}: {}.", rest.join(", "))
    }
}

/// What each root adds beyond the definition its version publishes.
///
/// The text is the root's own `operation.documentation`, rendered as it
/// arrived, so this states nothing the document does not.
pub(crate) fn notes(answer: &RootAnswer) -> Vec<(String, String)> {
    answer
        .statement()
        .map(CapabilityStatement::declarations)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|operation| {
            let note = operation.documentation.clone()?;
            Some((operation_label(&operation.resource, &operation.code), note))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The recorded answer of each root, as the server sends it.
    const RECORDED: [(FhirVersion, &str); 4] = [
        (
            FhirVersion::R4,
            include_str!("../fixtures/capability-statement-r4.json"),
        ),
        (
            FhirVersion::R4B,
            include_str!("../fixtures/capability-statement-r4b.json"),
        ),
        (
            FhirVersion::R5,
            include_str!("../fixtures/capability-statement-r5.json"),
        ),
        (
            FhirVersion::R6,
            include_str!("../fixtures/capability-statement-r6.json"),
        ),
    ];

    /// The four recorded documents, read the way the screen reads them.
    fn recorded() -> Vec<(FhirVersion, RootAnswer)> {
        RECORDED
            .into_iter()
            .map(|(version, body)| {
                let statement: CapabilityStatement =
                    serde_json::from_str(body).expect("the recorded answer parses");
                (version, RootAnswer::read(Ok(statement)))
            })
            .collect()
    }

    /// The cells of the row labelled `label`, or a panic naming what was there.
    fn row<'a>(rows: &'a [Row], label: &str) -> &'a Row {
        rows.iter()
            .find(|row| row.label == label)
            .unwrap_or_else(|| {
                let present: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
                panic!("{label} is compared, among {present:?}")
            })
    }

    #[test]
    fn every_root_gets_a_column_carrying_the_release_it_declares() {
        let comparison = compare(&recorded());
        let declared: Vec<(&str, Option<&str>)> = comparison
            .columns
            .iter()
            .map(|column| (column.version.label(), column.fhir_version.as_deref()))
            .collect();
        assert_eq!(
            declared,
            [
                ("R4", Some("4.0.1")),
                ("R4B", Some("4.3.0")),
                ("R5", Some("5.0.0")),
                ("R6", Some("6.0.0-ballot5")),
            ],
            "the release comes from the document, never from the viewer"
        );
    }

    // NOTE: R4, R4B, and R5 publish `ConceptMap/$closure` and the R6 ballot
    // ships no definition for it (<https://hl7.org/fhir/R5/conceptmap-operation-closure.html>).
    #[test]
    fn the_closure_the_r6_ballot_dropped_reads_as_not_declared() {
        let comparison = compare(&recorded());
        assert_eq!(
            row(&comparison.operations, "$closure").cells,
            [
                Cell::Stated("declared".to_owned()),
                Cell::Stated("declared".to_owned()),
                Cell::Stated("declared".to_owned()),
                Cell::NotDeclared,
            ],
            "an operation one root drops keeps its row, so the difference shows"
        );
    }

    // NOTE: R4 and R4B declare `CodeSystem/$lookup` at the type level only, and
    // R5 added the instance level
    // (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>).
    #[test]
    fn the_instance_level_lookup_split_reads_from_the_documents() {
        let comparison = compare(&recorded());
        assert_eq!(
            row(&comparison.operations, "CodeSystem/$lookup").cells,
            [
                Cell::Stated("type".to_owned()),
                Cell::Stated("type".to_owned()),
                Cell::Stated("type, instance".to_owned()),
                Cell::Stated("type, instance".to_owned()),
            ],
            "the levels are the ones each version's own definition declares"
        );
        assert_eq!(
            row(&comparison.operations, "ConceptMap/$translate").cells,
            [
                Cell::Stated("type, instance".to_owned()),
                Cell::Stated("type, instance".to_owned()),
                Cell::Stated("type, instance".to_owned()),
                Cell::Stated("type, instance".to_owned()),
            ],
            "an operation every version declares alike reads alike"
        );
    }

    #[test]
    fn only_the_ballot_root_states_what_it_serves() {
        let comparison = compare(&recorded());
        let cells = &row(&comparison.facts, "What the root says it serves").cells;
        assert_eq!(
            cells
                .iter()
                .filter(|cell| **cell == Cell::NotDeclared)
                .count(),
            3,
            "the three published releases state nothing there: {cells:?}"
        );
        let Some(Cell::Stated(note)) = cells.last() else {
            panic!("the ballot root states what it serves: {cells:?}")
        };
        assert!(
            note.contains("ballot"),
            "the note is the root's own words: {note}"
        );
    }

    #[test]
    fn a_root_the_deployment_does_not_serve_reads_as_absent() {
        let answer = RootAnswer::read(Err(FhirError::Status {
            url: "/r6/metadata".to_owned(),
            status: StatusCode::NOT_FOUND,
            body: String::new(),
        }));
        assert!(matches!(answer, RootAnswer::Absent), "{answer:?}");
        let mut answers = recorded();
        answers[3] = (FhirVersion::R6, answer);
        let comparison = compare(&answers);
        assert_eq!(
            comparison.columns[3].state, "not served",
            "the header says so in words"
        );
        assert_eq!(
            row(&comparison.operations, "CodeSystem/$lookup").cells[3],
            Cell::Unread,
            "an unserved root states nothing, and the other three still compare"
        );
        assert_eq!(
            row(&comparison.operations, "CodeSystem/$lookup").cells[2],
            Cell::Stated("type, instance".to_owned())
        );
    }

    #[test]
    fn a_refused_read_leaves_the_other_three_columns_standing() {
        let answer = RootAnswer::read(Err(FhirError::Transport {
            url: "/r5/metadata".to_owned(),
            message: "network error".to_owned(),
        }));
        assert!(matches!(answer, RootAnswer::Refused(_)), "{answer:?}");
        let mut answers = recorded();
        answers[2] = (FhirVersion::R5, answer);
        let comparison = compare(&answers);
        assert_eq!(comparison.columns[2].state, "refused");
        let cells = &row(&comparison.operations, "CodeSystem/$lookup").cells;
        assert_eq!(cells[2], Cell::Unread);
        assert_eq!(cells[0], Cell::Stated("type".to_owned()));
        assert_eq!(cells[3], Cell::Stated("type, instance".to_owned()));
    }

    #[test]
    fn the_rows_are_the_union_of_what_the_documents_declare() {
        let comparison = compare(&recorded());
        let keys: Vec<&str> = comparison
            .operations
            .iter()
            .map(|row| row.label.as_str())
            .collect();
        assert!(
            keys.len() > 5,
            "the roots declare more than a handful: {keys:?}"
        );
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len(), "no operation is compared twice");
    }

    #[test]
    fn a_root_that_declares_nothing_still_gets_its_column() {
        let empty: CapabilityStatement =
            serde_json::from_str(r#"{"resourceType":"CapabilityStatement"}"#)
                .expect("the answer parses");
        let answers = vec![(FhirVersion::R4B, RootAnswer::read(Ok(empty)))];
        let comparison = compare(&answers);
        assert_eq!(comparison.columns.len(), 1);
        assert_eq!(comparison.columns[0].state, "answered");
        assert!(
            comparison.operations.is_empty(),
            "there is nothing to compare, and the screen says so"
        );
        assert_eq!(
            comparison.columns[0].fhir_version, None,
            "the column states no release, because the root declared none"
        );
        assert_eq!(
            row(&comparison.facts, "Software").cells,
            [Cell::NotDeclared],
            "a fact the root omitted is stated as absent"
        );
    }

    #[test]
    fn the_notes_are_the_words_the_root_sent() {
        let answers = recorded();
        let (_, r4) = &answers[0];
        let pre_adopted = notes(r4);
        assert!(
            pre_adopted
                .iter()
                .any(|(operation, note)| operation == "CodeSystem/$lookup"
                    && note.contains("useSupplement")),
            "R4 says what it pre-adopts: {pre_adopted:?}"
        );
        let (_, r6) = &answers[3];
        assert!(
            notes(r6)
                .iter()
                .all(|(operation, _)| operation != "ValueSet/$expand"),
            "the ballot root adds nothing to its own $expand definition"
        );
    }

    #[test]
    fn the_announced_sentence_names_every_root_that_did_not_answer() {
        assert_eq!(state_sentence(&recorded()), "4 of 4 roots answered.");
        let mut answers = recorded();
        answers[3] = (FhirVersion::R6, RootAnswer::Absent);
        assert_eq!(
            state_sentence(&answers),
            "3 of 4 roots answered: R6 not served.",
            "a reader who cannot see the table still hears which root is missing"
        );
    }

    #[test]
    fn an_unread_root_states_nothing_anywhere() {
        let answers = vec![(FhirVersion::R4B, RootAnswer::Unread)];
        let comparison = compare(&answers);
        assert_eq!(comparison.columns[0].state, "not read");
        assert_eq!(row(&comparison.facts, "Software").cells, [Cell::Unread]);
        assert_eq!(notes(&RootAnswer::Unread), Vec::<(String, String)>::new());
    }
}
