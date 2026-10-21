// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The NCHS ICD-10-CM release: the tabular XML and the order file.
//!
//! The tabular list (`icd10cm_tabular_<year>.xml`) carries the chapters,
//! sections, and the nested `diag` entries with their notes (`includes`,
//! `inclusionTerm`, `excludes1`, `excludes2`, `codeFirst`,
//! `useAdditionalCode`, `codeAlso`, `sevenChrNote`, `sevenChrDef`); the order
//! file (`icd10cm_order_<year>.txt`) carries every code in tabular order with
//! its header flag and short and long descriptions, in the fixed columns the
//! CMS order-file description gives (order 1..5, code 7..13, flag 15, short
//! description 17..76, long description from 78). Codes the order file adds
//! beyond the tabular list (the seventh-character codes) hang under the
//! longest tabular code that prefixes them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::Event;

use crate::xml::{attribute, reference};
use crate::{Class, Classification, PREFERRED, Rubric, collapse, with_period};

/// The rubric kind of the order file's short description.
pub const SHORT: &str = "short";
/// The name of the tabular XML, without the year.
pub const TABULAR_PREFIX: &str = "icd10cm_tabular_";
/// The name of the order file, without the year.
pub const ORDER_PREFIX: &str = "icd10cm_order_";
/// The class kinds, root first.
pub const KINDS: [&str; 4] = ["chapter", "block", "category", "subcategory"];

/// The note containers of a `diag`, `section`, or `chapter`.
const NOTE_KINDS: [&str; 9] = [
    "includes",
    "inclusionTerm",
    "excludes1",
    "excludes2",
    "codeFirst",
    "useAdditionalCode",
    "codeAlso",
    "sevenChrNote",
    "notes",
];

/// A failure to read the release.
#[derive(Debug, thiserror::Error)]
pub enum Icd10cmError {
    /// A directory or file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// A file the reader needs is under none of the roots.
    #[error("no `{name}<year>` file under the given paths")]
    Missing {
        /// The file name prefix.
        name: &'static str,
    },
    /// The tabular XML is not well-formed.
    #[error("the tabular list is not well-formed XML")]
    Xml(#[from] quick_xml::Error),
    /// The tabular XML names no version.
    #[error("the tabular list names no version")]
    NoVersion,
    /// An order-file line is shorter than its fixed columns.
    #[error("{path}:{line}: the line is shorter than the order-file columns")]
    Short {
        /// The order file.
        path: PathBuf,
        /// The 1-based line.
        line: usize,
    },
}

/// The two files of a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Files {
    /// The tabular XML.
    pub tabular: PathBuf,
    /// The order file.
    pub order: PathBuf,
}

/// Finds the tabular XML and the order file under `roots`, at any depth.
///
/// # Errors
///
/// Returns [`Icd10cmError`] when a directory does not read or a file is
/// missing.
pub fn locate(roots: &[PathBuf]) -> Result<Files, Icd10cmError> {
    let mut tabular = None;
    let mut order = None;
    for root in roots {
        walk(root, &mut |path| {
            let Some(name) = path.file_name().and_then(std::ffi::OsStr::to_str) else {
                return;
            };
            let lower = name.to_ascii_lowercase();
            if lower.starts_with(TABULAR_PREFIX)
                && Path::new(&lower)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
                && tabular.is_none()
            {
                tabular = Some(path.to_path_buf());
            }
            if lower.starts_with(ORDER_PREFIX)
                && Path::new(&lower)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("txt"))
                && order.is_none()
            {
                order = Some(path.to_path_buf());
            }
        })?;
    }
    Ok(Files {
        tabular: tabular.ok_or(Icd10cmError::Missing {
            name: TABULAR_PREFIX,
        })?,
        order: order.ok_or(Icd10cmError::Missing { name: ORDER_PREFIX })?,
    })
}

fn walk(root: &Path, visit: &mut dyn FnMut(&Path)) -> Result<(), Icd10cmError> {
    let io = |source| Icd10cmError::Io {
        path: root.to_path_buf(),
        source,
    };
    if root.is_file() {
        visit(root);
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(root)
        .map_err(io)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()
        .map_err(io)?;
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            walk(&entry, visit)?;
        } else {
            visit(&entry);
        }
    }
    Ok(())
}

/// One node of the tabular list while it is open.
#[derive(Debug, Default)]
struct Open {
    element: String,
    name: String,
    desc: String,
    rubrics: Vec<Rubric>,
    /// The `sectionRef` ranges of a chapter, for its code.
    children: Vec<String>,
}

#[derive(Debug, Default)]
struct Tabular {
    version: Option<String>,
    classes: Vec<Class>,
    /// The open chapter, section, and diag nodes, outermost first.
    stack: Vec<Open>,
    /// The open note container and whether a `note` inside it is open.
    note: Option<(String, bool)>,
    /// The open `extension` character of a `sevenChrDef`.
    extension: Option<String>,
    /// The element whose text is being read: `version`, `name`, `desc`.
    text_into: Option<String>,
    buffer: String,
}

/// The class kind of a diag code by its length without the period.
fn diag_kind(code: &str) -> &'static str {
    if code.replace('.', "").len() <= 3 {
        "category"
    } else {
        "subcategory"
    }
}

impl Tabular {
    fn start(&mut self, start: &quick_xml::events::BytesStart<'_>) {
        let qname = start.name();
        let name = qname.as_ref();
        match name {
            "chapter" | "section" | "diag" => {
                let mut open = Open {
                    element: name.to_owned(),
                    ..Open::default()
                };
                if name == "section" {
                    open.name = attribute(start, "id").unwrap_or_default();
                }
                self.stack.push(open);
            }
            "version" | "name" | "desc" if self.note.is_none() => {
                self.text_into = Some(name.to_owned());
                self.buffer.clear();
            }
            "sectionRef" => {
                if let (Some(open), Some(id)) = (self.stack.last_mut(), attribute(start, "id")) {
                    open.children.push(id);
                }
            }
            kind if NOTE_KINDS.contains(&kind) || kind == "sevenChrDef" => {
                self.note = Some((kind.to_owned(), false));
            }
            "note" => {
                if let Some((_, reading)) = &mut self.note {
                    *reading = true;
                    self.buffer.clear();
                }
            }
            "extension" => {
                if let Some((_, reading)) = &mut self.note {
                    *reading = true;
                    self.buffer.clear();
                    self.extension = attribute(start, "char");
                }
            }
            _ => {}
        }
    }

    fn text(&mut self, text: &str) {
        if self.text_into.is_some() || self.note.as_ref().is_some_and(|(_, reading)| *reading) {
            self.buffer.push_str(text);
        }
    }

    /// A closed `name` or `desc`: the buffered text names the open node, or describes it.
    fn close_name_or_desc(&mut self, element: &str) {
        let text = collapse(&self.buffer);
        self.text_into = None;
        if let Some(open) = self.stack.last_mut() {
            if element == "name" {
                open.name = text;
            } else {
                open.desc = text;
            }
        }
    }

    /// A closed `note` or `extension`: its text becomes a rubric of the open node,
    /// prefixed by the seventh character when the element names one.
    fn close_note(&mut self, language: &str) {
        if let (Some((kind, reading)), Some(open)) = (&mut self.note, self.stack.last_mut())
            && *reading
        {
            let text = collapse(&self.buffer);
            let text = match self.extension.take() {
                Some(c) => format!("{c}: {text}"),
                None => text,
            };
            open.rubrics.push(Rubric {
                kind: kind.clone(),
                language: language.to_owned(),
                text,
            });
            *reading = false;
        }
    }

    fn end(&mut self, name: &str, language: &str) {
        match name {
            "version" => {
                self.version = Some(collapse(&self.buffer));
                self.text_into = None;
            }
            "name" | "desc" if self.text_into.as_deref() == Some(name) => {
                self.close_name_or_desc(name);
            }
            "note" | "extension" => self.close_note(language),
            kind if NOTE_KINDS.contains(&kind) || kind == "sevenChrDef" => self.note = None,
            "chapter" | "section" | "diag" => {
                if let Some(open) = self.stack.pop() {
                    self.close(open, language);
                }
            }
            _ => {}
        }
    }

    /// Turns a closed node into a class under the nearest open ancestor that
    /// is a class of its own.
    fn close(&mut self, open: Open, language: &str) {
        let code = match open.element.as_str() {
            // NOTE: a chapter has no code of its own; its range of blocks (from the
            // description's parenthesis, else the first and last section) names it.
            "chapter" => chapter_code(&open),
            "section" if !open.name.contains('-') => {
                // A one-category section shares its code with the category;
                // the category hangs under the chapter instead.
                let Some(parent) = self.stack.last_mut() else {
                    return;
                };
                parent.rubrics.extend(open.rubrics);
                return;
            }
            _ => with_period(&open.name),
        };
        let kind = match open.element.as_str() {
            "chapter" => "chapter",
            "section" => "block",
            _ => diag_kind(&code),
        };
        let parent = self
            .stack
            .iter()
            .rev()
            .find_map(|o| match o.element.as_str() {
                "chapter" => Some(chapter_code(o)),
                "section" if !o.name.contains('-') => None,
                _ => Some(with_period(&o.name)),
            });
        let mut rubrics = vec![Rubric {
            kind: PREFERRED.to_owned(),
            language: language.to_owned(),
            text: open.desc,
        }];
        rubrics.extend(open.rubrics);
        self.classes.push(Class {
            code,
            kind: kind.to_owned(),
            parent,
            usage: None,
            valid: None,
            active: true,
            rubrics,
        });
    }
}

fn chapter_code(open: &Open) -> String {
    if let Some(range) = open
        .desc
        .rsplit_once('(')
        .and_then(|(_, tail)| tail.strip_suffix(')'))
        .filter(|r| r.contains('-'))
    {
        return range.to_owned();
    }
    match (open.children.first(), open.children.last()) {
        (Some(first), Some(last)) => {
            let start = first.split('-').next().unwrap_or(first);
            let end = last.rsplit('-').next().unwrap_or(last);
            format!("{start}-{end}")
        }
        _ => open.name.clone(),
    }
}

/// Reads the tabular XML `text`.
///
/// # Errors
///
/// Returns [`Icd10cmError`] when the document is not well-formed or names
/// no version.
pub fn read_tabular(text: &str, language: &str) -> Result<Classification, Icd10cmError> {
    let mut reader = Reader::from_str(text);
    let mut tabular = Tabular::default();
    loop {
        match reader.read_event()? {
            Event::Start(start) => tabular.start(&start),
            Event::Empty(start) => {
                let name = start.name().as_ref().to_owned();
                tabular.start(&start);
                tabular.end(&name, language);
            }
            Event::Text(t) => tabular.text(&t.xml_content(XmlVersion::Explicit1_0)),
            Event::CData(t) => tabular.text(&t.xml_content(XmlVersion::Explicit1_0)),
            Event::GeneralRef(r) => tabular.text(&reference(&r)),
            Event::End(end) => tabular.end(end.name().as_ref(), language),
            Event::Eof => break,
            _ => {}
        }
    }
    let version = tabular.version.ok_or(Icd10cmError::NoVersion)?;
    Ok(Classification {
        name: String::from("ICD-10-CM"),
        title: String::from(
            "International Classification of Diseases, Tenth Revision, Clinical Modification",
        ),
        version: Some(version),
        language: language.to_owned(),
        kinds: KINDS.iter().map(|k| (*k).to_owned()).collect(),
        classes: tabular.classes,
        ..Classification::default()
    })
}

/// One line of the order file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderLine {
    /// The code, with its period.
    pub code: String,
    /// Whether the code is valid for HIPAA-covered transactions (`1`).
    pub valid: bool,
    /// The short description.
    pub short: String,
    /// The long description.
    pub long: String,
}

/// Parses the order file `text`.
///
/// # Errors
///
/// Returns [`Icd10cmError::Short`] for a line shorter than the fixed columns
/// (`path` names the file in the message).
pub fn read_order(text: &str, path: &Path) -> Result<Vec<OrderLine>, Icd10cmError> {
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let field = |from: usize, to: usize| line.get(from..to).map(str::trim);
        let (Some(code), Some(flag)) = (field(6, 13), field(14, 15)) else {
            return Err(Icd10cmError::Short {
                path: path.to_path_buf(),
                line: index.saturating_add(1),
            });
        };
        let short = field(16, 76).unwrap_or_default();
        let long = line.get(77..).map(str::trim).unwrap_or_default();
        out.push(OrderLine {
            code: with_period(code),
            valid: flag == "1",
            short: short.to_owned(),
            long: long.to_owned(),
        });
    }
    Ok(out)
}

/// The parent of an order-file code absent from the tabular list: the
/// longest known code that prefixes it, the placeholder `X`s dropped.
fn parent_of(code: &str, known: &BTreeSet<String>) -> Option<String> {
    let plain = code.replace('.', "");
    let mut length = plain.len();
    while length > 3 {
        length -= 1;
        let Some(prefix) = plain.get(..length) else {
            continue;
        };
        let prefix = prefix.trim_end_matches('X');
        let candidate = with_period(prefix);
        if known.contains(&candidate) {
            return Some(candidate);
        }
    }
    known
        .contains(&plain.chars().take(3).collect::<String>())
        .then(|| plain.chars().take(3).collect())
}

/// Merges the order file into the tabular classification: the header flag
/// and the short description onto known codes, new classes for the rest.
pub fn merge(classification: &mut Classification, order: &[OrderLine]) {
    let mut known: BTreeSet<String> = classification
        .classes
        .iter()
        .map(|c| c.code.clone())
        .collect();
    let mut index: BTreeMap<String, usize> = classification
        .classes
        .iter()
        .enumerate()
        .map(|(i, c)| (c.code.clone(), i))
        .collect();
    let language = classification.language.clone();
    for line in order {
        if let Some(&i) = index.get(&line.code) {
            if let Some(class) = classification.classes.get_mut(i) {
                class.valid = Some(line.valid);
                if !line.short.is_empty() {
                    class.rubrics.push(Rubric {
                        kind: SHORT.to_owned(),
                        language: language.clone(),
                        text: line.short.clone(),
                    });
                }
            }
            continue;
        }
        let parent = parent_of(&line.code, &known);
        let mut rubrics = vec![Rubric {
            kind: PREFERRED.to_owned(),
            language: language.clone(),
            text: line.long.clone(),
        }];
        if !line.short.is_empty() {
            rubrics.push(Rubric {
                kind: SHORT.to_owned(),
                language: language.clone(),
                text: line.short.clone(),
            });
        }
        index.insert(line.code.clone(), classification.classes.len());
        known.insert(line.code.clone());
        classification.classes.push(Class {
            code: line.code.clone(),
            kind: diag_kind(&line.code).to_owned(),
            parent,
            usage: None,
            valid: Some(line.valid),
            active: true,
            rubrics,
        });
    }
}

/// Reads the release from its two files.
///
/// # Errors
///
/// Returns [`Icd10cmError`] when a file does not read or parse.
pub fn read(files: &Files) -> Result<Classification, Icd10cmError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| Icd10cmError::Io { path, source }
    };
    let tabular = std::fs::read_to_string(&files.tabular).map_err(io(&files.tabular))?;
    let mut classification = read_tabular(&tabular, "en")?;
    let order_text = std::fs::read_to_string(&files.order).map_err(io(&files.order))?;
    let order = read_order(&order_text, &files.order)?;
    merge(&mut classification, &order);
    Ok(classification)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{parent_of, read_order};

    #[test]
    fn the_order_columns_and_the_seventh_character_parent() {
        let text = "00001 A00     0 Cholera                                                      Cholera\n\
                    00002 S0200XA 1 Fx of vault of skull, init                                   Fracture of vault of skull, initial encounter for closed fracture\n";
        let lines = read_order(text, std::path::Path::new("order.txt")).expect("parses");
        assert_eq!(lines[0].code, "A00");
        assert!(!lines[0].valid);
        assert_eq!(lines[1].code, "S02.00XA");
        assert!(lines[1].valid);
        assert_eq!(lines[1].short, "Fx of vault of skull, init");
        assert!(lines[1].long.starts_with("Fracture of vault"));
        let known: BTreeSet<String> = ["S02", "S02.0", "S02.00"].map(String::from).into();
        assert_eq!(parent_of("S02.00XA", &known).as_deref(), Some("S02.00"));
        assert_eq!(parent_of("S02.0XXA", &known).as_deref(), Some("S02.0"));
        assert_eq!(parent_of("S02.9", &known).as_deref(), Some("S02"));
        assert!(read_order("0001 A00", std::path::Path::new("x")).is_err());
    }
}
