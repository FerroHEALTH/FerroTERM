// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The G-Standaard (Z-Index) product ladder from the fixed-length files
//! (<https://www.z-index.nl/documentatie/bestandsbeschrijvingen>).
//!
//! `BST711T` holds the generic products (GPK), `BST052T` the prescription
//! products (PRK), `BST031T` the trade products (HPK), and `BST004T` the
//! articles (the ZI number); `BST020T` holds every name by number and
//! `BST902T` the thesauri the coded fields point into. The reader turns
//! them into four flat [`Classification`]s, one per rung, with the rungs above
//! a concept and the coded attributes as properties. No FHIR representation
//! of the G-Standaard is published; the systems are the `urn:oid` URIs the
//! Dutch medication building blocks use.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ::classification::{Class, Classification, PREFERRED, Rubric, collapse};

/// The GPK system (OID 2.16.840.1.113883.2.4.4.1).
pub const GPK_SYSTEM: &str = "urn:oid:2.16.840.1.113883.2.4.4.1";
/// The PRK system (OID 2.16.840.1.113883.2.4.4.10).
pub const PRK_SYSTEM: &str = "urn:oid:2.16.840.1.113883.2.4.4.10";
/// The HPK system (OID 2.16.840.1.113883.2.4.4.7).
pub const HPK_SYSTEM: &str = "urn:oid:2.16.840.1.113883.2.4.4.7";
/// The article (ZI number) system (OID 2.16.840.1.113883.2.4.4.8).
pub const ARTICLE_SYSTEM: &str = "urn:oid:2.16.840.1.113883.2.4.4.8";
/// The rubric kinds that are designations: the full name, the short trade
/// name (`NMNM40`), and the label name (`NMETIK`).
pub const DESIGNATION_KINDS: [&str; 3] = [PREFERRED, "short", "label"];
/// The property naming the GPK a concept falls under.
pub const GPK: &str = "gpk";
/// The property naming the PRK a concept falls under.
pub const PRK: &str = "prk";
/// The property naming the HPK a concept falls under.
pub const HPK: &str = "hpk";
/// The property carrying the ATC code of a GPK.
pub const ATC: &str = "atc";
/// The Dutch language tag the names carry.
const DUTCH: &str = "nl";
/// The mutation code of a removed record.
const REMOVED: &str = "9";

/// The files of the ladder, by their `BSTnnn` prefix and record length.
const FILES: [(&str, usize); 6] = [
    ("BST711", 160),
    ("BST052", 128),
    ("BST031", 480),
    ("BST004", 320),
    ("BST020", 160),
    ("BST902", 128),
];

/// A failure to read the ladder.
#[derive(Debug, thiserror::Error)]
pub enum GStandaardError {
    /// The directory or a file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// A file the ladder needs is not in the directory.
    #[error("no `{file}T` file under {root}")]
    Missing {
        /// The directory.
        root: PathBuf,
        /// The file prefix.
        file: &'static str,
    },
    /// A record is shorter than the published length.
    #[error("{path} line {line} is shorter than {length} characters")]
    Short {
        /// The file.
        path: PathBuf,
        /// The line, 1-based.
        line: usize,
        /// The published record length.
        length: usize,
    },
}

/// The four code systems a G-Standaard release yields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ladder {
    /// The generic products.
    pub gpk: Classification,
    /// The prescription products.
    pub prk: Classification,
    /// The trade products.
    pub hpk: Classification,
    /// The articles (ZI numbers).
    pub article: Classification,
}

impl Ladder {
    /// The rungs with their system URIs and artifact names, top to bottom.
    #[must_use]
    pub fn rungs(&self) -> [(&'static str, &'static str, &Classification); 4] {
        [
            ("gpk", GPK_SYSTEM, &self.gpk),
            ("prk", PRK_SYSTEM, &self.prk),
            ("hpk", HPK_SYSTEM, &self.hpk),
            ("artikel", ARTICLE_SYSTEM, &self.article),
        ]
    }
}

/// One fixed-length record, addressed by the published 1-based positions.
struct Record {
    chars: Vec<char>,
}

impl Record {
    /// The trimmed text at `start` (1-based) for `length` characters.
    fn field(&self, start: usize, length: usize) -> String {
        let from = start.saturating_sub(1);
        self.chars
            .get(from..from.saturating_add(length))
            .unwrap_or_default()
            .iter()
            .collect::<String>()
            .trim()
            .to_owned()
    }

    /// A numeric key at `start`: the digits without their leading zeros,
    /// `None` when blank or zero.
    fn key(&self, start: usize, length: usize) -> Option<String> {
        let text = self.field(start, length);
        let trimmed = text.trim_start_matches('0');
        (!trimmed.is_empty() && trimmed.bytes().all(|b| b.is_ascii_digit()))
            .then(|| trimmed.to_owned())
    }

    fn removed(&self) -> bool {
        self.field(5, 1) == REMOVED
    }
}

fn find(root: &Path, prefix: &'static str) -> Result<Option<PathBuf>, GStandaardError> {
    let io = |source| GStandaardError::Io {
        path: root.to_path_buf(),
        source,
    };
    for entry in std::fs::read_dir(root).map_err(io)? {
        let path = entry.map_err(io)?.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|n| n.to_ascii_uppercase().starts_with(prefix))
        {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// Reads the Latin-1 records of `path`, each at least `length` characters.
fn records(path: &Path, length: usize) -> Result<Vec<Record>, GStandaardError> {
    let bytes = std::fs::read(path).map_err(|source| GStandaardError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let text: String = bytes.iter().map(|&b| char::from(b)).collect();
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        if chars.len() < length {
            return Err(GStandaardError::Short {
                path: path.to_path_buf(),
                line: index.saturating_add(1),
                length,
            });
        }
        out.push(Record { chars });
    }
    Ok(out)
}

/// The names of `BST020T` by name number: the full name, the short name,
/// and the label name.
#[derive(Debug, Default)]
struct Names {
    by_number: BTreeMap<String, (String, String, String)>,
}

impl Names {
    fn designations(&self, number: Option<String>) -> Vec<Rubric> {
        let Some((full, short, label)) = number.and_then(|n| self.by_number.get(&n)) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (kind, text) in [(PREFERRED, full), ("short", short), ("label", label)] {
            if !text.is_empty() && !out.iter().any(|r: &Rubric| r.text == *text) {
                out.push(Rubric {
                    kind: kind.to_owned(),
                    language: DUTCH.to_owned(),
                    text: text.clone(),
                });
            }
        }
        out
    }
}

/// The thesaurus items of `BST902T` by `(thesaurus, item)`.
#[derive(Debug, Default)]
struct Thesauri {
    items: BTreeMap<(String, String), String>,
}

impl Thesauri {
    /// The 50-character name of the item a record points at, or the raw item
    /// code when the thesaurus does not carry it.
    fn resolve(
        &self,
        record: &Record,
        thesaurus_at: usize,
        item_at: usize,
        item_length: usize,
    ) -> Option<String> {
        let thesaurus = record.key(thesaurus_at, 3 + usize::from(item_length == 6))?;
        let item = record.key(item_at, item_length)?;
        Some(
            self.items
                .get(&(thesaurus, item.clone()))
                .cloned()
                .unwrap_or(item),
        )
    }
}

fn property(kind: &str, text: impl Into<String>) -> Rubric {
    Rubric {
        kind: kind.to_owned(),
        language: DUTCH.to_owned(),
        text: text.into(),
    }
}

fn classification(
    name: &str,
    title: &str,
    kind: &str,
    version: &str,
    classes: BTreeMap<String, Class>,
) -> Classification {
    Classification {
        name: name.to_owned(),
        title: title.to_owned(),
        version: Some(version.to_owned()),
        language: DUTCH.to_owned(),
        kinds: vec![kind.to_owned()],
        classes: classes.into_values().collect(),
        designation_kinds: DESIGNATION_KINDS.iter().map(|k| (*k).to_owned()).collect(),
        hierarchy: None,
    }
}

fn class(code: &str, kind: &str, active: bool, rubrics: Vec<Rubric>) -> Class {
    Class {
        code: code.to_owned(),
        kind: kind.to_owned(),
        parent: None,
        usage: None,
        valid: None,
        active,
        rubrics,
    }
}

/// The records of the ladder, by the `BSTnnn` prefix of their file.
type Files = BTreeMap<&'static str, Vec<Record>>;

/// One rung of the ladder: its classes and the rung above each of them.
struct Rung {
    classes: BTreeMap<String, Class>,
    parents: BTreeMap<String, String>,
}

/// Reads the published files of the ladder under `root`.
///
/// # Errors
///
/// Returns [`GStandaardError`] when a file other than `BST902T` is missing,
/// does not read, or has a record shorter than its published length.
fn read_files(root: &Path) -> Result<Files, GStandaardError> {
    let mut files: Files = BTreeMap::new();
    for (prefix, length) in FILES {
        match find(root, prefix)? {
            Some(path) => {
                files.insert(prefix, records(&path, length)?);
            }
            None if prefix == "BST902" => {}
            None => {
                return Err(GStandaardError::Missing {
                    root: root.to_path_buf(),
                    file: prefix,
                });
            }
        }
    }
    Ok(files)
}

/// Takes the records of `prefix`, none when the release lacks the file.
fn take(files: &mut Files, prefix: &str) -> Vec<Record> {
    files.remove(prefix).unwrap_or_default()
}

/// Reads `BST020T`: the full, short, and label names by name number.
fn read_names(records: Vec<Record>) -> Names {
    let mut names = Names::default();
    for record in records {
        if record.removed() {
            continue;
        }
        if let Some(number) = record.key(6, 7) {
            names.by_number.insert(
                number,
                (
                    collapse(&record.field(86, 50)),
                    collapse(&record.field(46, 40)),
                    collapse(&record.field(19, 27)),
                ),
            );
        }
    }
    names
}

/// Reads `BST902T`: the item names of the thesauri the coded fields point
/// into.
fn read_thesauri(records: Vec<Record>) -> Thesauri {
    let mut thesauri = Thesauri::default();
    for record in records {
        if record.removed() {
            continue;
        }
        if let (Some(thesaurus), Some(item)) = (record.key(6, 4), record.key(10, 6)) {
            thesauri
                .items
                .insert((thesaurus, item), collapse(&record.field(62, 50)));
        }
    }
    thesauri
}

/// Reads `BST711T`: the generic products with their substance, strength,
/// form, route, and ATC code.
fn read_gpk(records: Vec<Record>, names: &Names, thesauri: &Thesauri) -> BTreeMap<String, Class> {
    let mut gpk: BTreeMap<String, Class> = BTreeMap::new();
    for record in records {
        if record.removed() {
            continue;
        }
        let Some(code) = record.key(6, 8) else {
            continue;
        };
        let mut rubrics = names.designations(record.key(34, 7));
        if let Some(substance) = record
            .key(41, 7)
            .and_then(|n| names.by_number.get(&n))
            .map(|(full, _, _)| full.clone())
            .filter(|s| !s.is_empty())
        {
            rubrics.push(property("substance", substance));
        }
        let strength = record.field(48, 25);
        if !strength.is_empty() {
            rubrics.push(property("strength", collapse(&strength)));
        }
        if let Some(form) = thesauri.resolve(&record, 22, 25, 3) {
            rubrics.push(property("form", form));
        }
        if let Some(route) = thesauri.resolve(&record, 28, 31, 3) {
            rubrics.push(property("route", route));
        }
        let atc = record.field(119, 8);
        if !atc.is_empty() {
            rubrics.push(property(ATC, atc));
        }
        gpk.insert(code.clone(), class(&code, "gpk", true, rubrics));
    }
    gpk
}

/// Reads `BST052T`: the prescription products with their unit and the GPK
/// each falls under.
fn read_prk(records: Vec<Record>, names: &Names, thesauri: &Thesauri) -> Rung {
    let mut parents: BTreeMap<String, String> = BTreeMap::new();
    let mut classes: BTreeMap<String, Class> = BTreeMap::new();
    for record in records {
        if record.removed() {
            continue;
        }
        let Some(code) = record.key(6, 8) else {
            continue;
        };
        let mut rubrics = names.designations(record.key(14, 7));
        if let Some(parent) = record.key(21, 8) {
            rubrics.push(property(GPK, parent.clone()));
            parents.insert(code.clone(), parent);
        }
        if let Some(unit) = thesauri.resolve(&record, 49, 53, 6) {
            rubrics.push(property("unit", unit));
        }
        classes.insert(code.clone(), class(&code, "prk", true, rubrics));
    }
    Rung { classes, parents }
}

/// Reads `BST031T`: the trade products with their brand and firm and the
/// rungs above them.
fn read_hpk(records: Vec<Record>, names: &Names, prk_gpk: &BTreeMap<String, String>) -> Rung {
    let mut parents: BTreeMap<String, String> = BTreeMap::new();
    let mut classes: BTreeMap<String, Class> = BTreeMap::new();
    for record in records {
        if record.removed() {
            continue;
        }
        let Some(code) = record.key(6, 8) else {
            continue;
        };
        let mut rubrics = names.designations(record.key(30, 7));
        if let Some(parent) = record.key(14, 8) {
            rubrics.push(property(PRK, parent.clone()));
            if let Some(grand) = prk_gpk.get(&parent) {
                rubrics.push(property(GPK, grand.clone()));
            }
            parents.insert(code.clone(), parent);
        }
        for (kind, start) in [("brand", 37), ("firm", 87)] {
            let text = record.field(start, 50);
            if !text.is_empty() {
                rubrics.push(property(kind, collapse(&text)));
            }
        }
        classes.insert(code.clone(), class(&code, "hpk", true, rubrics));
    }
    Rung { classes, parents }
}

/// Reads `BST004T`: the articles with the rungs above them, a removal date
/// serving them inactive.
fn read_articles(
    records: Vec<Record>,
    names: &Names,
    hpk_prk: &BTreeMap<String, String>,
    prk_gpk: &BTreeMap<String, String>,
) -> BTreeMap<String, Class> {
    let mut article: BTreeMap<String, Class> = BTreeMap::new();
    for record in records {
        if record.removed() {
            continue;
        }
        let Some(code) = record.key(6, 8) else {
            continue;
        };
        let mut rubrics = names.designations(record.key(22, 7));
        if let Some(parent) = record.key(14, 8) {
            rubrics.push(property(HPK, parent.clone()));
            if let Some(prk_code) = hpk_prk.get(&parent) {
                rubrics.push(property(PRK, prk_code.clone()));
                if let Some(gpk_code) = prk_gpk.get(prk_code) {
                    rubrics.push(property(GPK, gpk_code.clone()));
                }
            }
        }
        let removed_on = record.key(151, 8);
        if let Some(date) = &removed_on {
            rubrics.push(property("removed", date.clone()));
        }
        article.insert(
            code.clone(),
            class(&code, "artikel", removed_on.is_none(), rubrics),
        );
    }
    article
}

/// Reads the ladder from the `BSTnnnT` files under `root`, recording
/// `version` (the release, `202609`; the files carry none).
///
/// Removed records (mutation code `9`) are skipped; an article with a removal
/// date (`XPUHDD`) is served inactive. Names come from `BST020T`; the coded
/// pharmaceutical form, route, and unit are resolved through `BST902T` when
/// it is present, else served as their item codes.
///
/// # Errors
///
/// Returns [`GStandaardError`] when a file is missing, does not read, or has
/// a record shorter than its published length.
pub fn read(root: &Path, version: &str) -> Result<Ladder, GStandaardError> {
    let mut files = read_files(root)?;
    let names = read_names(take(&mut files, "BST020"));
    let thesauri = read_thesauri(take(&mut files, "BST902"));
    let gpk = read_gpk(take(&mut files, "BST711"), &names, &thesauri);
    let prk = read_prk(take(&mut files, "BST052"), &names, &thesauri);
    let hpk = read_hpk(take(&mut files, "BST031"), &names, &prk.parents);
    let article = read_articles(
        take(&mut files, "BST004"),
        &names,
        &hpk.parents,
        &prk.parents,
    );
    Ok(Ladder {
        gpk: classification(
            "GPK",
            "G-Standaard generieke producten",
            "gpk",
            version,
            gpk,
        ),
        prk: classification(
            "PRK",
            "G-Standaard voorschrijfproducten",
            "prk",
            version,
            prk.classes,
        ),
        hpk: classification(
            "HPK",
            "G-Standaard handelsproducten",
            "hpk",
            version,
            hpk.classes,
        ),
        article: classification("ZI", "G-Standaard artikelen", "artikel", version, article),
    })
}

#[cfg(test)]
mod tests {
    use super::Record;

    #[test]
    fn keys_drop_leading_zeros_and_blank_or_zero_is_absent() {
        let record = Record {
            chars: "07111000123450000000  ".chars().collect(),
        };
        assert_eq!(record.field(1, 4), "0711");
        assert_eq!(record.key(6, 8).as_deref(), Some("12345"));
        assert_eq!(record.key(14, 8), None, "all zeros");
        assert_eq!(record.key(21, 2), None, "blank");
        assert!(!record.removed());
    }
}
