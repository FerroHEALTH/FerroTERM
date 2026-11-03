// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SNOMED CT provider: one built edition version read from its artifact
//! directory (`store.redb`, `hierarchy.bin`, `text.bin`, and `manifest.json`).
//!
//! Identity is the SNOMED CT URI standard: the system `http://snomed.info/sct`
//! and the edition version URI as `version`
//! (<https://hl7.org/fhir/R4B/snomedct.html>). Display is the preferred term of
//! a language reference set for the requested language; the FHIR-defined
//! properties `inactive`, `sufficientlyDefined`, `moduleId`, `parent`, `child`,
//! and every concept-model attribute keyed by its concept id come from the
//! store. Reads are point reads and bitmap lookups; nothing walks the graph
//! per request.

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

mod conceptmap;
mod ecl;
pub mod expression;

use concept_graph::attributes::{Attributes, AttributesError};
use concept_graph::identifiers::{Identifiers, IdentifiersError};
use concept_graph::members::{MembersError, Memberships};
use concept_graph::ordinal::Ordinal;
use concept_graph::refsets::{RefsetMembers, RefsetsError};
use concept_store::record;
use concept_store::store::{Store, StoreError, Vocabulary};
use rf2::constants;
use rf2::id::ConceptId;
use roaring::RoaringBitmap;
use sct_ecl::ast::ExpressionConstraint;
use serde::Deserialize;

use crate::artifact::Source;
use crate::compose::{Compose, ConceptRef, Include, SystemRef};
use crate::filter::{Filter, FilterOperator};
use crate::normal_form;
use crate::provider::{
    Capability, CodeSystemProvider, Compositional, Concept, ConceptSet, ContentMode, Declaration,
    DesignationUse, FilterDefinition, HierarchyMeaning, Identity, ImplicitArgument, ImplicitForm,
    InvalidValue, Located, MalformedValueSet, Property, PropertyDefinition, PropertyKind,
    PropertyValue, ProviderError, Status,
};
use crate::stored::artifact::{Artifact, Layout};
use crate::stored::system::{StoredProvider, StoredSystem};
use crate::stored::{self, primary_subtag, storage};

/// The SNOMED CT system URI.
pub const SYSTEM: &str = "http://snomed.info/sct";

/// The query key of an implicit value set URI
/// (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets").
const FHIR_VS: &str = "fhir_vs";

/// The query key of an implicit concept map URI
/// (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Concept Maps").
const FHIR_CM: &str = "fhir_cm";

/// The characters SNOMED CT Compositional Grammar spends on the structure of
/// an expression: the definition status prefix (`===`, `<<<`), the `+` between
/// focus concepts, the `:` before a refinement, the `=` of an attribute, the
/// braces of an attribute group, the `,` between attributes, and the `|` of a
/// term (<http://snomed.org/scg>). A concept reference alone carries none of
/// them, so their presence is what separates an expression from an SCTID.
const SCG_OPERATORS: [char; 8] = ['+', ':', '=', '{', '}', ',', '|', '<'];

/// The filter property that says whether post-coordination is allowed
/// (<https://hl7.org/fhir/R4B/snomedct.html>, "Filter Properties").
const EXPRESSIONS: &str = "expressions";

/// The copyright notice every implicit value set template of the FHIR SNOMED
/// CT page carries, verbatim (<https://hl7.org/fhir/R4B/snomedct.html>,
/// "Implicit Value Sets").
pub const TEMPLATE_COPYRIGHT: &str = "This value set includes content from SNOMED CT, which is copyright \u{a9} 2002+ International Health Terminology Standards Development Organisation (SNOMED International), and distributed by agreement between SNOMED International and HL7. Implementer use of SNOMED CT is not covered by this agreement";

/// The SNOMED CT properties the FHIR SNOMED CT page defines that this server
/// does not generate (<https://hl7.org/fhir/R4B/snomedct.html>, "SNOMED CT
/// Properties").
///
/// Every one the page defines is served, so a `property` this server refuses
/// is one no SNOMED CT specification defines.
pub const UNSERVED_PROPERTIES: [&str; 0] = [];

/// The property carrying the normal form with terms.
const NORMAL_FORM: &str = "normalForm";

/// The property carrying it with concept ids only.
const NORMAL_FORM_TERSE: &str = "normalFormTerse";

/// The two the provider generates rather than reads
/// (<https://hl7.org/fhir/R4B/snomedct.html>, "SNOMED CT Properties").
const GENERATED_PROPERTIES: [&str; 2] = [NORMAL_FORM, NORMAL_FORM_TERSE];

/// The FHIR-defined SNOMED properties this provider serves, in output order
/// (<https://hl7.org/fhir/R4B/snomedct.html>, the properties section).
pub const FHIR_PROPERTIES: [(&str, PropertyKind); 6] = [
    ("inactive", PropertyKind::Boolean),
    ("sufficientlyDefined", PropertyKind::Boolean),
    ("moduleId", PropertyKind::Code),
    ("effectiveTime", PropertyKind::String),
    ("parent", PropertyKind::Code),
    ("child", PropertyKind::Code),
];

/// A failure to open an artifact directory.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// The manifest names another system.
    #[error("the artifact is for `{0}`, not SNOMED CT")]
    NotSnomed(String),
    /// The artifact's shared files do not read.
    #[error(transparent)]
    Artifact(#[from] stored::OpenError),
    /// The store cannot be read.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A vocabulary entry the provider relies on is missing.
    #[error("the store's {vocabulary} vocabulary has no `{name}`")]
    MissingVocabulary {
        /// Which vocabulary.
        vocabulary: &'static str,
        /// The missing entry.
        name: String,
    },
    /// The reference set memberships file does not read.
    #[error("cannot read the reference set memberships")]
    Members(#[from] MembersError),
    /// The attribute relationships file does not read.
    #[error("cannot read the attribute relationships")]
    Attributes(#[from] AttributesError),
    /// The reference set member rows file does not read.
    #[error("cannot read the reference set member rows")]
    Refsets(#[from] RefsetsError),
    /// The alternate identifiers file does not read.
    #[error("cannot read the alternate identifiers")]
    Identifiers(#[from] IdentifiersError),
}

#[derive(Debug, Deserialize)]
struct Manifest {
    system: String,
    edition: String,
    version: String,
    /// The release date the offline build recorded for the edition.
    #[serde(default, rename = "releaseDate")]
    release_date: Option<String>,
    /// The reference set memberships; an artifact built before they were
    /// written has none.
    #[serde(default)]
    refsets: Option<String>,
    /// The attribute relationships, the reference set member rows, and the
    /// alternate identifiers the ECL evaluator reads; an artifact built before
    /// they were written has none.
    #[serde(default)]
    attributes: Option<String>,
    #[serde(default)]
    members: Option<String>,
    #[serde(default)]
    identifiers: Option<String>,
    #[serde(default)]
    languages: Vec<String>,
}

/// The vocabulary ordinals the provider resolves once at open.
#[derive(Debug, Clone)]
struct Keys {
    definition_status: u32,
    module: u32,
    /// Attribute type properties: (key ordinal, SCTID as text), sorted by ordinal.
    attributes: Vec<(u32, String)>,
    fsn: u32,
    synonym: u32,
    definition: u32,
    /// Language reference sets: (ordinal, SCTID as text), sorted by ordinal.
    refsets: Vec<(u32, String)>,
}

/// What SNOMED CT decides: the reference sets, the concept model, the
/// expression constraints and expressions, and the edition's version rules.
pub struct Snomed {
    memberships: Memberships,
    attributes: Attributes,
    member_tables: RefsetMembers,
    identifiers: Identifiers,
    roots: OnceLock<RoaringBitmap>,
    leaves: OnceLock<RoaringBitmap>,
    defined: OnceLock<RoaringBitmap>,
    /// Parsed expression constraints by their text, bounded.
    expressions: Mutex<HashMap<String, Arc<ExpressionConstraint>>>,
    /// The post-coordinated expressions this edition has resolved, bounded.
    scg: Mutex<expression::Expressions>,
    /// The store ordinal of each attribute type, by its index in the attribute
    /// table; read once on the first subsumption over an expression.
    attribute_ordinals: OnceLock<Vec<Option<u32>>>,
    /// The inactive concepts, read once on the first request that needs them.
    inactive: OnceLock<ConceptSet>,
    keys: Keys,
    edition: String,
    base_language: String,
}

impl std::fmt::Debug for Snomed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Snomed")
            .field("edition", &self.edition)
            .field("base_language", &self.base_language)
            .finish_non_exhaustive()
    }
}

/// One SNOMED CT edition version behind the seam.
pub type SnomedProvider = StoredProvider<Snomed>;

/// What each structure of a served edition holds in memory, in bytes.
///
/// No FHIR or SNOMED CT specification governs this: our own design, and the
/// figures are what the structures count rather than what a process reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footprint {
    /// The resident columns of the store, by column name.
    pub store_columns: [(&'static str, usize); 4],
    /// The child-to-parent adjacency.
    pub is_a: usize,
    /// The ancestor and descendant bitmaps.
    pub closure: usize,
    /// The parent-to-child adjacency, transposed when the edition opens.
    pub children: usize,
    /// The designation dictionary and its postings.
    pub text: usize,
    /// The reference set member tables.
    pub member_tables: usize,
    /// The attribute rows.
    pub attributes: usize,
    /// The attribute inverted index, derived when the edition opens.
    pub attributes_inverted: usize,
    /// The reference set membership bitmaps.
    pub memberships: usize,
    /// The alternate identifier table.
    pub identifiers: usize,
}

impl SnomedProvider {
    /// Opens the artifact directory `dir`; `default_language` is the BCP 47
    /// tag used when a request names none, when the edition carries it.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError`] when the manifest, the store, or a side file does not
    /// read, or the artifact is not a SNOMED CT edition.
    pub fn open(dir: &Path, default_language: &str) -> Result<Self, OpenError> {
        let (artifact, manifest) = Artifact::open(dir, Layout::Tree, |manifest: &Manifest| {
            stored::serves(&manifest.system, SYSTEM, OpenError::NotSnomed)
        })?;
        let memberships = match &manifest.refsets {
            Some(name) => Memberships::read_from(&mut stored::side_file(dir, name)?)?,
            None => Memberships::default(),
        };
        let (attributes, member_tables, identifiers) = Self::read_ecl_files(dir, &manifest)?;
        let keys = Self::resolve_keys(&artifact.store)?;
        let base_language = Self::base_language(default_language, &manifest.languages);
        // NOTE: no FHIR/SNOMED spec governs this: our own design, and the
        // release date is the edition identifier the build records.
        let source = Source::new(
            dir,
            manifest
                .release_date
                .as_deref()
                .unwrap_or(&manifest.version),
        );
        let mut properties: Vec<PropertyDefinition> = FHIR_PROPERTIES
            .iter()
            .map(|(code, kind)| PropertyDefinition {
                code: (*code).to_owned(),
                uri: None,
                description: None,
                kind: *kind,
            })
            .collect();
        // The two generated properties are declared so a client can discover
        // them, and are answered only where a request names one.
        properties.extend(GENERATED_PROPERTIES.map(|code| PropertyDefinition {
            code: code.to_owned(),
            uri: None,
            description: None,
            kind: PropertyKind::String,
        }));
        properties.extend(keys.attributes.iter().map(|(_, sctid)| PropertyDefinition {
            code: sctid.clone(),
            uri: Some(format!("http://snomed.info/id/{sctid}")),
            description: None,
            kind: PropertyKind::Code,
        }));
        let identity = Identity {
            url: SYSTEM.to_owned(),
            version: manifest.version,
            title: Some(String::from("SNOMED CT")),
            name: None,
            // NOTE: the canonical R4B CodeSystem for SNOMED CT declares
            // versionNeeded = false (<https://hl7.org/fhir/R4B/snomedct.html>).
            version_needed: false,
        };
        let declaration = Declaration {
            content: ContentMode::NotPresent,
            // NOTE: caseSensitive = false per the canonical R4B CodeSystem;
            // SNOMED codes are digits, so the flag never changes a lookup.
            case_sensitive: false,
            hierarchy_meaning: Some(HierarchyMeaning::IsA),
            // NOTE: SNOMED CT defines Compositional Grammar
            // (<http://snomed.org/scg>) and this server evaluates it, so
            // an expression is a code this version locates.
            compositional: Compositional::Supported,
            languages: manifest.languages,
            properties,
            // NOTE: the FHIR SNOMED CT page defines `concept is-a` and `concept in`
            // (<https://hl7.org/fhir/R4B/snomedct.html>, "Filter Properties").
            filters: Self::filter_definitions(),
            capabilities: BTreeSet::from([
                Capability::Subsumption,
                Capability::Enumeration,
                Capability::ImplicitConceptMaps,
            ]),
            implicit_forms: implicit_forms(),
        };
        Ok(Self::new(
            identity,
            declaration,
            source,
            artifact,
            Snomed {
                memberships,
                attributes,
                member_tables,
                identifiers,
                roots: OnceLock::new(),
                leaves: OnceLock::new(),
                defined: OnceLock::new(),
                expressions: Mutex::new(HashMap::new()),
                scg: Mutex::new(expression::Expressions::default()),
                attribute_ordinals: OnceLock::new(),
                inactive: OnceLock::new(),
                keys,
                edition: manifest.edition,
                base_language,
            },
        ))
    }

    /// The edition URI (`http://snomed.info/sct/{module}`), the version URI
    /// without its date.
    #[must_use]
    pub fn edition_uri(&self) -> &str {
        &self.system().edition
    }

    /// What every structure of this edition holds in memory.
    ///
    /// Each figure is the structure's own count of the heap its allocations
    /// hold, so a served edition's memory is accounted for structure by
    /// structure. The caches an operation fills later
    /// (the root, leaf, defined, and inactive sets, and the parsed expression
    /// constraints) are not here: they are empty until a request asks for one.
    #[must_use]
    pub fn footprint(&self) -> Footprint {
        Footprint {
            store_columns: self.stored().store.column_sizes(),
            is_a: self.stored().tree.graph.is_a.size_in_bytes(),
            closure: self.stored().tree.graph.closure.size_in_bytes(),
            children: self.stored().tree.children.size_in_bytes(),
            text: self.stored().text.size_in_bytes(),
            member_tables: self.system().member_tables.size_in_bytes(),
            attributes: self.system().attributes.rows_size_in_bytes(),
            attributes_inverted: self.system().attributes.inverted_size_in_bytes(),
            memberships: self.system().memberships.size_in_bytes(),
            identifiers: self.system().identifiers.size_in_bytes(),
        }
    }

    fn resolve_keys(store: &Store) -> Result<Keys, OpenError> {
        let key = |vocabulary: Vocabulary, what: &'static str, name: &str| {
            store.vocabulary_ordinal(vocabulary, name)?.ok_or_else(|| {
                OpenError::MissingVocabulary {
                    vocabulary: what,
                    name: name.to_owned(),
                }
            })
        };
        // The fixed keys the build writes before the attribute types; `parent`
        // is answered from the hierarchy, not the stored property.
        let fixed = ["parent", "definitionStatus", "module"];
        let definition_status = key(Vocabulary::PropertyKeys, "property key", "definitionStatus")?;
        let module = key(Vocabulary::PropertyKeys, "property key", "module")?;
        let mut attributes = Vec::new();
        let mut ordinal = 0_u32;
        while let Some(name) = store.vocabulary(Vocabulary::PropertyKeys, ordinal)? {
            if !fixed.contains(&name.as_str()) {
                attributes.push((ordinal, name));
            }
            ordinal = ordinal.saturating_add(1);
        }
        let fsn = key(
            Vocabulary::DesignationUses,
            "designation use",
            &constants::FULLY_SPECIFIED_NAME.to_string(),
        )?;
        let synonym = key(
            Vocabulary::DesignationUses,
            "designation use",
            &constants::SYNONYM.to_string(),
        )?;
        let definition = key(
            Vocabulary::DesignationUses,
            "designation use",
            &constants::DEFINITION.to_string(),
        )?;
        let mut refsets = Vec::new();
        let mut ordinal = 0_u32;
        while let Some(name) = store.vocabulary(Vocabulary::LanguageRefsets, ordinal)? {
            refsets.push((ordinal, name));
            ordinal = ordinal.saturating_add(1);
        }
        Ok(Keys {
            definition_status,
            module,
            attributes,
            fsn,
            synonym,
            definition,
            refsets,
        })
    }

    /// The filters of the FHIR SNOMED CT page
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "Filter Properties").
    fn filter_definitions() -> Vec<FilterDefinition> {
        vec![
            FilterDefinition {
                code: String::from("concept"),
                description: Some(String::from(
                    "`is-a`: the concept and its descendants; `descendent-of`: its descendants; `in`: the members of the reference set",
                )),
                operators: vec![
                    FilterOperator::IsA,
                    FilterOperator::DescendentOf,
                    FilterOperator::In,
                ],
                value: String::from("an SCTID"),
            },
            FilterDefinition {
                code: String::from("constraint"),
                description: Some(String::from(
                    "the concepts the SNOMED CT expression constraint selects",
                )),
                operators: vec![FilterOperator::Equal],
                value: String::from("an ECL expression"),
            },
            FilterDefinition {
                code: String::from(EXPRESSIONS),
                description: Some(String::from(
                    "whether post-coordinated expressions are permitted",
                )),
                operators: vec![FilterOperator::Equal],
                value: String::from("true or false"),
            },
        ]
    }

    /// The files the ECL evaluator reads, when the artifact has them.
    fn read_ecl_files(
        dir: &Path,
        manifest: &Manifest,
    ) -> Result<(Attributes, RefsetMembers, Identifiers), OpenError> {
        let read = |name: &str| stored::side_file(dir, name);
        let attributes = match &manifest.attributes {
            Some(name) => Attributes::read_from(&mut read(name)?)?,
            None => Attributes::default(),
        };
        let member_tables = match &manifest.members {
            Some(name) => RefsetMembers::read_from(&mut read(name)?)?,
            None => RefsetMembers::default(),
        };
        let identifiers = match &manifest.identifiers {
            Some(name) => Identifiers::read_from(&mut read(name)?)?,
            None => Identifiers::default(),
        };
        Ok((attributes, member_tables, identifiers))
    }

    /// The attribute relationships of the edition, with their role groups.
    #[expect(
        clippy::same_name_method,
        reason = "the ECL evaluator's trait exposes the same graph part; this is the crate's own accessor"
    )]
    #[must_use]
    pub fn attributes(&self) -> &Attributes {
        &self.system().attributes
    }

    /// The active reference set member rows of the edition, with their fields.
    #[must_use]
    pub fn member_tables(&self) -> &RefsetMembers {
        &self.system().member_tables
    }

    /// The alternate identifiers of the edition.
    #[expect(
        clippy::same_name_method,
        reason = "the ECL evaluator's trait exposes the same graph part; this is the crate's own accessor"
    )]
    #[must_use]
    pub fn identifiers(&self) -> &Identifiers {
        &self.system().identifiers
    }

    /// The language reference sets of the edition, as SCTIDs.
    #[must_use]
    pub fn language_refsets(&self) -> Vec<&str> {
        self.system()
            .keys
            .refsets
            .iter()
            .map(|(_, s)| s.as_str())
            .collect()
    }

    fn ordinal(concept: Concept) -> Ordinal {
        Ordinal::new(concept.index())
    }

    /// The language this edition's own displays are in: the configured
    /// `default_language` when the edition carries descriptions in it, else
    /// the first language the edition declares.
    ///
    /// No FHIR/SNOMED spec governs this: our own design. An edition that
    /// carries no description in the configured tag would otherwise answer a
    /// display in a language it never states.
    fn base_language(default_language: &str, declared: &[String]) -> String {
        let wanted = primary_subtag(default_language);
        if declared.is_empty() || declared.iter().any(|l| primary_subtag(l) == wanted) {
            return wanted;
        }
        declared
            .first()
            .map_or(wanted, |first| primary_subtag(first))
    }

    /// The preferred synonym of `concept` in `language`, by the first language
    /// reference set (in store order) whose preferred synonym is in that
    /// language.
    fn preferred_in(
        &self,
        ordinal: Ordinal,
        language: &str,
    ) -> Result<Option<String>, ProviderError> {
        let refsets = self.system().keys.refsets.iter().map(|(refset, _)| *refset);
        self.stored()
            .store
            .display(ordinal, refsets, |found| primary_subtag(found) == language)
            .map_err(storage)
    }

    /// The display for `language` (or the base language), by the SNOMED rule:
    /// the preferred term of the language reference set; then, our own fallback
    /// order (no spec governs it): an active synonym in the language, the
    /// preferred term in the base language, the FSN, any designation.
    fn choose_display(
        &self,
        ordinal: Ordinal,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        let wanted = language.map_or_else(|| self.system().base_language.clone(), primary_subtag);
        if let Some(term) = self.preferred_in(ordinal, &wanted)? {
            return Ok(Some(term));
        }
        let designations = self.stored().store.designations(ordinal).map_err(storage)?;
        if let Some(synonym) = designations.iter().find(|d| {
            d.active
                && d.use_ordinal == self.system().keys.synonym
                && primary_subtag(&d.language) == wanted
        }) {
            return Ok(Some(synonym.term.clone()));
        }
        if wanted != self.system().base_language
            && let Some(term) = self.preferred_in(ordinal, &self.system().base_language)?
        {
            return Ok(Some(term));
        }
        if let Some(fsn) = designations
            .iter()
            .find(|d| d.active && d.use_ordinal == self.system().keys.fsn)
        {
            return Ok(Some(fsn.term.clone()));
        }
        Ok(designations.first().map(|d| d.term.clone()))
    }

    /// The concept named by `text`, as the store spells it.
    fn sctid_of(&self, url: &str, text: &str) -> Result<String, ProviderError> {
        match self.locate(text)? {
            Some(located) => Ok(located.code),
            None => Err(match ConceptId::parse(text) {
                Ok(_) => ProviderError::UnknownCode(text.to_owned()),
                Err(_) => ProviderError::malformed_implicit_value_set(
                    url,
                    format!("`{text}` is not an SCTID"),
                ),
            }),
        }
    }

    /// The preferred term of `code` in the server's default language, else
    /// `code` itself: the templates' "[sctid or preferred description]"
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets").
    fn term_or_code(&self, code: &str) -> String {
        self.locate(code)
            .ok()
            .flatten()
            .and_then(|located| {
                self.choose_display(Self::ordinal(located.concept), None)
                    .ok()
                    .flatten()
            })
            .unwrap_or_else(|| code.to_owned())
    }

    /// The `version` an implicit URI on `base` resolves to: `None` for the
    /// bare system URI, this edition's version URI for its own edition or
    /// version base.
    ///
    /// A base naming another edition is
    /// [`ProviderError::UnservedImplicitVersion`], never a malformed URI: the
    /// FHIR SNOMED CT page admits any edition version as the base
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets"), so
    /// the caller asks the other loaded editions before the server answers
    /// that it holds no such version.
    pub(crate) fn implicit_version(&self, base: &str) -> Result<Option<String>, ProviderError> {
        if base == SYSTEM {
            Ok(None)
        } else if base == self.system().edition || base == self.identity().version {
            Ok(Some(self.identity().version.clone()))
        } else {
            Err(ProviderError::UnservedImplicitVersion {
                url: SYSTEM.to_owned(),
                version: base.to_owned(),
            })
        }
    }

    /// The filter behind an `isa/[sctid]` or `refset/[sctid]` form of `url`.
    fn implicit_filter(&self, url: &str, form: &str) -> Result<Filter, ProviderError> {
        let malformed = |reason: String| ProviderError::malformed_implicit_value_set(url, reason);
        let (kind, argument) = form.split_once('/').unwrap_or((form, ""));
        match kind {
            "isa" => Ok(Filter {
                property: String::from("concept"),
                op: FilterOperator::IsA,
                value: self.sctid_of(url, argument)?,
            }),
            "refset" => {
                let refset = self.sctid_of(url, argument)?;
                if ConceptId::parse(&refset)
                    .ok()
                    .and_then(|id| self.system().memberships.members(id.value()))
                    .is_none()
                {
                    // NOTE: a language reference set references descriptions, so
                    // "all concept ids in the specified reference set" selects none
                    // (<https://hl7.org/fhir/R4B/snomedct.html>).
                    if self
                        .system()
                        .keys
                        .refsets
                        .iter()
                        .any(|(_, id)| *id == refset)
                    {
                        return Err(ProviderError::invalid_filter_value(
                            "concept",
                            refset,
                            "a language reference set references descriptions, not concepts",
                        ));
                    }
                    return Err(ProviderError::UnknownCode(refset));
                }
                Ok(Filter {
                    property: String::from("concept"),
                    op: FilterOperator::In,
                    value: refset,
                })
            }
            "ecl" => {
                let text = percent_decode(argument).ok_or_else(|| {
                    malformed(String::from("the expression constraint is not URI-encoded"))
                })?;
                self.expression(&text).map_err(|error| {
                    ProviderError::MalformedImplicitValueSet(Box::new(MalformedValueSet {
                        url: url.to_owned(),
                        reason: error.to_string(),
                        position: url_offset(url, argument, error.offset()),
                        source: Some(error),
                    }))
                })?;
                Ok(Filter {
                    property: String::from("constraint"),
                    op: FilterOperator::Equal,
                    value: text,
                })
            }
            _ => Err(malformed(format!("`{form}` is not a `fhir_vs` form"))),
        }
    }

    /// The parsed form of an expression constraint, from the cache when it
    /// was parsed before (the cache is cleared when it holds 256 entries).
    fn expression(
        &self,
        text: &str,
    ) -> Result<Arc<ExpressionConstraint>, sct_scg::front::ParseError> {
        if let Ok(cache) = self.system().expressions.lock()
            && let Some(parsed) = cache.get(text)
        {
            return Ok(Arc::clone(parsed));
        }
        let parsed = Arc::new(sct_ecl::parse(text)?);
        if let Ok(mut cache) = self.system().expressions.lock() {
            if cache.len() >= 256 {
                cache.clear();
            }
            cache.insert(text.to_owned(), Arc::clone(&parsed));
        }
        Ok(parsed)
    }

    /// The concepts an expression constraint selects; malformed text is an
    /// invalid filter value with the parser's position, an identifier the
    /// edition lacks an invalid code.
    fn constraint(&self, text: &str) -> Result<ConceptSet, ProviderError> {
        let parsed = self.expression(text).map_err(|error| {
            ProviderError::InvalidFilterValue(Box::new(InvalidValue {
                property: String::from("constraint"),
                value: text.to_owned(),
                reason: error.to_string(),
                position: Some(error.offset()),
                source: Some(error),
            }))
        })?;
        sct_ecl::eval::evaluate(self, &parsed).map_err(|error| ecl::refusal(error, text))
    }

    /// Whether the concept is sufficiently defined by its conditions.
    ///
    /// The RF2 `definitionStatusId` decides it, which is the same field the
    /// `sufficientlyDefined` property is derived from
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "SNOMED CT Properties").
    fn is_defined(&self, ordinal: Ordinal) -> Result<bool, ProviderError> {
        let stored = self.stored().store.properties(ordinal).map_err(storage)?;
        Ok(stored
            .iter()
            .filter(|(key, _)| *key == self.system().keys.definition_status)
            .any(|(_, values)| {
                values.iter().any(|value| {
                    matches!(value, record::PropertyValue::Code(code) if *code == constants::DEFINED.to_string())
                })
            }))
    }

    /// The Necessary Normal Form of `concept`, with terms when `terms` asks.
    ///
    /// The expression is the concept's inferred view rendered, which the
    /// release already holds: the focus concepts are its `is a` parents and
    /// the attributes are its other inferred rows, so this is two point reads
    /// and no walk (`crate::normal_form`).
    ///
    /// Returns `None` for a concept with no inferred parent, which is the root
    /// of the hierarchy and nothing else.
    fn normal_form(&self, concept: Concept, terms: bool) -> Result<Option<String>, ProviderError> {
        let ordinal = Self::ordinal(concept);
        // NOTE: the term is the display this server would answer with, which
        // the grammar admits ("the term from any SNOMED CT description that is
        // associated with the concept", the Compositional Grammar §5).
        let language = if terms {
            expression::Terms::In(None)
        } else {
            expression::Terms::None
        };
        let parents = self.stored().tree.graph.is_a.neighbours(ordinal);
        Ok(normal_form::Expression {
            defined: self.is_defined(ordinal)?,
            focus: self.references(parents.iter().copied(), language)?,
            attributes: self.inferred_attributes(ordinal, 0, language)?,
        }
        .render())
    }
}

impl SnomedProvider {
    /// The inactive concepts, scanned once from the store and kept.
    fn inactive_set(&self) -> Result<ConceptSet, ProviderError> {
        if let Some(set) = self.system().inactive.get() {
            return Ok(set.clone());
        }
        let mut set = ConceptSet::new();
        for index in 0..self.stored().concepts {
            if let Some(record) = self
                .stored()
                .store
                .concept(Ordinal::new(index))
                .map_err(storage)?
                && !record.active
            {
                set.insert(index);
            }
        }
        Ok(self.system().inactive.get_or_init(|| set).clone())
    }

    /// The members of the reference sets a `concept in` filter names.
    fn reference_set_members(&self, filter: &Filter) -> Result<ConceptSet, ProviderError> {
        let mut set = ConceptSet::new();
        for value in filter
            .value
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            // The error names the value and the reason; the id error adds nothing.
            let Ok(refset) = ConceptId::parse(value) else {
                return Err(ProviderError::invalid_filter_value(
                    &filter.property,
                    value,
                    "not an SCTID",
                ));
            };
            let members = self
                .system()
                .memberships
                .members(refset.value())
                .ok_or_else(|| ProviderError::UnknownCode(value.to_owned()))?;
            set |= members;
        }
        Ok(set)
    }

    /// Whether the expression `record` passes `filter`: through its focus
    /// concepts, which subsume it.
    fn expression_passes(
        &self,
        record: &expression::Registered,
        filter: &Filter,
    ) -> Result<bool, ProviderError> {
        if filter.op == FilterOperator::Equal && filter.property == EXPRESSIONS {
            return Ok(filter.value.trim() == "true");
        }
        let selected = self.filter(filter)?;
        Ok(record
            .focus_concepts()
            .iter()
            .all(|focus| selected.contains(*focus)))
    }
}

impl StoredSystem for Snomed {
    // NOTE: the edition URI names a distribution, and the service "may default to
    // the most recent version of the named SNOMED CT distribution"
    // (<https://hl7.org/fhir/R4B/snomedct.html>, "Versions").
    fn answers_version(provider: &SnomedProvider, version: &str) -> bool {
        version == provider.system().edition
    }

    fn unserved_properties(_provider: &SnomedProvider) -> &[&'static str] {
        &UNSERVED_PROPERTIES
    }

    fn generated_properties(_provider: &SnomedProvider) -> &[&'static str] {
        &GENERATED_PROPERTIES
    }

    fn generated_property(
        provider: &SnomedProvider,
        concept: Concept,
        name: &str,
    ) -> Result<Option<Property>, ProviderError> {
        let terms = match name {
            NORMAL_FORM => true,
            NORMAL_FORM_TERSE => false,
            _ => return Ok(None),
        };
        let generated = match provider.registered(concept)? {
            Some(record) => provider.expression_normal_form(&record, terms)?,
            None => provider.normal_form(concept, terms)?,
        };
        let Some(expression) = generated else {
            return Ok(None);
        };
        Ok(Some(Property {
            code: name.to_owned(),
            value: PropertyValue::String(expression),
            ..Property::default()
        }))
    }

    /// The language of the displays this provider returns, so a
    /// `displayLanguage` the edition carries no description for is answered as
    /// a language the system has no display in
    /// (`OperationDefinition/CodeSystem-validate-code`: `displayLanguage`
    /// "Specifies the language to be used for description when validating the
    /// display property").
    fn language(provider: &SnomedProvider) -> Option<&str> {
        Some(&provider.system().base_language)
    }

    fn locate(provider: &SnomedProvider, code: &str) -> Result<Option<Located>, ProviderError> {
        // NOTE: an expression in Compositional Grammar is a valid code and is
        // "subject to the same rules as precoordinated concepts"
        // (<https://hl7.org/fhir/R4B/snomedct.html>, "Code").
        if provider.is_expression(code) {
            return Ok(provider
                .resolve_expression(code)?
                .ok()
                .map(|(concept, code)| Located { concept, code }));
        }
        // NOTE: a string that is not a well-formed SCTID (check digit,
        // partition) is not a code of this system; that is "absent", not an
        // error (<https://hl7.org/fhir/R4B/snomedct.html>, valid code values).
        if ConceptId::parse(code).is_err() {
            return Ok(None);
        }
        Ok(provider
            .stored()
            .store
            .ordinal(code)
            .map_err(storage)?
            .map(|ordinal| Located {
                concept: Concept::new(ordinal.index()),
                code: code.to_owned(),
            }))
    }

    // NOTE: SNOMED CT Expressions in Compositional Grammar are valid codes
    // (<https://hl7.org/fhir/R4B/snomedct.html>, "Code"), and a concept
    // reference alone carries none of the grammar's own characters.
    fn is_expression(_provider: &SnomedProvider, code: &str) -> bool {
        code.contains(SCG_OPERATORS)
    }

    /// Why an expression is not a code of this version: the grammar's own
    /// position for a malformed one, and the concept named otherwise (the
    /// Compositional Grammar specification, §7.3 Validating).
    fn rejection(provider: &SnomedProvider, code: &str) -> Option<String> {
        if !provider.is_expression(code) {
            return None;
        }
        // NOTE: no FHIR/SNOMED spec governs this: our own design. A store this
        // read cannot reach says nothing more than "absent", which is what the
        // caller reports when this answers `None`.
        provider
            .resolve_expression(code)
            .ok()?
            .err()
            .map(|error| error.to_string())
    }

    /// An expression carries no designation: "SNOMED International does not
    /// define terms for expressions"
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "Display"), so the generated
    /// display is the only term and `$lookup` adds it by itself.
    fn is_postcoordinated(_provider: &SnomedProvider, concept: Concept) -> bool {
        SnomedProvider::is_expression_handle(concept)
    }

    /// `expressions = true` is the filter that admits post-coordination
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "Filter Properties").
    fn admits_post_coordination(_provider: &SnomedProvider, filter: &Filter) -> bool {
        filter.op == FilterOperator::Equal
            && filter.property == EXPRESSIONS
            && filter.value.trim() == "true"
    }

    fn code(provider: &SnomedProvider, concept: Concept) -> Result<Option<String>, ProviderError> {
        if let Some(record) = provider.registered(concept)? {
            return Ok(Some(record.canonical().to_owned()));
        }
        Ok(provider
            .stored()
            .store
            .code(SnomedProvider::ordinal(concept))
            .map_err(storage)?
            .map(ToOwned::to_owned))
    }

    fn display(
        provider: &SnomedProvider,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        if let Some(record) = provider.registered(concept)? {
            return Ok(Some(provider.expression_display(&record, language)?));
        }
        provider.choose_display(SnomedProvider::ordinal(concept), language)
    }

    fn definition(
        provider: &SnomedProvider,
        concept: Concept,
    ) -> Result<Option<String>, ProviderError> {
        if SnomedProvider::is_expression_handle(concept) {
            return Ok(None);
        }
        Ok(provider
            .stored()
            .store
            .designations(SnomedProvider::ordinal(concept))
            .map_err(storage)?
            .into_iter()
            .find(|d| d.active && d.use_ordinal == provider.system().keys.definition)
            .map(|d| d.term))
    }

    /// An expression is active when every concept it names is active.
    ///
    /// The concept references of an expression "must refer to active concepts
    /// in the given version and edition of SNOMED CT" (the Compositional
    /// Grammar specification, §7.3 Validating). Inactive is not invalid on
    /// this server, so an expression over an inactive concept validates and
    /// says which it is (<https://hl7.org/fhir/R4B/snomedct.html>, "Inactive").
    fn status(provider: &SnomedProvider, concept: Concept) -> Result<Status, ProviderError> {
        if let Some(record) = provider.registered(concept)? {
            return Ok(Status {
                standards_status: None,
                active: provider.expression_is_active(&record)?,
                inactive_reason: None,
                abstract_concept: false,
                codeless: false,
            });
        }
        Ok(Status {
            active: provider.stored().active(concept)?,
            ..Status::default()
        })
    }

    /// The use of a description: its type, as the SNOMED CT concept the type
    /// is.
    fn designation_use(provider: &SnomedProvider, use_ordinal: u32) -> DesignationUse {
        let keys = &provider.system().keys;
        let (code, display) = if use_ordinal == keys.fsn {
            (constants::FULLY_SPECIFIED_NAME, "Fully specified name")
        } else if use_ordinal == keys.definition {
            (constants::DEFINITION, "Definition")
        } else {
            (constants::SYNONYM, "Synonym")
        };
        DesignationUse {
            system: SYSTEM.to_owned(),
            code: code.to_string(),
            display: Some(display.to_owned()),
        }
    }

    fn properties(
        provider: &SnomedProvider,
        concept: Concept,
    ) -> Result<Vec<Property>, ProviderError> {
        // NOTE: the FHIR-defined properties of "the given code or expression"
        // that an expression itself states (<https://hl7.org/fhir/R4B/snomedct.html>,
        // "SNOMED CT Properties"): its status and its own refinement.
        if let Some(record) = provider.registered(concept)? {
            let mut out = vec![
                Property {
                    code: String::from("inactive"),
                    value: PropertyValue::Boolean(!provider.expression_is_active(&record)?),
                    ..Property::default()
                },
                Property {
                    code: String::from("sufficientlyDefined"),
                    value: PropertyValue::Boolean(record.sufficiently_defined()),
                    ..Property::default()
                },
            ];
            out.extend(SnomedProvider::expression_attributes(&record));
            return Ok(out);
        }
        let ordinal = SnomedProvider::ordinal(concept);
        let Some(record) = provider.stored().store.concept(ordinal).map_err(storage)? else {
            return Ok(Vec::new());
        };
        let stored = provider
            .stored()
            .store
            .properties(ordinal)
            .map_err(storage)?;
        let of = |key: u32| stored.iter().find(|(k, _)| *k == key).map(|(_, v)| v);
        let mut out = vec![Property {
            code: String::from("inactive"),
            value: PropertyValue::Boolean(!record.active),
            ..Property::default()
        }];
        let defined = of(provider.system().keys.definition_status).is_some_and(|values| {
            values
                .iter()
                .any(|v| matches!(v, record::PropertyValue::Code(c) if *c == constants::DEFINED.to_string()))
        });
        out.push(Property {
            code: String::from("sufficientlyDefined"),
            value: PropertyValue::Boolean(defined),
            ..Property::default()
        });
        if let Some(values) = of(provider.system().keys.module)
            && let Some(record::PropertyValue::Code(module)) = values.first()
        {
            out.push(Property {
                code: String::from("moduleId"),
                value: PropertyValue::Code(module.clone()),
                ..Property::default()
            });
        }
        if let Some(time) = record.effective_time {
            out.push(Property {
                code: String::from("effectiveTime"),
                value: PropertyValue::String(time),
                ..Property::default()
            });
        }
        provider.stored().relatives(concept, &mut out)?;
        for (key, sctid) in &provider.system().keys.attributes {
            let Some(values) = of(*key) else {
                continue;
            };
            for value in values {
                let Some(value) = provider.stored().property_value(value.clone())? else {
                    continue;
                };
                out.push(Property {
                    code: sctid.clone(),
                    value,
                    ..Property::default()
                });
            }
        }
        Ok(out)
    }

    /// The inactive concepts, scanned once from the store and kept.
    fn inactive(provider: &SnomedProvider) -> Option<Result<ConceptSet, ProviderError>> {
        Some(provider.inactive_set())
    }

    /// `concept in [sctid]` is reference set membership, `constraint = [ecl]`
    /// the evaluated expression constraint, and `expressions` every concept of
    /// the version either way (<https://hl7.org/fhir/R4B/snomedct.html>,
    /// "Filter Properties"); every other filter is the generic evaluation over
    /// the closure and the store.
    ///
    /// The `expressions` value decides whether an expression may join the
    /// selection, which is a question about a code the version does not
    /// enumerate, so it is answered in `filter_matches` and not by this set.
    fn filter(
        provider: &SnomedProvider,
        filter: &Filter,
    ) -> Option<Result<ConceptSet, ProviderError>> {
        if filter.op == FilterOperator::Equal {
            match filter.property.as_str() {
                "constraint" => return Some(provider.constraint(&filter.value)),
                EXPRESSIONS => {
                    return Some(match filter.value.trim() {
                        "false" | "true" => provider.all(),
                        other => Err(ProviderError::invalid_filter_value(
                            &filter.property,
                            other,
                            "expected `true` or `false`",
                        )),
                    });
                }
                _ => {}
            }
        }
        (filter.property == "concept" && filter.op == FilterOperator::In)
            .then(|| provider.reference_set_members(filter))
    }

    /// A SNOMED CT selection admits the post-coordinated expressions its
    /// grammar builds, and no expansion lists them, which is the case `$expand`
    /// names for `valueset-unclosed`: "unbounded due to the inclusion of
    /// post-coordinated value sets (e.g. SNOMED CT, UCUM)"
    /// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>, Notes). The
    /// `expressions = false` filter is what keeps them out
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "Filter Properties"), so a
    /// selection stating it is complete.
    fn unclosed(_provider: &SnomedProvider, filters: &[Filter]) -> bool {
        !filters.iter().any(|filter| {
            filter.property == EXPRESSIONS
                && filter.op == FilterOperator::Equal
                && filter.value.trim() == "false"
        })
    }

    /// Whether one concept passes a filter.
    ///
    /// An expression is not in any evaluated set, because the version does not
    /// enumerate it, so it is tested against the filter through its focus
    /// concepts: it is subsumed by each of them, so a selection that holds them
    /// all and is closed under subtype holds it too. No FHIR or SNOMED CT
    /// specification governs the membership of an expression in a value set:
    /// this is our own design, and a selection that is not closed under subtype
    /// (reference set membership, a property equality) can admit an expression
    /// it does not hold. The `expressions` filter bounds it: a value set says
    /// `true` before any expression is considered.
    fn filter_matches(
        provider: &SnomedProvider,
        concept: Concept,
        filter: &Filter,
    ) -> Option<Result<bool, ProviderError>> {
        match provider.registered(concept) {
            Ok(Some(record)) => Some(provider.expression_passes(&record, filter)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        }
    }

    /// The subsumption between two codes when either is an expression; the
    /// materialized closure answers the rest.
    fn subsumes(
        provider: &SnomedProvider,
        a: Concept,
        b: Concept,
    ) -> Result<Option<concept_graph::subsumption::Outcome>, ProviderError> {
        if !SnomedProvider::is_expression_handle(a) && !SnomedProvider::is_expression_handle(b) {
            return Ok(None);
        }
        provider.expression_subsumes(a, b).map(Some)
    }

    /// The concepts this edition's historical association reference sets name
    /// in place of an inactive one (<https://hl7.org/fhir/R4B/snomedct.html>).
    fn successors(
        provider: &SnomedProvider,
        concept: Concept,
    ) -> Result<Vec<crate::provider::Successor>, ProviderError> {
        conceptmap::successors(provider, concept)
    }

    /// The implicit concept maps of the FHIR SNOMED CT page
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Concept Maps"):
    /// `?fhir_cm=[sctid]` names a map reference set of this edition, on the
    /// bare system URI or on this edition's edition or version URI.
    fn implicit_concept_map(
        provider: &SnomedProvider,
        url: &str,
        selection: crate::provider::MapSelection<'_>,
    ) -> Option<Result<crate::conceptmap::model::ConceptMapModel, ProviderError>> {
        let (base, form) = implicit_parts(url, FHIR_CM)?;
        let malformed = |reason: String| ProviderError::MalformedImplicitConceptMap {
            url: url.to_owned(),
            reason,
        };
        // NOTE: the base must be this edition or the bare system URI; the map itself
        // always states the served version
        // (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Concept Maps").
        if let Err(error) = provider.implicit_version(base) {
            return Some(Err(error));
        }
        let Some(decoded) = percent_decode(form) else {
            return Some(Err(malformed(format!(
                "`{form}` is not a percent-encoded SCTID"
            ))));
        };
        let Ok(refset) = ConceptId::parse(decoded.trim()) else {
            return Some(Err(malformed(format!("`{decoded}` is not an SCTID"))));
        };
        Some(conceptmap::concept_map(
            provider,
            url,
            refset.value(),
            selection,
        ))
    }

    /// The implicit value sets of the FHIR SNOMED CT page
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets"):
    /// `?fhir_vs`, `?fhir_vs=isa/[sctid]`, `?fhir_vs=refset`,
    /// `?fhir_vs=refset/[sctid]`, and `?fhir_vs=ecl/[ecl]`, on the bare system
    /// URI or on this edition's edition or version URI.
    fn implicit_value_set(
        provider: &SnomedProvider,
        url: &str,
    ) -> Option<Result<Compose, ProviderError>> {
        let (base, form) = implicit_parts(url, FHIR_VS)?;
        let malformed = |reason: String| ProviderError::malformed_implicit_value_set(url, reason);
        let version = match provider.implicit_version(base) {
            Ok(version) => version,
            Err(error) => return Some(Err(error)),
        };
        let system = SystemRef {
            url: SYSTEM.to_owned(),
            version,
        };
        let include = match form {
            "" => Include {
                system: Some(system),
                ..Include::default()
            },
            "refset" => {
                // NOTE: the set is "all concept ids that correspond to reference sets
                // that are explicitly defined in the specified SNOMED CT edition",
                // with no category excluded (<https://hl7.org/fhir/R4B/snomedct.html>).
                let mut defined: BTreeSet<u64> = provider.system().memberships.refsets().collect();
                defined.extend(
                    provider
                        .system()
                        .keys
                        .refsets
                        .iter()
                        .filter_map(|(_, id)| ConceptId::parse(id).ok())
                        .map(ConceptId::value),
                );
                let mut concepts = Vec::new();
                for refset in defined {
                    if let Ok(Some(located)) = provider.locate(&refset.to_string()) {
                        concepts.push(ConceptRef {
                            deprecated: false,
                            code: located.code,
                            display: None,
                        });
                    }
                }
                if concepts.is_empty() {
                    return Some(Err(malformed(String::from(
                        "the edition defines no reference sets",
                    ))));
                }
                Include {
                    system: Some(system),
                    concepts,
                    ..Include::default()
                }
            }
            other => {
                let filter = match provider.implicit_filter(url, other) {
                    Ok(filter) => filter,
                    Err(error) => return Some(Err(error)),
                };
                Include {
                    system: Some(system),
                    filters: vec![filter],
                    ..Include::default()
                }
            }
        };
        Some(Ok(Compose {
            include: vec![include],
            ..Compose::default()
        }))
    }

    /// The template fields of an implicit value set of the FHIR SNOMED CT page
    /// (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets").
    ///
    /// The page prints a template per form and says "the content of the
    /// resource must conform to the template provided". It prints none for the
    /// bare `?fhir_vs`, so that form carries only the fields every template
    /// shares: the edition version and the copyright.
    fn implicit_metadata(
        provider: &SnomedProvider,
        url: &str,
    ) -> crate::provider::ImplicitMetadata {
        let Some((_, form)) = implicit_parts(url, FHIR_VS) else {
            return crate::provider::ImplicitMetadata::default();
        };
        let (name, description) = match form.split_once('/').unwrap_or((form, "")) {
            ("isa", sctid) => (
                Some(format!("SNOMED CT Concept {sctid} and descendants")),
                Some(format!(
                    "All SNOMED CT concepts for {}",
                    provider.term_or_code(sctid)
                )),
            ),
            ("refset", "") => (
                Some(String::from("SNOMED CT Reference Sets")),
                Some(String::from(
                    "All SNOMED CT concepts associated with a reference set",
                )),
            ),
            ("refset", sctid) => (
                Some(format!("SNOMED CT Reference Set {sctid}")),
                Some(format!(
                    "All SNOMED CT concepts in the reference set {}",
                    provider.term_or_code(sctid)
                )),
            ),
            ("ecl", ecl) => {
                let ecl = percent_decode(ecl).unwrap_or_else(|| ecl.to_owned());
                (
                    Some(format!("SNOMED CT Concepts matching {ecl}")),
                    Some(format!(
                        "All SNOMED CT concepts that match the expression constraint {ecl}"
                    )),
                )
            }
            _ => (None, None),
        };
        crate::provider::ImplicitMetadata {
            version: Some(provider.identity().version.clone()),
            name,
            title: None,
            experimental: None,
            date: None,
            description,
            copyright: Some(String::from(TEMPLATE_COPYRIGHT)),
        }
    }
}

/// Decodes the percent-encoding of a URI component
/// (<https://www.rfc-editor.org/rfc/rfc3986#section-2.1>); `None` for a
/// stray `%` or bytes that are not UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        if byte == b'%' {
            let hex = bytes.get(i.checked_add(1)?..i.checked_add(3)?)?;
            let text = std::str::from_utf8(hex).ok()?;
            out.push(u8::from_str_radix(text, 16).ok()?);
            i = i.checked_add(3)?;
        } else {
            out.push(byte);
            i = i.checked_add(1)?;
        }
    }
    String::from_utf8(out).ok()
}

/// The byte offset into `url` of the byte at `offset` in the decoding of
/// `argument`, the encoded expression constraint that ends `url`.
fn url_offset(url: &str, argument: &str, offset: usize) -> Option<usize> {
    let start = url.len().checked_sub(argument.len())?;
    start.checked_add(crate::position::encoded_offset(argument, offset)?)
}

/// The base (the system, edition, or version URI) and the form of an implicit
/// URI whose query key is `key` (`fhir_vs` or `fhir_cm`), when `url` has the
/// shape (<https://hl7.org/fhir/R4B/snomedct.html>).
fn implicit_parts<'a>(url: &'a str, key: &str) -> Option<(&'a str, &'a str)> {
    let (base, query) = url.split_once('?')?;
    let form = match query.strip_prefix(key)? {
        "" => "",
        rest => rest.strip_prefix('=')?,
    };
    (base == SYSTEM || base.starts_with("http://snomed.info/sct/")).then_some((base, form))
}

/// The implicit value set forms the provider resolves: the five `?fhir_vs`
/// forms of <https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets".
fn implicit_forms() -> Vec<ImplicitForm> {
    vec![
        ImplicitForm::new("http://snomed.info/sct?fhir_vs", ImplicitArgument::None),
        ImplicitForm::new(
            "http://snomed.info/sct?fhir_vs=isa/[sctid]",
            ImplicitArgument::Code,
        ),
        ImplicitForm::new(
            "http://snomed.info/sct?fhir_vs=refset",
            ImplicitArgument::None,
        ),
        ImplicitForm::new(
            "http://snomed.info/sct?fhir_vs=refset/[sctid]",
            ImplicitArgument::Code,
        ),
        ImplicitForm::new(
            "http://snomed.info/sct?fhir_vs=ecl/[ecl]",
            ImplicitArgument::Expression,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::{FHIR_CM, FHIR_VS, implicit_parts, percent_decode};

    #[test]
    fn percent_encoding_decodes_and_a_stray_percent_is_refused() {
        assert_eq!(
            percent_decode("%3C%3C%20404684003%20%7Cfinding%7C").as_deref(),
            Some("<< 404684003 |finding|")
        );
        assert_eq!(percent_decode("<< 1").as_deref(), Some("<< 1"));
        assert_eq!(percent_decode("%3"), None);
        assert_eq!(percent_decode("%zz"), None);
    }

    #[test]
    fn implicit_urls_split_into_base_and_form() {
        assert_eq!(
            implicit_parts("http://snomed.info/sct?fhir_vs", FHIR_VS),
            Some(("http://snomed.info/sct", ""))
        );
        assert_eq!(
            implicit_parts(
                "http://snomed.info/sct/11000146104/version/20260630?fhir_vs=isa/1",
                FHIR_VS
            ),
            Some((
                "http://snomed.info/sct/11000146104/version/20260630",
                "isa/1"
            ))
        );
        assert_eq!(
            implicit_parts("http://snomed.info/sct?fhir_cm=1", FHIR_CM),
            Some(("http://snomed.info/sct", "1"))
        );
        assert_eq!(
            implicit_parts("http://snomed.info/sct?fhir_cm=1", FHIR_VS),
            None,
            "one key never reads the other's URI"
        );
        assert_eq!(
            implicit_parts("http://snomed.info/sct?fhir_vs", FHIR_CM),
            None
        );
        assert_eq!(implicit_parts("http://loinc.org/vs", FHIR_VS), None);
        assert_eq!(
            implicit_parts("http://snomed.info/sct?fhir_vsx", FHIR_VS),
            None
        );
    }
}
