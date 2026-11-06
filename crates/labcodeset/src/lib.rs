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

mod parser;

use std::path::{Path, PathBuf};

use quick_xml::Reader;
use quick_xml::events::Event;

use parser::Parser;

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
