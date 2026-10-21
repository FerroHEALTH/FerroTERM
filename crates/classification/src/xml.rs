// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The XML reading shared by the two readers.

use quick_xml::XmlVersion;
use quick_xml::events::{BytesRef, BytesStart};

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

#[cfg(test)]
mod tests {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    use super::{attribute, reference};

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
