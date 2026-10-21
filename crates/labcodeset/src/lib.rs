// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Nederlandse Labcodeset publication
//! (<https://www.nictiz.nl/wat-we-doen/activiteiten/terminologie/nederlandse-labcodeset/>).
//!
//! Nictiz publishes the Labcodeset as one XML document, `labconcepts-<date>.xml`,
//! whose schema (`Nederlandse Labcodeset v6.xsd`) ships in the release zip. A
//! `publication` carries the laboratory concepts (each a LOINC concept with
//! its axes, a Dutch translation, SNOMED CT materials, an outcome list, and
//! UCUM units), the material table, the unit table, the ordinal outcome value
//! sets, and the nominal outcome refsets. The reader turns the document into
//! [`Publication`]; the build writes it as FHIR resources over the LOINC,
//! SNOMED CT, and UCUM providers.

use std::path::{Path, PathBuf};

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// The Dutch language tag the publication's translations carry.
pub const DUTCH: &str = "nl-NL";
/// The OID of SNOMED CT, as the ordinal value sets name it.
pub const SNOMED_OID: &str = "2.16.840.1.113883.6.96";

/// A failure to read a publication.
#[derive(Debug, thiserror::Error)]
pub enum LabcodesetError {
    /// The file or directory cannot be read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// No `labconcepts-*.xml` under the directory.
    #[error("no `labconcepts-*.xml` document under {root}")]
    NoDocument {
        /// The directory.
        root: PathBuf,
    },
    /// The XML does not parse.
    #[error("{path} is not well-formed XML")]
    Xml {
        /// The document.
        path: PathBuf,
        /// The parser's error.
        #[source]
        source: quick_xml::Error,
    },
    /// An attribute does not parse.
    #[error("{path}: an attribute of `{element}` does not parse")]
    Attribute {
        /// The document.
        path: PathBuf,
        /// The element.
        element: String,
        /// The parser's error.
        #[source]
        source: quick_xml::events::attributes::AttrError,
    },
    /// An element the schema does not define.
    #[error("{path}: unexpected element `{element}` inside `{parent}`")]
    Unexpected {
        /// The document.
        path: PathBuf,
        /// The parent element.
        parent: String,
        /// The element met.
        element: String,
    },
    /// A required attribute is absent.
    #[error("{path}: `{element}` has no `{attribute}` attribute")]
    MissingAttribute {
        /// The document.
        path: PathBuf,
        /// The element.
        element: String,
        /// The attribute.
        attribute: &'static str,
    },
    /// A required child element is absent.
    #[error("{path}: `{parent}` has no `{element}` element")]
    MissingElement {
        /// The document.
        path: PathBuf,
        /// The parent element.
        parent: String,
        /// The element.
        element: &'static str,
    },
    /// The document ends inside an element.
    #[error("{path}: the document ends inside `{element}`")]
    Truncated {
        /// The document.
        path: PathBuf,
        /// The open element.
        element: String,
    },
}

/// The status of a Labcodeset concept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConceptStatus {
    /// In use.
    Active,
    /// Withdrawn from the set; `retired_reason` and `retired_replacement` say why.
    Retired,
}

/// The status of the LOINC concept as LOINC states it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoincStatus {
    /// Active.
    Active,
    /// Deprecated by LOINC.
    Deprecated,
    /// Discouraged by LOINC.
    Discouraged,
}

/// The six LOINC axes, in English or in Dutch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Axes {
    /// The component (analyte).
    pub component: String,
    /// The property.
    pub property: Option<String>,
    /// The timing.
    pub timing: Option<String>,
    /// The system (specimen).
    pub system: Option<String>,
    /// The scale.
    pub scale: Option<String>,
    /// The method.
    pub method: Option<String>,
}

/// The Dutch translation of a LOINC concept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Translation {
    /// The language tag (`nl-NL`).
    pub language: String,
    /// The translated axes.
    pub axes: Axes,
    /// The translated class.
    pub class: Option<String>,
    /// The translated long name.
    pub long_name: Option<String>,
}

/// A LOINC replacement the publication records for a deprecated concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    /// The deprecated code.
    pub from: String,
    /// The code to use instead.
    pub to: String,
    /// The publication's comment.
    pub comment: String,
}

/// The LOINC concept a Labcodeset concept is built on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoincConcept {
    /// The LOINC code.
    pub code: String,
    /// LOINC's status of the code.
    pub status: LoincStatus,
    /// The English axes.
    pub axes: Axes,
    /// The LOINC class.
    pub class: String,
    /// Order, observation, or both.
    pub order_observation: Option<String>,
    /// `Panel` for a panel concept.
    pub panel_type: Option<String>,
    /// The English long common name.
    pub long_name: String,
    /// The replacement of a deprecated code.
    pub replacement: Option<Replacement>,
    /// The Dutch translation.
    pub translation: Option<Translation>,
}

/// A SNOMED CT material a concept may be observed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialRef {
    /// The SNOMED CT concept identifier.
    pub code: String,
    /// The display the publication gives.
    pub display_name: String,
}

/// A SNOMED CT reference set the publication points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refset {
    /// The reference set's concept identifier.
    pub concept_id: String,
    /// Its Dutch preferred term.
    pub preferred_term: String,
    /// Where the publication says to find it.
    pub source: String,
}

/// The outcome list of a concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A nominal list: a SNOMED CT reference set.
    Refset(Refset),
    /// An ordinal list: one of the publication's value sets, by its OID.
    ValueSet(String),
}

/// One laboratory concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabConcept {
    /// Active or retired.
    pub status: ConceptStatus,
    /// The LOINC concept.
    pub loinc: LoincConcept,
    /// The materials.
    pub materials: Vec<MaterialRef>,
    /// The outcome list.
    pub outcome: Option<Outcome>,
    /// The unit identifiers, into the publication's unit table.
    pub units: Vec<String>,
    /// Why the concept was retired.
    pub retired_reason: Option<String>,
    /// The concepts replacing a retired one, as the publication writes them.
    pub retired_replacement: Option<String>,
    /// A release note.
    pub release_note: Option<String>,
}

/// One row of the material table: a SNOMED CT material for a LOINC system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Material {
    /// The SNOMED CT concept identifier.
    pub code: String,
    /// The display the publication gives.
    pub display_name: String,
    /// The LOINC system axis text it stands for.
    pub system: Option<String>,
}

/// One row of the unit table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// The identifier concepts refer to.
    pub id: String,
    /// `active` or `retired`.
    pub status: Option<String>,
    /// The UCUM expression.
    pub ucum: String,
    /// The English name.
    pub name: Option<String>,
    /// The Dutch name.
    pub dutch_name: String,
}

/// One concept of an ordinal value set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrdinalConcept {
    /// The code.
    pub code: String,
    /// The code system, as an OID.
    pub code_system: String,
    /// The code system's name, when given.
    pub code_system_name: Option<String>,
    /// The display.
    pub display_name: String,
    /// The hierarchy level.
    pub level: Option<String>,
    /// The kind (`L` for a leaf).
    pub kind: Option<String>,
    /// Descriptions by language.
    pub descriptions: Vec<(Option<String>, String)>,
}

/// One ordinal outcome value set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrdinalValueSet {
    /// The OID.
    pub id: String,
    /// The effective date.
    pub effective_date: Option<String>,
    /// The machine name.
    pub name: Option<String>,
    /// The display name.
    pub display_name: String,
    /// The status.
    pub status: Option<String>,
    /// The version label.
    pub version_label: Option<String>,
    /// The concepts.
    pub concepts: Vec<OrdinalConcept>,
}

/// A Labcodeset publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publication {
    /// The `effectiveDate` attribute (`20260818-120038313`).
    pub effective_date: String,
    /// The publication's description.
    pub description: String,
    /// The laboratory concepts, in document order.
    pub concepts: Vec<LabConcept>,
    /// The material table.
    pub materials: Vec<Material>,
    /// The unit table.
    pub units: Vec<Unit>,
    /// The ordinal value sets.
    pub ordinals: Vec<OrdinalValueSet>,
    /// The nominal reference sets.
    pub nominals: Vec<Refset>,
}

impl Publication {
    /// The release date, the first eight digits of the effective date
    /// (`20260818`).
    #[must_use]
    pub fn release(&self) -> String {
        self.effective_date
            .chars()
            .take_while(char::is_ascii_digit)
            .take(8)
            .collect()
    }

    /// The unit with `id`.
    #[must_use]
    pub fn unit(&self, id: &str) -> Option<&Unit> {
        self.units.iter().find(|u| u.id == id)
    }
}

/// Reads a publication: the `labconcepts-*.xml` document, or the directory
/// holding it.
///
/// # Errors
///
/// Returns [`LabcodesetError`] when the document is missing, malformed, or
/// carries an element or attribute the schema does not define.
pub fn read(path: &Path) -> Result<Publication, LabcodesetError> {
    let document = if path.is_dir() {
        find_document(path)?
    } else {
        path.to_path_buf()
    };
    let text = std::fs::read_to_string(&document).map_err(|source| LabcodesetError::Io {
        path: document.clone(),
        source,
    })?;
    parse(&text, &document)
}

fn find_document(root: &Path) -> Result<PathBuf, LabcodesetError> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|source| LabcodesetError::Io {
            path: dir.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| LabcodesetError::Io {
                path: dir.clone(),
                source,
            })?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if is_document(&path) {
                found.push(path);
            }
        }
    }
    found.sort();
    found.pop().ok_or_else(|| LabcodesetError::NoDocument {
        root: root.to_path_buf(),
    })
}

/// Whether `path` names a `labconcepts-*.xml` document.
fn is_document(path: &Path) -> bool {
    path.file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|n| n.starts_with("labconcepts"))
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
}

/// Parses a publication document.
///
/// # Errors
///
/// Returns [`LabcodesetError`] when the document is malformed or carries an
/// element or attribute the schema does not define; `path` names it.
pub fn parse(text: &str, path: &Path) -> Result<Publication, LabcodesetError> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut parser = Parser {
        reader,
        path: path.to_path_buf(),
    };
    loop {
        match parser.next()? {
            Event::Start(start) if start.name().as_ref() == "publication" => {
                let start = start.into_owned();
                return parser.publication(&start);
            }
            Event::Eof => {
                return Err(LabcodesetError::MissingElement {
                    path: path.to_path_buf(),
                    parent: String::from("(document)"),
                    element: "publication",
                });
            }
            Event::Start(other) | Event::Empty(other) => {
                return Err(LabcodesetError::Unexpected {
                    path: path.to_path_buf(),
                    parent: String::from("(document)"),
                    element: other.name().as_ref().to_owned(),
                });
            }
            _ => {}
        }
    }
}

struct Parser<'a> {
    reader: Reader<&'a [u8]>,
    path: PathBuf,
}

/// A child element as the parser hands it to a handler: its start tag and
/// whether it is self-closing.
struct Child<'e> {
    start: BytesStart<'e>,
    empty: bool,
}

impl Child<'_> {
    fn name(&self) -> String {
        self.start.name().as_ref().to_owned()
    }
}

impl Parser<'_> {
    fn next(&mut self) -> Result<Event<'static>, LabcodesetError> {
        self.reader
            .read_event()
            .map(Event::into_owned)
            .map_err(|source| LabcodesetError::Xml {
                path: self.path.clone(),
                source,
            })
    }

    fn attribute(
        &self,
        start: &BytesStart<'_>,
        name: &str,
    ) -> Result<Option<String>, LabcodesetError> {
        for attribute in start.attributes() {
            let attribute = attribute.map_err(|source| LabcodesetError::Attribute {
                path: self.path.clone(),
                element: start.name().as_ref().to_owned(),
                source,
            })?;
            if attribute.key.as_ref() == name {
                let value = attribute
                    .normalized_value(quick_xml::XmlVersion::Explicit1_0)
                    .map_err(|source| LabcodesetError::Xml {
                        path: self.path.clone(),
                        source,
                    })?;
                return Ok(Some(value.into_owned()));
            }
        }
        Ok(None)
    }

    fn required(
        &self,
        start: &BytesStart<'_>,
        name: &'static str,
    ) -> Result<String, LabcodesetError> {
        self.attribute(start, name)?
            .ok_or_else(|| LabcodesetError::MissingAttribute {
                path: self.path.clone(),
                element: start.name().as_ref().to_owned(),
                attribute: name,
            })
    }

    /// The text of a leaf element, unescaped and trimmed; empty for a
    /// self-closing one.
    fn text(&mut self, child: &Child<'_>) -> Result<String, LabcodesetError> {
        if child.empty {
            return Ok(String::new());
        }
        let raw = self
            .reader
            .read_text(child.start.name())
            .map_err(|source| LabcodesetError::Xml {
                path: self.path.clone(),
                source,
            })?;
        Ok(raw.xml10_content().trim().to_owned())
    }

    /// Skips an element's whole subtree.
    fn skip(&mut self, child: &Child<'_>) -> Result<(), LabcodesetError> {
        if child.empty {
            return Ok(());
        }
        self.reader
            .read_to_end(child.start.name())
            .map(|_| ())
            .map_err(|source| LabcodesetError::Xml {
                path: self.path.clone(),
                source,
            })
    }

    /// Hands every child element of `parent` to `handle`, until the parent
    /// closes; text between elements is an error.
    fn children(
        &mut self,
        parent: &BytesStart<'_>,
        empty: bool,
        mut handle: impl FnMut(&mut Self, &Child<'_>) -> Result<(), LabcodesetError>,
    ) -> Result<(), LabcodesetError> {
        if empty {
            return Ok(());
        }
        let parent_name = parent.name().as_ref().to_owned();
        loop {
            match self.next()? {
                Event::End(_) => return Ok(()),
                Event::Start(start) => handle(
                    self,
                    &Child {
                        start,
                        empty: false,
                    },
                )?,
                Event::Empty(start) => handle(self, &Child { start, empty: true })?,
                Event::Comment(_) | Event::PI(_) => {}
                Event::Eof => {
                    return Err(LabcodesetError::Truncated {
                        path: self.path.clone(),
                        element: parent_name,
                    });
                }
                _ => {
                    return Err(LabcodesetError::Unexpected {
                        path: self.path.clone(),
                        parent: parent_name,
                        element: String::from("(text)"),
                    });
                }
            }
        }
    }

    /// Hands every `element` child of the list element `list` to `handle`; a
    /// child of any other name is an error.
    fn items(
        &mut self,
        list: &Child<'_>,
        element: &str,
        mut handle: impl FnMut(&mut Self, &Child<'_>) -> Result<(), LabcodesetError>,
    ) -> Result<(), LabcodesetError> {
        let parent = list.start.clone();
        self.children(&parent, list.empty, |parser, item| {
            if item.name() == element {
                handle(parser, item)
            } else {
                Err(parser.unexpected(&parent, item))
            }
        })
    }

    fn unexpected(&self, parent: &BytesStart<'_>, child: &Child<'_>) -> LabcodesetError {
        LabcodesetError::Unexpected {
            path: self.path.clone(),
            parent: parent.name().as_ref().to_owned(),
            element: child.name(),
        }
    }

    fn missing(&self, parent: &BytesStart<'_>, element: &'static str) -> LabcodesetError {
        LabcodesetError::MissingElement {
            path: self.path.clone(),
            parent: parent.name().as_ref().to_owned(),
            element,
        }
    }

    fn publication(&mut self, start: &BytesStart<'_>) -> Result<Publication, LabcodesetError> {
        let effective_date = self.required(start, "effectiveDate")?;
        let mut description = String::new();
        let mut concepts = Vec::new();
        let mut materials = Vec::new();
        let mut units = Vec::new();
        let mut ordinals = Vec::new();
        let mut nominals = Vec::new();
        self.children(start, false, |parser, child| {
            match child.name().as_str() {
                "desc" => description = parser.text(child)?,
                "lab_concepts" => parser.items(child, "lab_concept", |parser, item| {
                    concepts.push(parser.lab_concept(item)?);
                    Ok(())
                })?,
                "map" => parser.items(child, "material", |parser, item| {
                    materials.push(Material {
                        code: parser.required(&item.start, "code")?,
                        display_name: parser.required(&item.start, "displayName")?,
                        system: parser.attribute(&item.start, "system")?,
                    });
                    parser.skip(item)
                })?,
                "units" => parser.items(child, "unit", |parser, item| {
                    units.push(parser.unit(item)?);
                    Ok(())
                })?,
                "ordinals" => parser.items(child, "valueSet", |parser, item| {
                    ordinals.push(parser.ordinal(item)?);
                    Ok(())
                })?,
                "nominals" => parser.items(child, "refset", |parser, item| {
                    nominals.push(parser.refset(item)?);
                    Ok(())
                })?,
                "panels" => parser.skip(child)?,
                _ => return Err(parser.unexpected(start, child)),
            }
            Ok(())
        })?;
        Ok(Publication {
            effective_date,
            description,
            concepts,
            materials,
            units,
            ordinals,
            nominals,
        })
    }

    fn lab_concept(&mut self, node: &Child<'_>) -> Result<LabConcept, LabcodesetError> {
        let status = match self.required(&node.start, "status")?.as_str() {
            "active" => ConceptStatus::Active,
            "retired" => ConceptStatus::Retired,
            _ => {
                return Err(LabcodesetError::MissingAttribute {
                    path: self.path.clone(),
                    element: node.name(),
                    attribute: "status (active or retired)",
                });
            }
        };
        let mut loinc = None;
        let mut materials = Vec::new();
        let mut outcome = None;
        let mut units = Vec::new();
        let mut retired_reason = None;
        let mut retired_replacement = None;
        let mut release_note = None;
        let start = node.start.clone();
        self.children(&start, node.empty, |parser, child| {
            match child.name().as_str() {
                "loincConcept" => loinc = Some(parser.loinc_concept(child)?),
                "materials" => parser.items(child, "material", |parser, item| {
                    materials.push(MaterialRef {
                        code: parser.required(&item.start, "code")?,
                        display_name: parser.required(&item.start, "displayName")?,
                    });
                    parser.skip(item)
                })?,
                "outcomes" => {
                    let parent = child.start.clone();
                    parser.children(&parent, child.empty, |parser, item| {
                        match item.name().as_str() {
                            "refset" => outcome = Some(Outcome::Refset(parser.refset(item)?)),
                            "valueSet" => {
                                outcome =
                                    Some(Outcome::ValueSet(parser.required(&item.start, "ref")?));
                                parser.skip(item)?;
                            }
                            _ => return Err(parser.unexpected(&parent, item)),
                        }
                        Ok(())
                    })?;
                }
                "units" => parser.items(child, "unit", |parser, item| {
                    units.push(parser.required(&item.start, "ref")?);
                    parser.skip(item)
                })?,
                "retired-reason" => retired_reason = Some(parser.text(child)?),
                "retired-replacement" => retired_replacement = Some(parser.text(child)?),
                "releasenote" => release_note = Some(parser.text(child)?),
                _ => return Err(parser.unexpected(&start, child)),
            }
            Ok(())
        })?;
        Ok(LabConcept {
            status,
            loinc: loinc.ok_or_else(|| self.missing(&start, "loincConcept"))?,
            materials,
            outcome,
            units,
            retired_reason,
            retired_replacement,
            release_note,
        })
    }

    fn loinc_concept(&mut self, node: &Child<'_>) -> Result<LoincConcept, LabcodesetError> {
        let code = self.required(&node.start, "loinc_num")?;
        let status = match self.required(&node.start, "status")?.as_str() {
            "ACTIVE" => LoincStatus::Active,
            "DEPRECATED" => LoincStatus::Deprecated,
            "DISCOURAGED" => LoincStatus::Discouraged,
            _ => {
                return Err(LabcodesetError::MissingAttribute {
                    path: self.path.clone(),
                    element: node.name(),
                    attribute: "status (ACTIVE, DEPRECATED, or DISCOURAGED)",
                });
            }
        };
        let mut axes = Axes::default();
        let mut class = None;
        let mut order_observation = None;
        let mut panel_type = None;
        let mut long_name = None;
        let mut replacement = None;
        let mut translation = None;
        let start = node.start.clone();
        self.children(&start, node.empty, |parser, child| {
            let name = child.name();
            if axis(&mut axes, &name, parser, child)? {
                return Ok(());
            }
            match name.as_str() {
                "class" => class = Some(parser.text(child)?),
                "orderObs" => order_observation = Some(parser.text(child)?),
                "panelType" => panel_type = Some(parser.text(child)?),
                "longName" => long_name = Some(parser.text(child)?),
                "map" => {
                    replacement = Some(Replacement {
                        from: parser.required(&child.start, "from")?,
                        to: parser.required(&child.start, "to")?,
                        comment: parser
                            .attribute(&child.start, "comment")?
                            .unwrap_or_default(),
                    });
                    parser.skip(child)?;
                }
                "translation" => translation = Some(parser.translation(child)?),
                "references" => parser.skip(child)?,
                _ => return Err(parser.unexpected(&start, child)),
            }
            Ok(())
        })?;
        Ok(LoincConcept {
            code,
            status,
            axes,
            class: class.ok_or_else(|| self.missing(&start, "class"))?,
            order_observation,
            panel_type,
            long_name: long_name.ok_or_else(|| self.missing(&start, "longName"))?,
            replacement,
            translation,
        })
    }

    fn translation(&mut self, node: &Child<'_>) -> Result<Translation, LabcodesetError> {
        let language = self.required(&node.start, "language")?;
        let mut axes = Axes::default();
        let mut class = None;
        let mut long_name = None;
        let start = node.start.clone();
        self.children(&start, node.empty, |parser, child| {
            let name = child.name();
            if axis(&mut axes, &name, parser, child)? {
                return Ok(());
            }
            match name.as_str() {
                "class" => class = Some(parser.text(child)?),
                "longName" => long_name = Some(parser.text(child)?),
                _ => return Err(parser.unexpected(&start, child)),
            }
            Ok(())
        })?;
        Ok(Translation {
            language,
            axes,
            class,
            long_name,
        })
    }

    fn refset(&mut self, node: &Child<'_>) -> Result<Refset, LabcodesetError> {
        let refset = Refset {
            concept_id: self.required(&node.start, "conceptId")?,
            preferred_term: self.required(&node.start, "preferredTerm")?,
            source: self.required(&node.start, "src")?,
        };
        self.skip(node)?;
        Ok(refset)
    }

    fn unit(&mut self, node: &Child<'_>) -> Result<Unit, LabcodesetError> {
        let id = self.required(&node.start, "id")?;
        let status = self.attribute(&node.start, "status")?;
        let mut ucum = None;
        let mut name = None;
        let mut dutch_name = None;
        let start = node.start.clone();
        self.children(&start, node.empty, |parser, child| {
            match child.name().as_str() {
                "rm" => ucum = Some(parser.text(child)?),
                "name" => name = Some(parser.text(child)?),
                "nlname" => dutch_name = Some(parser.text(child)?),
                _ => return Err(parser.unexpected(&start, child)),
            }
            Ok(())
        })?;
        Ok(Unit {
            id,
            status,
            ucum: ucum.ok_or_else(|| self.missing(&start, "rm"))?,
            name,
            dutch_name: dutch_name.ok_or_else(|| self.missing(&start, "nlname"))?,
        })
    }

    fn ordinal(&mut self, node: &Child<'_>) -> Result<OrdinalValueSet, LabcodesetError> {
        let id = self.required(&node.start, "id")?;
        let display_name = self.required(&node.start, "displayName")?;
        let effective_date = self.attribute(&node.start, "effectiveDate")?;
        let name = self.attribute(&node.start, "name")?;
        let status = self.attribute(&node.start, "statusCode")?;
        let version_label = self.attribute(&node.start, "versionLabel")?;
        let mut concepts = Vec::new();
        let start = node.start.clone();
        self.children(&start, node.empty, |parser, child| {
            if child.name() != "conceptList" {
                return Err(parser.unexpected(&start, child));
            }
            let list = child.start.clone();
            parser.children(&list, child.empty, |parser, item| {
                if item.name() != "concept" {
                    return Err(parser.unexpected(&list, item));
                }
                concepts.push(parser.ordinal_concept(item)?);
                Ok(())
            })
        })?;
        Ok(OrdinalValueSet {
            id,
            effective_date,
            name,
            display_name,
            status,
            version_label,
            concepts,
        })
    }

    fn ordinal_concept(&mut self, node: &Child<'_>) -> Result<OrdinalConcept, LabcodesetError> {
        let mut concept = OrdinalConcept {
            code: self.required(&node.start, "code")?,
            code_system: self.required(&node.start, "codeSystem")?,
            code_system_name: self.attribute(&node.start, "codeSystemName")?,
            display_name: self.required(&node.start, "displayName")?,
            level: self.attribute(&node.start, "level")?,
            kind: self.attribute(&node.start, "type")?,
            descriptions: Vec::new(),
        };
        let start = node.start.clone();
        self.children(&start, node.empty, |parser, child| {
            if child.name() != "desc" {
                return Err(parser.unexpected(&start, child));
            }
            let language = parser.attribute(&child.start, "language")?;
            let text = parser.text(child)?;
            concept.descriptions.push((language, text));
            Ok(())
        })?;
        Ok(concept)
    }
}

/// Reads a LOINC axis element into `axes`; `false` when `name` is no axis.
fn axis(
    axes: &mut Axes,
    name: &str,
    parser: &mut Parser<'_>,
    child: &Child<'_>,
) -> Result<bool, LabcodesetError> {
    let slot = match name {
        "component" => {
            axes.component = parser.text(child)?;
            return Ok(true);
        }
        "property" => &mut axes.property,
        "timing" => &mut axes.timing,
        "system" => &mut axes.system,
        "scale" => &mut axes.scale,
        "method" => &mut axes.method,
        _ => return Ok(false),
    };
    *slot = Some(parser.text(child)?);
    Ok(true)
}
