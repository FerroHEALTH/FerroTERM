// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading an Atom syndication document into the typed [`Feed`] model.
//!
//! The reader is namespace-aware: an element counts only when it resolves into
//! the namespace that defines it, so a feed is free to bind the extension
//! prefixes to any names it likes. Elements and attributes outside the four
//! known namespaces are ignored, which is what RFC 4287 §6.3 asks a consumer
//! to do with foreign markup it does not understand.

use core::str::FromStr;

use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::name::{QName, ResolveResult};

use crate::model::{
    ATOM_NS, Category, CategoryTerm, Checksum, ContentLink, Entry, Feed, LinkRel, NCTS_NS, ONTO_NS,
    SCT_NS,
};

/// A syndication document that cannot be read.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// The bytes are not well-formed XML.
    #[error("the syndication document is not well-formed XML")]
    Xml(#[from] quick_xml::Error),
    /// An element's attribute list is malformed.
    #[error("the syndication document has a malformed attribute")]
    Attribute(#[from] quick_xml::events::attributes::AttrError),
    /// The document root is not an Atom `feed`.
    #[error("the document root is <{root}>, not an Atom <feed>")]
    NotAFeed {
        /// The local name of the element the document opens with.
        root: String,
    },
    /// The document ends before its Atom `feed` closes.
    ///
    /// A listing cut short would otherwise read as a complete feed with fewer
    /// entries, and the run would take less than the service published.
    #[error("the syndication document ends before its <feed> closes")]
    Truncated,
    /// A date element does not carry an RFC 3339 timestamp.
    #[error("<{element}> carries '{value}', which is not an RFC 3339 timestamp")]
    Timestamp {
        /// The element the value was read from.
        element: String,
        /// The value as the document wrote it.
        value: String,
        /// Why the value is not a timestamp.
        #[source]
        source: jiff::Error,
    },
    /// A link's `length` attribute is not a byte count.
    #[error("a link declares length '{value}', which is not a byte count")]
    Length {
        /// The value as the document wrote it.
        value: String,
        /// Why the value is not a byte count.
        #[source]
        source: core::num::ParseIntError,
    },
}

/// Reads an Atom syndication document into the typed feed model.
///
/// # Errors
///
/// Returns [`ParseError::NotAFeed`] when the root element is not an Atom
/// `feed`, [`ParseError::Xml`] when the document is not well-formed,
/// [`ParseError::Timestamp`] when a date element is not RFC 3339, and
/// [`ParseError::Length`] when a link's advertised size is not a byte count.
pub fn feed(xml: &str) -> Result<Feed, ParseError> {
    Reader::new(xml).run()
}

/// The namespaces this reader distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Namespace {
    Atom,
    Ncts,
    Onto,
    Sct,
    /// No namespace at all, which is where every unprefixed attribute sits
    /// (Namespaces in XML 1.0 §6.2, <https://www.w3.org/TR/xml-names/>).
    Unbound,
    /// A namespace this reader does not know.
    Foreign,
}

impl Namespace {
    fn of(resolved: &ResolveResult<'_>) -> Self {
        match resolved {
            ResolveResult::Unbound => Self::Unbound,
            ResolveResult::Unknown(_) => Self::Foreign,
            ResolveResult::Bound(namespace) => match namespace.as_ref() {
                ATOM_NS => Self::Atom,
                NCTS_NS => Self::Ncts,
                ONTO_NS => Self::Onto,
                SCT_NS => Self::Sct,
                _ => Self::Foreign,
            },
        }
    }
}

/// The element whose character data is being collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Title,
    Id,
    Updated,
    Published,
    Summary,
    Rights,
    Generator,
    Profile,
    ContentItemIdentifier,
    ContentItemVersion,
    FhirVersion,
    BundleInterpretation,
    EditionDependency,
}

impl Field {
    fn of(namespace: Namespace, local: &str) -> Option<Self> {
        match (namespace, local) {
            (Namespace::Atom, "title") => Some(Self::Title),
            (Namespace::Atom, "id") => Some(Self::Id),
            (Namespace::Atom, "updated") => Some(Self::Updated),
            (Namespace::Atom, "published") => Some(Self::Published),
            (Namespace::Atom, "summary") => Some(Self::Summary),
            (Namespace::Atom, "rights") => Some(Self::Rights),
            (Namespace::Atom, "generator") => Some(Self::Generator),
            (Namespace::Ncts, "atomSyndicationFormatProfile") => Some(Self::Profile),
            (Namespace::Ncts, "contentItemIdentifier") => Some(Self::ContentItemIdentifier),
            (Namespace::Ncts, "contentItemVersion") => Some(Self::ContentItemVersion),
            (Namespace::Ncts, "fhirVersion") => Some(Self::FhirVersion),
            (Namespace::Ncts, "bundleInterpretation") => Some(Self::BundleInterpretation),
            (Namespace::Sct, "editionDependency") => Some(Self::EditionDependency),
            _ => None,
        }
    }

    const fn element(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Id => "id",
            Self::Updated => "updated",
            Self::Published => "published",
            Self::Summary => "summary",
            Self::Rights => "rights",
            Self::Generator => "generator",
            Self::Profile => "atomSyndicationFormatProfile",
            Self::ContentItemIdentifier => "contentItemIdentifier",
            Self::ContentItemVersion => "contentItemVersion",
            Self::FhirVersion => "fhirVersion",
            Self::BundleInterpretation => "bundleInterpretation",
            Self::EditionDependency => "editionDependency",
        }
    }
}

/// The reader's state while it walks one document.
struct Reader<'a> {
    xml: quick_xml::NsReader<&'a [u8]>,
    feed: Feed,
    entry: Option<Entry>,
    /// How deep inside an Atom `source` element the reader is.
    ///
    /// An entry may quote the feed it came from, and that copy repeats
    /// `title`, `id`, and `link` (RFC 4287 §4.2.11); none of it is the entry's.
    source_depth: usize,
    /// How deep inside an Atom `author` or `contributor` element the reader is.
    ///
    /// Both hold a `name`, a `uri`, and an `email` (RFC 4287 §3.2), so their
    /// character data is not the entry's either.
    person_depth: usize,
    field: Option<Field>,
    text: String,
    seen_root: bool,
    /// How many elements are open. A document that ends above zero was cut
    /// short, and a short feed would be read as a complete one.
    depth: usize,
}

impl<'a> Reader<'a> {
    fn new(xml: &'a str) -> Self {
        Self {
            xml: quick_xml::NsReader::from_str(xml),
            feed: Feed::default(),
            entry: None,
            source_depth: 0,
            person_depth: 0,
            field: None,
            text: String::new(),
            seen_root: false,
            depth: 0,
        }
    }

    fn run(mut self) -> Result<Feed, ParseError> {
        loop {
            let namespace;
            let event;
            {
                let (resolved, next) = self.xml.read_resolved_event()?;
                namespace = Namespace::of(&resolved);
                event = next;
            }
            match event {
                Event::Start(start) => {
                    self.depth = self.depth.saturating_add(1);
                    self.start(namespace, &start, false)?;
                }
                Event::Empty(start) => self.start(namespace, &start, true)?,
                Event::End(end) => {
                    self.depth = self.depth.saturating_sub(1);
                    let local = local_name(end.name());
                    self.end(namespace, &local)?;
                }
                Event::Text(text) => self.push(&text),
                Event::CData(cdata) => self.push(&cdata),
                Event::GeneralRef(reference) => self.push(&resolve_reference(&reference)),
                Event::Eof => break,
                Event::Decl(_) | Event::PI(_) | Event::Comment(_) | Event::DocType(_) => {}
            }
        }
        if !self.seen_root || self.depth > 0 {
            return Err(ParseError::Truncated);
        }
        Ok(self.feed)
    }

    fn push(&mut self, text: &str) {
        if self.field.is_some() {
            self.text.push_str(text);
        }
    }

    fn start(
        &mut self,
        namespace: Namespace,
        start: &BytesStart<'_>,
        empty: bool,
    ) -> Result<(), ParseError> {
        let local = local_name(start.name());
        if !self.seen_root {
            self.seen_root = true;
            if namespace != Namespace::Atom || local != "feed" {
                return Err(ParseError::NotAFeed { root: local });
            }
            return Ok(());
        }
        if namespace == Namespace::Atom && local == "source" {
            if !empty {
                self.source_depth = self.source_depth.saturating_add(1);
            }
            return Ok(());
        }
        if namespace == Namespace::Atom && matches!(local.as_str(), "author" | "contributor") {
            if !empty {
                self.person_depth = self.person_depth.saturating_add(1);
            }
            return Ok(());
        }
        if self.source_depth > 0 || self.person_depth > 0 {
            return Ok(());
        }
        match (namespace, local.as_str()) {
            (Namespace::Atom, "entry") => {
                self.entry = Some(Entry::default());
                return Ok(());
            }
            (Namespace::Atom, "link") => {
                let link = content_link(&self.xml, start)?;
                if let Some(entry) = self.entry.as_mut() {
                    entry.links.push(link);
                }
                return Ok(());
            }
            (Namespace::Atom, "category") => {
                let category = category(start)?;
                if let Some(entry) = self.entry.as_mut() {
                    entry.category = Some(category);
                }
                return Ok(());
            }
            (Namespace::Onto, "permission") => {
                let code = attribute(start, "code")?;
                if let Some(entry) = self.entry.as_mut() {
                    entry.permission = code;
                }
                return Ok(());
            }
            _ => {}
        }
        if let Some(field) = Field::of(namespace, &local) {
            self.text.clear();
            self.field = Some(field);
            if empty {
                self.commit(field)?;
            }
        }
        Ok(())
    }

    fn end(&mut self, namespace: Namespace, local: &str) -> Result<(), ParseError> {
        if namespace == Namespace::Atom && local == "source" {
            self.source_depth = self.source_depth.saturating_sub(1);
            return Ok(());
        }
        if namespace == Namespace::Atom && matches!(local, "author" | "contributor") {
            self.person_depth = self.person_depth.saturating_sub(1);
            return Ok(());
        }
        if self.source_depth > 0 || self.person_depth > 0 {
            return Ok(());
        }
        if namespace == Namespace::Atom && local == "entry" {
            if let Some(entry) = self.entry.take() {
                self.feed.entries.push(entry);
            }
            return Ok(());
        }
        let closing = self
            .field
            .filter(|field| Field::of(namespace, local) == Some(*field));
        if let Some(field) = closing {
            self.commit(field)?;
        }
        Ok(())
    }

    fn commit(&mut self, field: Field) -> Result<(), ParseError> {
        self.field = None;
        let value = core::mem::take(&mut self.text).trim().to_owned();
        match self.entry.as_mut() {
            Some(entry) => commit_entry(entry, field, value),
            None => commit_feed(&mut self.feed, field, value),
        }
    }
}

fn commit_entry(entry: &mut Entry, field: Field, value: String) -> Result<(), ParseError> {
    match field {
        Field::Title => entry.title = value,
        Field::Id => entry.id = value,
        Field::Updated => entry.updated = Some(timestamp(field, &value)?),
        Field::Published => entry.published = Some(timestamp(field, &value)?),
        Field::Summary => entry.summary = Some(value),
        Field::Rights => entry.rights = Some(value),
        Field::ContentItemIdentifier => entry.content_item_identifier = Some(value),
        Field::ContentItemVersion => entry.content_item_version = Some(value),
        Field::FhirVersion => entry.fhir_version = Some(value),
        Field::BundleInterpretation => entry.bundle_interpretation = Some(value),
        Field::EditionDependency => entry.edition_dependency = Some(value),
        Field::Generator | Field::Profile => {}
    }
    Ok(())
}

fn commit_feed(feed: &mut Feed, field: Field, value: String) -> Result<(), ParseError> {
    match field {
        Field::Title => feed.title = Some(value),
        Field::Id => feed.id = Some(value),
        Field::Updated => feed.updated = Some(timestamp(field, &value)?),
        Field::Generator => feed.generator = Some(value),
        Field::Profile => feed.profile = Some(value),
        Field::Published
        | Field::Summary
        | Field::Rights
        | Field::ContentItemIdentifier
        | Field::ContentItemVersion
        | Field::FhirVersion
        | Field::BundleInterpretation
        | Field::EditionDependency => {}
    }
    Ok(())
}

/// Reads one Atom `link` element, resolving its namespaced attributes.
fn content_link(
    reader: &quick_xml::NsReader<&[u8]>,
    start: &BytesStart<'_>,
) -> Result<ContentLink, ParseError> {
    let mut link = ContentLink::default();
    for attribute in start.attributes() {
        let attribute = attribute?;
        let (resolved, local) = reader.resolver().resolve_attribute(attribute.key);
        let namespace = Namespace::of(&resolved);
        let local = local.as_ref().to_owned();
        let value = value_of(&attribute)?;
        match (namespace, local.as_str()) {
            (Namespace::Unbound, "href") => link.href = value,
            (Namespace::Unbound, "rel") => link.rel = LinkRel::parse(&value),
            (Namespace::Unbound, "type") => link.media_type = Some(value),
            (Namespace::Unbound, "length") => {
                let length = u64::from_str(&value).map_err(|source| ParseError::Length {
                    value: value.clone(),
                    source,
                })?;
                link.length = Some(length);
            }
            (Namespace::Ncts, "sha256Hash") => {
                link.checksum = Some(Checksum::Sha256(value.to_ascii_lowercase()));
            }
            (Namespace::Sct, "md5Hash") => {
                link.checksum = Some(Checksum::Md5(value.to_ascii_lowercase()));
            }
            (Namespace::Onto, "validated") => link.validated = value == "true",
            _ => {}
        }
    }
    Ok(link)
}

/// Reads one Atom `category` element.
fn category(start: &BytesStart<'_>) -> Result<Category, ParseError> {
    let term = attribute(start, "term")?.unwrap_or_default();
    Ok(Category {
        term: CategoryTerm::parse(&term),
        label: attribute(start, "label")?,
        scheme: attribute(start, "scheme")?,
    })
}

/// The value of an unprefixed attribute, or `None` when the element has none.
fn attribute(start: &BytesStart<'_>, name: &str) -> Result<Option<String>, ParseError> {
    for attribute in start.attributes() {
        let attribute = attribute?;
        if attribute.key.as_ref() == name {
            return Ok(Some(value_of(&attribute)?));
        }
    }
    Ok(None)
}

fn value_of(attribute: &Attribute<'_>) -> Result<String, ParseError> {
    Ok(attribute
        .normalized_value(quick_xml::XmlVersion::Explicit1_0)?
        .into_owned())
}

fn local_name(name: QName<'_>) -> String {
    name.local_name().as_ref().to_owned()
}

fn timestamp(field: Field, value: &str) -> Result<jiff::Timestamp, ParseError> {
    jiff::Timestamp::from_str(value).map_err(|source| ParseError::Timestamp {
        element: field.element().to_owned(),
        value: value.to_owned(),
        source,
    })
}

/// The text a character or entity reference stands for.
///
/// The five predefined entities and numeric references resolve; any other
/// entity is kept as written, so an entity a document declares in a DTD this
/// reader does not read stays visible instead of vanishing.
fn resolve_reference(reference: &BytesRef<'_>) -> String {
    if let Ok(Some(resolved)) = reference.resolve_char_ref() {
        return resolved.to_string();
    }
    match &**reference {
        "amp" => String::from("&"),
        "lt" => String::from("<"),
        "gt" => String::from(">"),
        "quot" => String::from("\""),
        "apos" => String::from("'"),
        other => format!("&{other};"),
    }
}
