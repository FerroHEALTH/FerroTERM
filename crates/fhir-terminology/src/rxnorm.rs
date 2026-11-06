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

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use concept_graph::ordinal::Ordinal;
use concept_graph::relations::{Relations, RelationsError};
use concept_store::keys::{KeyTable, KeyTableError};
use concept_store::record;
use concept_store::store::Store;
use roaring::RoaringBitmap;
use serde::Deserialize;

use crate::provider::compose::{Compose, Include, SystemRef};
use crate::provider::declaration::{
    Capability, Compositional, ContentMode, Declaration, FilterDefinition, ImplicitArgument,
    ImplicitForm, PropertyDefinition, PropertyKind,
};
use crate::provider::filter::{Filter, FilterOperator};
use crate::provider::source::Source;
use crate::provider::{
    Concept, ConceptSet, Identity, Located, Property, PropertyValue, Status, error::ProviderError,
};
use crate::stored::artifact::{Artifact, Layout};
use crate::stored::system::{StoredProvider, StoredSystem};
use crate::stored::{self, storage};

/// The system URI.
pub const SYSTEM: &str = "http://www.nlm.nih.gov/research/umls/rxnorm";
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
    /// The artifact serves another system.
    #[error("the artifact serves `{0}`, not RxNorm")]
    NotRxNorm(String),
    /// The artifact's shared files do not read.
    #[error(transparent)]
    Artifact(#[from] stored::OpenError),
    /// The store cannot be read.
    #[error(transparent)]
    Store(#[from] concept_store::store::StoreError),
    /// The relations file does not read.
    #[error("cannot read the relations file")]
    Relations(#[from] RelationsError),
    /// The atom table does not read.
    #[error("cannot read the atom table")]
    Atoms(#[from] KeyTableError),
}

#[derive(Debug, Deserialize)]
struct Manifest {
    system: String,
    version: String,
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

/// What `RxNorm` decides: the relationships, the atoms, the indexed
/// attributes, and the `RXNORM` display.
pub struct RxNorm {
    relations: Relations,
    atoms: KeyTable,
    /// The code of every concept by ordinal, so a property listing hundreds
    /// of relationship targets stays a point read.
    codes: Vec<String>,
    /// The language the release's own atoms are in.
    base_language: String,
    /// The concepts per `(property name, value)` for `TTY`, `SAB`, and `STY`.
    indexed: BTreeMap<(String, String), RoaringBitmap>,
}

impl std::fmt::Debug for RxNorm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RxNorm")
            .field("relationships", &self.relations.edges())
            .finish_non_exhaustive()
    }
}

/// One `RxNorm` release behind the seam.
pub type RxNormProvider = StoredProvider<RxNorm>;

/// The code of every concept of `store` by ordinal, and the concepts per
/// `(name, value)` of the indexed attributes.
type Columns = (Vec<String>, BTreeMap<(String, String), RoaringBitmap>);

/// Reads the code column and the inverted index of `TTY`, `SAB`, and `STY`.
fn columns(artifact: &Artifact) -> Result<Columns, concept_store::store::StoreError> {
    let store: &Store = &artifact.store;
    let mut indexed: BTreeMap<(String, String), RoaringBitmap> = BTreeMap::new();
    let mut codes = Vec::with_capacity(usize::try_from(artifact.concepts).unwrap_or_default());
    for index in 0..artifact.concepts {
        codes.push(
            store
                .concept(Ordinal::new(index))?
                .map(|c| c.code)
                .unwrap_or_default(),
        );
        for (key, values) in store.properties(Ordinal::new(index))? {
            let Some(name) = artifact
                .keys
                .get(&key)
                .filter(|n| INDEXED.contains(&n.as_str()))
            else {
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
    Ok((codes, indexed))
}

impl RxNormProvider {
    /// Opens the artifact directory `dir`.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError`] when the manifest, the store, or a side file
    /// does not read, or the artifact is not an `RxNorm` release.
    pub fn open(dir: &Path) -> Result<Self, OpenError> {
        let (artifact, manifest) = Artifact::open(dir, Layout::Flat, |manifest: &Manifest| {
            stored::serves(&manifest.system, SYSTEM, OpenError::NotRxNorm)
        })?;
        let relations = Relations::read_from(&mut stored::side_file(dir, &manifest.relations)?)?;
        let atoms = KeyTable::read_from(&mut stored::side_file(dir, &manifest.atoms)?)?;
        let (codes, indexed) = columns(&artifact)?;
        Ok(Self::new(
            Identity {
                url: SYSTEM.to_owned(),
                title: Some(String::from("RxNorm")),
                name: None,
                version_needed: false,
                version: manifest.version.clone(),
            },
            declaration(
                &artifact.keys,
                &relations,
                manifest.semantic_types,
                manifest.languages,
            ),
            Source::new(dir, &manifest.version),
            artifact,
            RxNorm {
                relations,
                atoms,
                codes,
                base_language: if manifest.language.is_empty() {
                    String::from(DEFAULT_LANGUAGE)
                } else {
                    manifest.language
                },
                indexed,
            },
        ))
    }

    /// The concept a relationship filter value names: `CUI:[RXCUI]` or `AUI:[RXAUI]`.
    fn named(&self, filter: &Filter, value: &str) -> Result<Ordinal, ProviderError> {
        let invalid =
            |reason: &str| ProviderError::invalid_filter_value(&filter.property, value, reason);
        match value.split_once(':') {
            Some(("CUI", rxcui)) => self
                .stored()
                .store
                .ordinal(rxcui.trim())
                .map_err(storage)?
                .ok_or_else(|| ProviderError::UnknownCode(rxcui.trim().to_owned())),
            Some(("AUI", rxaui)) => {
                let id: u64 = rxaui
                    .trim()
                    .parse()
                    .map_err(|source| invalid(&format!("the AUI is not a number: {source}")))?;
                self.system()
                    .atoms
                    .get(id)
                    .map(Ordinal::new)
                    .ok_or_else(|| ProviderError::UnknownCode(value.to_owned()))
            }
            _ => Err(invalid("expected `CUI:[RXCUI]` or `AUI:[RXAUI]`")),
        }
    }
}

impl StoredSystem for RxNorm {
    /// The language of the displays this provider returns, so a
    /// `displayLanguage` the release carries no atom for is answered as a
    /// language the system has no display in
    /// (`OperationDefinition/CodeSystem-validate-code`: `displayLanguage`
    /// "Specifies the language to be used for description when validating the
    /// display property").
    fn language(provider: &RxNormProvider) -> Option<&str> {
        Some(&provider.system().base_language)
    }

    fn locate(provider: &RxNormProvider, code: &str) -> Result<Option<Located>, ProviderError> {
        let code = code.trim();
        if code.is_empty() || !code.bytes().all(|b| b.is_ascii_digit()) {
            return Ok(None);
        }
        provider.stored().located(code)
    }

    /// The `RXNORM` string of the most preferred term type: the first
    /// designation, the build's order.
    fn display(
        provider: &RxNormProvider,
        concept: Concept,
        _language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        provider
            .stored()
            .first_term(Ordinal::new(concept.index()), &[None], |_| true)
    }

    fn status(provider: &RxNormProvider, concept: Concept) -> Result<Status, ProviderError> {
        let active = provider.stored().active(concept)?;
        Ok(Status {
            active,
            inactive_reason: (!active).then(|| String::from("obsolete")),
            ..Status::default()
        })
    }

    fn properties(
        provider: &RxNormProvider,
        concept: Concept,
    ) -> Result<Vec<Property>, ProviderError> {
        let Some(mut out) = provider.stored().stored_properties(concept)? else {
            return Ok(Vec::new());
        };
        let system = provider.system();
        for (kind, target) in system.relations.outgoing(Ordinal::new(concept.index())) {
            let (Some(name), Some(code)) = (
                system
                    .relations
                    .types()
                    .get(usize::try_from(kind).unwrap_or(usize::MAX)),
                system
                    .codes
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
    fn implicit_value_set(
        _provider: &RxNormProvider,
        url: &str,
    ) -> Option<Result<Compose, ProviderError>> {
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

    /// `STY`, `SAB`, and `TTY` from the inverted index; a `REL` code or `RELA`
    /// label from the typed edges arriving at the named concept; everything
    /// else the generic evaluation.
    fn filter(
        provider: &RxNormProvider,
        filter: &Filter,
    ) -> Option<Result<ConceptSet, ProviderError>> {
        let system = provider.system();
        let property = filter.property.as_str();
        let values = || {
            filter
                .value
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
        };
        if !matches!(filter.op, FilterOperator::Equal | FilterOperator::In) {
            return None;
        }
        if INDEXED.contains(&property) {
            let mut set = ConceptSet::new();
            for value in values() {
                if let Some(concepts) = system.indexed.get(&(property.to_owned(), value.to_owned()))
                {
                    set |= concepts;
                }
            }
            return Some(Ok(set));
        }
        let kind = system.relations.kind(property)?;
        let mut set = ConceptSet::new();
        for value in values() {
            match provider.named(filter, value) {
                Ok(target) => {
                    set.extend(system.relations.sources(target, kind).map(Ordinal::index));
                }
                Err(error) => return Some(Err(error)),
            }
        }
        Some(Ok(set))
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
