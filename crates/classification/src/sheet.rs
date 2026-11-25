// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The rows of a publisher's table: the first worksheet of an Office Open XML
//! workbook (`.xlsx`, ECMA-376 Part 1 §18.3), or a delimited text export.
//!
//! A workbook cell is a shared string (`t="s"`, §18.4.8), an inline string
//! (`t="inlineStr"`), or a value written as it stands; formulas and styles
//! are not read. A delimited file is split on the first of tab, `|`, `;`, or
//! `,` its header line holds.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::Rubric;
use crate::xml::{Element, records};

/// A failure to read a table.
#[derive(Debug, thiserror::Error)]
pub enum SheetError {
    /// The file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The workbook is not a readable zip.
    #[error("{path} is not a readable workbook")]
    Zip {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: zip::result::ZipError,
    },
    /// A workbook part is not well-formed XML.
    #[error("{path}: a workbook part does not parse")]
    Xml {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: quick_xml::Error,
    },
    /// The workbook names no worksheet.
    #[error("{path} holds no worksheet")]
    NoSheet {
        /// The path.
        path: PathBuf,
    },
    /// The delimited text does not parse.
    #[error("cannot parse {path}")]
    Csv {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: csv::Error,
    },
    /// No row carries the columns the reader needs.
    #[error("{path} has no header row with the column `{column}`")]
    Column {
        /// The path.
        path: PathBuf,
        /// The column looked for.
        column: &'static str,
    },
}

/// A table: its rows of cells, every cell trimmed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sheet {
    /// The worksheet's name, when the source is a workbook.
    pub name: Option<String>,
    /// The rows in order, empty cells as empty strings.
    pub rows: Vec<Vec<String>>,
}

/// A header row located in a [`Sheet`]: the index of the row and the column
/// of every name asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// The index of the header row; data rows follow it.
    pub row: usize,
    columns: Vec<Option<usize>>,
}

impl Header {
    /// The column of the `index`th name given to [`Sheet::header`].
    #[must_use]
    pub fn column(&self, index: usize) -> Option<usize> {
        self.columns.get(index).copied().flatten()
    }

    /// The non-empty cell of `row` under the `index`th name.
    #[must_use]
    pub fn cell<'a>(&self, row: &'a [String], index: usize) -> Option<&'a str> {
        self.column(index)
            .and_then(|c| row.get(c))
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    /// An English rubric of each `(index, kind)` whose cell in `row` is not empty.
    #[must_use]
    pub fn rubrics(&self, row: &[String], kinds: &[(usize, &str)]) -> Vec<Rubric> {
        kinds
            .iter()
            .filter_map(|(index, kind)| {
                self.cell(row, *index)
                    .map(|value| Rubric::english(kind, value))
            })
            .collect()
    }
}

/// One column a reader looks for: the header names it accepts (compared
/// case-insensitively, after trimming) and whether it is required.
#[derive(Debug, Clone, Copy)]
pub struct Column {
    /// The name reported when a required column is missing.
    pub name: &'static str,
    /// Every header text that names the column, lower case.
    pub aliases: &'static [&'static str],
    /// Whether the header row must hold it.
    pub required: bool,
}

impl Sheet {
    /// The first row (of the first 20) holding every required column, with
    /// the position of each column asked for.
    ///
    /// # Errors
    ///
    /// Returns [`SheetError::Column`] naming the first required column when
    /// no row holds them all.
    pub fn header(&self, path: &Path, columns: &[Column]) -> Result<Header, SheetError> {
        let find = |row: &[String], column: &Column| {
            row.iter().position(|cell| {
                column
                    .aliases
                    .contains(&cell.trim().to_lowercase().as_str())
            })
        };
        for (index, row) in self.rows.iter().enumerate().take(20) {
            let found: Vec<Option<usize>> = columns.iter().map(|c| find(row, c)).collect();
            if columns
                .iter()
                .zip(&found)
                .all(|(column, at)| !column.required || at.is_some())
            {
                return Ok(Header {
                    row: index,
                    columns: found,
                });
            }
        }
        Err(SheetError::Column {
            path: path.to_path_buf(),
            column: columns.iter().find(|c| c.required).map_or("", |c| c.name),
        })
    }

    /// The rows after `header` with a code under its first column, as
    /// `(code, row)`.
    pub fn coded<'a>(
        &'a self,
        header: &'a Header,
    ) -> impl Iterator<Item = (&'a str, &'a [String])> + 'a {
        self.data(header)
            .filter_map(|row| header.cell(row, 0).map(|code| (code, row)))
    }

    /// The rows after `header`.
    pub fn data<'a>(&'a self, header: &Header) -> impl Iterator<Item = &'a [String]> + 'a {
        self.rows
            .iter()
            .skip(header.row.saturating_add(1))
            .map(Vec::as_slice)
    }
}

/// Reads `path` as a workbook when its name ends in `.xlsx`, otherwise as
/// delimited text.
///
/// # Errors
///
/// Returns [`SheetError`] when the file does not read or parse.
pub fn read(path: &Path) -> Result<Sheet, SheetError> {
    let is_workbook = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("xlsx"));
    if is_workbook {
        read_workbook(path)
    } else {
        read_delimited(path)
    }
}

/// The delimiter of a text export: the first of tab, `|`, `;`, and `,` on the header line.
fn delimiter(text: &str) -> u8 {
    let header = text.lines().next().unwrap_or_default();
    b"\t|;,"
        .iter()
        .copied()
        .find(|d| header.contains(char::from(*d)))
        .unwrap_or(b',')
}

fn read_delimited(path: &Path) -> Result<Sheet, SheetError> {
    let text = std::fs::read_to_string(path).map_err(|source| SheetError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter(text))
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|source| SheetError::Csv {
            path: path.to_path_buf(),
            source,
        })?;
        rows.push(record.iter().map(|c| c.trim().to_owned()).collect());
    }
    Ok(Sheet { name: None, rows })
}

/// The text of one zip part, `None` when the workbook lacks it.
fn part(
    archive: &mut zip::ZipArchive<std::fs::File>,
    name: &str,
    path: &Path,
) -> Result<Option<String>, SheetError> {
    let mut entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(source) => {
            return Err(SheetError::Zip {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .map_err(|source| SheetError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(Some(text))
}

/// Every element named `record` in `text`.
fn elements(text: &str, record: &str, path: &Path) -> Result<Vec<Element>, SheetError> {
    let mut out = Vec::new();
    records::<quick_xml::Error>(text, record, |e| {
        out.push(e);
        Ok(())
    })
    .map_err(|source| SheetError::Xml {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(out)
}

/// The text of a string item (`si` or `is`, §18.4.8): its `t`, or the `t`
/// of each rich-text run; phonetic runs (`rPh`) are left out.
fn string_item(item: &Element) -> String {
    item.children
        .iter()
        .filter_map(|c| match c.local() {
            "t" => Some(c.text.as_str()),
            "r" => c.child("t").map(|t| t.text.as_str()),
            _ => None,
        })
        .collect()
}

/// The zero-based column of a cell reference (`C12` is 2).
fn column_of(reference: &str) -> Option<usize> {
    let mut column: usize = 0;
    let mut letters = 0;
    for c in reference.chars().take_while(char::is_ascii_alphabetic) {
        let digit = usize::from(
            u8::try_from(c.to_ascii_uppercase())
                .ok()?
                .checked_sub(b'A')?,
        );
        column = column.checked_mul(26)?.checked_add(digit.checked_add(1)?)?;
        letters += 1;
    }
    (letters > 0).then(|| column.saturating_sub(1))
}

fn read_workbook(path: &Path) -> Result<Sheet, SheetError> {
    let file = std::fs::File::open(path).map_err(|source| SheetError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|source| SheetError::Zip {
        path: path.to_path_buf(),
        source,
    })?;
    let no_sheet = || SheetError::NoSheet {
        path: path.to_path_buf(),
    };
    let workbook = part(&mut archive, "xl/workbook.xml", path)?.ok_or_else(no_sheet)?;
    let first = elements(&workbook, "sheet", path)?
        .into_iter()
        .next()
        .ok_or_else(no_sheet)?;
    let relation = first.attr("id").map(str::to_owned);
    let relations = part(&mut archive, "xl/_rels/workbook.xml.rels", path)?.unwrap_or_default();
    let target = elements(&relations, "Relationship", path)?
        .into_iter()
        .find(|r| r.attr("Id") == relation.as_deref())
        .and_then(|r| r.attr("Target").map(str::to_owned))
        .ok_or_else(no_sheet)?;
    let target = target
        .strip_prefix('/')
        .map_or_else(|| format!("xl/{target}"), str::to_owned);
    let strings: Vec<String> = match part(&mut archive, "xl/sharedStrings.xml", path)? {
        Some(text) => elements(&text, "si", path)?
            .iter()
            .map(string_item)
            .collect(),
        None => Vec::new(),
    };
    let sheet = part(&mut archive, &target, path)?.ok_or_else(no_sheet)?;
    let mut rows = Vec::new();
    for row in elements(&sheet, "row", path)? {
        let mut cells: Vec<String> = Vec::new();
        for cell in row.all("c") {
            let value = match cell.attr("t") {
                Some("s") => cell
                    .text_at("v")
                    .and_then(|v| v.parse::<usize>().ok())
                    .and_then(|i| strings.get(i))
                    .cloned()
                    .unwrap_or_default(),
                Some("inlineStr") => cell.child("is").map(string_item).unwrap_or_default(),
                _ => cell.text_at("v").unwrap_or_default().to_owned(),
            };
            let at = cell.attr("r").and_then(column_of).unwrap_or(cells.len());
            if cells.len() <= at {
                cells.resize(at.saturating_add(1), String::new());
            }
            if let Some(slot) = cells.get_mut(at) {
                value.trim().clone_into(slot);
            }
        }
        rows.push(cells);
    }
    Ok(Sheet {
        name: first.attr("name").map(str::to_owned),
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::column_of;

    #[test]
    fn a_cell_reference_names_its_column() {
        assert_eq!(column_of("A1"), Some(0));
        assert_eq!(column_of("C12"), Some(2));
        assert_eq!(column_of("AA3"), Some(26));
        assert_eq!(column_of("12"), None);
    }
}
