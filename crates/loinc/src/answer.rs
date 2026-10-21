// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answer lists (`AnswerList.csv`): each list (`LL…`) with its answers
//! (`LA…`), and the terms each list is linked to (`LoincAnswerListLink.csv`).

use std::collections::BTreeMap;

use crate::release::{ANSWER_LINKS, ANSWER_LISTS, Release, ReleaseError, Table, csv_at, field};

/// One answer of a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// `AnswerStringId` (`LA…`).
    pub code: String,
    /// `DisplayText`.
    pub display: String,
    /// `SequenceNumber`, when given.
    pub sequence: Option<u32>,
}

/// One answer list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnswerList {
    /// `AnswerListId` (`LL…`).
    pub code: String,
    /// `AnswerListName`.
    pub name: String,
    /// The answers, in file order.
    pub answers: Vec<Answer>,
    /// The terms the list is linked to (`LoincNumber`).
    pub terms: Vec<String>,
}

/// Reads the answer lists and their links, by list code.
///
/// # Errors
///
/// Returns [`ReleaseError`] when a file does not parse or lacks a column; a
/// release without the files yields no lists.
pub fn read(release: &Release) -> Result<BTreeMap<String, AnswerList>, ReleaseError> {
    let mut lists: BTreeMap<String, AnswerList> = BTreeMap::new();
    if let Some(path) = release.optional(ANSWER_LISTS) {
        let mut table = Table::open(path)?;
        let list_at = table.column("AnswerListId")?;
        let name_at = table.column("AnswerListName")?;
        let answer_at = table.column("AnswerStringId")?;
        let display_at = table.column("DisplayText")?;
        let sequence_at = table.column("SequenceNumber")?;
        let path = table.path.clone();
        for record in table.reader.records() {
            let record = record.map_err(|e| csv_at(&path, e))?;
            let code = field(&record, list_at).to_owned();
            if code.is_empty() {
                continue;
            }
            let list = lists.entry(code.clone()).or_insert_with(|| AnswerList {
                code,
                name: field(&record, name_at).to_owned(),
                ..AnswerList::default()
            });
            let answer = field(&record, answer_at);
            if !answer.is_empty() {
                list.answers.push(Answer {
                    code: answer.to_owned(),
                    display: field(&record, display_at).to_owned(),
                    sequence: field(&record, sequence_at).parse().ok(),
                });
            }
        }
    }
    if let Some(path) = release.optional(ANSWER_LINKS) {
        let mut table = Table::open(path)?;
        let term_at = table.column("LoincNumber")?;
        let list_at = table.column("AnswerListId")?;
        let path = table.path.clone();
        for record in table.reader.records() {
            let record = record.map_err(|e| csv_at(&path, e))?;
            let list = field(&record, list_at);
            if let Some(entry) = lists.get_mut(list) {
                entry.terms.push(field(&record, term_at).to_owned());
            }
        }
    }
    Ok(lists)
}
