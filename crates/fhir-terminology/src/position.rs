// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Positions inside a refused value: a parser's byte offset, carried back
//! through percent-encoding and counted as the column a client reads.
//!
//! The `operationoutcome-issue-col` extension states a column and defines
//! neither its base nor its unit
//! (<https://hl7.org/fhir/extensions/StructureDefinition-operationoutcome-issue-col.html>),
//! so the choice here is our own design: a column is 1-based and counts
//! Unicode scalar values.

/// The 1-based column of the character at byte `offset` of `text`, counted in
/// Unicode scalar values; `None` when `offset` is past the end of `text`.
///
/// An offset inside a multi-byte character counts that character, and the end
/// of `text` is the column after its last character.
///
/// # Examples
///
/// ```
/// use fhir_terminology::position::column;
///
/// assert_eq!(column("<< 1", 0), Some(1));
/// assert_eq!(column("é OR", 3), Some(3));
/// assert_eq!(column("é", 2), Some(2));
/// assert_eq!(column("é", 3), None);
/// ```
#[must_use]
pub fn column(text: &str, offset: usize) -> Option<usize> {
    if offset > text.len() {
        return None;
    }
    let before = text
        .char_indices()
        .take_while(|(index, _)| *index < offset)
        .count();
    let inside = !text.is_char_boundary(offset);
    // An offset inside a character counts that character once, not twice.
    before.checked_sub(usize::from(inside))?.checked_add(1)
}

/// The byte offset into `encoded` of the byte at `offset` in its
/// percent-decoding (<https://www.rfc-editor.org/rfc/rfc3986#section-2.1>).
///
/// An escape (`%XX`) is three bytes of `encoded` for one decoded byte; every
/// other byte stands for itself, `+` included, so the same walk serves a query
/// value decoded as `application/x-www-form-urlencoded`. A decoded offset
/// inside an escape's byte maps to the escape's start; the end of the decoding
/// maps to the end of `encoded`. `None` when `offset` is past the end of the
/// decoding.
///
/// # Examples
///
/// ```
/// use fhir_terminology::position::encoded_offset;
///
/// assert_eq!(encoded_offset("%3C%3C%201", 2), Some(6));
/// assert_eq!(encoded_offset("a%20b", 3), Some(5));
/// assert_eq!(encoded_offset("ab", 3), None);
/// ```
#[must_use]
pub fn encoded_offset(encoded: &str, offset: usize) -> Option<usize> {
    let bytes = encoded.as_bytes();
    let mut index = 0_usize;
    let mut decoded = 0_usize;
    while decoded < offset {
        let step = match bytes.get(index) {
            None => return None,
            Some(b'%') if is_escape(bytes, index) => 3,
            Some(_) => 1,
        };
        index = index.checked_add(step)?;
        decoded = decoded.checked_add(1)?;
    }
    Some(index)
}

/// Whether `bytes` holds a `%XX` escape at `index`.
fn is_escape(bytes: &[u8], index: usize) -> bool {
    let digit = |at: usize| {
        index
            .checked_add(at)
            .and_then(|at| bytes.get(at))
            .is_some_and(u8::is_ascii_hexdigit)
    };
    digit(1) && digit(2)
}

#[cfg(test)]
mod tests {
    use super::{column, encoded_offset};

    #[test]
    fn a_column_counts_characters_from_one() {
        assert_eq!(column("abc", 0), Some(1), "the first character is column 1");
        assert_eq!(column("abc", 2), Some(3), "one column per character");
        assert_eq!(
            column("abc", 3),
            Some(4),
            "the end is the column after the last character"
        );
        assert_eq!(
            column("abc", 4),
            None,
            "an offset past the end has no column"
        );
    }

    #[test]
    fn a_multi_byte_character_is_one_column() {
        // `—` is three bytes in UTF-8 and `é` two; each is one Unicode scalar value.
        let text = "é—x";
        assert_eq!(
            column(text, 5),
            Some(3),
            "x follows two characters of five bytes"
        );
        assert_eq!(column(text, 3), Some(2), "an offset inside `—` counts `—`");
        assert_eq!(column(text, 1), Some(1), "an offset inside `é` counts `é`");
        assert_eq!(column(text, 6), Some(4), "the end follows three characters");
    }

    #[test]
    fn an_escape_is_three_encoded_bytes_for_one_decoded_byte() {
        assert_eq!(
            encoded_offset("%3C%3Cx", 2),
            Some(6),
            "two escapes precede `x`"
        );
        assert_eq!(
            encoded_offset("%E2%80%94x", 3),
            Some(9),
            "a UTF-8 character is three escapes"
        );
        assert_eq!(
            encoded_offset("%E2%80%94x", 1),
            Some(3),
            "an offset inside a character maps to its escape"
        );
        assert_eq!(
            encoded_offset("a+b", 2),
            Some(2),
            "`+` is one byte either way"
        );
        assert_eq!(
            encoded_offset("a%2", 2),
            Some(2),
            "a stray `%` stands for itself"
        );
        assert_eq!(encoded_offset("ab", 2), Some(2), "the end maps to the end");
        assert_eq!(encoded_offset("ab", 3), None, "past the end maps nowhere");
    }
}
