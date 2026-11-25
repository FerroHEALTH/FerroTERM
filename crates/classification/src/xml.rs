// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The XML reading shared by the readers.

use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

/// The normalized value of the attribute `name` on `event`.
pub(crate) fn attribute(event: &BytesStart<'_>, name: &str) -> Option<String> {
    event
        .attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| a.normalized_value(XmlVersion::Explicit1_0).ok())
        .map(std::borrow::Cow::into_owned)
}

/// The text a character or entity reference stands for.
///
/// Numeric references and the five predefined entities resolve; any other
/// entity is kept as written, so a document-defined entity stays visible
/// instead of vanishing.
pub(crate) fn reference(event: &BytesRef<'_>) -> String {
    if let Ok(Some(c)) = event.resolve_char_ref() {
        return c.to_string();
    }
    match &**event {
        "amp" => String::from("&"),
        "lt" => String::from("<"),
        "gt" => String::from(">"),
        "quot" => String::from("\""),
        "apos" => String::from("'"),
        other => format!("&{other};"),
    }
}

/// One element read whole: its name, attributes, text, and child elements.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Element {
    /// The qualified name (`skos:notation`).
    pub(crate) name: String,
    /// The attributes by qualified name, normalized.
    pub(crate) attributes: Vec<(String, String)>,
    /// The element's own character data, references resolved.
    pub(crate) text: String,
    /// The child elements in document order.
    pub(crate) children: Vec<Element>,
}

/// `name` without its namespace prefix.
fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, l)| l)
}

impl Element {
    /// The local name.
    pub(crate) fn local(&self) -> &str {
        local(&self.name)
    }

    /// The attribute whose local name is `name`.
    pub(crate) fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| local(k) == name)
            .map(|(_, v)| v.as_str())
    }

    /// The first child whose local name is `name`.
    pub(crate) fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.local() == name)
    }

    /// Every child whose local name is `name`.
    pub(crate) fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.local() == name)
    }

    /// The text of the child at the `/`-separated local-name `path`, trimmed.
    pub(crate) fn text_at(&self, path: &str) -> Option<&str> {
        let mut at = self;
        for step in path.split('/') {
            at = at.child(step)?;
        }
        Some(at.text.trim()).filter(|t| !t.is_empty())
    }
}

fn open(event: &BytesStart<'_>) -> Element {
    Element {
        name: event.name().as_ref().to_owned(),
        attributes: event
            .attributes()
            .flatten()
            .filter_map(|a| {
                let value = a.normalized_value(XmlVersion::Explicit1_0).ok()?;
                Some((a.key.as_ref().to_owned(), value.into_owned()))
            })
            .collect(),
        ..Element::default()
    }
}

/// The first element whose local name is `name`, with its attributes and no
/// content; the document is read only up to its start tag.
pub(crate) fn first(text: &str, name: &str) -> Result<Option<Element>, quick_xml::Error> {
    let mut reader = Reader::from_str(text);
    loop {
        match reader.read_event()? {
            Event::Start(start) | Event::Empty(start) if local(start.name().as_ref()) == name => {
                return Ok(Some(open(&start)));
            }
            Event::Eof => return Ok(None),
            _ => {}
        }
    }
}

/// Calls `each` with every element whose local name is `record`, read whole,
/// in document order; a record nested in another stays inside its parent.
///
/// Only one record is held at a time, so a large document streams.
pub(crate) fn records<E: From<quick_xml::Error>>(
    text: &str,
    record: &str,
    mut each: impl FnMut(Element) -> Result<(), E>,
) -> Result<(), E> {
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<Element> = Vec::new();
    loop {
        match reader.read_event()? {
            Event::Start(start) => {
                if !stack.is_empty() || local(start.name().as_ref()) == record {
                    stack.push(open(&start));
                }
            }
            Event::Empty(start) => {
                let element = open(&start);
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(element);
                } else if element.local() == record {
                    each(element)?;
                }
            }
            Event::Text(text) => {
                if let Some(top) = stack.last_mut() {
                    top.text
                        .push_str(&text.xml_content(XmlVersion::Explicit1_0));
                }
            }
            Event::CData(data) => {
                if let Some(top) = stack.last_mut() {
                    top.text
                        .push_str(&data.xml_content(XmlVersion::Explicit1_0));
                }
            }
            Event::GeneralRef(r) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&reference(&r));
                }
            }
            Event::End(_) => {
                if let Some(done) = stack.pop() {
                    match stack.last_mut() {
                        Some(parent) => parent.children.push(done),
                        None => each(done)?,
                    }
                }
            }
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    use super::{Element, attribute, records, reference};

    #[test]
    fn records_read_whole_with_nested_records_kept_inside() {
        let xml = "<root><r id='1'><n>a&amp;b</n><r id='2'/></r><x/><r id='3'><n/></r></root>";
        let mut seen: Vec<Element> = Vec::new();
        records::<quick_xml::Error>(xml, "r", |e| {
            seen.push(e);
            Ok(())
        })
        .expect("parses");
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].attr("id"), Some("1"));
        assert_eq!(seen[0].text_at("n"), Some("a&b"));
        assert_eq!(seen[0].all("r").count(), 1);
        assert_eq!(seen[1].attr("id"), Some("3"));
        assert_eq!(seen[1].text_at("n"), None);
    }

    /// The references in `xml`, in document order, as `reference` resolves them.
    fn references(xml: &str) -> Vec<String> {
        let mut reader = Reader::from_str(xml);
        let mut out = Vec::new();
        loop {
            match reader.read_event() {
                Ok(Event::GeneralRef(event)) => out.push(reference(&event)),
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(error) => panic!("the fixture parses: {error}"),
            }
        }
        out
    }

    #[test]
    fn the_predefined_entities_resolve_and_any_other_stays_as_written() {
        // The five XML 1.0 predefined entities
        // (<https://www.w3.org/TR/xml/#sec-predefined-ent>).
        assert_eq!(
            references("<r>&amp;&lt;&gt;&quot;&apos;</r>"),
            ["&", "<", ">", "\"", "'"]
        );
        // A classification may define its own entities in a DTD this reader
        // does not read, so one is kept visible rather than dropped.
        assert_eq!(references("<r>&dagger;</r>"), ["&dagger;"]);
    }

    #[test]
    fn a_numeric_reference_resolves_in_both_bases() {
        assert_eq!(
            references("<r>&#8224;&#x2020;</r>"),
            ["\u{2020}", "\u{2020}"]
        );
    }

    #[test]
    fn an_attribute_is_found_by_name_and_normalized() {
        let mut reader = Reader::from_str("<rubric kind='preferred' id='x'/>");
        let Ok(Event::Empty(event)) = reader.read_event() else {
            panic!("the fixture holds one empty element");
        };
        assert_eq!(attribute(&event, "kind").as_deref(), Some("preferred"));
        assert_eq!(attribute(&event, "id").as_deref(), Some("x"));
        assert_eq!(attribute(&event, "code"), None);
    }
}
