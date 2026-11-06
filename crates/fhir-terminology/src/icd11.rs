// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ICD-11 provider: one of the three WHO code systems (the MMS and ICF
//! linearizations, the Foundation) read from its artifact directory.
//!
//! The contract is the HL7 terminology ecosystem test cases for ICD-11 over
//! THO's identity page (<https://terminology.hl7.org/CodeSystem-ICD11MMS.html>):
//! a code is a short code (`1A00`), an entity URI in the unversioned or the
//! versioned form, or, in a linearization, a postcoordination expression;
//! `id`, `code`, `parent`, and `child` are URI-valued properties; an entity
//! without a short code is `notSelectable`; a stem's postcoordination scales
//! are properties and implicit value sets
//! (`<uri>/postcoordinationScale/<axis>`); an expression's `stem` and
//! `postcoordinationValues` report how its values bound to the axes. The
//! axis binding rule (an unfilled axis first, then required axes, then WHO's
//! order) and the reading of `/` (a value when it fits an unfilled axis of the
//! stem before it, else a new stem) are the suite's, recorded in its
//! documentation; no FHIR specification governs them.

mod postcoordination;

use postcoordination::Cluster;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::RwLock;

use ::icd11::Linearization;
use ::icd11::expression::{Expression, Token};
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
    CodeSystemProvider, Concept, Hierarchy, Identity, Located, Property, PropertyValue, Status,
    Subproperty, error::ProviderError,
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
    let mut id = String::new();
    for (k, values) in store.properties(ordinal)? {
        if k == key
            && let Some(record::PropertyValue::Code(uri)) = values.first()
            && let Some(entity) = linearization.id_of(uri)
        {
            id = entity;
        }
    }
    Ok(id)
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

    /// The short code of the entity at `ordinal`, else its URI.
    fn code_or_uri(&self, ordinal: u32) -> String {
        Self::at(ordinal)
            .and_then(|i| self.system().codes.get(i))
            .and_then(Clone::clone)
            .unwrap_or_else(|| self.uri(ordinal))
    }

    /// The entity ordinal a URI names, in either form.
    fn ordinal_of_uri(&self, uri: &str) -> Option<u32> {
        let id = self.system().linearization.id_of(uri)?;
        self.system().keys.get(Linearization::key_of(&id)?)
    }

    /// The entity ordinal a token names: a URI in either form, or a short
    /// code (the linearizations only; the Foundation has no short codes).
    fn resolve(&self, token: &Token) -> Result<Option<u32>, ProviderError> {
        if token.uri {
            return Ok(self.ordinal_of_uri(&token.text));
        }
        if self.system().linearization == Linearization::Foundation {
            return Ok(None);
        }
        Ok(self
            .stored()
            .store
            .ordinal(&token.text)
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

    /// A short code, an entity URI in either form, or (in a linearization) a
    /// postcoordination expression; case sensitive throughout.
    fn locate(provider: &Icd11Provider, code: &str) -> Result<Option<Located>, ProviderError> {
        let text = code.trim();
        if text.is_empty() {
            return Ok(None);
        }
        let icf = provider.system().linearization == Linearization::Icf;
        let Some(expression) = Expression::parse(text, icf) else {
            return Ok(None);
        };
        if expression.is_simple() {
            let Some(token) = expression.members.first().map(|m| &m.stem) else {
                return Ok(None);
            };
            return Ok(provider.resolve(token)?.map(|ordinal| Located {
                concept: Concept::new(ordinal),
                code: provider.code_or_uri(ordinal),
            }));
        }
        if provider.system().linearization == Linearization::Foundation {
            return Ok(None);
        }
        provider.cluster(text, &expression)
    }

    fn code(provider: &Icd11Provider, concept: Concept) -> Result<Option<String>, ProviderError> {
        if concept.index() < provider.stored().concepts {
            return Ok(Some(provider.code_or_uri(concept.index())));
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
            .properties(Ordinal::new(concept.index()))
            .map_err(storage)?
            .into_iter()
            .find(|(k, _)| *k == key)
            .and_then(|(_, values)| {
                values.into_iter().find_map(|v| match v {
                    record::PropertyValue::String(s) => Some(s),
                    _ => None,
                })
            }))
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
        let mut out = Vec::new();
        if let Some(code) = Icd11Provider::at(ordinal)
            .and_then(|i| provider.system().codes.get(i))
            .and_then(Clone::clone)
        {
            out.push(Property {
                code: String::from("code"),
                value: PropertyValue::Code(code),
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
            if name == "definition" {
                continue;
            }
            for value in values {
                let value = match value {
                    record::PropertyValue::Code(c) => PropertyValue::Code(c),
                    record::PropertyValue::String(s) => PropertyValue::String(s),
                    record::PropertyValue::Boolean(b) => PropertyValue::Boolean(b),
                    record::PropertyValue::Integer(i) => PropertyValue::Integer(i),
                    record::PropertyValue::Decimal(d) => PropertyValue::Decimal(d),
                    record::PropertyValue::DateTime(d) => PropertyValue::DateTime(d),
                    record::PropertyValue::Concept(_) => continue,
                };
                out.push(Property {
                    code: name.clone(),
                    value,
                    ..Property::default()
                });
            }
        }
        for parent in provider.stored().tree.parents(concept) {
            out.push(Property {
                code: String::from("parent"),
                value: PropertyValue::Code(provider.uri(parent)),
                ..Property::default()
            });
        }
        for child in provider.stored().tree.children(concept) {
            out.push(Property {
                code: String::from("child"),
                value: PropertyValue::Code(provider.uri(child)),
                ..Property::default()
            });
        }
        for scale in provider.system().scales.get(&ordinal).into_iter().flatten() {
            out.push(Property {
                code: String::from("postcoordinationScale"),
                value: PropertyValue::Code(scale.axis.clone()),
                subproperties: vec![
                    Subproperty {
                        code: String::from("valueSet"),
                        value: PropertyValue::Uri(format!(
                            "{}{SCALE_SEGMENT}{}",
                            provider.uri(ordinal),
                            axis_short(&scale.axis)
                        )),
                        description: None,
                    },
                    Subproperty {
                        code: String::from("requiredPostcoordination"),
                        value: PropertyValue::Boolean(scale.required),
                        description: None,
                    },
                    Subproperty {
                        code: String::from("allowMultipleValues"),
                        value: PropertyValue::Code(scale.multiple.clone()),
                        description: None,
                    },
                ],
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
                    value: provider.uri(e),
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
        ("id", PropertyKind::Code, "The entity URI"),
        ("code", PropertyKind::Code, "The short code"),
        ("parent", PropertyKind::Code, "The parent entity's URI"),
        ("child", PropertyKind::Code, "A child entity's URI"),
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
        for (code, description) in [
            ("postcoordinationScale", "An axis the stem takes values on"),
            ("stem", "A stem of a postcoordination expression"),
            (
                "postcoordinationValues",
                "A value of a postcoordination expression, on its axis",
            ),
        ] {
            properties.push(PropertyDefinition {
                code: code.to_owned(),
                uri: None,
                description: Some(description.to_owned()),
                kind: PropertyKind::Code,
            });
        }
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
