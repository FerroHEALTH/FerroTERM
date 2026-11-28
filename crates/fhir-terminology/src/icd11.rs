// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ICD-11 provider: one of the three WHO code systems (the MMS and ICF
//! linearizations, the Foundation) read from its artifact directory.
//!
//! The contract is the HL7 terminology ecosystem IG's ICD-11 page
//! (`HL7/fhir-tx-ecosystem-ig` at `9c2bbce`, `input/pagecontent/icd-11.md`):
//! in a linearization the entity id is the code and the normal form, the
//! short classification code an alternative spelling, and an expression's
//! code its normal form; the Foundation's code is the entity URI. A stem's
//! postcoordination scales are implicit value sets
//! (`<uri>/postcoordinationScale/<axis>`), and an expression's `stem` and
//! `postcoordination` properties report how its codes bound to the axes.

mod postcoordination;

use postcoordination::Cluster;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::RwLock;

use ::icd11::Linearization;
use ::icd11::expression::Expression;
use concept_graph::ordinal::Ordinal;
use concept_store::keys::{KeyTable, KeyTableError};
use concept_store::record;
use concept_store::store::{Store, StoreError};
use serde::Deserialize;

use crate::provider::compose::{Compose, Include, SystemRef};
use crate::provider::declaration::{
    Capability, Compositional, ContentMode, Declaration, HierarchyMeaning, ImplicitArgument,
    ImplicitForm, PropertyDefinition, PropertyKind,
};
use crate::provider::filter::{Filter, FilterOperator};
use crate::provider::source::Source;
use crate::provider::{
    CodeSystemProvider, Concept, ConceptSet, Hierarchy, Identity, Located, Property, PropertyValue,
    Status, error::ProviderError,
};
use crate::registries::interned::Interned;
use crate::stored::artifact::{Artifact, Layout};
use crate::stored::system::{StoredProvider, StoredSystem};
use crate::stored::{self, storage};

/// The manifest `kind` of an artifact this provider opens.
pub const KIND: &str = "icd11";
/// The designation use ordinal of the title.
const TITLE: u32 = 0;
/// The path segment of a postcoordination scale's implicit value set.
const SCALE_SEGMENT: &str = "/postcoordinationScale/";

/// A failure to open an artifact as ICD-11.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// The artifact is of another kind.
    #[error("the artifact is `{0}`, not ICD-11")]
    NotIcd11(String),
    /// The manifest names no ICD-11 code system.
    #[error("`{0}` is not an ICD-11 code system")]
    Linearization(String),
    /// The artifact's shared files do not read.
    #[error(transparent)]
    Artifact(#[from] stored::OpenError),
    /// The store cannot be read.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The key table does not read.
    #[error("cannot read the key table")]
    Keys(#[from] KeyTableError),
    /// The scales file does not parse.
    #[error("{path} is not a scales file")]
    Scales {
        /// The path.
        path: std::path::PathBuf,
        /// The cause.
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    linearization: String,
    version: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    keys: String,
    #[serde(default)]
    scales: String,
    #[serde(default)]
    languages: Vec<String>,
}

/// One postcoordination scale of a stem, as stored.
#[derive(Debug, Clone, Deserialize)]
struct Scale {
    stem: u32,
    axis: String,
    required: bool,
    multiple: String,
    entities: Vec<u32>,
}

/// What ICD-11 decides: the entity URIs, the short codes, the
/// postcoordination scales, and the expressions it validates.
pub struct Icd11 {
    linearization: Linearization,
    release: String,
    keys: KeyTable,
    /// The scales by stem ordinal, in WHO's declaration order.
    scales: BTreeMap<u32, Vec<Scale>>,
    /// The entity id of every concept by ordinal.
    ids: Vec<String>,
    /// The short code of every concept by ordinal, when it has one.
    codes: Vec<Option<String>>,
    /// The postcoordination expressions met, by their text.
    expressions: Interned,
    /// The validated expressions, by interned index.
    clusters: RwLock<Vec<Cluster>>,
}

impl std::fmt::Debug for Icd11 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Icd11")
            .field("linearization", &self.linearization)
            .field("release", &self.release)
            .finish_non_exhaustive()
    }
}

/// One ICD-11 code system behind the seam.
pub type Icd11Provider = StoredProvider<Icd11>;

/// The short axis name of a schema URI (`infectiousAgent`).
fn axis_short(axis: &str) -> &str {
    axis.rsplit('/').next().unwrap_or(axis)
}

/// The entity id the `id` property of the concept at `ordinal` states, empty
/// when the concept states none.
fn entity_id(
    store: &Store,
    linearization: Linearization,
    ordinal: Ordinal,
    id_key: Option<u32>,
) -> Result<String, StoreError> {
    let Some(key) = id_key else {
        return Ok(String::new());
    };
    Ok(match store.property(ordinal, key)?.first() {
        Some(record::PropertyValueRef::Code(uri)) => linearization.id_of(uri).unwrap_or_default(),
        _ => String::new(),
    })
}

impl Icd11Provider {
    /// Opens the artifact directory `dir`.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError`] when the manifest, the store, or a side file
    /// does not read, or the artifact is not an ICD-11 code system.
    pub fn open(dir: &Path) -> Result<Self, OpenError> {
        let (artifact, manifest) = Artifact::open(dir, Layout::Tree, |manifest: &Manifest| {
            stored::serves(&manifest.kind, KIND, OpenError::NotIcd11)
        })?;
        let linearization = Linearization::parse(&manifest.linearization)
            .ok_or_else(|| OpenError::Linearization(manifest.linearization.clone()))?;
        let keys = KeyTable::read_from(&mut stored::side_file(dir, &manifest.keys)?)?;
        let scales_path = dir.join(&manifest.scales);
        let scales_text =
            std::fs::read_to_string(&scales_path).map_err(|source| stored::OpenError::Io {
                path: scales_path.clone(),
                source,
            })?;
        let read: Vec<Scale> =
            serde_json::from_str(&scales_text).map_err(|source| OpenError::Scales {
                path: scales_path,
                source,
            })?;
        let mut scales: BTreeMap<u32, Vec<Scale>> = BTreeMap::new();
        for scale in read {
            scales.entry(scale.stem).or_default().push(scale);
        }
        let id_key = artifact.key_of("id");
        let concepts = artifact.concepts;
        let mut ids = Vec::with_capacity(usize::try_from(concepts).unwrap_or_default());
        let mut codes = Vec::with_capacity(usize::try_from(concepts).unwrap_or_default());
        for index in 0..concepts {
            let ordinal = Ordinal::new(index);
            let stored_code = artifact
                .store
                .concept(ordinal)?
                .map(|c| c.code)
                .unwrap_or_default();
            let entity = entity_id(&artifact.store, linearization, ordinal, id_key)?;
            let id = if entity.is_empty() {
                linearization.id_of(&stored_code).unwrap_or_default()
            } else {
                entity
            };
            let code = (linearization.id_of(&stored_code).is_none()).then_some(stored_code);
            ids.push(id);
            codes.push(code);
        }
        Ok(Self::new(
            Identity {
                url: linearization.system().to_owned(),
                version: manifest.version.clone(),
                title: manifest.title,
                name: None,
                version_needed: false,
            },
            declaration(linearization, manifest.languages),
            Source::new(dir, &manifest.version),
            artifact,
            Icd11 {
                linearization,
                release: manifest.version,
                keys,
                scales,
                ids,
                codes,
                expressions: Interned::default(),
                clusters: RwLock::new(Vec::new()),
            },
        ))
    }

    /// The code system.
    #[must_use]
    pub fn linearization(&self) -> Linearization {
        self.system().linearization
    }

    fn at(ordinal: u32) -> Option<usize> {
        usize::try_from(ordinal).ok()
    }

    /// The unversioned URI of the entity at `ordinal`.
    fn uri(&self, ordinal: u32) -> String {
        Self::at(ordinal)
            .and_then(|i| self.system().ids.get(i))
            .map(|id| self.system().linearization.uri(id))
            .unwrap_or_default()
    }

    /// The code of the entity at `ordinal`: its entity id in a linearization,
    /// its URI in the Foundation.
    // NOTE: tx-ecosystem IG `icd-11.md` at 9c2bbce, ICD-11 Codes: in MMS and ICF
    // "the entity id is the code, and the normal form"; the Foundation's codes
    // are "for now, the full entity uri".
    fn code_of(&self, ordinal: u32) -> String {
        if self.system().linearization == Linearization::Foundation {
            return self.uri(ordinal);
        }
        Self::at(ordinal)
            .and_then(|i| self.system().ids.get(i))
            .cloned()
            .unwrap_or_default()
    }

    /// The short classification code of the entity at `ordinal`, when it has one.
    fn classification_code(&self, ordinal: u32) -> Option<String> {
        Self::at(ordinal)
            .and_then(|i| self.system().codes.get(i))
            .and_then(Clone::clone)
    }

    /// The entity ordinal a URI names, in either form.
    fn ordinal_of_uri(&self, uri: &str) -> Option<u32> {
        let id = self.system().linearization.id_of(uri)?;
        self.system().keys.get(Linearization::key_of(&id)?)
    }

    /// The entity ordinal the entity id `text` names, spelled exactly as the
    /// release spells it.
    fn ordinal_of_id(&self, text: &str) -> Option<u32> {
        let ordinal = self.system().keys.get(Linearization::key_of(text)?)?;
        let spelled = Self::at(ordinal).and_then(|i| self.system().ids.get(i))?;
        (spelled == text).then_some(ordinal)
    }

    /// The entity ordinal a linearization's token names: an entity id or a
    /// short classification code.
    // NOTE: tx-ecosystem IG `icd-11.md` at 9c2bbce, ICD-11 Codes: the
    // classification code is "an alternative spelling", and "the full entity
    // uri is not a code in MMS or ICF".
    fn resolve(&self, token: &str) -> Result<Option<u32>, ProviderError> {
        if let Some(ordinal) = self.ordinal_of_id(token) {
            return Ok(Some(ordinal));
        }
        Ok(self
            .stored()
            .store
            .ordinal(token)
            .map_err(storage)?
            .map(Ordinal::index))
    }

    /// The language this linearization's own titles are in: the first the
    /// artifact declares, which is the first language bundle it was built from.
    fn base_language(&self) -> Option<&str> {
        self.declaration().languages.first().map(String::as_str)
    }

    /// The title of the entity at `ordinal` in `language`, else in the base
    /// language, else in English, else any.
    fn title(&self, ordinal: u32, language: Option<&str>) -> Result<Option<String>, ProviderError> {
        let wanted = language
            .or_else(|| self.base_language())
            .map(stored::primary_subtag);
        self.stored().first_term(
            Ordinal::new(ordinal),
            &[wanted.as_deref(), Some("en"), None],
            |use_ordinal| use_ordinal == TITLE,
        )
    }
}

impl StoredSystem for Icd11 {
    /// The language of the titles this provider returns, so a `displayLanguage`
    /// the artifact carries no bundle for is answered as a language the system
    /// has no display in rather than satisfied by the base-language title.
    fn language(provider: &Icd11Provider) -> Option<&str> {
        provider.base_language()
    }

    /// In the Foundation an entity URI; in a linearization an entity id, a
    /// short code, or a postcoordination expression of either; case
    /// sensitive throughout.
    fn locate(provider: &Icd11Provider, code: &str) -> Result<Option<Located>, ProviderError> {
        let text = code.trim();
        let located = |ordinal: u32| Located {
            concept: Concept::new(ordinal),
            code: provider.code_of(ordinal),
        };
        let linearization = provider.system().linearization;
        if linearization == Linearization::Foundation {
            return Ok(provider.ordinal_of_uri(text).map(located));
        }
        if let Some(ordinal) = provider.ordinal_of_id(text) {
            return Ok(Some(located(ordinal)));
        }
        let Some(expression) = Expression::parse(text, linearization == Linearization::Icf) else {
            return Ok(None);
        };
        if expression.is_simple() {
            return Ok(provider.resolve(&expression.root.code)?.map(located));
        }
        provider.cluster(text, &expression)
    }

    fn code(provider: &Icd11Provider, concept: Concept) -> Result<Option<String>, ProviderError> {
        if concept.index() < provider.stored().concepts {
            return Ok(Some(provider.code_of(concept.index())));
        }
        Ok(provider.system().expressions.code(Concept::new(
            concept.index().saturating_sub(provider.stored().concepts),
        )))
    }

    fn display(
        provider: &Icd11Provider,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        if concept.index() < provider.stored().concepts {
            return provider.title(concept.index(), language);
        }
        match provider.cluster_of(concept) {
            Some(cluster) => Ok(Some(provider.cluster_display(&cluster, language)?)),
            None => Ok(None),
        }
    }

    // NOTE: tx-ecosystem IG `icd-11.md` at 9c2bbce, Filter Properties: the text
    // filter matches "the start of the classification code - so `XN8` finds
    // `XN8P1` and `XN8KD`".
    fn search_codes(provider: &Icd11Provider, text: &str) -> ConceptSet {
        let text = text.trim();
        let mut concepts = ConceptSet::new();
        if text.is_empty() {
            return concepts;
        }
        for (ordinal, code) in (0_u32..).zip(&provider.system().codes) {
            let starts = code.as_deref().is_some_and(|code| {
                code.get(..text.len())
                    .is_some_and(|head| head.eq_ignore_ascii_case(text))
            });
            if starts {
                concepts.insert(ordinal);
            }
        }
        concepts
    }

    fn definition(
        provider: &Icd11Provider,
        concept: Concept,
    ) -> Result<Option<String>, ProviderError> {
        if concept.index() >= provider.stored().concepts {
            return Ok(None);
        }
        let Some(key) = provider.stored().key_of("definition") else {
            return Ok(None);
        };
        Ok(provider
            .stored()
            .store
            .property(Ordinal::new(concept.index()), key)
            .map_err(storage)?
            .into_iter()
            .find_map(|v| match v {
                record::PropertyValueRef::String(s) => Some(s.to_owned()),
                _ => None,
            }))
    }

    fn advisories(
        provider: &Icd11Provider,
        concept: Concept,
    ) -> Result<Vec<String>, ProviderError> {
        Ok(provider
            .cluster_of(concept)
            .map(|cluster| provider.cluster_advisories(&cluster))
            .unwrap_or_default())
    }

    fn is_postcoordinated(provider: &Icd11Provider, concept: Concept) -> bool {
        concept.index() >= provider.stored().concepts
    }

    fn status(provider: &Icd11Provider, concept: Concept) -> Result<Status, ProviderError> {
        // A grouper without a code of its own is `notSelectable`: abstract in an
        // expansion and refused by `$validate-code` with `abstract = false`.
        let codeless = concept.index() < provider.stored().concepts
            && Icd11Provider::at(concept.index())
                .and_then(|i| provider.system().codes.get(i))
                .is_some_and(Option::is_none);
        Ok(Status {
            standards_status: None,
            active: true,
            inactive_reason: None,
            abstract_concept: codeless,
            codeless,
        })
    }

    fn properties(
        provider: &Icd11Provider,
        concept: Concept,
    ) -> Result<Vec<Property>, ProviderError> {
        if concept.index() >= provider.stored().concepts {
            return match provider.cluster_of(concept) {
                Some(cluster) => provider.cluster_properties(&cluster, None),
                None => Ok(Vec::new()),
            };
        }
        let ordinal = concept.index();
        let mut out = vec![Property {
            code: String::from("code"),
            value: Some(PropertyValue::Code(provider.code_of(ordinal))),
            ..Property::default()
        }];
        if let Some(code) = provider.classification_code(ordinal) {
            out.push(Property {
                code: String::from("classificationCode"),
                value: Some(PropertyValue::Code(code)),
                ..Property::default()
            });
        }
        for (key, values) in provider
            .stored()
            .store
            .properties(Ordinal::new(ordinal))
            .map_err(storage)?
        {
            let Some(name) = provider.stored().keys.get(&key) else {
                continue;
            };
            if name == "definition" || name == "id" {
                continue;
            }
            for value in values {
                let value = match value {
                    record::PropertyValueRef::Code(c) => PropertyValue::Code(c.to_owned()),
                    record::PropertyValueRef::String(s) => PropertyValue::String(s.to_owned()),
                    record::PropertyValueRef::Boolean(b) => PropertyValue::Boolean(b),
                    record::PropertyValueRef::Integer(i) => PropertyValue::Integer(i),
                    record::PropertyValueRef::Decimal(d) => PropertyValue::Decimal(d.to_owned()),
                    record::PropertyValueRef::DateTime(d) => PropertyValue::DateTime(d.to_owned()),
                    record::PropertyValueRef::Concept(_) => continue,
                };
                out.push(Property {
                    code: name.clone(),
                    value: Some(value),
                    ..Property::default()
                });
            }
        }
        for parent in provider.stored().tree.parents(concept) {
            out.push(Property {
                code: String::from("parent"),
                value: Some(PropertyValue::Code(provider.code_of(parent))),
                ..Property::default()
            });
        }
        for child in provider.stored().tree.children(concept) {
            out.push(Property {
                code: String::from("child"),
                value: Some(PropertyValue::Code(provider.code_of(child))),
                ..Property::default()
            });
        }
        Ok(out)
    }

    /// `<entity uri>/postcoordinationScale/<axis>`, in either URI form: the
    /// stem's scale as a value set, one `is-a` include per scale entity.
    fn implicit_value_set(
        provider: &Icd11Provider,
        url: &str,
    ) -> Option<Result<Compose, ProviderError>> {
        let (entity, axis) = url.rsplit_once(SCALE_SEGMENT)?;
        let stem = provider.ordinal_of_uri(entity)?;
        let scale = provider
            .system()
            .scales
            .get(&stem)?
            .iter()
            .find(|s| axis_short(&s.axis) == axis)?;
        let include = scale
            .entities
            .iter()
            .map(|&e| Include {
                system: Some(SystemRef {
                    url: provider.identity().url.clone(),
                    version: Some(provider.system().release.clone()),
                }),
                filters: vec![Filter {
                    property: String::from("concept"),
                    op: FilterOperator::IsA,
                    value: provider.code_of(e),
                }],
                ..Include::default()
            })
            .collect();
        Some(Ok(Compose {
            include,
            ..Compose::default()
        }))
    }

    /// A scale's value set carries the release as its version and date, and a
    /// name and title built from the stem and the axis (the WHO ICD-API's
    /// shape, which the ecosystem's `icd-11` cases expect).
    fn implicit_metadata(provider: &Icd11Provider, url: &str) -> crate::provider::ImplicitMetadata {
        let Some((entity, axis)) = url.rsplit_once(SCALE_SEGMENT) else {
            return crate::provider::ImplicitMetadata::default();
        };
        let stem = entity.rsplit('/').next().unwrap_or(entity);
        crate::provider::ImplicitMetadata {
            version: Some(provider.system().release.clone()),
            name: Some(format!("PostcoordinationScale_{stem}_{axis}")),
            title: Some(format!("Postcoordination scale {axis} of {entity}")),
            experimental: Some(false),
            date: Some(provider.system().release.clone()),
            description: None,
            copyright: None,
        }
    }
}

/// What the code system declares.
fn declaration(linearization: Linearization, languages: Vec<String>) -> Declaration {
    let mut properties: Vec<PropertyDefinition> = [
        (
            "code",
            PropertyKind::Code,
            "The code: the entity id (MMS, ICF) or URI (Foundation)",
        ),
        (
            "classificationCode",
            PropertyKind::Code,
            "The short classification code, where there is one",
        ),
        ("parent", PropertyKind::Code, "A parent of the entity"),
        ("child", PropertyKind::Code, "A child of the entity"),
        (
            "classKind",
            PropertyKind::Code,
            "chapter, block, category, or window",
        ),
        (
            "notSelectable",
            PropertyKind::Boolean,
            "The entity has no short code and cannot be used to code with",
        ),
        ("definition", PropertyKind::String, "The definition"),
        ("exclusion", PropertyKind::String, "An exclusion"),
        ("source", PropertyKind::Code, "The Foundation entity's URI"),
        ("browserUrl", PropertyKind::String, "The ICD-11 browser URL"),
    ]
    .into_iter()
    .map(|(code, kind, description)| PropertyDefinition {
        code: code.to_owned(),
        uri: None,
        description: Some(description.to_owned()),
        kind,
    })
    .collect();
    let capabilities = BTreeSet::from([Capability::Subsumption, Capability::Enumeration]);
    let mut implicit_forms = Vec::new();
    if linearization != Linearization::Foundation {
        // NOTE: `postcoordination` has no value of its own, and `CodeSystem.property.type`
        // (<https://hl7.org/fhir/R4B/codesystem-definitions.html#CodeSystem.property.type>)
        // names no type for one, so it is returned and not declared.
        properties.push(PropertyDefinition {
            code: String::from("stem"),
            uri: None,
            description: Some(String::from("The root of a postcoordinated expression")),
            kind: PropertyKind::Coding,
        });
        // NOTE: no FHIR/SNOMED spec governs this: our own design, the WHO
        // ICD-API's scale URI (<https://icd.who.int/icdapi>) with two placeholders.
        implicit_forms.push(ImplicitForm::new(
            "[entity]/postcoordinationScale/[axis]",
            ImplicitArgument::Expression,
        ));
    }
    Declaration {
        content: ContentMode::NotPresent,
        case_sensitive: true,
        hierarchy_meaning: Some(HierarchyMeaning::ClassifiedWith),
        // NOTE: a linearization defines postcoordination clusters
        // (<https://icd.who.int/icdapi>), and this provider validates and
        // locates one; the foundation defines no such grammar.
        // NOTE: WHO states no subsumption of a cluster under its stem's
        // ancestors (<https://icd.who.int/icdapi>), so an expansion of this
        // system lists every member and is not unclosed.
        compositional: if linearization == Linearization::Foundation {
            Compositional::None
        } else {
            Compositional::Supported
        },
        languages,
        properties,
        filters: Vec::new(),
        capabilities,
        implicit_forms,
    }
}
