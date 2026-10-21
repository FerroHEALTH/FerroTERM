// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The term table, `Loinc.csv`: one row per LOINC term with every column kept
//! by name, since the FHIR filters range over "any field of the table".

use std::collections::BTreeMap;

use crate::id;
use crate::release::{Release, ReleaseError, TERMS, Table, csv_at, field};

/// The status values of `STATUS`.
pub const ACTIVE: &str = "ACTIVE";
/// `DEPRECATED`: inactive for value sets (<https://hl7.org/fhir/R4B/loinc.html>).
pub const DEPRECATED: &str = "DEPRECATED";

/// One term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    /// `LOINC_NUM`.
    pub code: String,
    /// `LONG_COMMON_NAME`.
    pub long_common_name: String,
    /// `SHORTNAME`.
    pub short_name: String,
    /// `CONSUMER_NAME`.
    pub consumer_name: String,
    /// `STATUS`.
    pub status: String,
    /// `EXTERNAL_COPYRIGHT_NOTICE`, when the term has one.
    pub external_copyright: Option<String>,
    /// Every non-empty column by its header name.
    pub fields: BTreeMap<String, String>,
}

impl Term {
    /// Whether the term is active in the sense of the FHIR page:
    /// `STATUS != DEPRECATED`.
    #[must_use]
    pub fn active(&self) -> bool {
        !self.status.eq_ignore_ascii_case(DEPRECATED)
    }
}

/// The columns of `Loinc.csv`, in header order, and its rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Terms {
    /// The header names.
    pub columns: Vec<String>,
    /// The rows, in file order.
    pub rows: Vec<Term>,
}

/// Reads the term table.
///
/// # Errors
///
/// Returns [`ReleaseError`] when the file is missing, does not parse, lacks
/// `LOINC_NUM`, or a code is not shaped like a LOINC code.
pub fn read(release: &Release) -> Result<Terms, ReleaseError> {
    let mut table = Table::open(release.file(TERMS)?)?;
    let code_at = table.column("LOINC_NUM")?;
    let columns = table.columns();
    let at = |name: &str| columns.iter().position(|c| c.eq_ignore_ascii_case(name));
    let long_at = at("LONG_COMMON_NAME");
    let short_at = at("SHORTNAME");
    let consumer_at = at("CONSUMER_NAME");
    let status_at = at("STATUS");
    let copyright_at = at("EXTERNAL_COPYRIGHT_NOTICE");
    let mut rows = Vec::new();
    let path = table.path.clone();
    for record in table.reader.records() {
        let record = record.map_err(|e| csv_at(&path, e))?;
        let code = field(&record, code_at).to_owned();
        if !id::is_well_formed(&code) {
            return Err(ReleaseError::Code {
                path: table.path.clone(),
                code,
            });
        }
        let text = |at: Option<usize>| at.map(|i| field(&record, i).to_owned()).unwrap_or_default();
        let mut fields = BTreeMap::new();
        for (index, column) in columns.iter().enumerate() {
            let value = field(&record, index);
            if !value.is_empty() {
                fields.insert(column.clone(), value.to_owned());
            }
        }
        rows.push(Term {
            code,
            long_common_name: text(long_at),
            short_name: text(short_at),
            consumer_name: text(consumer_at),
            status: text(status_at),
            external_copyright: copyright_at
                .map(|i| field(&record, i))
                .filter(|v| !v.is_empty())
                .map(str::to_owned),
            fields,
        });
    }
    Ok(Terms { columns, rows })
}
