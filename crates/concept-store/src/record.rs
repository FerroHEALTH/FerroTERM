// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The byte encodings of the stored records.
//!
//! Little-endian, length-prefixed strings, a tag byte per property value.
//! Encoding and decoding are inverse; a decode failure names what was found.

use std::fmt;

use concept_graph::ordinal::Ordinal;

/// A damaged record.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecordError {
    /// The bytes end before the record does.
    #[error("record truncated at byte {at}")]
    Truncated {
        /// The offset where more bytes were expected.
        at: usize,
    },
    /// A string is not UTF-8.
    #[error("string at byte {at} is not UTF-8")]
    Utf8 {
        /// The offset of the string.
        at: usize,
        /// The cause.
        #[source]
        source: std::str::Utf8Error,
    },
    /// A property value tag is not one this build knows.
    #[error("unknown property value tag {tag}")]
    Tag {
        /// The tag byte.
        tag: u8,
    },
    /// Bytes remain after the record.
    #[error("{remaining} trailing byte(s) after the record")]
    Trailing {
        /// How many bytes remain.
        remaining: usize,
    },
}

/// A stored concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Concept {
    /// The native code, for example the SCTID.
    pub code: String,
    /// Whether the concept is active in this version.
    pub active: bool,
    /// The version the concept last changed, as the code system writes it.
    pub effective_time: Option<String>,
    /// The module or owner concept, when the code system has one.
    pub module: Option<Ordinal>,
}

/// A stored designation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Designation {
    /// The native identifier of the designation, when the code system has one.
    pub id: Option<String>,
    /// The text.
    pub term: String,
    /// The BCP 47 language.
    pub language: String,
    /// The designation use (a `DESIGNATION_USES` ordinal).
    pub use_ordinal: u32,
    /// Whether the designation is active.
    pub active: bool,
}

/// A stored designation borrowing its text from the record's bytes, so a
/// reader that scans for one designation allocates only for the one it keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesignationRef<'a> {
    /// The native identifier of the designation, when the code system has one.
    pub id: Option<&'a str>,
    /// The text.
    pub term: &'a str,
    /// The BCP 47 language.
    pub language: &'a str,
    /// The designation use (a `DESIGNATION_USES` ordinal).
    pub use_ordinal: u32,
    /// Whether the designation is active.
    pub active: bool,
}

impl From<DesignationRef<'_>> for Designation {
    fn from(designation: DesignationRef<'_>) -> Self {
        Self {
            id: designation.id.map(str::to_owned),
            term: designation.term.to_owned(),
            language: designation.language.to_owned(),
            use_ordinal: designation.use_ordinal,
            active: designation.active,
        }
    }
}

/// One typed property value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyValue {
    /// A reference to another concept of the same version.
    Concept(Ordinal),
    /// A code the code system defines.
    Code(String),
    /// A string.
    String(String),
    /// An integer.
    Integer(i64),
    /// A boolean.
    Boolean(bool),
    /// A decimal in its lexical form.
    Decimal(String),
    /// A date or time in its lexical form.
    DateTime(String),
}

/// One typed property value borrowing its text from the record's bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyValueRef<'a> {
    /// A reference to another concept of the same version.
    Concept(Ordinal),
    /// A code the code system defines.
    Code(&'a str),
    /// A string.
    String(&'a str),
    /// An integer.
    Integer(i64),
    /// A boolean.
    Boolean(bool),
    /// A decimal in its lexical form.
    Decimal(&'a str),
    /// A date or time in its lexical form.
    DateTime(&'a str),
}

impl From<PropertyValueRef<'_>> for PropertyValue {
    fn from(value: PropertyValueRef<'_>) -> Self {
        match value {
            PropertyValueRef::Concept(o) => Self::Concept(o),
            PropertyValueRef::Code(s) => Self::Code(s.to_owned()),
            PropertyValueRef::String(s) => Self::String(s.to_owned()),
            PropertyValueRef::Integer(i) => Self::Integer(i),
            PropertyValueRef::Boolean(b) => Self::Boolean(b),
            PropertyValueRef::Decimal(s) => Self::Decimal(s.to_owned()),
            PropertyValueRef::DateTime(s) => Self::DateTime(s.to_owned()),
        }
    }
}

impl fmt::Display for PropertyValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concept(o) => write!(f, "{o}"),
            Self::Code(s) | Self::String(s) | Self::Decimal(s) | Self::DateTime(s) => {
                f.write_str(s)
            }
            Self::Integer(i) => write!(f, "{i}"),
            Self::Boolean(b) => write!(f, "{b}"),
        }
    }
}

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        self.u32(u32::try_from(s.len()).unwrap_or(u32::MAX));
        self.0.extend_from_slice(s.as_bytes());
    }
    fn opt_str(&mut self, s: Option<&str>) {
        match s {
            Some(s) => {
                self.u8(1);
                self.str(s);
            }
            None => self.u8(0),
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], RecordError> {
        let end = self
            .at
            .checked_add(n)
            .ok_or(RecordError::Truncated { at: self.at })?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or(RecordError::Truncated { at: self.at })?;
        self.at = end;
        Ok(slice)
    }
    fn u8(&mut self) -> Result<u8, RecordError> {
        self.take(1)?
            .first()
            .copied()
            .ok_or(RecordError::Truncated { at: self.at })
    }
    fn u32(&mut self) -> Result<u32, RecordError> {
        // The error names the offset; the slice-length error adds nothing to it.
        let Ok(bytes) = <[u8; 4]>::try_from(self.take(4)?) else {
            return Err(RecordError::Truncated { at: self.at });
        };
        Ok(u32::from_le_bytes(bytes))
    }
    fn i64(&mut self) -> Result<i64, RecordError> {
        let Ok(bytes) = <[u8; 8]>::try_from(self.take(8)?) else {
            return Err(RecordError::Truncated { at: self.at });
        };
        Ok(i64::from_le_bytes(bytes))
    }
    fn str(&mut self) -> Result<String, RecordError> {
        self.borrowed_str().map(str::to_owned)
    }
    fn borrowed_str(&mut self) -> Result<&'a str, RecordError> {
        let Ok(len) = usize::try_from(self.u32()?) else {
            return Err(RecordError::Truncated { at: self.at });
        };
        let at = self.at;
        let bytes = self.take(len)?;
        std::str::from_utf8(bytes).map_err(|source| RecordError::Utf8 { at, source })
    }
    fn opt_str(&mut self) -> Result<Option<String>, RecordError> {
        match self.u8()? {
            0 => Ok(None),
            _ => self.str().map(Some),
        }
    }
    fn finish(self) -> Result<(), RecordError> {
        let remaining = self.bytes.len().saturating_sub(self.at);
        if remaining == 0 {
            Ok(())
        } else {
            Err(RecordError::Trailing { remaining })
        }
    }
}

impl Concept {
    /// The record's bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer(Vec::new());
        w.str(&self.code);
        w.u8(u8::from(self.active));
        w.opt_str(self.effective_time.as_deref());
        match self.module {
            Some(m) => {
                w.u8(1);
                w.u32(m.index());
            }
            None => w.u8(0),
        }
        w.0
    }

    /// Decodes a record.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] for truncated, non-UTF-8, or trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader { bytes, at: 0 };
        let code = r.str()?;
        let active = r.u8()? != 0;
        let effective_time = r.opt_str()?;
        let module = match r.u8()? {
            0 => None,
            _ => Some(Ordinal::new(r.u32()?)),
        };
        r.finish()?;
        Ok(Self {
            code,
            active,
            effective_time,
            module,
        })
    }

    /// Decodes the native code of a record and stops there.
    ///
    /// The code is the first field, so a caller that wants only the code
    /// reads one length and one string instead of the whole record.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] when the code is truncated or not UTF-8.
    pub fn decode_code(bytes: &[u8]) -> Result<String, RecordError> {
        code(bytes).map(str::to_owned)
    }
}

/// The display of one concept per language reference set, as the build chose
/// it: the term of the preferred designation of the display use.
///
/// This is not every preferred designation. A display needs one term and the
/// language to match it against, so an entry is a reference set, a language,
/// and a term, and nothing else about the designation is carried. Holding the
/// whole designation for every use doubled a SNOMED artifact (#322).
#[derive(Debug)]
pub struct Displays;

impl Displays {
    /// The bytes of `entries`, each a reference set, a language, and a term.
    ///
    /// Entries are written in the order given, which is the order a reader
    /// scans them in, so the reference set order the build wrote decides which
    /// entry a reader meets first.
    #[must_use]
    pub fn encode(entries: &[(u32, &str, &str)]) -> Vec<u8> {
        let mut w = Writer(Vec::new());
        w.u32(u32::try_from(entries.len()).unwrap_or(u32::MAX));
        for (refset, language, term) in entries {
            w.u32(*refset);
            w.str(language);
            w.str(term);
        }
        w.0
    }

    /// The language and term `refset` chose, if the concept has one.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] when the entries are truncated or not UTF-8.
    pub fn find(bytes: &[u8], refset: u32) -> Result<Option<(String, String)>, RecordError> {
        Ok(Self::find_borrowed(bytes, refset)?
            .map(|(language, term)| (language.to_owned(), term.to_owned())))
    }

    /// [`Self::find`] borrowing the language and term from `bytes`, so a
    /// caller that rejects the language allocates nothing.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] when the entries are truncated or not UTF-8.
    pub fn find_borrowed(bytes: &[u8], refset: u32) -> Result<Option<(&str, &str)>, RecordError> {
        let mut r = Reader { bytes, at: 0 };
        let count = r.u32()?;
        for _ in 0..count {
            let entry_refset = r.u32()?;
            let language = r.borrowed_str()?;
            let term = r.borrowed_str()?;
            if entry_refset == refset {
                return Ok(Some((language, term)));
            }
        }
        Ok(None)
    }
}

/// The designations of one concept, packed in index order.
///
/// Each entry carries its designation index, so a gap in the indices the build
/// wrote cannot shift a later designation into the wrong slot.
#[derive(Debug)]
pub struct Designations;

impl Designations {
    /// The bytes of `entries`, each a designation index and its encoded record.
    #[must_use]
    pub fn encode(entries: &[(u32, &[u8])]) -> Vec<u8> {
        let mut w = Writer(Vec::new());
        w.u32(u32::try_from(entries.len()).unwrap_or(u32::MAX));
        for (index, bytes) in entries {
            w.u32(*index);
            w.u32(u32::try_from(bytes.len()).unwrap_or(u32::MAX));
            w.0.extend_from_slice(bytes);
        }
        w.0
    }

    /// Every designation, in the order the build wrote them.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] when the entries are truncated or a designation
    /// is damaged.
    pub fn decode(bytes: &[u8]) -> Result<Vec<Designation>, RecordError> {
        let mut out = Vec::new();
        Self::walk(bytes, |_, record| {
            out.push(Designation::decode(record)?);
            Ok(true)
        })?;
        Ok(out)
    }

    /// The designation stored under `index`, if the concept has one.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] when the entries are truncated or the
    /// designation is damaged.
    pub fn at(bytes: &[u8], index: u32) -> Result<Option<DesignationRef<'_>>, RecordError> {
        let mut found = None;
        Self::walk(bytes, |at, record| {
            if at != index {
                return Ok(true);
            }
            found = Some(DesignationRef::decode(record)?);
            Ok(false)
        })?;
        Ok(found)
    }

    /// Calls `visit` with each designation until it answers `false`.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] when the entries are truncated or a designation
    /// is damaged.
    pub fn scan<'a>(
        bytes: &'a [u8],
        mut visit: impl FnMut(DesignationRef<'a>) -> bool,
    ) -> Result<(), RecordError> {
        Self::walk(bytes, |_, record| {
            Ok(visit(DesignationRef::decode(record)?))
        })
    }

    /// Calls `visit` with each index and record until it answers `false`.
    fn walk<'a>(
        bytes: &'a [u8],
        mut visit: impl FnMut(u32, &'a [u8]) -> Result<bool, RecordError>,
    ) -> Result<(), RecordError> {
        let mut r = Reader { bytes, at: 0 };
        let count = r.u32()?;
        for _ in 0..count {
            let index = r.u32()?;
            let Ok(len) = usize::try_from(r.u32()?) else {
                return Err(RecordError::Truncated { at: r.at });
            };
            let record = r.take(len)?;
            if !visit(index, record)? {
                return Ok(());
            }
        }
        r.finish()
    }
}

/// The properties of one concept, in property key order.
#[derive(Debug)]
pub struct Properties;

impl Properties {
    /// The bytes of `entries`, each a property key and its encoded value list.
    #[must_use]
    pub fn encode(entries: &[(u32, &[u8])]) -> Vec<u8> {
        let mut w = Writer(Vec::new());
        w.u32(u32::try_from(entries.len()).unwrap_or(u32::MAX));
        for (key, values) in entries {
            w.u32(*key);
            w.u32(u32::try_from(values.len()).unwrap_or(u32::MAX));
            w.0.extend_from_slice(values);
        }
        w.0
    }

    /// Every property key and its values, in the order the build wrote them.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] when the entries are truncated or a value list
    /// is damaged.
    pub fn decode(bytes: &[u8]) -> Result<Vec<(u32, Vec<PropertyValueRef<'_>>)>, RecordError> {
        let mut out = Vec::new();
        Self::walk(bytes, |key, values| {
            out.push((key, PropertyValueRef::decode_list(values)?));
            Ok(())
        })?;
        Ok(out)
    }

    /// The values of property `key`, empty when the concept has none; the
    /// value lists of other keys are skipped, not decoded.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] when the entries are truncated or the value
    /// list of `key` is damaged.
    pub fn find(bytes: &[u8], key: u32) -> Result<Vec<PropertyValueRef<'_>>, RecordError> {
        let mut out = Vec::new();
        Self::walk(bytes, |at, values| {
            if at == key {
                out = PropertyValueRef::decode_list(values)?;
            }
            Ok(())
        })?;
        Ok(out)
    }

    /// Calls `visit` with each key and its encoded value list.
    fn walk<'a>(
        bytes: &'a [u8],
        mut visit: impl FnMut(u32, &'a [u8]) -> Result<(), RecordError>,
    ) -> Result<(), RecordError> {
        let mut r = Reader { bytes, at: 0 };
        let count = r.u32()?;
        for _ in 0..count {
            let key = r.u32()?;
            let Ok(len) = usize::try_from(r.u32()?) else {
                return Err(RecordError::Truncated { at: r.at });
            };
            visit(key, r.take(len)?)?;
        }
        r.finish()
    }
}

/// The acceptability of one concept's designations, per language reference set.
#[derive(Debug)]
pub struct Acceptability;

impl Acceptability {
    /// The bytes of `entries`, each a designation index, a language reference
    /// set ordinal, and an acceptability ordinal.
    #[must_use]
    pub fn encode(entries: &[(u32, u32, u32)]) -> Vec<u8> {
        let mut w = Writer(Vec::new());
        w.u32(u32::try_from(entries.len()).unwrap_or(u32::MAX));
        for (index, refset, acceptability) in entries {
            w.u32(*index);
            w.u32(*refset);
            w.u32(*acceptability);
        }
        w.0
    }

    /// The acceptability of designation `index` in `refset`, if the reference
    /// set carries the designation.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] when the entries are truncated.
    pub fn find(bytes: &[u8], index: u32, refset: u32) -> Result<Option<u32>, RecordError> {
        for (at, in_refset, acceptability) in Self::decode(bytes)? {
            if at == index && in_refset == refset {
                return Ok(Some(acceptability));
            }
        }
        Ok(None)
    }

    /// Every entry, in the order the build wrote them.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] when the entries are truncated.
    pub fn decode(bytes: &[u8]) -> Result<Vec<(u32, u32, u32)>, RecordError> {
        let mut r = Reader { bytes, at: 0 };
        let count = r.u32()?;
        let mut out = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
        for _ in 0..count {
            out.push((r.u32()?, r.u32()?, r.u32()?));
        }
        r.finish()?;
        Ok(out)
    }
}

/// The native code a concept record starts with, borrowed from `bytes`.
///
/// # Errors
///
/// Returns [`RecordError`] when the code is truncated or not UTF-8.
pub fn code(bytes: &[u8]) -> Result<&str, RecordError> {
    let mut reader = Reader { bytes, at: 0 };
    let Ok(len) = usize::try_from(reader.u32()?) else {
        return Err(RecordError::Truncated { at: reader.at });
    };
    let at = reader.at;
    let taken = reader.take(len)?;
    std::str::from_utf8(taken).map_err(|source| RecordError::Utf8 { at, source })
}

impl Designation {
    /// The record's bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer(Vec::new());
        w.opt_str(self.id.as_deref());
        w.str(&self.term);
        w.str(&self.language);
        w.u32(self.use_ordinal);
        w.u8(u8::from(self.active));
        w.0
    }

    /// Decodes a record.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] for truncated, non-UTF-8, or trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, RecordError> {
        DesignationRef::decode(bytes).map(Self::from)
    }
}

impl<'a> DesignationRef<'a> {
    /// Decodes a [`Designation`] record, borrowing its text.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] for truncated, non-UTF-8, or trailing bytes.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, RecordError> {
        let mut r = Reader { bytes, at: 0 };
        let id = match r.u8()? {
            0 => None,
            _ => Some(r.borrowed_str()?),
        };
        let designation = Self {
            id,
            term: r.borrowed_str()?,
            language: r.borrowed_str()?,
            use_ordinal: r.u32()?,
            active: r.u8()? != 0,
        };
        r.finish()?;
        Ok(designation)
    }
}

impl PropertyValue {
    fn encode_into(&self, w: &mut Writer) {
        match self {
            Self::Concept(o) => {
                w.u8(0);
                w.u32(o.index());
            }
            Self::Code(s) => {
                w.u8(1);
                w.str(s);
            }
            Self::String(s) => {
                w.u8(2);
                w.str(s);
            }
            Self::Integer(i) => {
                w.u8(3);
                w.i64(*i);
            }
            Self::Boolean(b) => {
                w.u8(4);
                w.u8(u8::from(*b));
            }
            Self::Decimal(s) => {
                w.u8(5);
                w.str(s);
            }
            Self::DateTime(s) => {
                w.u8(6);
                w.str(s);
            }
        }
    }

    /// Encodes a list of values as one record.
    #[must_use]
    pub fn encode_list(values: &[Self]) -> Vec<u8> {
        let mut w = Writer(Vec::new());
        w.u32(u32::try_from(values.len()).unwrap_or(u32::MAX));
        for value in values {
            value.encode_into(&mut w);
        }
        w.0
    }
}

impl<'a> PropertyValueRef<'a> {
    fn decode_from(r: &mut Reader<'a>) -> Result<Self, RecordError> {
        Ok(match r.u8()? {
            0 => Self::Concept(Ordinal::new(r.u32()?)),
            1 => Self::Code(r.borrowed_str()?),
            2 => Self::String(r.borrowed_str()?),
            3 => Self::Integer(r.i64()?),
            4 => Self::Boolean(r.u8()? != 0),
            5 => Self::Decimal(r.borrowed_str()?),
            6 => Self::DateTime(r.borrowed_str()?),
            tag => return Err(RecordError::Tag { tag }),
        })
    }

    /// Decodes a list of values, borrowing their text.
    ///
    /// # Errors
    ///
    /// Returns [`RecordError`] for truncated bytes or an unknown tag.
    pub fn decode_list(bytes: &'a [u8]) -> Result<Vec<Self>, RecordError> {
        let mut r = Reader { bytes, at: 0 };
        let count = r.u32()?;
        let mut values = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
        for _ in 0..count {
            values.push(Self::decode_from(&mut r)?);
        }
        r.finish()?;
        Ok(values)
    }
}
