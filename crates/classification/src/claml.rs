// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Classification Markup Language (`ClaML`, ISO 13120).
//!
//! The document structure is the `ClaML` DTD: `Title` (name, version), `Meta`
//! (`lang`), `ClassKinds`, `Modifier` and `ModifierClass`, and `Class` with
//! `SuperClass`, `ModifiedBy`, `ExcludeModifier`, and `Rubric`/`Label`. A
//! `Reference` inside a label (`class="in brackets"`) becomes the referenced
//! code in parentheses; the other label children (`Fragment`, `Para`, `Term`)
//! contribute their text. No FHIR specification governs how a modifier
//! expands; a modified code is the class code followed by the modifier
//! class code, with the period of the FHIR ICD page, its title the class
//! title followed by the modifier title after a comma: our own design.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use crate::xml::{attribute, reference};
use crate::{Class, Classification, PREFERRED, Rubric, collapse, with_period};

/// The rubric kind the expansion adds to a modified class, valued the modifier class code.
pub const MODIFIER: &str = "modifier";

/// A failure to read a `ClaML` document.
#[derive(Debug, thiserror::Error)]
pub enum ClamlError {
    /// The file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The document is not well-formed XML.
    #[error("the document is not well-formed XML")]
    Xml(#[from] quick_xml::Error),
    /// The document has no `ClaML` root.
    #[error("the document has no `ClaML` root element")]
    NotClaml,
    /// The document has no `Title`.
    #[error("the document has no `Title`")]
    NoTitle,
    /// A `Class` lacks its `code` or `kind`.
    #[error("a `{element}` lacks its `{attribute}` attribute")]
    Attribute {
        /// The element.
        element: &'static str,
        /// The attribute.
        attribute: &'static str,
    },
    /// A class names a modifier the document does not define.
    #[error("class `{class}` is modified by `{modifier}`, which the document does not define")]
    UnknownModifier {
        /// The class code.
        class: String,
        /// The modifier code.
        modifier: String,
    },
}

/// One `ModifierClass`: a code and its titles by language.
#[derive(Debug, Clone, Default)]
struct ModifierClass {
    code: String,
    titles: BTreeMap<String, String>,
}

/// One `ModifiedBy` on a class.
#[derive(Debug, Clone)]
struct ModifiedBy {
    modifier: String,
    /// The modifier classes that apply; empty when all do.
    valid: Vec<String>,
}

/// A class as parsed, before modifier expansion.
#[derive(Debug, Clone, Default)]
struct Parsed {
    class: Class,
    modified_by: Vec<ModifiedBy>,
    excludes: Vec<String>,
}

/// The reader's state while an element is open.
#[derive(Debug, Default)]
struct Cursor {
    /// The open element names, outermost first.
    path: Vec<String>,
    title_text: bool,
    class: Option<Parsed>,
    modifier_class: Option<(String, ModifierClass)>,
    rubric_kind: Option<String>,
    label: Option<(String, String)>,
    /// Whether the open `Reference` renders in brackets.
    reference: Option<bool>,
}

#[derive(Debug, Default)]
struct Document {
    classification: Classification,
    parsed: Vec<Parsed>,
    modifiers: BTreeMap<String, Vec<ModifierClass>>,
    saw_root: bool,
}

fn required(
    start: &BytesStart<'_>,
    element: &'static str,
    name: &'static str,
) -> Result<String, ClamlError> {
    attribute(start, name).ok_or(ClamlError::Attribute {
        element,
        attribute: name,
    })
}

/// A `Meta` element: the one named `lang` states the document language.
fn meta(document: &mut Document, start: &BytesStart<'_>) {
    if attribute(start, "name").as_deref() == Some("lang")
        && let Some(value) = attribute(start, "value")
    {
        document.classification.language = value;
    }
}

impl Cursor {
    fn in_label(&self) -> bool {
        self.label.is_some()
    }

    fn append(&mut self, text: &str) {
        if let Some((_, buffer)) = &mut self.label {
            buffer.push_str(text);
        }
    }

    /// An open `ModifierClass`: its titles collect until it closes.
    fn open_modifier_class(&mut self, start: &BytesStart<'_>) -> Result<(), ClamlError> {
        let modifier = required(start, "ModifierClass", "modifier")?;
        let code = required(start, "ModifierClass", "code")?;
        self.modifier_class = Some((
            modifier,
            ModifierClass {
                code,
                titles: BTreeMap::new(),
            },
        ));
        Ok(())
    }

    /// An open `Class`: its rubrics and modifiers collect until it closes.
    fn open_class(&mut self, start: &BytesStart<'_>) -> Result<(), ClamlError> {
        let code = required(start, "Class", "code")?;
        let kind = required(start, "Class", "kind")?;
        self.class = Some(Parsed {
            class: Class {
                code,
                kind,
                usage: attribute(start, "usage"),
                ..Class::default()
            },
            ..Parsed::default()
        });
        Ok(())
    }

    /// A `SuperClass`: the first one names the open class's parent.
    fn super_class(&mut self, start: &BytesStart<'_>) {
        if let Some(parsed) = &mut self.class
            && parsed.class.parent.is_none()
        {
            parsed.class.parent = attribute(start, "code");
        }
    }

    /// A `ModifiedBy`: the open class takes the modifier it names.
    fn modified_by(&mut self, start: &BytesStart<'_>) -> Result<(), ClamlError> {
        if let Some(parsed) = &mut self.class {
            parsed.modified_by.push(ModifiedBy {
                modifier: required(start, "ModifiedBy", "code")?,
                valid: Vec::new(),
            });
        }
        Ok(())
    }

    /// A `ValidModifierClass`: it narrows the last `ModifiedBy` to one modifier class.
    fn valid_modifier_class(&mut self, start: &BytesStart<'_>) {
        if let Some(parsed) = &mut self.class
            && let (Some(last), Some(code)) =
                (parsed.modified_by.last_mut(), attribute(start, "code"))
        {
            last.valid.push(code);
        }
    }

    /// An `ExcludeModifier`: the open class drops the modifier it names.
    fn exclude_modifier(&mut self, start: &BytesStart<'_>) {
        if let Some(parsed) = &mut self.class
            && let Some(code) = attribute(start, "code")
        {
            parsed.excludes.push(code);
        }
    }

    /// A `Reference` inside a label: `class="in brackets"` parenthesizes it.
    fn open_reference(&mut self, start: &BytesStart<'_>) {
        let brackets = attribute(start, "class").is_some_and(|c| c == "in brackets");
        self.append(if brackets { " (" } else { " " });
        self.reference = Some(brackets);
    }

    fn start(&mut self, document: &mut Document, start: &BytesStart<'_>) -> Result<(), ClamlError> {
        let name = start.name().as_ref().to_owned();
        match name.as_str() {
            "ClaML" => document.saw_root = true,
            "Title" => {
                document.classification.name = attribute(start, "name").unwrap_or_default();
                document.classification.version = attribute(start, "version");
                self.title_text = true;
            }
            "Meta" => meta(document, start),
            "ClassKind" => document
                .classification
                .kinds
                .extend(attribute(start, "name")),
            "ModifierClass" => self.open_modifier_class(start)?,
            "Class" => self.open_class(start)?,
            "SuperClass" => self.super_class(start),
            "ModifiedBy" => self.modified_by(start)?,
            "ValidModifierClass" => self.valid_modifier_class(start),
            "ExcludeModifier" => self.exclude_modifier(start),
            "Rubric" if self.class.is_some() || self.modifier_class.is_some() => {
                self.rubric_kind = Some(required(start, "Rubric", "kind")?);
            }
            "Label" if self.rubric_kind.is_some() => {
                let language = attribute(start, "xml:lang")
                    .unwrap_or_else(|| document.classification.language.clone());
                self.label = Some((language, String::new()));
            }
            "Reference" if self.in_label() => self.open_reference(start),
            "Fragment" | "Para" | "Term" | "ListItem" | "Cell" if self.in_label() => {
                self.append(" ");
            }
            _ => {}
        }
        self.path.push(name);
        Ok(())
    }

    /// A closed `Label`: its text is a modifier title, or a rubric of the open class.
    fn close_label(&mut self) {
        let (Some((language, text)), Some(kind)) = (self.label.take(), &self.rubric_kind) else {
            return;
        };
        let text = collapse(&text);
        if text.is_empty() {
            return;
        }
        if let Some((_, class)) = &mut self.modifier_class {
            if kind == PREFERRED {
                class.titles.insert(language, text);
            }
        } else if let Some(parsed) = &mut self.class {
            parsed.class.rubrics.push(Rubric {
                kind: kind.clone(),
                language,
                text,
            });
        }
    }

    fn end(&mut self, document: &mut Document, name: &str) {
        self.path.pop();
        match name {
            "Title" => self.title_text = false,
            "Reference" => {
                if self.reference.take() == Some(true) {
                    self.append(")");
                }
            }
            "Label" => self.close_label(),
            "Rubric" => self.rubric_kind = None,
            "ModifierClass" => {
                if let Some((modifier, class)) = self.modifier_class.take() {
                    document.modifiers.entry(modifier).or_default().push(class);
                }
            }
            "Class" => {
                if let Some(parsed) = self.class.take() {
                    document.parsed.push(parsed);
                }
            }
            _ => {}
        }
    }

    fn text(&mut self, document: &mut Document, text: &str) {
        if self.title_text {
            document.classification.title.push_str(text);
        } else if self.in_label() {
            self.append(text);
        }
    }
}

/// Reads the `ClaML` document `text`.
///
/// # Errors
///
/// Returns [`ClamlError`] when the document is not well-formed, is not
/// `ClaML`, has no title, a class lacks its code or kind, or a class names an
/// undefined modifier.
pub fn read(text: &str) -> Result<Classification, ClamlError> {
    let mut reader = Reader::from_str(text);
    let mut document = Document {
        classification: Classification {
            language: String::from("en"),
            ..Classification::default()
        },
        ..Document::default()
    };
    let mut cursor = Cursor::default();
    loop {
        match reader.read_event()? {
            Event::Start(start) => cursor.start(&mut document, &start)?,
            Event::Empty(start) => {
                let name = start.name().as_ref().to_owned();
                cursor.start(&mut document, &start)?;
                cursor.end(&mut document, &name);
            }
            Event::Text(text) => {
                let content = text.xml_content(XmlVersion::Explicit1_0);
                cursor.text(&mut document, &content);
            }
            Event::CData(data) => {
                let content = data.xml_content(XmlVersion::Explicit1_0);
                cursor.text(&mut document, &content);
            }
            Event::GeneralRef(r) => cursor.text(&mut document, &reference(&r)),
            Event::End(end) => cursor.end(&mut document, end.name().as_ref()),
            Event::Eof => break,
            _ => {}
        }
    }
    if !document.saw_root {
        return Err(ClamlError::NotClaml);
    }
    if document.classification.name.is_empty() && document.classification.title.is_empty() {
        return Err(ClamlError::NoTitle);
    }
    document.classification.title = collapse(&document.classification.title);
    if document.classification.title.is_empty() {
        document
            .classification
            .title
            .clone_from(&document.classification.name);
    }
    periods(&mut document);
    expand(&mut document)?;
    Ok(document.classification)
}

/// Reads the `ClaML` file at `path`.
///
/// # Errors
///
/// Returns [`ClamlError`] when the file does not read or does not parse.
pub fn read_file(path: &Path) -> Result<Classification, ClamlError> {
    let text = std::fs::read_to_string(path).map_err(|source| ClamlError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    read(&text)
}

/// The modifiers that apply to `code`: those of its ancestors and its own,
/// root first, less those an intermediate class excludes.
fn applicable(by_code: &BTreeMap<&str, &Parsed>, code: &str) -> Vec<ModifiedBy> {
    let mut path = Vec::new();
    let mut current = Some(code);
    let mut seen = BTreeSet::new();
    while let Some(c) = current {
        if !seen.insert(c) {
            break;
        }
        let Some(parsed) = by_code.get(c) else {
            break;
        };
        path.push(*parsed);
        current = parsed.class.parent.as_deref();
    }
    let mut out: Vec<ModifiedBy> = Vec::new();
    for parsed in path.iter().rev() {
        out.retain(|m| !parsed.excludes.contains(&m.modifier));
        out.extend(parsed.modified_by.iter().cloned());
    }
    out
}

/// Gives each ICD-10 subcategory code the period after its third character.
///
/// The period belongs to a code whose three-character category is one of its
/// ancestors (<https://hl7.org/fhir/R4B/icd.html>). A code on another axis
/// keeps its spelling: the ICD-O morphology class `M953` under `M` is not
/// the ICD-10 subcategory `M95.3` under `M95`.
fn periods(document: &mut Document) {
    let parents: BTreeMap<&str, Option<&str>> = document
        .parsed
        .iter()
        .map(|p| (p.class.code.as_str(), p.class.parent.as_deref()))
        .collect();
    let mut renamed: BTreeMap<String, String> = BTreeMap::new();
    for parsed in &document.parsed {
        let code = parsed.class.code.as_str();
        let spelled = with_period(code);
        if spelled == code {
            continue;
        }
        let Some((category, _)) = code.split_at_checked(3) else {
            continue;
        };
        if descends_from(&parents, code, category) {
            renamed.insert(code.to_owned(), spelled);
        }
    }
    respell(document, &renamed);
}

/// Whether `ancestor` is above `code` in the parent chain.
fn descends_from(parents: &BTreeMap<&str, Option<&str>>, code: &str, ancestor: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut current = parents.get(code).copied().flatten();
    while let Some(above) = current {
        if above == ancestor {
            return true;
        }
        if !seen.insert(above) {
            return false;
        }
        current = parents.get(above).copied().flatten();
    }
    false
}

/// Gives every renamed class, and every class naming one as its parent, its new code.
fn respell(document: &mut Document, renamed: &BTreeMap<String, String>) {
    for parsed in &mut document.parsed {
        if let Some(spelled) = renamed.get(&parsed.class.code) {
            parsed.class.code.clone_from(spelled);
        }
        if let Some(spelled) = parsed
            .class
            .parent
            .as_deref()
            .and_then(|parent| renamed.get(parent))
        {
            parsed.class.parent = Some(spelled.clone());
        }
    }
}

/// Expands the modifiers onto the leaf classes they apply to.
fn expand(document: &mut Document) -> Result<(), ClamlError> {
    let by_code: BTreeMap<&str, &Parsed> = document
        .parsed
        .iter()
        .map(|p| (p.class.code.as_str(), p))
        .collect();
    let parents: BTreeSet<&str> = document
        .parsed
        .iter()
        .filter_map(|p| p.class.parent.as_deref())
        .collect();
    let mut generated: Vec<Class> = Vec::new();
    for parsed in &document.parsed {
        if parents.contains(parsed.class.code.as_str()) {
            continue;
        }
        let modifiers = applicable(&by_code, &parsed.class.code);
        if modifiers.is_empty() {
            continue;
        }
        generated.extend(modified_classes(document, parsed, &modifiers)?);
    }
    document.classification.classes = document.parsed.iter().map(|p| p.class.clone()).collect();
    document.classification.classes.extend(generated);
    Ok(())
}

/// The classes `parsed` gains from the modifiers that apply to it, one round
/// per modifier over the classes the round before produced.
fn modified_classes(
    document: &Document,
    parsed: &Parsed,
    modifiers: &[ModifiedBy],
) -> Result<Vec<Class>, ClamlError> {
    let mut generated: Vec<Class> = Vec::new();
    let mut leaves = vec![parsed.class.clone()];
    for modified_by in modifiers {
        let classes = document
            .modifiers
            .get(&modified_by.modifier)
            .ok_or_else(|| ClamlError::UnknownModifier {
                class: parsed.class.code.clone(),
                modifier: modified_by.modifier.clone(),
            })?;
        let next = modified_leaves(
            &leaves,
            classes,
            &modified_by.valid,
            &document.classification.language,
        );
        generated.extend(
            leaves
                .iter()
                .filter(|l| l.code != parsed.class.code)
                .cloned(),
        );
        leaves = next;
    }
    generated.extend(leaves);
    Ok(generated)
}

/// The classes `leaves` becomes under one modifier: every leaf crossed with
/// every modifier class the `ValidModifierClass` list admits.
fn modified_leaves(
    leaves: &[Class],
    classes: &[ModifierClass],
    valid: &[String],
    default_language: &str,
) -> Vec<Class> {
    let mut next = Vec::new();
    for leaf in leaves {
        for class in classes {
            if !valid.is_empty() && !valid.contains(&class.code) {
                continue;
            }
            next.push(modified(leaf, class, default_language));
        }
    }
    next
}

/// The class `leaf` gets from the modifier class `modifier`.
fn modified(leaf: &Class, modifier: &ModifierClass, default_language: &str) -> Class {
    let mut rubrics = Vec::new();
    let mut languages: BTreeSet<&str> = modifier.titles.keys().map(String::as_str).collect();
    languages.extend(
        leaf.rubrics
            .iter()
            .filter(|r| r.kind == PREFERRED)
            .map(|r| r.language.as_str()),
    );
    for language in languages {
        let title = match (leaf.title(language), modifier.titles.get(language)) {
            (Some(base), Some(suffix)) => format!("{base}, {suffix}"),
            (Some(base), None) => base.to_owned(),
            (None, Some(suffix)) => suffix.clone(),
            (None, None) => continue,
        };
        rubrics.push(Rubric {
            kind: PREFERRED.to_owned(),
            language: language.to_owned(),
            text: title,
        });
    }
    rubrics.push(Rubric {
        kind: MODIFIER.to_owned(),
        language: default_language.to_owned(),
        text: modifier.code.clone(),
    });
    Class {
        code: with_period(&format!("{}{}", leaf.code, modifier.code)),
        kind: leaf.kind.clone(),
        parent: Some(leaf.code.clone()),
        usage: leaf.usage.clone(),
        valid: leaf.valid,
        active: true,
        rubrics,
    }
}
