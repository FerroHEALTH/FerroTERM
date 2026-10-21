// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! BCP 47 language tags (`urn:ietf:bcp:47`): the grammar of RFC 5646 §2.1
//! over the IANA Language Subtag Registry.
//!
//! A tag is well-formed when it parses; it is valid when every subtag is
//! registered for its position (RFC 5646 §2.2.9). Only a valid tag is a code
//! of the system. The system cannot be enumerated
//! (<https://hl7.org/fhir/R4B/valueset-all-languages.html>).

use std::collections::BTreeSet;

use concept_graph::subsumption::Outcome;

use super::interned::Interned;
use super::subtags::{Kind, REGISTRY_DATA, Registry};
use crate::compose::Compose;
use crate::filter::{Filter, FilterOperator};
use crate::provider::{
    CodeSystemProvider, Compositional, Concept, ConceptSet, ContentMode, Declaration, Designation,
    FilterDefinition, Hierarchy, Identity, Located, Property, PropertyDefinition, PropertyKind,
    PropertyValue, ProviderError, Status,
};

/// The system URI (<https://hl7.org/fhir/R4B/terminologies-systems.html>).
pub const URL: &str = "urn:ietf:bcp:47";

/// The parts of a language tag, in canonical case (RFC 5646 §2.1.1).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tag {
    /// The primary language subtag, lower case.
    pub language: Option<String>,
    /// The extended language subtag, lower case.
    pub extlang: Option<String>,
    /// The script subtag, title case.
    pub script: Option<String>,
    /// The region subtag, upper case.
    pub region: Option<String>,
    /// The variant subtags, lower case.
    pub variants: Vec<String>,
    /// The extension sequences, each as written after canonical lowering.
    pub extensions: Vec<String>,
    /// The private-use sequence, lower case, without the leading `x-`.
    pub private_use: Option<String>,
    /// A grandfathered tag, as registered.
    pub grandfathered: Option<String>,
}

impl Tag {
    /// The tag in canonical case.
    #[must_use]
    pub fn canonical(&self) -> String {
        if let Some(tag) = &self.grandfathered {
            return tag.clone();
        }
        let mut parts: Vec<String> = Vec::new();
        parts.extend(self.language.clone());
        parts.extend(self.extlang.clone());
        parts.extend(self.script.clone());
        parts.extend(self.region.clone());
        parts.extend(self.variants.iter().cloned());
        parts.extend(self.extensions.iter().cloned());
        if let Some(private) = &self.private_use {
            parts.push(format!("x-{private}"));
        }
        parts.join("-")
    }
}

/// What the grammar and the registry say about a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Analysis {
    /// The text does not parse as a language tag.
    Malformed(String),
    /// The text parses, and these subtags are not registered.
    WellFormed {
        /// The parsed tag.
        tag: Tag,
        /// The unregistered subtags, as written.
        unknown: Vec<String>,
    },
    /// The text parses and every subtag is registered.
    Valid(Tag),
}

fn is_alpha(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphabetic())
}

fn is_digit(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn is_alnum(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn title(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.chars().enumerate() {
        if i == 0 {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// The subtags of a tag, read left to right.
struct Cursor<'a> {
    parts: &'a [&'a str],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.parts.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<&'a str> {
        let part = self.peek()?;
        self.at += 1;
        Some(part)
    }

    fn rest(&self) -> &'a [&'a str] {
        self.parts.get(self.at..).unwrap_or_default()
    }
}

/// The parse in progress: the tag and the subtags the registry lacks.
struct Parsing<'r> {
    registry: &'r Registry,
    tag: Tag,
    unknown: Vec<String>,
}

impl Parsing<'_> {
    fn note(&mut self, kind: Kind, subtag: &str) {
        if self.registry.get(kind, subtag).is_none() {
            self.unknown.push(subtag.to_owned());
        }
    }

    /// `language ["-" extlang]` (RFC 5646 §2.2.1, §2.2.2).
    fn language(&mut self, cursor: &mut Cursor<'_>) -> Result<(), String> {
        let Some(first) = cursor.bump() else {
            return Err(String::from("empty"));
        };
        if !is_alpha(first) || first.len() > 8 || first.len() < 2 {
            return Err(format!("`{first}` is not a language subtag"));
        }
        self.tag.language = Some(first.to_ascii_lowercase());
        if first.len() == 4 {
            // NOTE: a 4-letter primary subtag is reserved for future use (RFC 5646
            // §2.2.1); it parses and is never registered.
            self.unknown.push(first.to_owned());
            return Ok(());
        }
        self.note(Kind::Language, first);
        if first.len() <= 3 {
            let mut extlangs = 0;
            while let Some(next) = cursor.peek() {
                if extlangs == 3 || next.len() != 3 || !is_alpha(next) {
                    break;
                }
                if extlangs == 0 {
                    self.tag.extlang = Some(next.to_ascii_lowercase());
                    self.note(Kind::Extlang, next);
                }
                extlangs += 1;
                cursor.bump();
            }
        }
        Ok(())
    }

    /// `["-" script] ["-" region] *("-" variant)` (RFC 5646 §2.2.3 to §2.2.5).
    fn script_region_variants(&mut self, cursor: &mut Cursor<'_>) {
        if let Some(next) = cursor.peek()
            && next.len() == 4
            && is_alpha(next)
        {
            self.tag.script = Some(title(next));
            self.note(Kind::Script, next);
            cursor.bump();
        }
        if let Some(next) = cursor.peek()
            && ((next.len() == 2 && is_alpha(next)) || (next.len() == 3 && is_digit(next)))
        {
            self.tag.region = Some(next.to_ascii_uppercase());
            self.note(Kind::Region, next);
            cursor.bump();
        }
        while let Some(next) = cursor.peek() {
            let variant = next.len() >= 5
                || (next.len() == 4 && next.as_bytes().first().is_some_and(u8::is_ascii_digit));
            if !variant {
                break;
            }
            self.tag.variants.push(next.to_ascii_lowercase());
            self.note(Kind::Variant, next);
            cursor.bump();
        }
    }

    /// `*("-" extension) ["-" privateuse]` (RFC 5646 §2.2.6, §2.2.7).
    fn extensions_and_private(&mut self, cursor: &mut Cursor<'_>) -> Result<(), String> {
        let mut singletons = BTreeSet::new();
        while let Some(next) = cursor.peek() {
            if next.len() != 1 {
                return Err(format!("unexpected subtag `{next}`"));
            }
            if next.eq_ignore_ascii_case("x") {
                self.tag.private_use = Some(private_use(cursor.rest())?);
                return Ok(());
            }
            let singleton = next.to_ascii_lowercase();
            if !singletons.insert(singleton.clone()) {
                return Err(format!("extension `{singleton}` repeats"));
            }
            cursor.bump();
            let mut sequence = vec![singleton.clone()];
            while let Some(sub) = cursor.peek() {
                if sub.len() < 2 {
                    break;
                }
                sequence.push(sub.to_ascii_lowercase());
                cursor.bump();
            }
            if sequence.len() == 1 {
                return Err(format!("extension `{singleton}` has no subtags"));
            }
            self.tag.extensions.push(sequence.join("-"));
        }
        Ok(())
    }
}

/// Parses `text` against the vendored IANA registry ([`analyze`]).
#[must_use]
pub fn analyze_tag(text: &str) -> Analysis {
    analyze(text, &REGISTRY_DATA)
}

/// Parses `text` against RFC 5646 §2.1 and checks each subtag against
/// `registry` (RFC 5646 §2.2.9).
#[must_use]
pub fn analyze(text: &str, registry: &Registry) -> Analysis {
    let text = text.trim();
    if let Some(record) = registry.get(Kind::Grandfathered, text) {
        return Analysis::Valid(Tag {
            grandfathered: Some(record.subtag.clone()),
            ..Tag::default()
        });
    }
    let parts: Vec<&str> = text.split('-').collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || p.len() > 8 || !is_alnum(p))
    {
        return Analysis::Malformed(String::from(
            "a subtag is empty, longer than 8 characters, or not alphanumeric",
        ));
    }
    if parts.first().is_some_and(|p| p.eq_ignore_ascii_case("x")) {
        return match private_use(&parts) {
            Ok(private) => Analysis::Valid(Tag {
                private_use: Some(private),
                ..Tag::default()
            }),
            Err(reason) => Analysis::Malformed(reason),
        };
    }
    let mut cursor = Cursor {
        parts: &parts,
        at: 0,
    };
    let mut parsing = Parsing {
        registry,
        tag: Tag::default(),
        unknown: Vec::new(),
    };
    if let Err(reason) = parsing.language(&mut cursor) {
        return Analysis::Malformed(reason);
    }
    parsing.script_region_variants(&mut cursor);
    if let Err(reason) = parsing.extensions_and_private(&mut cursor) {
        return Analysis::Malformed(reason);
    }
    if parsing.unknown.is_empty() {
        Analysis::Valid(parsing.tag)
    } else {
        Analysis::WellFormed {
            tag: parsing.tag,
            unknown: parsing.unknown,
        }
    }
}

/// A private-use sequence `x-…` (RFC 5646 §2.2.7): well-formed is valid.
fn private_use(parts: &[&str]) -> Result<String, String> {
    let Some((_, rest)) = parts.split_first() else {
        return Err(String::from("empty"));
    };
    if rest.is_empty() {
        return Err(String::from("`x` needs at least one private-use subtag"));
    }
    Ok(rest
        .iter()
        .map(|p| p.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("-"))
}

/// The subtags a tag is matched by, lower case (RFC 4647 §3.3.2, step 1).
///
/// A grandfathered tag is one opaque subtag: its subtags "either do not
/// individually appear in the registry or appear but with a different semantic
/// meaning", and the tag "in its entirety, represents a language or collection
/// of languages" (RFC 5646 §2.2.8).
fn match_subtags(tag: &Tag) -> Vec<String> {
    if let Some(grandfathered) = &tag.grandfathered {
        return vec![grandfathered.to_ascii_lowercase()];
    }
    tag.canonical()
        .split('-')
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Whether `range` matches `tag` under RFC 4647 §3.3.2 extended filtering.
///
/// Both lists come from [`match_subtags`], so neither carries the wildcard
/// that steps 1 and 3.A admit, and every comparison is already case-folded.
fn extended_filter(range: &[String], tag: &[String]) -> bool {
    let (Some(first_range), Some(first_tag)) = (range.first(), tag.first()) else {
        return false;
    };
    if first_range != first_tag {
        return false;
    }
    let mut at_range = 1;
    let mut at_tag = 1;
    while let Some(wanted) = range.get(at_range) {
        let Some(found) = tag.get(at_tag) else {
            return false;
        };
        if wanted == found {
            at_range += 1;
            at_tag += 1;
        } else if found.len() == 1 {
            return false;
        } else {
            at_tag += 1;
        }
    }
    true
}

/// The subtags one include fixes, of the positions the registry bounds.
#[derive(Debug)]
struct Selection {
    /// The primary language subtag, which an enumerable selection fixes.
    language: String,
    /// The script subtag, when the include fixes one.
    script: Option<String>,
    /// The region subtag, when the include fixes one.
    region: Option<String>,
}

/// The BCP 47 provider.
#[derive(Debug)]
pub struct Bcp47Provider {
    identity: Identity,
    declaration: Declaration,
    interned: Interned,
    registry: &'static Registry,
}

impl Default for Bcp47Provider {
    fn default() -> Self {
        Self::new()
    }
}

impl Bcp47Provider {
    /// The provider over the vendored registry.
    #[must_use]
    pub fn new() -> Self {
        let registry: &'static Registry = &REGISTRY_DATA;
        let code_property = |code: &str, description: &str| PropertyDefinition {
            code: code.to_owned(),
            uri: None,
            description: Some(description.to_owned()),
            kind: PropertyKind::Code,
        };
        let filter = |code: &str, description: &str| FilterDefinition {
            code: code.to_owned(),
            description: Some(description.to_owned()),
            operators: vec![
                FilterOperator::Equal,
                FilterOperator::In,
                FilterOperator::Exists,
            ],
            value: String::from("a subtag"),
        };
        Self {
            identity: Identity {
                url: URL.to_owned(),
                version: registry.file_date.clone(),
                title: Some(String::from("IETF BCP 47 language tags")),
                name: None,
                version_needed: false,
            },
            declaration: Declaration {
                content: ContentMode::NotPresent,
                case_sensitive: false,
                hierarchy_meaning: None,
                // NOTE: RFC 5646 §2.1 defines the subtag grammar of a language
                // tag, and this provider parses it, so the grammar is both
                // defined and supported.
                compositional: Compositional::Supported,
                // NOTE: the registry descriptions this system displays are not
                // "guaranteed to be in any particular language" (RFC 5646
                // §3.1.5), so it has no language to state.
                languages: Vec::new(),
                properties: vec![
                    code_property("language", "The primary language subtag"),
                    code_property("extlang", "The extended language subtag"),
                    code_property("script", "The script subtag"),
                    code_property("region", "The region subtag"),
                    code_property("variant", "A variant subtag"),
                ],
                filters: vec![
                    filter("language", "Tags with this primary language"),
                    filter("extlang", "Tags with this extended language"),
                    filter("script", "Tags with this script"),
                    filter("region", "Tags with this region"),
                    filter("variant", "Tags with this variant"),
                ],
                capabilities: BTreeSet::new(),
                implicit_forms: Vec::new(),
            },
            interned: Interned::new(),
            registry,
        }
    }

    /// What the grammar and the registry say about `text`.
    #[must_use]
    pub fn analyze(&self, text: &str) -> Analysis {
        analyze(text, self.registry)
    }

    fn tag(&self, concept: Concept) -> Option<Tag> {
        let code = self.interned.code(concept)?;
        match analyze(&code, self.registry) {
            Analysis::Valid(tag) => Some(tag),
            _ => None,
        }
    }

    fn description(&self, kind: Kind, subtag: &str) -> Option<String> {
        self.registry
            .get(kind, subtag)
            .and_then(|r| r.descriptions.first().cloned())
    }

    /// What `filters` fix, when they fix the primary language subtag and name
    /// nothing else this system cannot enumerate.
    fn selection(filters: &[Filter]) -> Option<Selection> {
        let mut language = None;
        let mut script = None;
        let mut region = None;
        for filter in filters {
            if filter.op != FilterOperator::Equal {
                return None;
            }
            let slot = match filter.property.as_str() {
                "language" => &mut language,
                "script" => &mut script,
                "region" => &mut region,
                _ => return None,
            };
            if slot.is_some() {
                return None;
            }
            *slot = Some(filter.value.trim().to_owned());
        }
        Some(Selection {
            language: language?,
            script,
            region,
        })
    }

    /// The tags `selection` enumerates, in the order they are listed.
    ///
    /// Every position holds a registered subtag, so an unregistered one leaves
    /// the selection empty rather than failing it.
    fn tags(&self, selection: &Selection) -> Vec<String> {
        let registered = |kind: Kind, subtag: &Option<String>| match subtag {
            None => Some(None),
            Some(value) => self
                .registry
                .get(kind, value)
                .map(|record| Some(record.subtag.clone())),
        };
        let (Some(language), Some(script), Some(region)) = (
            self.registry
                .get(Kind::Language, &selection.language)
                .map(|record| record.subtag.clone()),
            registered(Kind::Script, &selection.script),
            registered(Kind::Region, &selection.region),
        ) else {
            return Vec::new();
        };
        let open = |fixed: Option<String>, kind: Kind, varies: bool| -> Vec<Option<String>> {
            match fixed {
                Some(value) => vec![Some(value)],
                None if varies => std::iter::once(None)
                    .chain(
                        self.registry
                            .of_kind(kind)
                            .into_iter()
                            .map(|record| Some(record.subtag.clone())),
                    )
                    .collect(),
                None => vec![None],
            }
        };
        let vary_region = region.is_none();
        let regions = open(region, Kind::Region, vary_region);
        let scripts = open(script, Kind::Script, !vary_region);
        let mut tags = Vec::with_capacity(scripts.len().saturating_mul(regions.len()));
        for script in &scripts {
            for region in &regions {
                tags.push(
                    Tag {
                        language: Some(language.clone()),
                        script: script.clone(),
                        region: region.clone(),
                        ..Tag::default()
                    }
                    .canonical(),
                );
            }
        }
        tags
    }

    /// The ordinals of the tags `filters` enumerate, or `None` when this
    /// system does not enumerate that selection.
    fn enumerate(&self, filters: &[Filter]) -> Result<Option<Vec<u32>>, ProviderError> {
        let Some(selection) = Self::selection(filters) else {
            return Ok(None);
        };
        let mut out = Vec::new();
        for tag in self.tags(&selection) {
            out.push(self.interned.intern(&tag)?.index());
        }
        Ok(Some(out))
    }

    /// Why `filters` select nothing this system enumerates.
    fn refusal(&self, filters: &[Filter]) -> ProviderError {
        if filters.is_empty() {
            return ProviderError::NotEnumerable;
        }
        for filter in filters {
            if !self
                .declaration
                .filters
                .iter()
                .any(|declared| declared.code == filter.property)
            {
                return ProviderError::UnsupportedFilter {
                    property: filter.property.clone(),
                    operator: filter.op.code().to_owned(),
                };
            }
        }
        ProviderError::FilterNotEnumerable
    }

    fn part<'a>(tag: &'a Tag, property: &str) -> Vec<&'a str> {
        match property {
            "language" => tag.language.iter().map(String::as_str).collect(),
            "extlang" => tag.extlang.iter().map(String::as_str).collect(),
            "script" => tag.script.iter().map(String::as_str).collect(),
            "region" => tag.region.iter().map(String::as_str).collect(),
            "variant" => tag.variants.iter().map(String::as_str).collect(),
            _ => Vec::new(),
        }
    }
}

impl CodeSystemProvider for Bcp47Provider {
    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        match analyze(code, self.registry) {
            Analysis::Valid(tag) => {
                let canonical = tag.canonical();
                let concept = self.interned.intern(&canonical)?;
                Ok(Some(Located {
                    concept,
                    code: canonical,
                }))
            }
            Analysis::Malformed(_) | Analysis::WellFormed { .. } => Ok(None),
        }
    }

    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        Ok(self.interned.code(concept))
    }

    fn display(
        &self,
        concept: Concept,
        _language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        let Some(tag) = self.tag(concept) else {
            return Ok(None);
        };
        if let Some(grandfathered) = &tag.grandfathered {
            return Ok(self.description(Kind::Grandfathered, grandfathered));
        }
        let Some(language) = &tag.language else {
            return Ok(tag
                .private_use
                .as_ref()
                .map(|p| format!("private use x-{p}")));
        };
        let mut text = self
            .description(Kind::Language, language)
            .unwrap_or_else(|| language.clone());
        if let Some(extlang) = &tag.extlang
            && let Some(description) = self.description(Kind::Extlang, extlang)
        {
            text = description;
        }
        let mut qualifiers = Vec::new();
        if let Some(script) = &tag.script {
            qualifiers.extend(self.description(Kind::Script, script));
        }
        if let Some(region) = &tag.region {
            qualifiers.extend(self.description(Kind::Region, region));
        }
        for variant in &tag.variants {
            qualifiers.extend(self.description(Kind::Variant, variant));
        }
        if !qualifiers.is_empty() {
            text.push_str(" (");
            text.push_str(&qualifiers.join(", "));
            text.push(')');
        }
        Ok(Some(text))
    }

    fn status(&self, concept: Concept) -> Result<Status, ProviderError> {
        let Some(tag) = self.tag(concept) else {
            return Ok(Status::default());
        };
        let deprecated = [
            (Kind::Language, tag.language.as_deref()),
            (Kind::Extlang, tag.extlang.as_deref()),
            (Kind::Script, tag.script.as_deref()),
            (Kind::Region, tag.region.as_deref()),
            (Kind::Grandfathered, tag.grandfathered.as_deref()),
        ]
        .into_iter()
        .chain(
            tag.variants
                .iter()
                .map(|v| (Kind::Variant, Some(v.as_str()))),
        )
        .any(|(kind, subtag)| {
            subtag.is_some_and(|s| {
                self.registry
                    .get(kind, s)
                    .is_some_and(|r| r.deprecated.is_some())
            })
        });
        Ok(Status {
            standards_status: None,
            active: !deprecated,
            inactive_reason: deprecated.then(|| String::from("deprecated")),
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
        let Some(tag) = self.tag(concept) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for name in ["language", "extlang", "script", "region", "variant"] {
            for value in Self::part(&tag, name) {
                out.push(Property {
                    code: name.to_owned(),
                    value: PropertyValue::Code(value.to_owned()),
                    ..Property::default()
                });
            }
        }
        Ok(out)
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

    /// A selection of language tags is incomplete: the registry of subtags is
    /// finite, and the tags built from them with variants, extensions, and
    /// private use (RFC 5646 §2.2.6 to §2.2.7) are not.
    fn unclosed(&self, _filters: &[Filter]) -> bool {
        true
    }

    fn unclosed_reason(&self) -> Option<String> {
        Some(format!(
            "The code System '{URL}' has a grammar and so has infinite members"
        ))
    }

    fn filter(&self, filter: &Filter) -> Result<ConceptSet, ProviderError> {
        self.filter_all(std::slice::from_ref(filter))
    }

    /// A fixed primary language bounds a selection of language tags.
    ///
    /// `language = 2*3ALPHA / 4ALPHA / 5*8ALPHA`, `script = 4ALPHA`, and
    /// `region = 2ALPHA / 3DIGIT` (RFC 5646 §2.1) each hold one registered
    /// subtag, so fixing the language leaves a finite list: the tag itself and
    /// one tag per registered subtag at the most specific position the include
    /// left open, the region before the script, which is the shape RFC 5646
    /// §4.1 recommends writing.
    // NOTE: no FHIR or BCP 47 specification says which finite subset of a grammar
    // system an expansion lists: enumerating one open position is our own design.
    fn filter_all(&self, filters: &[Filter]) -> Result<ConceptSet, ProviderError> {
        match self.enumerate(filters)? {
            Some(order) => Ok(order.into_iter().collect()),
            None => Err(self.refusal(filters)),
        }
    }

    /// The tags in registry order, which the interned ordinals do not carry.
    fn filter_ordered(&self, filters: &[Filter]) -> Result<Option<Vec<u32>>, ProviderError> {
        self.enumerate(filters)
    }

    fn filter_matches(&self, concept: Concept, filter: &Filter) -> Result<bool, ProviderError> {
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
        let Some(tag) = self.tag(concept) else {
            return Ok(false);
        };
        let parts = Self::part(&tag, &filter.property);
        Ok(match filter.op {
            FilterOperator::Equal => parts.iter().any(|p| p.eq_ignore_ascii_case(&filter.value)),
            FilterOperator::In => filter
                .value
                .split(',')
                .any(|wanted| parts.iter().any(|p| p.eq_ignore_ascii_case(wanted.trim()))),
            FilterOperator::Exists => {
                let wanted = filter.value.trim().eq_ignore_ascii_case("true");
                wanted != parts.is_empty()
            }
            other => {
                return Err(ProviderError::UnsupportedFilter {
                    property: filter.property.clone(),
                    operator: other.code().to_owned(),
                });
            }
        })
    }

    /// A tag subsumes another when it matches it as a language range under
    /// RFC 4647 §3.3.2 extended filtering, so `en` subsumes `en-US` and
    /// `en-US` subsumes `en-Latn-US`, the range "de-*-DE" and "its synonym
    /// `de-DE`" matching `de-Latn-DE` in that section.
    ///
    /// No FHIR or BCP 47 specification defines subsumption over language
    /// tags: reading each tag as the extended language range of the same
    /// spelling is our own design.
    fn subsumes(&self, a: Concept, b: Concept) -> Result<Option<Outcome>, ProviderError> {
        let (Some(a), Some(b)) = (self.tag(a), self.tag(b)) else {
            return Ok(None);
        };
        let (a, b) = (match_subtags(&a), match_subtags(&b));
        Ok(Some(
            match (extended_filter(&a, &b), extended_filter(&b, &a)) {
                (true, true) => Outcome::Equivalent,
                (true, false) => Outcome::Subsumes,
                (false, true) => Outcome::SubsumedBy,
                (false, false) => Outcome::NotSubsumed,
            },
        ))
    }
}
