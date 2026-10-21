// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `RxNorm` provider: one release read from its artifact directory
//! (`store.redb`, `text.bin`, `relations.bin`, `atoms.bin`, and
//! `manifest.json`).
//!
//! The FHIR `RxNorm` page is the contract (<https://hl7.org/fhir/R4B/rxnorm.html>):
//! the codes are the `RXCUI`s with a `SAB = RXNORM` atom, the display is the
//! `RXNORM` string (`SCD` or `SBD` first), the filters are `STY`, `SAB`,
//! `TTY`, and every `REL` code and `RELA` label (`=`, `in`, the value
//! `CUI:[RXCUI]` or `AUI:[RXAUI]`), the one implicit value set is
//! `http://www.nlm.nih.gov/research/umls/rxnorm/vs`, and there is no
//! subsumption. The page defines no lookup properties, so the properties
//! served (`TTY`, `SAB`, `STY`, the `RXNORM` attributes, the relationships)
//! are our own design.

use crate::artifact::MANIFEST_FILE;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use concept_graph::ordinal::Ordinal;
use concept_graph::relations::{Relations, RelationsError};
use concept_store::keys::{KeyTable, KeyTableError};
use concept_store::record;
use concept_store::store::{Store, StoreError, Vocabulary};
use concept_store::tables;
use designation_index::index::{Query, TextIndex};
use roaring::RoaringBitmap;
use serde::Deserialize;

use crate::artifact::Source;
use crate::compose::{Compose, Include, SystemRef};
use crate::filter::{Filter, FilterOperator};
use crate::provider::{
    Capability, CodeSystemProvider, Compositional, Concept, ConceptSet, ContentMode, Declaration,
    Designation, DesignationUse, FilterDefinition, Identity, ImplicitArgument, ImplicitForm,
    Located, Property, PropertyDefinition, PropertyKind, PropertyValue, ProviderError, Status,
};

/// The system URI.
pub const SYSTEM: &str = "http://www.nlm.nih.gov/research/umls/rxnorm";
/// The manifest version this provider reads.
pub const MANIFEST_VERSION: u32 = 2;
/// The properties with an inverted index, which are also the FHIR filters
/// beside the relationships.
const INDEXED: [&str; 3] = ["TTY", "SAB", "STY"];
/// The language of an artifact whose manifest states none.
///
/// The `RXNORM` atoms a release names its concepts by carry `LAT` `ENG`
/// (<https://www.nlm.nih.gov/research/umls/rxnorm/docs/techdoc.html>). No
/// FHIR/SNOMED spec governs this: our own design.
const DEFAULT_LANGUAGE: &str = "en";

/// A failure to open an artifact as `RxNorm`.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// A file does not read.
    #[error("cannot read {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The manifest does not parse.
    #[error("{path} is not an artifact manifest")]
    Manifest {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: serde_json::Error,
    },
    /// The artifact serves another system.
    #[error("the artifact serves `{0}`, not RxNorm")]
    NotRxNorm(String),
    /// The manifest is of another layout version.
    #[error("the manifest is version {0}; this server reads version {MANIFEST_VERSION}")]
    ManifestVersion(u32),
    /// The store cannot be opened or read.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The designation index file does not read.
    #[error("cannot read the designation index file")]
    Text(#[from] designation_index::persist::PersistError),
    /// The relations file does not read.
    #[error("cannot read the relations file")]
    Relations(#[from] RelationsError),
    /// The atom table does not read.
    #[error("cannot read the atom table")]
    Atoms(#[from] KeyTableError),
    /// The concept count meta entry is missing or malformed.
    #[error("the store's concept count is `{0:?}`")]
    ConceptCount(Option<String>),
}

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(rename = "manifest")]
    layout: u32,
    system: String,
    version: String,
    store: String,
    text: String,
    #[serde(default)]
    relations: String,
    #[serde(default)]
    atoms: String,
    #[serde(default, rename = "semanticTypes")]
    semantic_types: bool,
    /// The language the release's own atoms are in; an artifact built before
    /// the build wrote it has none.
    #[serde(default)]
    language: String,
    #[serde(default)]
    languages: Vec<String>,
}

/// One `RxNorm` release behind the seam.
pub struct RxNormProvider {
    identity: Identity,
    declaration: Declaration,
    store: Store,
    text: TextIndex,
    relations: Relations,
    atoms: KeyTable,
    concepts: u32,
    /// The code of every concept by ordinal, so a property listing hundreds
    /// of relationship targets stays a point read.
    codes: Vec<String>,
    /// Property key ordinal to name.
    keys: BTreeMap<u32, String>,
    /// Designation use ordinal to name (the term types).
    uses: BTreeMap<u32, String>,
    /// The language the release's own atoms are in.
    base_language: String,
    /// The concepts per `(property name, value)` for `TTY`, `SAB`, and `STY`.
    indexed: BTreeMap<(String, String), RoaringBitmap>,
    /// The artifact the version was read from.
    source: Source,
}

impl std::fmt::Debug for RxNormProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RxNormProvider")
            .field("version", &self.identity.version)
            .field("concepts", &self.concepts)
            .field("relationships", &self.relations.edges())
            .finish_non_exhaustive()
    }
}

fn storage(error: StoreError) -> ProviderError {
    ProviderError::Storage(Box::new(error))
}

fn primary_subtag(language: &str) -> String {
    language
        .split(['-', '_'])
        .next()
        .unwrap_or(language)
        .to_ascii_lowercase()
}

fn convert(
    value: record::PropertyValue,
    store: &Store,
) -> Result<Option<PropertyValue>, ProviderError> {
    Ok(Some(match value {
        record::PropertyValue::Concept(target) => match store.concept(target).map_err(storage)? {
            Some(target) => PropertyValue::Code(target.code),
            None => return Ok(None),
        },
        record::PropertyValue::Code(c) => PropertyValue::Code(c),
        record::PropertyValue::String(s) => PropertyValue::String(s),
        record::PropertyValue::Integer(i) => PropertyValue::Integer(i),
        record::PropertyValue::Boolean(b) => PropertyValue::Boolean(b),
        record::PropertyValue::Decimal(d) => PropertyValue::Decimal(d),
        record::PropertyValue::DateTime(d) => PropertyValue::DateTime(d),
    }))
}

impl RxNormProvider {
    /// Opens the artifact directory `dir`.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError`] when the manifest, the store, or a side file
    /// does not read, or the artifact is not an `RxNorm` release.
    pub fn open(dir: &Path) -> Result<Self, OpenError> {
        let manifest_path = dir.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&manifest_path).map_err(|source| OpenError::Io {
            path: manifest_path.clone(),
            source,
        })?;
        let manifest: Manifest =
            serde_json::from_str(&text).map_err(|source| OpenError::Manifest {
                path: manifest_path,
                source,
            })?;
        if manifest.system != SYSTEM {
            return Err(OpenError::NotRxNorm(manifest.system));
        }
        if manifest.layout != MANIFEST_VERSION {
            return Err(OpenError::ManifestVersion(manifest.layout));
        }
        let store = Store::open(&dir.join(&manifest.store))?;
        let read = |name: &str| {
            let path = dir.join(name);
            crate::artifact::reader(&path).map_err(|source| OpenError::Io { path, source })
        };
        let text = designation_index::persist::read_from(&mut read(&manifest.text)?)?;
        let relations = Relations::read_from(&mut read(&manifest.relations)?)?;
        let atoms = KeyTable::read_from(&mut read(&manifest.atoms)?)?;
        let concepts = store.meta(tables::META_CONCEPTS)?;
        let concepts: u32 = concepts
            .as_deref()
            .and_then(|c| c.parse().ok())
            .ok_or(OpenError::ConceptCount(concepts))?;
        let mut keys = BTreeMap::new();
        let mut ordinal = 0;
        while let Some(name) = store.vocabulary(Vocabulary::PropertyKeys, ordinal)? {
            keys.insert(ordinal, name);
            ordinal += 1;
        }
        let mut uses = BTreeMap::new();
        let mut ordinal = 0;
        while let Some(name) = store.vocabulary(Vocabulary::DesignationUses, ordinal)? {
            uses.insert(ordinal, name);
            ordinal += 1;
        }
        let mut indexed: BTreeMap<(String, String), RoaringBitmap> = BTreeMap::new();
        let mut codes = Vec::with_capacity(usize::try_from(concepts).unwrap_or_default());
        for index in 0..concepts {
            codes.push(
                store
                    .concept(Ordinal::new(index))?
                    .map(|c| c.code)
                    .unwrap_or_default(),
            );
            for (key, values) in store.properties(Ordinal::new(index))? {
                let Some(name) = keys.get(&key).filter(|n| INDEXED.contains(&n.as_str())) else {
                    continue;
                };
                for value in values {
                    let (record::PropertyValue::Code(text) | record::PropertyValue::String(text)) =
                        value
                    else {
                        continue;
                    };
                    indexed
                        .entry((name.clone(), text))
                        .or_default()
                        .insert(index);
                }
            }
        }
        let declaration = declaration(
            &keys,
            &relations,
            manifest.semantic_types,
            manifest.languages,
        );
        let source = Source::new(dir, &manifest.version);
        Ok(Self {
            identity: Identity {
                url: SYSTEM.to_owned(),
                version: manifest.version,
                title: Some(String::from("RxNorm")),
                name: None,
                version_needed: false,
            },
            declaration,
            store,
            text,
            relations,
            atoms,
            concepts,
            codes,
            keys,
            uses,
            base_language: if manifest.language.is_empty() {
                String::from(DEFAULT_LANGUAGE)
            } else {
                manifest.language
            },
            indexed,
            source,
        })
    }

    fn ordinal(concept: Concept) -> Ordinal {
        Ordinal::new(concept.index())
    }

    fn use_coding(&self, use_ordinal: u32) -> DesignationUse {
        let code = self
            .uses
            .get(&use_ordinal)
            .cloned()
            .unwrap_or_else(|| use_ordinal.to_string());
        DesignationUse {
            system: SYSTEM.to_owned(),
            display: Some(code.clone()),
            code,
        }
    }

    /// The concept a relationship filter value names: `CUI:[RXCUI]` or `AUI:[RXAUI]`.
    fn named(&self, filter: &Filter, value: &str) -> Result<Ordinal, ProviderError> {
        let invalid =
            |reason: &str| ProviderError::invalid_filter_value(&filter.property, value, reason);
        match value.split_once(':') {
            Some(("CUI", rxcui)) => self
                .store
                .ordinal(rxcui.trim())
                .map_err(storage)?
                .ok_or_else(|| ProviderError::UnknownCode(rxcui.trim().to_owned())),
            Some(("AUI", rxaui)) => {
                let id: u64 = rxaui
                    .trim()
                    .parse()
                    .map_err(|source| invalid(&format!("the AUI is not a number: {source}")))?;
                self.atoms
                    .get(id)
                    .map(Ordinal::new)
                    .ok_or_else(|| ProviderError::UnknownCode(value.to_owned()))
            }
            _ => Err(invalid("expected `CUI:[RXCUI]` or `AUI:[RXAUI]`")),
        }
    }
}

impl CodeSystemProvider for RxNormProvider {
    fn identity(&self) -> &Identity {
        &self.identity
    }

    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    /// The language of the displays this provider returns, so a
    /// `displayLanguage` the release carries no atom for is answered as a
    /// language the system has no display in
    /// (`OperationDefinition/CodeSystem-validate-code`: `displayLanguage`
    /// "Specifies the language to be used for description when validating the
    /// display property").
    fn language(&self) -> Option<&str> {
        Some(&self.base_language)
    }

    fn artifact(&self) -> Option<&Source> {
        Some(&self.source)
    }

    fn locate(&self, code: &str) -> Result<Option<Located>, ProviderError> {
        let code = code.trim();
        if code.is_empty() || !code.bytes().all(|b| b.is_ascii_digit()) {
            return Ok(None);
        }
        let Some(ordinal) = self.store.ordinal(code).map_err(storage)? else {
            return Ok(None);
        };
        let stored = self.store.concept(ordinal).map_err(storage)?;
        Ok(stored.map(|c| Located {
            concept: Concept::new(ordinal.index()),
            code: c.code,
        }))
    }

    fn code(&self, concept: Concept) -> Result<Option<String>, ProviderError> {
        Ok(self
            .store
            .concept(Self::ordinal(concept))
            .map_err(storage)?
            .map(|c| c.code))
    }

    /// The `RXNORM` string of the most preferred term type: the first
    /// designation, the build's order.
    fn display(
        &self,
        concept: Concept,
        _language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        Ok(self
            .store
            .designations(Self::ordinal(concept))
            .map_err(storage)?
            .into_iter()
            .next()
            .map(|d| d.term))
    }

    fn status(&self, concept: Concept) -> Result<Status, ProviderError> {
        let record = self
            .store
            .concept(Self::ordinal(concept))
            .map_err(storage)?;
        let active = record.is_some_and(|c| c.active);
        Ok(Status {
            standards_status: None,
            active,
            inactive_reason: (!active).then(|| String::from("obsolete")),
            abstract_concept: false,
            codeless: false,
        })
    }

    fn designations(
        &self,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Vec<Designation>, ProviderError> {
        let wanted = language.map(primary_subtag);
        Ok(self
            .store
            .designations(Self::ordinal(concept))
            .map_err(storage)?
            .into_iter()
            .filter(|d| {
                wanted
                    .as_deref()
                    .is_none_or(|w| primary_subtag(&d.language) == w)
            })
            .map(|d| Designation {
                standards_status: None,
                language: Some(d.language.clone()),
                use_: Some(self.use_coding(d.use_ordinal)),
                value: d.term,
            })
            .collect())
    }

    fn properties(&self, concept: Concept) -> Result<Vec<Property>, ProviderError> {
        let ordinal = Self::ordinal(concept);
        let Some(record) = self.store.concept(ordinal).map_err(storage)? else {
            return Ok(Vec::new());
        };
        let mut out = vec![Property {
            code: String::from("inactive"),
            value: PropertyValue::Boolean(!record.active),
            ..Property::default()
        }];
        for (key, values) in self.store.properties(ordinal).map_err(storage)? {
            let Some(name) = self.keys.get(&key) else {
                continue;
            };
            for value in values {
                if let Some(value) = convert(value, &self.store)? {
                    out.push(Property {
                        code: name.clone(),
                        value,
                        ..Property::default()
                    });
                }
            }
        }
        for (kind, target) in self.relations.outgoing(ordinal) {
            let (Some(name), Some(code)) = (
                self.relations
                    .types()
                    .get(usize::try_from(kind).unwrap_or(usize::MAX)),
                self.codes
                    .get(usize::try_from(target.index()).unwrap_or(usize::MAX)),
            ) else {
                continue;
            };
            out.push(Property {
                code: name.clone(),
                value: PropertyValue::Code(code.clone()),
                ..Property::default()
            });
        }
        Ok(out)
    }

    /// `http://www.nlm.nih.gov/research/umls/rxnorm/vs`: every code
    /// (<https://hl7.org/fhir/R4B/rxnorm.html>, "Implicit Value Sets").
    fn implicit_value_set(&self, url: &str) -> Option<Result<Compose, ProviderError>> {
        let rest = url.strip_prefix(SYSTEM)?.strip_prefix("/vs")?;
        if !rest.is_empty() {
            return Some(Err(ProviderError::malformed_implicit_value_set(
                url,
                "RxNorm defines only `/vs`, all codes",
            )));
        }
        Some(Ok(Compose {
            include: vec![Include {
                system: Some(SystemRef {
                    url: SYSTEM.to_owned(),
                    version: None,
                }),
                ..Include::default()
            }],
            ..Compose::default()
        }))
    }

    fn all(&self) -> Result<ConceptSet, ProviderError> {
        Ok(crate::provider::every(self.concepts))
    }

    fn search(&self, text: &str, language: Option<&str>) -> Result<ConceptSet, ProviderError> {
        let query = Query {
            text: text.to_owned(),
            language: language.map(primary_subtag),
            ..Query::default()
        };
        let mut concepts = ConceptSet::new();
        for designation in self.text.matches(&query) {
            if let Some(entry) = self.text.entry(designation) {
                concepts.insert(entry.concept.index());
            }
        }
        Ok(concepts)
    }

    /// `STY`, `SAB`, and `TTY` from the inverted index; a `REL` code or `RELA`
    /// label from the typed edges arriving at the named concept; everything
    /// else the generic evaluation.
    fn filter(&self, filter: &Filter) -> Result<ConceptSet, ProviderError> {
        let property = filter.property.as_str();
        let values = || {
            filter
                .value
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
        };
        if INDEXED.contains(&property)
            && matches!(filter.op, FilterOperator::Equal | FilterOperator::In)
        {
            let mut set = ConceptSet::new();
            for value in values() {
                if let Some(concepts) = self.indexed.get(&(property.to_owned(), value.to_owned())) {
                    set |= concepts;
                }
            }
            return Ok(set);
        }
        if let Some(kind) = self.relations.kind(property)
            && matches!(filter.op, FilterOperator::Equal | FilterOperator::In)
        {
            let mut set = ConceptSet::new();
            for value in values() {
                let target = self.named(filter, value)?;
                set.extend(self.relations.sources(target, kind).map(Ordinal::index));
            }
            return Ok(set);
        }
        crate::filter::evaluate(self, filter)
    }
}

/// The property the RXNORM attribute `name` declares; an indexed attribute
/// other than `STY` carries a code.
fn attribute_property(name: &str, indexed: bool) -> PropertyDefinition {
    PropertyDefinition {
        code: name.to_owned(),
        uri: None,
        description: Some(match name {
            "TTY" => String::from("The RXNORM term types of the concept"),
            "SAB" => String::from("The sources with an atom for the concept"),
            "STY" => String::from("The semantic types of the concept"),
            other => format!("The RXNORM attribute `{other}`"),
        }),
        kind: if indexed && name != "STY" {
            PropertyKind::Code
        } else {
            PropertyKind::String
        },
    }
}

/// The filter the indexed RXNORM attribute `name` declares.
fn attribute_filter(name: &str) -> FilterDefinition {
    FilterDefinition {
        code: name.to_owned(),
        description: Some(match name {
            "TTY" => String::from("Concepts with an RXNORM atom of the term type"),
            "SAB" => String::from("Concepts with an atom from the source"),
            _ => String::from("Concepts of the semantic type"),
        }),
        operators: vec![FilterOperator::Equal, FilterOperator::In],
        value: match name {
            "TTY" => String::from("a term type (SCD, SBD, IN, ...)"),
            "SAB" => String::from("a source (RXNORM, MTHSPL, ...)"),
            _ => String::from("a semantic type name"),
        },
    }
}

/// What the provider declares: the FHIR filters (`STY` when the release has
/// semantic types, `SAB`, `TTY`, every `REL` and `RELA`), and the properties
/// the artifact carries, which the FHIR page leaves to the server.
fn declaration(
    keys: &BTreeMap<u32, String>,
    relations: &Relations,
    semantic_types: bool,
    languages: Vec<String>,
) -> Declaration {
    let mut properties = Vec::new();
    let mut filters = Vec::new();
    for name in keys.values() {
        let indexed = INDEXED.contains(&name.as_str());
        properties.push(attribute_property(name, indexed));
        if indexed && (name != "STY" || semantic_types) {
            filters.push(attribute_filter(name));
        }
    }
    for name in relations.types() {
        properties.push(PropertyDefinition {
            code: name.clone(),
            uri: None,
            description: Some(format!(
                "The concepts this one has the relationship `{name}` to"
            )),
            kind: PropertyKind::Code,
        });
        filters.push(FilterDefinition {
            code: name.clone(),
            description: Some(format!(
                "Concepts with the relationship `{name}` to the named concept"
            )),
            operators: vec![FilterOperator::Equal, FilterOperator::In],
            value: String::from("CUI:[RXCUI] or AUI:[RXAUI]"),
        });
    }
    Declaration {
        content: ContentMode::NotPresent,
        case_sensitive: true,
        hierarchy_meaning: None,
        compositional: Compositional::None,
        languages,
        properties,
        filters,
        capabilities: BTreeSet::from([Capability::Enumeration]),
        // NOTE: the one `/vs` form of <https://hl7.org/fhir/R4B/rxnorm.html>,
        // "Implicit Value Sets".
        implicit_forms: vec![ImplicitForm::new(
            "http://www.nlm.nih.gov/research/umls/rxnorm/vs",
            ImplicitArgument::None,
        )],
    }
}
