// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Opening one built edition: the manifest, the side files the ECL evaluator
//! reads, the vocabulary keys, and the base language.

use super::implicit::implicit_forms;
use super::{
    EXPRESSIONS, FHIR_PROPERTIES, GENERATED_PROPERTIES, Keys, Manifest, OpenError, SYSTEM, Snomed,
    SnomedProvider,
};
use crate::provider::Identity;
use crate::provider::declaration::{
    Capability, Compositional, ContentMode, Declaration, FilterDefinition, HierarchyMeaning,
    PropertyDefinition, PropertyKind,
};
use crate::provider::filter::FilterOperator;
use crate::provider::source::Source;
use crate::stored::artifact::{Artifact, Layout};
use crate::stored::{self, primary_subtag};
use concept_graph::attributes::Attributes;
use concept_graph::identifiers::Identifiers;
use concept_graph::members::Memberships;
use concept_graph::refsets::RefsetMembers;
use concept_store::store::{Store, Vocabulary};
use rf2::constants;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

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
                scg: Mutex::new(super::expression::Expressions::default()),
                attribute_ordinals: OnceLock::new(),
                inactive: OnceLock::new(),
                keys,
                edition: manifest.edition,
                base_language,
            },
        ))
    }

    pub(super) fn resolve_keys(store: &Store) -> Result<Keys, OpenError> {
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
    pub(super) fn filter_definitions() -> Vec<FilterDefinition> {
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
    pub(super) fn read_ecl_files(
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

    /// The language this edition's own displays are in: the configured
    /// `default_language` when the edition carries descriptions in it, else
    /// the first language the edition declares.
    ///
    /// No FHIR/SNOMED spec governs this: our own design. An edition that
    /// carries no description in the configured tag would otherwise answer a
    /// display in a language it never states.
    pub(super) fn base_language(default_language: &str, declared: &[String]) -> String {
        let wanted = primary_subtag(default_language);
        if declared.is_empty() || declared.iter().any(|l| primary_subtag(l) == wanted) {
            return wanted;
        }
        declared
            .first()
            .map_or(wanted, |first| primary_subtag(first))
    }
}
