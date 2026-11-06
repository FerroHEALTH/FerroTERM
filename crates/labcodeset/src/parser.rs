// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reader of the publication document: one handler per element the
//! schema defines, refusing any element or attribute it does not.

use std::path::PathBuf;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::{
    Axes, ConceptStatus, LabConcept, LabcodesetError, LoincConcept, LoincStatus, Material,
    MaterialRef, OrdinalConcept, OrdinalValueSet, Outcome, Publication, Refset, Replacement,
    Translation, Unit,
};

pub(crate) struct Parser<'a> {
    pub(crate) reader: Reader<&'a [u8]>,
    pub(crate) path: PathBuf,
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
    pub(crate) fn next(&mut self) -> Result<Event<'static>, LabcodesetError> {
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

    pub(crate) fn publication(
        &mut self,
        start: &BytesStart<'_>,
    ) -> Result<Publication, LabcodesetError> {
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
