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

pub mod edition;
mod implicit;
mod open;
mod selection;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

mod conceptmap;
mod ecl;
pub mod expression;

use concept_graph::attributes::{Attributes, AttributesError};
use concept_graph::identifiers::{Identifiers, IdentifiersError};
use concept_graph::members::{MembersError, Memberships};
use concept_graph::refsets::{RefsetMembers, RefsetsError};
use concept_store::record;
use concept_store::store::StoreError;
use rf2::constants;
use rf2::id::ConceptId;
use roaring::RoaringBitmap;
use sct_ecl::ast::ExpressionConstraint;
use serde::Deserialize;

use crate::provider::compose::{Compose, ConceptRef, Include, SystemRef};
use crate::provider::declaration::PropertyKind;
use crate::provider::filter::{Filter, FilterOperator};
use crate::provider::{
    CodeSystemProvider, Concept, ConceptSet, DesignationUse, Located, Property, PropertyValue,
    Status, error::ProviderError,
};
use crate::stored::system::{StoredProvider, StoredSystem};
use crate::stored::{self, primary_subtag, storage};
use implicit::{implicit_parts, percent_decode};

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
    ) -> Option<Result<crate::provider::concept_map::ConceptMapModel, ProviderError>> {
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
                            code: located.code,
                            ..ConceptRef::default()
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
