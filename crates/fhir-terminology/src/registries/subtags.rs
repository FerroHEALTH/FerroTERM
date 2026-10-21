// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The IANA Language Subtag Registry (RFC 5646 §3.1), parsed from the
//! vendored record-jar file.

use std::collections::BTreeMap;
use std::sync::LazyLock;

/// The vendored registry text.
const REGISTRY: &str = include_str!("../../data/iana/language-subtag-registry");

/// One registry record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// `Type`: `language`, `extlang`, `script`, `region`, `variant`,
    /// `grandfathered`, or `redundant`.
    pub kind: Kind,
    /// `Subtag` (or `Tag` for grandfathered and redundant records), as
    /// registered.
    pub subtag: String,
    /// `Description` lines.
    pub descriptions: Vec<String>,
    /// `Deprecated`, when the record is deprecated.
    pub deprecated: Option<String>,
    /// `Preferred-Value`.
    pub preferred: Option<String>,
    /// `Prefix` lines (extlang and variant records).
    pub prefixes: Vec<String>,
    /// `Suppress-Script`.
    pub suppress_script: Option<String>,
    /// `Scope` (`macrolanguage`, `collection`, `special`, `private-use`).
    pub scope: Option<String>,
}

/// A record type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// A primary language subtag.
    Language,
    /// An extended language subtag.
    Extlang,
    /// A script subtag.
    Script,
    /// A region subtag.
    Region,
    /// A variant subtag.
    Variant,
    /// A whole grandfathered tag.
    Grandfathered,
    /// A whole redundant tag.
    Redundant,
}

impl Kind {
    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "language" => Self::Language,
            "extlang" => Self::Extlang,
            "script" => Self::Script,
            "region" => Self::Region,
            "variant" => Self::Variant,
            "grandfathered" => Self::Grandfathered,
            "redundant" => Self::Redundant,
            _ => return None,
        })
    }
}

/// The parsed registry.
#[derive(Debug)]
pub struct Registry {
    /// `File-Date`.
    pub file_date: String,
    records: BTreeMap<(Kind, String), Record>,
}

impl Registry {
    /// The record of `subtag` (compared without case) of `kind`.
    #[must_use]
    pub fn get(&self, kind: Kind, subtag: &str) -> Option<&Record> {
        self.records.get(&(kind, subtag.to_ascii_lowercase()))
    }

    /// Every record, by kind then lowercased subtag.
    pub fn records(&self) -> impl Iterator<Item = &Record> {
        self.records.values()
    }

    /// The records of `kind`, letter subtags before digit ones, each ascending.
    ///
    /// The two spellings of a region are `2ALPHA` for the ISO 3166-1 code and
    /// `3DIGIT` for the UN M.49 code, in that order (RFC 5646 §2.1); the other
    /// kinds hold letters only, so the split leaves them alone.
    #[must_use]
    pub fn of_kind(&self, kind: Kind) -> Vec<&Record> {
        let mut out: Vec<&Record> = self
            .records
            .iter()
            .filter(|((k, _), _)| *k == kind)
            .map(|(_, record)| record)
            .collect();
        out.sort_by_key(|record| {
            record
                .subtag
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_digit)
        });
        out
    }
}

/// The subtags one `Subtag` field names.
///
/// "The sequence '..' (U+002E U+002E) in a field-body denotes a range of
/// values. Such a range represents all subtags of the same length that are in
/// alphabetic or numeric order within that range, including the values
/// explicitly mentioned" (RFC 5646 §3.1.1).
fn subtags_of(field: &str) -> Vec<String> {
    let Some((start, end)) = field.split_once("..") else {
        return vec![field.to_owned()];
    };
    let registered_case = start.as_bytes().to_vec();
    let (lower, upper) = (start.to_ascii_lowercase(), end.to_ascii_lowercase());
    let alphabet = if lower.bytes().all(|b| b.is_ascii_lowercase()) {
        (b'a', b'z')
    } else if lower.bytes().all(|b| b.is_ascii_digit()) {
        (b'0', b'9')
    } else {
        return Vec::new();
    };
    if lower.len() != upper.len() || lower.is_empty() || lower > upper {
        return Vec::new();
    }
    let mut current = lower.into_bytes();
    let end = upper.into_bytes();
    let mut out = Vec::new();
    loop {
        let cased: Vec<u8> = current
            .iter()
            .zip(&registered_case)
            .map(|(byte, pattern)| {
                if pattern.is_ascii_uppercase() {
                    byte.to_ascii_uppercase()
                } else {
                    *byte
                }
            })
            .collect();
        match String::from_utf8(cased) {
            Ok(subtag) => out.push(subtag),
            Err(_) => return out,
        }
        if current == end {
            return out;
        }
        let mut at = current.len();
        loop {
            let Some(position) = at.checked_sub(1) else {
                return out;
            };
            at = position;
            let Some(byte) = current.get_mut(position) else {
                return out;
            };
            if *byte < alphabet.1 {
                *byte += 1;
                break;
            }
            *byte = alphabet.0;
        }
    }
}

/// The vendored registry, parsed once.
pub static REGISTRY_DATA: LazyLock<Registry> = LazyLock::new(|| parse(REGISTRY));

/// Parses a record-jar registry text.
#[must_use]
pub fn parse(text: &str) -> Registry {
    let mut file_date = String::new();
    let mut records = BTreeMap::new();
    for block in text.split("%%\n") {
        let fields = fields(block);
        if let Some(date) = fields.iter().find(|(k, _)| k == "File-Date") {
            file_date.clone_from(&date.1);
        }
        let Some(kind) = fields
            .iter()
            .find(|(k, _)| k == "Type")
            .and_then(|(_, v)| Kind::parse(v))
        else {
            continue;
        };
        let Some(subtag) = fields
            .iter()
            .find(|(k, _)| k == "Subtag" || k == "Tag")
            .map(|(_, v)| v.clone())
        else {
            continue;
        };
        let first = |name: &str| {
            fields
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        };
        let all = |name: &str| -> Vec<String> {
            fields
                .iter()
                .filter(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
                .collect()
        };
        for subtag in subtags_of(&subtag) {
            let record = Record {
                kind,
                subtag: subtag.clone(),
                descriptions: all("Description"),
                deprecated: first("Deprecated"),
                preferred: first("Preferred-Value"),
                prefixes: all("Prefix"),
                suppress_script: first("Suppress-Script"),
                scope: first("Scope"),
            };
            records.insert((kind, subtag.to_ascii_lowercase()), record);
        }
    }
    Registry { file_date, records }
}

/// The `Name: value` fields of one record, continuation lines (leading
/// whitespace) folded into the previous value.
fn fields(block: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in block.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some((_, value)) = out.last_mut() {
                value.push(' ');
                value.push_str(line.trim());
            }
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            out.push((name.trim().to_owned(), value.trim().to_owned()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Kind, REGISTRY_DATA, parse};

    #[test]
    fn the_vendored_registry_parses_with_every_kind() {
        let registry = &*REGISTRY_DATA;
        assert!(!registry.file_date.is_empty());
        assert_eq!(
            registry
                .get(Kind::Language, "EN")
                .map(|r| r.descriptions[0].as_str()),
            Some("English")
        );
        assert_eq!(
            registry
                .get(Kind::Region, "gb")
                .map(|r| r.descriptions[0].as_str()),
            Some("United Kingdom")
        );
        assert!(registry.get(Kind::Script, "Latn").is_some());
        assert!(registry.get(Kind::Variant, "1606nict").is_some());
        assert!(registry.get(Kind::Grandfathered, "i-klingon").is_some());
        assert!(registry.get(Kind::Language, "zz").is_none());
    }

    /// "The sequence '..' … denotes a range of values. Such a range represents
    /// all subtags of the same length that are in alphabetic or numeric order
    /// within that range, including the values explicitly mentioned"
    /// (RFC 5646 §3.1.1).
    #[test]
    fn a_range_field_registers_every_subtag_in_it() {
        let registry = &*REGISTRY_DATA;
        for subtag in ["qaa", "qbb", "qtz"] {
            let record = registry.get(Kind::Language, subtag).expect("in qaa..qtz");
            assert_eq!(record.descriptions, ["Private use"]);
            assert_eq!(record.subtag, subtag);
        }
        assert!(registry.get(Kind::Language, "qua").is_some());
        for subtag in ["QM", "QZ", "XA", "XZ"] {
            assert_eq!(
                registry
                    .get(Kind::Region, subtag)
                    .map(|r| r.descriptions.clone()),
                Some(vec![String::from("Private use")]),
                "{subtag}"
            );
        }
        assert!(registry.get(Kind::Region, "QL").is_none());
        let script = registry.get(Kind::Script, "Qabx").expect("in Qaaa..Qabx");
        assert_eq!(script.subtag, "Qabx", "a range keeps the registered case");
        assert!(registry.records().all(|r| !r.subtag.contains("..")));
    }

    /// A region is `2ALPHA` for the ISO 3166-1 code or `3DIGIT` for the UN
    /// M.49 code, in that order (RFC 5646 §2.1).
    #[test]
    fn regions_read_letters_before_digits() {
        let regions = REGISTRY_DATA.of_kind(Kind::Region);
        let digits = regions
            .iter()
            .position(|r| r.subtag.starts_with(|c: char| c.is_ascii_digit()))
            .expect("the UN M.49 codes");
        assert_eq!(
            regions.first().map(|r| r.subtag.as_str()),
            Some("AA"),
            "the letter codes come first"
        );
        assert!(
            regions.get(digits..).is_some_and(|rest| rest
                .iter()
                .all(|r| r.subtag.starts_with(|c: char| c.is_ascii_digit()))),
            "the digit codes are one run at the end"
        );
    }

    #[test]
    fn continuation_lines_fold_into_the_value() {
        let registry = parse(
            "File-Date: 2026-01-01\n%%\nType: variant\nSubtag: abcde\nDescription: A long\n  description\nPrefix: en\nAdded: 2020-01-01\n",
        );
        let record = registry.get(Kind::Variant, "abcde").expect("record");
        assert_eq!(record.descriptions, ["A long description"]);
        assert_eq!(record.prefixes, ["en"]);
        assert_eq!(registry.file_date, "2026-01-01");
    }
}
