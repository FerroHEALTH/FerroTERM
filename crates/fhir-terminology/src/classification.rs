// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The classification provider: one `ClaML` or ICD-10-CM release read from its
//! artifact directory (`store.redb`, `hierarchy.bin`, `text.bin`, and
//! `manifest.json` with `kind = classification`).
//!
//! The FHIR ICD page is the contract (<https://hl7.org/fhir/R4B/icd.html>):
//! codes carry the period, no filters and no implicit value sets are defined,
//! and the hierarchy is `classified-with`
//! (<https://terminology.hl7.org/ICD.html>). The provider therefore offers
//! the generic filters over the tree (`concept is-a`, `descendent-of`, ...),
//! `kind`, `usage`, `valid`, and every note kind as properties and filters,
//! and the title in the requested language as the display.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

use crate::provider::declaration::{
    Capability, Compositional, ContentMode, Declaration, FilterDefinition, HierarchyMeaning,
    PropertyDefinition, PropertyKind,
};
use crate::provider::filter::FilterOperator;
use crate::provider::source::Source;
use crate::provider::{
    CodeSystemProvider, Concept, Identity, Located, Property, Status, error::ProviderError,
};
use crate::stored;
use crate::stored::artifact::{Artifact, Layout};
use crate::stored::system::{StoredProvider, StoredSystem};

/// The manifest `kind` of an artifact this provider opens.
pub const KIND: &str = "classification";
/// The designation use ordinal of the title.
const PREFERRED: u32 = 0;
/// The property keys the build writes beside the note kinds.
const KIND_KEY: &str = "kind";
const USAGE_KEY: &str = "usage";
const VALID_KEY: &str = "valid";

/// A failure to open an artifact as a classification.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// The artifact is of another kind.
    #[error("the artifact is `{0}`, not a classification")]
    NotClassification(String),
    /// The artifact's shared files do not read.
    #[error(transparent)]
    Artifact(#[from] stored::OpenError),
}

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    kind: String,
    system: String,
    version: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    language: String,
    /// `classified-with` for a tree; absent for a flat table.
    #[serde(default, rename = "hierarchyMeaning")]
    hierarchy_meaning: Option<String>,
    #[serde(default)]
    languages: Vec<String>,
}

/// What a classification decides: the ICD display rule over its titles.
#[derive(Debug)]
pub struct Classification {
    /// The language the display falls back to.
    language: String,
}

/// One classification release behind the seam.
pub type ClassificationProvider = StoredProvider<Classification>;

impl ClassificationProvider {
    /// Opens the artifact directory `dir`.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError`] when the manifest, the store, or a side file
    /// does not read, or the artifact is not a classification.
    pub fn open(dir: &Path) -> Result<Self, OpenError> {
        let (artifact, manifest) = Artifact::open(dir, Layout::Tree, |manifest: &Manifest| {
            stored::serves(&manifest.kind, KIND, OpenError::NotClassification)
        })?;
        let tree = manifest.hierarchy_meaning.is_some();
        Ok(Self::new(
            Identity {
                url: manifest.system,
                title: (!manifest.title.is_empty()).then_some(manifest.title),
                name: None,
                version_needed: false,
                version: manifest.version.clone(),
            },
            declaration(&artifact.keys, manifest.languages, tree),
            Source::new(dir, &manifest.version),
            artifact,
            Classification {
                language: if manifest.language.is_empty() {
                    String::from("en")
                } else {
                    manifest.language
                },
            },
        ))
    }
}

impl StoredSystem for Classification {
    /// The language the release states for itself (the `ClaML` `Meta` element
    /// named `lang`), so a `displayLanguage` it carries no rubric for is
    /// answered as a language the system has no display in
    /// (`OperationDefinition/CodeSystem-validate-code`: `displayLanguage`
    /// "Specifies the language to be used for description when validating the
    /// display property").
    fn language(provider: &ClassificationProvider) -> Option<&str> {
        Some(&provider.system().language)
    }

    fn locate(
        provider: &ClassificationProvider,
        code: &str,
    ) -> Result<Option<Located>, ProviderError> {
        // NOTE: ICD codes are represented with the period included
        // (<https://hl7.org/fhir/R4B/icd.html>); a code without it is not the code.
        provider.stored().located(code.trim())
    }

    /// The title in the requested language, else in the classification's own
    /// language, else any.
    fn display(
        provider: &ClassificationProvider,
        concept: Concept,
        language: Option<&str>,
    ) -> Result<Option<String>, ProviderError> {
        provider.stored().display_in(
            concept,
            &provider.system().language,
            language,
            |use_ordinal| use_ordinal == PREFERRED,
        )
    }

    fn status(
        provider: &ClassificationProvider,
        concept: Concept,
    ) -> Result<Status, ProviderError> {
        Ok(Status {
            active: provider.stored().active(concept)?,
            ..Status::default()
        })
    }

    fn properties(
        provider: &ClassificationProvider,
        concept: Concept,
    ) -> Result<Vec<Property>, ProviderError> {
        provider
            .stored()
            .listed_properties(concept, provider.declaration().hierarchy_meaning.is_some())
    }
}

/// What the provider declares: `kind`, `usage`, `valid`, `parent`, `child`,
/// and every note kind as properties; `kind`, `usage`, `valid`, and the note
/// kinds as filters.
fn declaration(keys: &BTreeMap<u32, String>, languages: Vec<String>, tree: bool) -> Declaration {
    let mut properties = if tree {
        vec![
            PropertyDefinition {
                code: String::from("parent"),
                uri: None,
                description: Some(String::from("The class this one is classified under")),
                kind: PropertyKind::Code,
            },
            PropertyDefinition {
                code: String::from("child"),
                uri: None,
                description: Some(String::from("A class classified under this one")),
                kind: PropertyKind::Code,
            },
        ]
    } else {
        Vec::new()
    };
    let mut filters = Vec::new();
    for name in keys.values() {
        let (kind, operators, value, description) = match name.as_str() {
            KIND_KEY => (
                PropertyKind::Code,
                vec![FilterOperator::Equal, FilterOperator::In],
                "chapter | block | category | subcategory",
                "The level of the class",
            ),
            USAGE_KEY => (
                PropertyKind::Code,
                vec![
                    FilterOperator::Equal,
                    FilterOperator::In,
                    FilterOperator::Exists,
                ],
                "dagger | aster",
                "The usage mark of the class",
            ),
            VALID_KEY => (
                PropertyKind::Boolean,
                vec![FilterOperator::Equal],
                "true | false",
                "Whether the code is valid for use as a code",
            ),
            _ => (
                PropertyKind::String,
                vec![
                    FilterOperator::Equal,
                    FilterOperator::Regex,
                    FilterOperator::Exists,
                ],
                "a note text",
                "A note of this kind on the class",
            ),
        };
        properties.push(PropertyDefinition {
            code: name.clone(),
            uri: None,
            description: Some(description.to_owned()),
            kind,
        });
        filters.push(FilterDefinition {
            code: name.clone(),
            description: Some(description.to_owned()),
            operators,
            value: value.to_owned(),
        });
    }
    Declaration {
        content: ContentMode::NotPresent,
        case_sensitive: true,
        hierarchy_meaning: tree.then_some(HierarchyMeaning::ClassifiedWith),
        compositional: Compositional::None,
        languages,
        properties,
        filters,
        capabilities: if tree {
            BTreeSet::from([Capability::Subsumption, Capability::Enumeration])
        } else {
            BTreeSet::from([Capability::Enumeration])
        },
        implicit_forms: Vec::new(),
    }
}
