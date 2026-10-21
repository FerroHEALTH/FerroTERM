// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Statistical classifications read into one model.
//!
//! `claml` reads Classification Markup Language (ISO 13120: WHO ICD-10, the
//! national ICD-10 translations, ICPC-2); `icd10cm` reads the NCHS ICD-10-CM
//! tabular release (the tabular XML and the order file); `atc` reads the WHO
//! ATC/DDD index as a table or the G-Standaard ATC file. Both produce a
//! [`Classification`]: classes of a declared kind, each with one parent,
//! labelled and annotated by rubric kind. `ferroterm-build` turns the model
//! into the served artifacts.
#![doc(test(attr(deny(warnings))))]

pub mod atc;
pub mod claml;
pub mod icd10cm;
mod xml;

/// The rubric kind of a class's title.
pub const PREFERRED: &str = "preferred";

/// The rubric kinds a build indexes as designations when a classification
/// names none of its own.
pub const DEFAULT_DESIGNATION_KINDS: [&str; 5] = [
    "preferred",
    "preferredLong",
    "inclusion",
    "inclusionTerm",
    "short",
];
/// The `hierarchyMeaning` of a classification tree.
pub const CLASSIFIED_WITH: &str = "classified-with";

/// A classification: the classes of one release, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    /// The classification's name as the source states it (`Title@name`).
    pub name: String,
    /// The human title.
    pub title: String,
    /// The release version, when the source states one.
    pub version: Option<String>,
    /// The BCP 47 language of the labels when a label states none.
    pub language: String,
    /// The class kinds, in the order the source declares them.
    pub kinds: Vec<String>,
    /// The classes.
    pub classes: Vec<Class>,
    /// The rubric kinds that are designations (searchable texts); every other
    /// kind is a property.
    pub designation_kinds: Vec<String>,
    /// The `hierarchyMeaning` of the parent edges, `None` for a flat table.
    pub hierarchy: Option<String>,
}

impl Default for Classification {
    fn default() -> Self {
        Self {
            name: String::new(),
            title: String::new(),
            version: None,
            language: String::new(),
            kinds: Vec::new(),
            classes: Vec::new(),
            designation_kinds: DEFAULT_DESIGNATION_KINDS
                .iter()
                .map(|k| (*k).to_owned())
                .collect(),
            hierarchy: Some(CLASSIFIED_WITH.to_owned()),
        }
    }
}

/// One class: a chapter, block, category, or subcategory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    /// The code, with the period where the code has four or more characters.
    pub code: String,
    /// The class kind (`chapter`, `block`, `category`, `subcategory`).
    pub kind: String,
    /// The parent's code; `None` for a root.
    pub parent: Option<String>,
    /// The usage mark's name (`dagger`, `aster`) when the source marks one.
    pub usage: Option<String>,
    /// Whether the code is valid for use as a code, when the source says.
    pub valid: Option<bool>,
    /// Whether the class is current; an ended class is served inactive.
    pub active: bool,
    /// The rubrics: the title (`preferred`) and the notes, by kind.
    pub rubrics: Vec<Rubric>,
}

impl Default for Class {
    fn default() -> Self {
        Self {
            code: String::new(),
            kind: String::new(),
            parent: None,
            usage: None,
            valid: None,
            active: true,
            rubrics: Vec::new(),
        }
    }
}

/// One rubric: a text of a kind, in a language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rubric {
    /// The rubric kind (`preferred`, `inclusion`, `exclusion`, `note`, ...).
    pub kind: String,
    /// The BCP 47 language.
    pub language: String,
    /// The text, whitespace collapsed.
    pub text: String,
}

impl Class {
    /// The title in `language`, when the class has one.
    #[must_use]
    pub fn title(&self, language: &str) -> Option<&str> {
        self.rubrics
            .iter()
            .find(|r| r.kind == PREFERRED && r.language.eq_ignore_ascii_case(language))
            .map(|r| r.text.as_str())
    }
}

/// `code` with the period an ICD code carries after its third character.
///
/// Codes of three characters or fewer, codes that already carry a period,
/// and codes that are not a letter followed by two digits (blocks, chapters)
/// are returned unchanged.
#[must_use]
pub fn with_period(code: &str) -> String {
    let bytes = code.as_bytes();
    let shaped = bytes.len() > 3
        && code.chars().all(|c| c.is_ascii_alphanumeric())
        && bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && bytes
            .get(1..3)
            .is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    match (shaped, code.split_at_checked(3)) {
        (true, Some((head, tail))) => format!("{head}.{tail}"),
        _ => code.to_owned(),
    }
}

/// `text` with runs of whitespace collapsed to one space and the ends trimmed.
#[must_use]
pub fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{collapse, with_period};

    #[test]
    fn the_period_lands_after_the_third_character() {
        assert_eq!(with_period("A000"), "A00.0");
        assert_eq!(with_period("S02.00"), "S02.00");
        assert_eq!(with_period("A00"), "A00");
        assert_eq!(with_period("A00-A09"), "A00-A09");
        assert_eq!(with_period("II"), "II");
        assert_eq!(with_period("S0200"), "S02.00");
        assert_eq!(collapse("  a \n b  "), "a b");
    }
}
