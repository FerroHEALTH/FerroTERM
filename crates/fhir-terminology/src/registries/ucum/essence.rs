// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The vendored `ucum-essence.xml`: prefixes, base units, and defined units.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};

const ESSENCE: &str = include_str!("../../../data/ucum/ucum-essence.xml");

/// A prefix (`k`, `m`, `u`, …) and its factor.
#[derive(Debug, Clone, PartialEq)]
pub struct Prefix {
    /// The case-sensitive code.
    pub code: String,
    /// The name.
    pub name: String,
    /// The factor.
    pub value: f64,
}

/// One of the seven base units, with its dimension index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseUnit {
    /// The case-sensitive code.
    pub code: String,
    /// The name.
    pub name: String,
    /// The property (`length`, `time`, …).
    pub property: String,
    /// The position in the dimension vector.
    pub dimension: usize,
}

/// A defined unit, expressed in other units.
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    /// The case-sensitive code.
    pub code: String,
    /// The name.
    pub name: String,
    /// The property.
    pub property: String,
    /// Whether a prefix may precede it.
    pub is_metric: bool,
    /// A special unit: its value is a function of the base, not a factor.
    pub is_special: bool,
    /// An arbitrary unit: not commensurable with anything else.
    pub is_arbitrary: bool,
    /// The class (`si`, `clinical`, …).
    pub class: String,
    /// `value@Unit`: the unit this one is defined in.
    pub definition: String,
    /// `value@value`: the factor over that unit.
    pub factor: f64,
}

/// The parsed essence.
#[derive(Debug)]
pub struct Essence {
    /// The `version` attribute of the root.
    pub version: String,
    /// The prefixes by code.
    pub prefixes: BTreeMap<String, Prefix>,
    /// The base units by code.
    pub base_units: BTreeMap<String, BaseUnit>,
    /// The defined units by code.
    pub units: BTreeMap<String, Unit>,
}

/// The vendored essence, parsed once.
pub static ESSENCE_DATA: LazyLock<Essence> = LazyLock::new(|| parse(ESSENCE));

/// Which element is being read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Reading {
    #[default]
    None,
    Prefix,
    BaseUnit,
    Unit,
}

/// The fields collected for one element.
#[derive(Debug, Default)]
struct Pending {
    reading: Reading,
    code: String,
    name: String,
    property: String,
    is_metric: bool,
    is_special: bool,
    is_arbitrary: bool,
    class: String,
    definition: String,
    factor: Option<f64>,
    text_into: Option<&'static str>,
}

fn attribute(event: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<String> {
    event
        .attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| a.normalized_value(XmlVersion::Explicit1_0).ok())
        .map(std::borrow::Cow::into_owned)
}

impl Pending {
    /// Begins an element.
    fn start(&mut self, essence: &mut Essence, start: &quick_xml::events::BytesStart<'_>) {
        match start.name().as_ref() {
            "root" => essence.version = attribute(start, "version").unwrap_or_default(),
            name @ ("prefix" | "base-unit" | "unit") => {
                *self = Self {
                    reading: match name {
                        "prefix" => Reading::Prefix,
                        "base-unit" => Reading::BaseUnit,
                        _ => Reading::Unit,
                    },
                    code: attribute(start, "Code").unwrap_or_default(),
                    is_metric: attribute(start, "isMetric").as_deref() == Some("yes"),
                    is_special: attribute(start, "isSpecial").as_deref() == Some("yes"),
                    is_arbitrary: attribute(start, "isArbitrary").as_deref() == Some("yes"),
                    class: attribute(start, "class").unwrap_or_default(),
                    ..Self::default()
                };
            }
            "name" if self.reading != Reading::None && self.name.is_empty() => {
                self.text_into = Some("name");
            }
            "property" if self.reading != Reading::None => self.text_into = Some("property"),
            // NOTE: a special unit's `value` names the function and its scale and base
            // unit in a nested `function` element; the base is what dimensional
            // analysis needs.
            "value" | "function" if self.reading != Reading::None => {
                self.definition = attribute(start, "Unit").unwrap_or_default();
                self.factor = attribute(start, "value").and_then(|v| v.parse().ok());
            }
            _ => {}
        }
    }

    /// Takes the text of a `name` or `property` element.
    fn text(&mut self, value: String) {
        match self.text_into.take() {
            Some("name") => self.name = value,
            Some(_) => self.property = value,
            None => {}
        }
    }

    /// Ends an element, adding the record to `essence`.
    fn end(&mut self, essence: &mut Essence, name: &str, dimensions: &mut usize) {
        match (name, self.reading) {
            ("prefix", Reading::Prefix) => {
                essence.prefixes.insert(
                    self.code.clone(),
                    Prefix {
                        code: self.code.clone(),
                        name: self.name.clone(),
                        value: self.factor.unwrap_or(1.0),
                    },
                );
            }
            ("base-unit", Reading::BaseUnit) => {
                essence.base_units.insert(
                    self.code.clone(),
                    BaseUnit {
                        code: self.code.clone(),
                        name: self.name.clone(),
                        property: self.property.clone(),
                        dimension: *dimensions,
                    },
                );
                *dimensions += 1;
            }
            ("unit", Reading::Unit) => {
                essence.units.insert(
                    self.code.clone(),
                    Unit {
                        code: self.code.clone(),
                        name: self.name.clone(),
                        property: self.property.clone(),
                        is_metric: self.is_metric,
                        is_special: self.is_special,
                        is_arbitrary: self.is_arbitrary,
                        class: self.class.clone(),
                        definition: self.definition.clone(),
                        factor: self.factor.unwrap_or(1.0),
                    },
                );
            }
            _ => return,
        }
        self.reading = Reading::None;
    }
}

/// Parses an essence document.
#[must_use]
pub fn parse(text: &str) -> Essence {
    let mut reader = Reader::from_str(text);
    let mut essence = Essence {
        version: String::new(),
        prefixes: BTreeMap::new(),
        base_units: BTreeMap::new(),
        units: BTreeMap::new(),
    };
    let mut pending = Pending::default();
    let mut dimensions = 0usize;
    while let Ok(event) = reader.read_event() {
        match event {
            Event::Start(start) | Event::Empty(start) => pending.start(&mut essence, &start),
            Event::Text(text) => {
                pending.text(text.xml_content(XmlVersion::Explicit1_0).trim().to_owned());
            }
            Event::End(end) => pending.end(&mut essence, end.name().as_ref(), &mut dimensions),
            Event::Eof => break,
            _ => {}
        }
    }
    essence
}

#[cfg(test)]
mod tests {
    use super::ESSENCE_DATA;

    #[test]
    fn the_vendored_essence_parses_whole() {
        let essence = &*ESSENCE_DATA;
        assert_eq!(essence.version, "2.2");
        assert_eq!(essence.prefixes.len(), 24);
        assert_eq!(essence.base_units.len(), 7);
        assert_eq!(essence.units.len(), 305);
        assert_eq!(essence.prefixes.get("k").map(|p| p.value), Some(1e3));
        assert_eq!(
            essence.base_units.get("m").map(|b| b.property.as_str()),
            Some("length")
        );
        let newton = essence.units.get("N").expect("N");
        assert_eq!(newton.definition, "kg.m/s2");
        assert!(newton.is_metric);
        let inch = essence.units.get("[in_i]").expect("inch");
        assert_eq!(inch.definition, "cm");
        assert!((inch.factor - 2.54).abs() < 1e-12);
        assert!(
            essence
                .units
                .get("Cel")
                .is_some_and(|c| c.is_special && c.definition == "K")
        );
        assert!(essence.units.get("[iU]").is_some_and(|u| u.is_arbitrary));
    }
}
