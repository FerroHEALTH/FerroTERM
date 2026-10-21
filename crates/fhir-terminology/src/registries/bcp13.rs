// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! BCP 13 media types (`urn:ietf:bcp:13`): the grammar of RFC 6838 §4.2 and
//! RFC 2045 §5.1 over the IANA Media Types registry.
//!
//! A media type is a code when it parses; `registered` says whether its
//! `type/subtype` is in the registry. Parameters narrow a type, so
//! `text/plain` subsumes `text/plain; charset=utf-8`. The system cannot be
//! enumerated, only the registered types can.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use concept_graph::subsumption::Outcome;

use super::interned::Interned;
use crate::compose::Compose;
use crate::filter::{Filter, FilterOperator};
use crate::provider::{
    CodeSystemProvider, Compositional, Concept, ConceptSet, ContentMode, Declaration, Designation,
    FilterDefinition, Hierarchy, Identity, Located, Property, PropertyDefinition, PropertyKind,
    PropertyValue, ProviderError, Status,
};

/// The system URI (<https://hl7.org/fhir/R4B/terminologies-systems.html>).
pub const URL: &str = "urn:ietf:bcp:13";

/// The vendored registry, one CSV per top-level type.
const REGISTRY_CSV: [(&str, &str); 10] = [
    (
        "application",
        include_str!("../../data/iana/media-types/application.csv"),
    ),
    (
        "audio",
        include_str!("../../data/iana/media-types/audio.csv"),
    ),
    ("font", include_str!("../../data/iana/media-types/font.csv")),
    (
        "haptics",
        include_str!("../../data/iana/media-types/haptics.csv"),
    ),
    (
        "image",
        include_str!("../../data/iana/media-types/image.csv"),
    ),
    (
        "message",
        include_str!("../../data/iana/media-types/message.csv"),
    ),
    (
        "model",
        include_str!("../../data/iana/media-types/model.csv"),
    ),
    (
        "multipart",
        include_str!("../../data/iana/media-types/multipart.csv"),
    ),
    ("text", include_str!("../../data/iana/media-types/text.csv")),
    (
        "video",
        include_str!("../../data/iana/media-types/video.csv"),
    ),
];

/// The parameters whose meaning the server knows, so a difference in them
/// decides subsumption; a difference in any other parameter cannot be
/// decided. No specification lists these: our own design.
const KNOWN_PARAMETERS: [&str; 10] = [
    "boundary",
    "charset",
    "codecs",
    "delsp",
    "fhirversion",
    "format",
    "level",
    "profile",
    "type",
    "version",
];

/// The registered `type/subtype` values, lower case, sorted.
static REGISTERED: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut out = BTreeSet::new();
    for (_, text) in REGISTRY_CSV {
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .from_reader(text.as_bytes());
        for record in reader.records().flatten() {
            if let Some(template) = record.get(1)
                && let Some(parsed) = parse(template)
            {
                out.insert(parsed.base());
            }
        }
    }
    out.into_iter().collect()
});

/// A parsed media type, lower case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaType {
    /// The top-level type.
    pub kind: String,
    /// The subtype, suffix included.
    pub subtype: String,
    /// The parameters in the order given, names and values lower case.
    pub parameters: Vec<(String, String)>,
}

impl MediaType {
    /// `type/subtype`.
    #[must_use]
    pub fn base(&self) -> String {
        format!("{}/{}", self.kind, self.subtype)
    }

    /// `type/subtype; name=value; …`.
    #[must_use]
    pub fn canonical(&self) -> String {
        let mut text = self.base();
        for (name, value) in &self.parameters {
            text.push_str("; ");
            text.push_str(name);
            text.push('=');
            text.push_str(value);
        }
        text
    }

    fn parameter_map(&self) -> BTreeMap<&str, &str> {
        self.parameters
            .iter()
            .map(|(n, v)| (n.as_str(), v.as_str()))
            .collect()
    }
}

/// A restricted name (RFC 6838 §4.2): a letter or digit, then up to 126 of
/// `[A-Za-z0-9!#$&^_.+-]`.
fn is_restricted_name(s: &str) -> bool {
    let bytes = s.as_bytes();
    let Some(first) = bytes.first() else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && bytes.len() <= 127
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(b))
}

/// An RFC 2045 token: no space, control, or tspecial characters.
fn is_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b > 0x20 && b < 0x7f && !b"()<>@,;:\\\"/[]?=".contains(&b))
}

/// Parses `text` as a media type with parameters.
#[must_use]
pub fn parse(text: &str) -> Option<MediaType> {
    let mut pieces = text.split(';');
    let base = pieces.next()?.trim();
    let (kind, subtype) = base.split_once('/')?;
    if !is_restricted_name(kind) || !is_restricted_name(subtype) {
        return None;
    }
    let mut parameters = Vec::new();
    for piece in pieces {
        let piece = piece.trim();
        if piece.is_empty() {
            return None;
        }
        let (name, value) = piece.split_once('=')?;
        let name = name.trim();
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(value);
        if !is_token(name) || !is_token(value) {
            return None;
        }
        parameters.push((name.to_ascii_lowercase(), value.to_ascii_lowercase()));
    }
    Some(MediaType {
        kind: kind.to_ascii_lowercase(),
        subtype: subtype.to_ascii_lowercase(),
        parameters,
    })
}

/// The BCP 13 provider.
#[derive(Debug)]
pub struct Bcp13Provider {
    identity: Identity,
    declaration: Declaration,
    interned: Interned,
    registered: u32,
}

impl Default for Bcp13Provider {
    fn default() -> Self {
        Self::new()
    }
}

impl Bcp13Provider {
    /// The provider over the vendored registry; the registered types take
    /// the first ordinals so `registered = true` enumerates.
    #[must_use]
    pub fn new() -> Self {
        let interned = Interned::new();
        for base in REGISTERED.iter() {
            // NOTE: the table starts empty and holds fewer than u32::MAX entries, so
            // interning the registry cannot fail; a failure would be a defect.
            if interned.intern(base).is_err() {
                break;
            }
        }
        let registered = u32::try_from(interned.len()).unwrap_or(u32::MAX);
        let filter =
            |code: &str, description: &str, operators: Vec<FilterOperator>| FilterDefinition {
                code: code.to_owned(),
                description: Some(description.to_owned()),
                operators,
                value: String::from("a value"),
            };
        Self {
            identity: Identity {
                url: URL.to_owned(),
                version: String::new(),
                title: Some(String::from("IETF BCP 13 media types")),
                name: None,
                version_needed: false,
            },
            declaration: Declaration {
                content: ContentMode::NotPresent,
                case_sensitive: false,
                hierarchy_meaning: None,
                // NOTE: RFC 6838 §4.2 defines the `type/subtype` construction
                // with parameters, and this provider parses it, so the grammar
                // is both defined and supported.
                compositional: Compositional::Supported,
                // NOTE: a media type is a protocol identifier that RFC 6838
                // registers under one spelling and never translates, and this
                // system's display is that spelling, so it states no language.
                languages: Vec::new(),
                properties: vec![
                    PropertyDefinition {
                        code: String::from("base"),
                        uri: None,
                        description: Some(String::from("The type, and the type/subtype")),
                        kind: PropertyKind::Code,
                    },
                    PropertyDefinition {
                        code: String::from("registered"),
                        uri: None,
                        description: Some(String::from(
                            "Whether type/subtype is registered with IANA",
                        )),
                        kind: PropertyKind::Boolean,
                    },
                ],
                filters: vec![
                    filter(
                        "base",
                        "Media types of this type, or of this type/subtype",
                        vec![FilterOperator::Equal, FilterOperator::In],
                    ),
                    filter(
                        "registered",
                        "Registered (true) or unregistered (false) types; only `true` enumerates",
                        vec![FilterOperator::Equal],
                    ),
                ],
                capabilities: BTreeSet::new(),
                implicit_forms: Vec::new(),
            },
            interned,
            registered,
        }
    }

    fn media_type(&self, concept: Concept) -> Option<MediaType> {
        self.interned.code(concept).and_then(|code| parse(&code))
    }

    fn is_registered(media: &MediaType) -> bool {
        REGISTERED.binary_search(&media.base()).is_ok()
    }

    fn matches(media: &MediaType, filter: &Filter) -> Result<bool, ProviderError> {
        let unsupported = || ProviderError::UnsupportedFilter {
            property: filter.property.clone(),
            operator: filter.op.code().to_owned(),
        };
        match (filter.property.as_str(), filter.op) {
            ("base", FilterOperator::Equal) => {
                let wanted = filter.value.trim().to_ascii_lowercase();
                Ok(media.kind == wanted || media.base() == wanted)
            }
            ("base", FilterOperator::In) => Ok(filter.value.split(',').any(|w| {
                let wanted = w.trim().to_ascii_lowercase();
                media.kind == wanted || media.base() == wanted
            })),
            ("registered", FilterOperator::Equal) => {
                let wanted = match filter.value.trim() {
                    "true" => true,
                    "false" => false,
                    other => {
                        return Err(ProviderError::invalid_filter_value(
                            &filter.property,
                            other,
                            "`true` or `false`",
                        ));
                    }
                };
                Ok(Self::is_registered(media) == wanted)
            }
            _ => Err(unsupported()),
        }
    }
}

impl CodeSystemProvider for Bcp13Provider {
    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        let Some(media) = parse(code) else {
            return Ok(None);
        };
        let canonical = media.canonical();
        let concept = self.interned.intern(&canonical)?;
        Ok(Some(Located {
            concept,
            code: canonical,
        }))
    }

    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        Ok(self.interned.code(concept))
    }

    fn display(
        &self,
        concept: Concept,
        _language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        Ok(self.interned.code(concept))
    }

    fn status(&self, _concept: Concept) -> Result<Status, ProviderError> {
        Ok(Status {
            standards_status: None,
            active: true,
            inactive_reason: None,
            abstract_concept: false,
            codeless: false,
        })
    }

    fn designations(
        &self,
        _concept: Concept,
        _language: Option<&str>,
    ) -> Result<Vec<Designation>, ProviderError> {
        Ok(Vec::new())
    }

    fn properties(&self, concept: Concept) -> Result<Vec<Property>, ProviderError> {
        let Some(media) = self.media_type(concept) else {
            return Ok(Vec::new());
        };
        Ok(vec![
            Property {
                code: String::from("base"),
                value: PropertyValue::Code(media.kind.clone()),
                ..Property::default()
            },
            Property {
                code: String::from("base"),
                value: PropertyValue::Code(media.base()),
                ..Property::default()
            },
            Property {
                code: String::from("registered"),
                value: PropertyValue::Boolean(Self::is_registered(&media)),
                ..Property::default()
            },
        ])
    }

    fn hierarchy(&self) -> Option<&dyn Hierarchy> {
        None
    }

    fn implicit_value_set(&self, _url: &str) -> Option<Result<Compose, ProviderError>> {
        None
    }

    fn all(&self) -> Result<ConceptSet, ProviderError> {
        Err(ProviderError::NotEnumerable)
    }

    fn search(&self, _text: &str, _language: Option<&str>) -> Result<ConceptSet, ProviderError> {
        Err(ProviderError::NotEnumerable)
    }

    fn filter(&self, filter: &Filter) -> Result<ConceptSet, ProviderError> {
        self.filter_all(std::slice::from_ref(filter))
    }

    fn filter_all(&self, filters: &[Filter]) -> Result<ConceptSet, ProviderError> {
        let bounded = filters.iter().any(|f| {
            f.property == "registered" && f.op == FilterOperator::Equal && f.value.trim() == "true"
        });
        if !bounded {
            for filter in filters {
                if !self
                    .declaration
                    .filters
                    .iter()
                    .any(|f| f.code == filter.property)
                {
                    return Err(ProviderError::UnsupportedFilter {
                        property: filter.property.clone(),
                        operator: filter.op.code().to_owned(),
                    });
                }
            }
            return Err(ProviderError::NotEnumerable);
        }
        let mut set = ConceptSet::new();
        for index in 0..self.registered {
            let concept = Concept::new(index);
            let Some(media) = self.media_type(concept) else {
                continue;
            };
            let mut keep = true;
            for filter in filters {
                if !Self::matches(&media, filter)? {
                    keep = false;
                    break;
                }
            }
            if keep {
                set.insert(index);
            }
        }
        Ok(set)
    }

    /// Every filtered selection of this system is incomplete: the registry is
    /// finite, but each registered type also has the unboundedly many
    /// parameterised forms the grammar builds on it (RFC 2045 §5.1), which no
    /// enumeration carries.
    fn unclosed(&self, _filters: &[Filter]) -> bool {
        true
    }

    fn filter_matches(&self, concept: Concept, filter: &Filter) -> Result<bool, ProviderError> {
        let Some(media) = self.media_type(concept) else {
            return Ok(false);
        };
        Self::matches(&media, filter)
    }

    /// Parameters narrow a media type: `a` subsumes `b` when `a`'s parameters
    /// are among `b`'s with the same values. A difference in a parameter the
    /// server does not know cannot be decided.
    fn subsumes(&self, a: Concept, b: Concept) -> Result<Option<Outcome>, ProviderError> {
        let (Some(a), Some(b)) = (self.media_type(a), self.media_type(b)) else {
            return Ok(None);
        };
        if a.base() != b.base() {
            return Ok(Some(Outcome::NotSubsumed));
        }
        let pa = a.parameter_map();
        let pb = b.parameter_map();
        let differing: Vec<&str> = pa
            .iter()
            .filter(|(n, v)| pb.get(*n) != Some(v))
            .map(|(n, _)| *n)
            .chain(
                pb.iter()
                    .filter(|(n, v)| pa.get(*n) != Some(v))
                    .map(|(n, _)| *n),
            )
            .collect();
        if let Some(unknown) = differing.iter().find(|n| !KNOWN_PARAMETERS.contains(n)) {
            return Err(ProviderError::CannotDetermine(format!(
                "the parameter `{unknown}` of `{}` is not one the server knows the meaning of",
                a.base()
            )));
        }
        let a_in_b = pa.iter().all(|(n, v)| pb.get(n) == Some(v));
        let b_in_a = pb.iter().all(|(n, v)| pa.get(n) == Some(v));
        Ok(Some(match (a_in_b, b_in_a) {
            (true, true) => Outcome::Equivalent,
            (true, false) => Outcome::Subsumes,
            (false, true) => Outcome::SubsumedBy,
            (false, false) => Outcome::NotSubsumed,
        }))
    }
}
