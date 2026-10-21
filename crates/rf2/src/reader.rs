// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: Apache-2.0

//! Streaming reader for one RF2 file.
//!
//! RF2 files are UTF-8, tab-delimited, unquoted, with a header row naming
//! the columns and CRLF line ends. The reader validates the header against
//! the columns the caller expects, then yields one record per row with its
//! line number, so a field error names the file, the line, and the column.

use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};

use crate::id::IdError;
use crate::time::EffectiveTimeError;

/// A malformed field value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FieldError {
    /// Not a valid identifier.
    #[error(transparent)]
    Id(#[from] IdError),
    /// Not a valid effective time.
    #[error(transparent)]
    Time(#[from] EffectiveTimeError),
    /// Not `0` or `1`.
    #[error("{text:?} is not a boolean (0 or 1)")]
    Boolean {
        /// The offending text.
        text: String,
    },
    /// Not an integer.
    #[error("{text:?} is not an integer")]
    Integer {
        /// The offending text.
        text: String,
    },
    /// A column the row does not have.
    #[error("column {column} is missing")]
    Missing {
        /// The zero-based column index.
        column: usize,
    },
    /// A value with an invalid lexical form for its column.
    #[error("{text:?} is not a valid {what}")]
    Invalid {
        /// What the column holds.
        what: &'static str,
        /// The offending text.
        text: String,
    },
}

/// A failure while reading an RF2 file.
#[derive(Debug, thiserror::Error)]
pub enum Rf2Error {
    /// The file could not be opened or read.
    #[error("cannot read {path}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: io::Error,
    },
    /// The header row is not the expected column list.
    #[error("{path}: header is {actual:?}, expected {expected:?}")]
    Header {
        /// The file.
        path: PathBuf,
        /// The columns the caller expects.
        expected: Vec<String>,
        /// The columns the file names.
        actual: Vec<String>,
    },
    /// The file has no header row.
    #[error("{path}: the file is empty")]
    Empty {
        /// The file.
        path: PathBuf,
    },
    /// A row could not be split into fields.
    #[error("{path}: malformed row")]
    Csv {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: csv::Error,
    },
    /// A row has the wrong number of fields.
    #[error("{path}:{line}: {actual} fields, expected {expected}")]
    FieldCount {
        /// The file.
        path: PathBuf,
        /// The one-based line number.
        line: u64,
        /// The expected number of fields.
        expected: usize,
        /// The number found.
        actual: usize,
    },
    /// A field value is malformed.
    #[error("{path}:{line}: column {column} ({name}): {source}")]
    Field {
        /// The file.
        path: PathBuf,
        /// The one-based line number.
        line: u64,
        /// The zero-based column index.
        column: usize,
        /// The column name.
        name: String,
        /// The underlying error.
        #[source]
        source: FieldError,
    },
}

/// One data row, with the file and line it came from.
#[derive(Debug)]
pub struct Record<'a> {
    path: &'a Path,
    line: u64,
    columns: &'a [String],
    fields: csv::StringRecord,
}

impl Record<'_> {
    /// The one-based line number in the file.
    #[must_use]
    pub fn line(&self) -> u64 {
        self.line
    }

    /// The name of column `index`, if the header has one.
    #[must_use]
    pub fn column_name(&self, index: usize) -> Option<&str> {
        self.columns.get(index).map(String::as_str)
    }

    /// The raw text of column `index`.
    ///
    /// # Errors
    ///
    /// Returns [`Rf2Error::Field`] when the row has no such column.
    pub fn text(&self, index: usize) -> Result<&str, Rf2Error> {
        self.fields
            .get(index)
            .ok_or_else(|| self.field_error(index, FieldError::Missing { column: index }))
    }

    /// Parses column `index` with `parse`, attributing a failure to this row.
    ///
    /// # Errors
    ///
    /// Returns [`Rf2Error::Field`] when the column is missing or `parse` fails.
    pub fn parse<T, E: Into<FieldError>>(
        &self,
        index: usize,
        parse: impl FnOnce(&str) -> Result<T, E>,
    ) -> Result<T, Rf2Error> {
        let text = self.text(index)?;
        parse(text).map_err(|e| self.field_error(index, e.into()))
    }

    /// Reads an RF2 boolean (`0` or `1`) from column `index`.
    ///
    /// # Errors
    ///
    /// Returns [`Rf2Error::Field`] for any other text.
    pub fn boolean(&self, index: usize) -> Result<bool, Rf2Error> {
        self.parse(index, |text| match text {
            "1" => Ok(true),
            "0" => Ok(false),
            other => Err(FieldError::Boolean {
                text: other.to_owned(),
            }),
        })
    }

    /// Reads an integer from column `index`.
    ///
    /// # Errors
    ///
    /// Returns [`Rf2Error::Field`] for any other text.
    pub fn integer(&self, index: usize) -> Result<i64, Rf2Error> {
        self.parse(index, |text| {
            // The field error names the text; a `ParseIntError` adds nothing to it.
            let Ok(value) = text.parse::<i64>() else {
                return Err(FieldError::Integer {
                    text: text.to_owned(),
                });
            };
            Ok(value)
        })
    }

    /// An error attributed to column `index` of this row.
    #[must_use]
    pub fn field_error(&self, index: usize, source: FieldError) -> Rf2Error {
        Rf2Error::Field {
            path: self.path.to_path_buf(),
            line: self.line,
            column: index,
            name: self.columns.get(index).cloned().unwrap_or_default(),
            source,
        }
    }
}

/// A reader over one RF2 file whose header has been validated.
#[derive(Debug)]
pub struct Rf2Reader<R: Read> {
    path: PathBuf,
    columns: Vec<String>,
    inner: csv::Reader<R>,
    line: u64,
}

impl Rf2Reader<BufReader<File>> {
    /// Opens `path` and validates its header against `expected`.
    ///
    /// # Errors
    ///
    /// Returns [`Rf2Error`] when the file cannot be read or its header differs.
    pub fn open(path: &Path, expected: &[&str]) -> Result<Self, Rf2Error> {
        let file = File::open(path).map_err(|source| Rf2Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::new(path, BufReader::new(file), expected)
    }
}

impl<R: Read> Rf2Reader<R> {
    /// Wraps `read` (positioned at the header row) and validates the header.
    ///
    /// # Errors
    ///
    /// Returns [`Rf2Error`] when the header is missing or differs from `expected`.
    pub fn new(path: &Path, read: R, expected: &[&str]) -> Result<Self, Rf2Error> {
        Self::new_with(path, read, |actual| {
            if actual == expected {
                Ok(())
            } else {
                Err(Rf2Error::Header {
                    path: path.to_path_buf(),
                    expected: expected.iter().map(|c| (*c).to_owned()).collect(),
                    actual: actual.to_vec(),
                })
            }
        })
    }

    /// Wraps `read` and validates the header with `check`, which sees the
    /// column names and decides whether the file is acceptable.
    ///
    /// # Errors
    ///
    /// Returns [`Rf2Error::Empty`] for a file without a header, [`Rf2Error::Csv`]
    /// for an unreadable one, and whatever `check` returns.
    pub fn new_with(
        path: &Path,
        read: R,
        check: impl FnOnce(&[String]) -> Result<(), Rf2Error>,
    ) -> Result<Self, Rf2Error> {
        // NOTE: the release file specification defines escaping only inside the
        // concrete-value grammar (https://docs.snomed.org/snomed-ct-specifications/snomed-ct-release-file-specification/release-types-packages-and-files/3.1-common-features-of-all-release-files/3.1.1-general-structure-of-release-files),
        // so a double quote is ordinary text and quoting stays off: our own design.
        let mut inner = csv::ReaderBuilder::new()
            .delimiter(b'\t')
            .quoting(false)
            .has_headers(true)
            .flexible(true)
            .from_reader(read);
        let header = inner.headers().map_err(|source| Rf2Error::Csv {
            path: path.to_path_buf(),
            source,
        })?;
        let actual: Vec<String> = header
            .iter()
            .map(|field| field.trim_start_matches('\u{feff}').to_owned())
            .collect();
        if actual.is_empty() || actual == [""] {
            return Err(Rf2Error::Empty {
                path: path.to_path_buf(),
            });
        }
        check(&actual)?;
        Ok(Self {
            path: path.to_path_buf(),
            columns: actual,
            inner,
            line: 1,
        })
    }

    /// Wraps `read` positioned after a header the caller already read and
    /// checked, so the reader starts at the first data row.
    ///
    /// # Errors
    ///
    /// Returns [`Rf2Error::Header`] when `actual` differs from `expected`.
    pub fn new_validated(
        path: &Path,
        read: R,
        actual: &[String],
        expected: &[&str],
    ) -> Result<Self, Rf2Error> {
        if actual != expected {
            return Err(Rf2Error::Header {
                path: path.to_path_buf(),
                expected: expected.iter().map(|c| (*c).to_owned()).collect(),
                actual: actual.to_vec(),
            });
        }
        let inner = csv::ReaderBuilder::new()
            .delimiter(b'\t')
            .quoting(false)
            .has_headers(false)
            .flexible(true)
            .from_reader(read);
        Ok(Self {
            path: path.to_path_buf(),
            columns: actual.to_vec(),
            inner,
            line: 1,
        })
    }

    /// The header columns.
    #[must_use]
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// The next data row, `None` at the end of the file.
    ///
    /// # Errors
    ///
    /// Returns [`Rf2Error`] for an unreadable row or a wrong field count.
    pub fn next_record(&mut self) -> Result<Option<Record<'_>>, Rf2Error> {
        let mut fields = csv::StringRecord::new();
        let read = self
            .inner
            .read_record(&mut fields)
            .map_err(|source| Rf2Error::Csv {
                path: self.path.clone(),
                source,
            })?;
        if !read {
            return Ok(None);
        }
        self.line = self.line.saturating_add(1);
        if fields.len() != self.columns.len() {
            return Err(Rf2Error::FieldCount {
                path: self.path.clone(),
                line: self.line,
                expected: self.columns.len(),
                actual: fields.len(),
            });
        }
        Ok(Some(Record {
            path: &self.path,
            line: self.line,
            columns: &self.columns,
            fields,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Rf2Error, Rf2Reader};

    #[test]
    fn header_is_validated_and_rows_carry_line_numbers() {
        let text = "id\teffectiveTime\tactive\r\n1\t20200131\t1\r\n2\t20200131\t0\r\n";
        let mut reader = Rf2Reader::new(
            Path::new("x.txt"),
            text.as_bytes(),
            &["id", "effectiveTime", "active"],
        )
        .expect("header matches");
        let first = reader.next_record().expect("row").expect("present");
        assert_eq!(first.line(), 2);
        assert_eq!(first.text(0).expect("field"), "1");
        assert!(first.boolean(2).expect("bool"));
        let second = reader.next_record().expect("row").expect("present");
        assert!(!second.boolean(2).expect("bool"));
        assert!(reader.next_record().expect("end").is_none());
    }

    #[test]
    fn a_wrong_header_or_field_count_is_refused() {
        let wrong = Rf2Reader::new(
            Path::new("x.txt"),
            "id\tactive\r\n".as_bytes(),
            &["id", "effectiveTime"],
        );
        assert!(matches!(wrong, Err(Rf2Error::Header { .. })));
        let mut short = Rf2Reader::new(
            Path::new("x.txt"),
            "id\tactive\r\n1\r\n".as_bytes(),
            &["id", "active"],
        )
        .expect("header matches");
        assert!(matches!(
            short.next_record(),
            Err(Rf2Error::FieldCount { line: 2, .. })
        ));
        assert!(matches!(
            Rf2Reader::new(Path::new("x.txt"), "".as_bytes(), &["id"]),
            Err(Rf2Error::Empty { .. })
        ));
    }

    #[test]
    fn a_byte_order_mark_does_not_break_the_header() {
        let text = "\u{feff}id\tactive\r\n";
        let reader = Rf2Reader::new(Path::new("x.txt"), text.as_bytes(), &["id", "active"]);
        assert!(reader.is_ok());
    }
}
