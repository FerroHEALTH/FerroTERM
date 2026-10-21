// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The typed feed model: an Atom feed of entries carrying the NCTS extensions.
//!
//! The wire shape is the Atom Syndication Format (RFC 4287,
//! <https://www.rfc-editor.org/rfc/rfc4287>) with the extension namespaces the
//! terminology services publish: the NCTS Atom Syndication Format extensions
//! (`http://ns.electronichealth.net.au/ncts/syndication/asf/extensions/1.0.0`),
//! Ontoserver's own namespace (`http://ontoserver.csiro.au/syndication/`), and
//! SNOMED International's (`http://snomed.info/syndication/sct-extension/1.0.0`).
//! Ontoserver documents the profile and the update semantics at
//! <https://www.ontoserver.csiro.au/docs/6.22.5/syndication.html>.

use core::fmt;

/// The Atom namespace every feed and entry element lives in (RFC 4287 §1.2).
pub const ATOM_NS: &str = "http://www.w3.org/2005/Atom";
/// The NCTS Atom Syndication Format extension namespace.
pub const NCTS_NS: &str = "http://ns.electronichealth.net.au/ncts/syndication/asf/extensions/1.0.0";
/// Ontoserver's own extension namespace.
pub const ONTO_NS: &str = "http://ontoserver.csiro.au/syndication/";
/// SNOMED International's extension namespace, used by the MLDS feed.
pub const SCT_NS: &str = "http://snomed.info/syndication/sct-extension/1.0.0";

/// One syndication feed: the document-level metadata and its entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Feed {
    /// The feed's human-readable title (Atom `feed/title`).
    pub title: Option<String>,
    /// The feed's identifier (Atom `feed/id`).
    pub id: Option<String>,
    /// When the feed itself last changed (Atom `feed/updated`).
    pub updated: Option<jiff::Timestamp>,
    /// The software that produced the feed (Atom `feed/generator`).
    pub generator: Option<String>,
    /// The syndication profile the feed declares
    /// (`ncts:atomSyndicationFormatProfile`).
    pub profile: Option<String>,
    /// The entries, in document order.
    pub entries: Vec<Entry>,
}

/// One feed entry: a single downloadable content item and its metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    /// The entry identifier (Atom `entry/id`), typically a `urn:uuid:`.
    pub id: String,
    /// The entry title (Atom `entry/title`).
    pub title: String,
    /// The entry's category, the kind of content it offers.
    pub category: Option<Category>,
    /// When the entry last changed (Atom `entry/updated`).
    ///
    /// This is the date the replace rule compares.
    pub updated: Option<jiff::Timestamp>,
    /// When the entry was first published (Atom `entry/published`).
    pub published: Option<jiff::Timestamp>,
    /// The entry summary (Atom `entry/summary`).
    pub summary: Option<String>,
    /// The copyright statement (Atom `entry/rights`).
    pub rights: Option<String>,
    /// The canonical identifier of the content item
    /// (`ncts:contentItemIdentifier`).
    pub content_item_identifier: Option<String>,
    /// The version identifier of the content item
    /// (`ncts:contentItemVersion`).
    pub content_item_version: Option<String>,
    /// The FHIR version the content item is expressed in
    /// (`ncts:fhirVersion`).
    pub fhir_version: Option<String>,
    /// How a `FHIR_Bundle` entry is meant to be processed
    /// (`ncts:bundleInterpretation`).
    pub bundle_interpretation: Option<String>,
    /// The permission code Ontoserver attaches to a restricted entry
    /// (`onto:permission/@code`).
    pub permission: Option<String>,
    /// The edition this package depends on
    /// (`sct:packageDependency/sct:editionDependency`).
    pub edition_dependency: Option<String>,
    /// Every `link` of the entry, in document order.
    pub links: Vec<ContentLink>,
}

impl Entry {
    /// Returns the link that carries the entry's content.
    ///
    /// The NCTS profile puts the downloadable item on the `alternate` link; a
    /// `related` link carries companions such as a release-note PDF, which the
    /// MLDS feed publishes beside every package.
    #[must_use]
    pub fn content_link(&self) -> Option<&ContentLink> {
        self.links
            .iter()
            .find(|link| link.rel == LinkRel::Alternate)
    }

    /// Returns the entry's category term, or [`CategoryTerm::Absent`] when the
    /// entry declares no category.
    #[must_use]
    pub fn term(&self) -> CategoryTerm {
        self.category
            .as_ref()
            .map_or(CategoryTerm::Absent, |category| category.term.clone())
    }

    /// Returns the SNOMED CT edition and version the entry names, when its
    /// version identifier is a SNOMED CT version URI.
    #[must_use]
    pub fn snomed_release(&self) -> Option<SnomedRelease> {
        SnomedRelease::from_version_uri(self.content_item_version.as_deref()?)
    }
}

/// An entry's category: the kind of content the entry offers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Category {
    /// The category term, the machine-readable kind.
    pub term: CategoryTerm,
    /// The human-readable label the feed gives the term.
    pub label: Option<String>,
    /// The scheme the term is drawn from.
    pub scheme: Option<String>,
}

/// The kind of content a feed entry offers.
///
/// The named variants are the terms the NCTS scheme and Ontoserver publish; a
/// term outside them is kept verbatim in [`CategoryTerm::Other`] so a new kind
/// is reported rather than dropped.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CategoryTerm {
    /// A SNOMED CT RF2 distribution holding the Snapshot release type.
    SnomedRf2Snapshot,
    /// A SNOMED CT RF2 distribution holding the Delta release type.
    SnomedRf2Delta,
    /// A SNOMED CT RF2 distribution holding the Full release type.
    SnomedRf2Full,
    /// A SNOMED CT RF2 distribution holding every release type.
    SnomedRf2All,
    /// A FHIR `CodeSystem` resource.
    FhirCodeSystem,
    /// A FHIR `ValueSet` resource.
    FhirValueSet,
    /// A FHIR `ConceptMap` resource.
    FhirConceptMap,
    /// A FHIR `Bundle` of terminology resources.
    FhirBundle,
    /// A FHIR package (the NPM-style package format).
    FhirPackage,
    /// Ontoserver's precomputed binary index of a SNOMED CT release.
    BinaryIndex,
    /// A term the model does not name, kept verbatim.
    Other(String),
    /// The entry declared no category at all.
    Absent,
}

impl CategoryTerm {
    /// Every term the model names, in a stable order.
    pub const NAMED: [Self; 10] = [
        Self::SnomedRf2Snapshot,
        Self::SnomedRf2Delta,
        Self::SnomedRf2Full,
        Self::SnomedRf2All,
        Self::FhirCodeSystem,
        Self::FhirValueSet,
        Self::FhirConceptMap,
        Self::FhirBundle,
        Self::FhirPackage,
        Self::BinaryIndex,
    ];

    /// Reads a category term from the `term` attribute of an Atom `category`.
    #[must_use]
    pub fn parse(term: &str) -> Self {
        match term {
            "SCT_RF2_SNAPSHOT" => Self::SnomedRf2Snapshot,
            "SCT_RF2_DELTA" => Self::SnomedRf2Delta,
            "SCT_RF2_FULL" => Self::SnomedRf2Full,
            "SCT_RF2_ALL" => Self::SnomedRf2All,
            "FHIR_CodeSystem" => Self::FhirCodeSystem,
            "FHIR_ValueSet" => Self::FhirValueSet,
            "FHIR_ConceptMap" => Self::FhirConceptMap,
            "FHIR_Bundle" => Self::FhirBundle,
            "FHIR_Package" => Self::FhirPackage,
            "BINARY" => Self::BinaryIndex,
            other => Self::Other(other.to_owned()),
        }
    }

    /// The term as it is written on the wire.
    ///
    /// An entry with no category has no wire form and answers the empty string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::SnomedRf2Snapshot => "SCT_RF2_SNAPSHOT",
            Self::SnomedRf2Delta => "SCT_RF2_DELTA",
            Self::SnomedRf2Full => "SCT_RF2_FULL",
            Self::SnomedRf2All => "SCT_RF2_ALL",
            Self::FhirCodeSystem => "FHIR_CodeSystem",
            Self::FhirValueSet => "FHIR_ValueSet",
            Self::FhirConceptMap => "FHIR_ConceptMap",
            Self::FhirBundle => "FHIR_Bundle",
            Self::FhirPackage => "FHIR_Package",
            Self::BinaryIndex => "BINARY",
            Self::Other(term) => term,
            Self::Absent => "",
        }
    }

    /// Whether the term names a SNOMED CT RF2 distribution.
    #[must_use]
    pub const fn is_rf2(&self) -> bool {
        matches!(
            self,
            Self::SnomedRf2Snapshot
                | Self::SnomedRf2Delta
                | Self::SnomedRf2Full
                | Self::SnomedRf2All
        )
    }
}

impl fmt::Display for CategoryTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The relation an Atom `link` declares (RFC 4287 §4.2.7.2).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum LinkRel {
    /// `rel="alternate"`, the content item itself. The Atom default.
    #[default]
    Alternate,
    /// `rel="related"`, a companion resource such as a release note.
    Related,
    /// `rel="self"`, the feed's own address.
    Itself,
    /// A relation the model does not name, kept verbatim.
    Other(String),
}

impl LinkRel {
    /// Reads a link relation from the `rel` attribute of an Atom `link`.
    ///
    /// RFC 4287 §4.2.7.2 makes `alternate` the value of an absent attribute.
    #[must_use]
    pub fn parse(rel: &str) -> Self {
        match rel {
            "alternate" => Self::Alternate,
            "related" => Self::Related,
            "self" => Self::Itself,
            other => Self::Other(other.to_owned()),
        }
    }

    /// The relation as it is written on the wire.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Alternate => "alternate",
            Self::Related => "related",
            Self::Itself => "self",
            Self::Other(rel) => rel,
        }
    }
}

impl fmt::Display for LinkRel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One `link` of an entry: where the bytes are and how to recognize them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContentLink {
    /// The address the bytes are fetched from (`@href`).
    pub href: String,
    /// The relation the link declares (`@rel`).
    pub rel: LinkRel,
    /// The media type of the linked bytes (`@type`).
    pub media_type: Option<String>,
    /// The size in bytes the feed advertises (`@length`).
    pub length: Option<u64>,
    /// The digest the feed advertises, if any.
    pub checksum: Option<Checksum>,
    /// Whether the publisher marked the resource as already validated
    /// (`onto:validated`).
    pub validated: bool,
}

/// A digest a feed advertises for a content item.
///
/// The Ontoserver-based services publish `ncts:sha256Hash`; the MLDS feed
/// publishes `sct:md5Hash`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Checksum {
    /// A SHA-256 digest, lowercase hexadecimal.
    Sha256(String),
    /// An MD5 digest, lowercase hexadecimal.
    Md5(String),
}

impl Checksum {
    /// The algorithm's name, for a log line or a run record.
    #[must_use]
    pub const fn algorithm(&self) -> &'static str {
        match self {
            Self::Sha256(_) => "sha256",
            Self::Md5(_) => "md5",
        }
    }

    /// The expected digest, lowercase hexadecimal.
    #[must_use]
    pub fn hex(&self) -> &str {
        match self {
            Self::Sha256(hex) | Self::Md5(hex) => hex,
        }
    }

    /// Whether `computed` is this checksum's digest, compared case-insensitively.
    #[must_use]
    pub fn matches(&self, computed: &str) -> bool {
        self.hex().eq_ignore_ascii_case(computed)
    }
}

impl fmt::Debug for Checksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.algorithm(), self.hex())
    }
}

impl fmt::Display for Checksum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.algorithm(), self.hex())
    }
}

/// The edition and version a SNOMED CT version URI names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnomedRelease {
    /// The edition's module identifier, the SCTID in the URI.
    pub edition: String,
    /// The release version, the `YYYYMMDD` date in the URI.
    pub version: String,
}

impl SnomedRelease {
    /// Reads an edition and version from a SNOMED CT version URI.
    ///
    /// The form is `http://snomed.info/sct/{sctid}/version/{YYYYMMDD}`, which
    /// the SNOMED CT URI Standard defines as the version URI of an edition
    /// (<https://confluence.ihtsdotools.org/display/DOCURI>). A URI in any
    /// other shape answers `None`.
    #[must_use]
    pub fn from_version_uri(uri: &str) -> Option<Self> {
        let rest = uri.strip_prefix("http://snomed.info/sct/")?;
        let (edition, version) = rest.split_once("/version/")?;
        let digits = |value: &str| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
        if !digits(edition) || !digits(version) {
            return None;
        }
        Some(Self {
            edition: edition.to_owned(),
            version: version.to_owned(),
        })
    }
}

impl fmt::Display for SnomedRelease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "http://snomed.info/sct/{}/version/{}",
            self.edition, self.version
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{CategoryTerm, Checksum, LinkRel, SnomedRelease};

    #[test]
    fn every_named_term_round_trips_through_its_wire_form() {
        for term in CategoryTerm::NAMED {
            assert_eq!(
                CategoryTerm::parse(term.as_str()),
                term,
                "the wire form of {term:?} must parse back to it"
            );
        }
    }

    #[test]
    fn an_unnamed_term_is_kept_verbatim() {
        assert_eq!(
            CategoryTerm::parse("FHIR_StructureDefinition"),
            CategoryTerm::Other("FHIR_StructureDefinition".to_owned())
        );
    }

    #[test]
    fn an_unnamed_relation_is_kept_verbatim() {
        assert_eq!(
            LinkRel::parse("enclosure"),
            LinkRel::Other("enclosure".to_owned())
        );
    }

    #[test]
    fn a_digest_compares_case_insensitively() {
        let checksum = Checksum::Md5("ABCDEF".to_owned());
        assert!(checksum.matches("abcdef"), "case must not matter");
        assert!(
            !checksum.matches("abcdee"),
            "a different digest must not match"
        );
    }

    #[test]
    fn a_version_uri_yields_its_edition_and_version() {
        let release =
            SnomedRelease::from_version_uri("http://snomed.info/sct/11000001107/version/20260101");
        assert_eq!(
            release,
            Some(SnomedRelease {
                edition: "11000001107".to_owned(),
                version: "20260101".to_owned(),
            })
        );
    }

    #[test]
    fn a_uri_in_another_shape_yields_nothing() {
        assert_eq!(
            SnomedRelease::from_version_uri("http://example.invalid/ValueSet/one"),
            None
        );
        assert_eq!(
            SnomedRelease::from_version_uri("http://snomed.info/sct/11000001107"),
            None
        );
    }
}
