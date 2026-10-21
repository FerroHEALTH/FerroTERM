// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The implicit concept maps of SNOMED CT: `http://snomed.info/sct?fhir_cm=[sctid]`.
//!
//! The FHIR SNOMED CT page defines the form over four association reference
//! sets, gives each the relationship its members assert, and fixes the shape of
//! the `ConceptMap` it produces
//! (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Concept Maps"). An
//! association member points at another SNOMED concept through
//! `targetComponentId`, so its group names SNOMED as both source and target.
//!
//! The same page says a map reference set also defines an implicit concept
//! map, then gives the query part only the four association ids and one
//! template whose group target is SNOMED, so the shape of a map reference
//! set's concept map is spec-silent and ours. A map member points at a code of
//! another system through `mapTarget`, and no RF2 row records which system
//! that is, so the reference set says which scheme it maps to and
//! [`MAP_SCHEMES`] holds the FHIR URI of each. Carrying the complex and
//! extended map columns as `product` parts is our own design too.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use concept_graph::ordinal::Ordinal;
use concept_graph::refsets::{Table, ValueRef};

use crate::conceptmap::model::{ConceptMapModel, DependsOn, Element, Group, Relationship, Target};
use crate::provider::{CodeSystemProvider, Concept, MapSelection, ProviderError, Successor};
use crate::snomed::{FHIR_CM, FHIR_VS, SYSTEM, SnomedProvider};

/// The reference set field an association member points through.
const TARGET_COMPONENT: &str = "targetComponentId";
/// The reference set field a map member points through.
const MAP_TARGET: &str = "mapTarget";
/// The complex and extended map fields that travel as `product` parts, in the
/// order the RF2 release format declares them.
///
/// No FHIR version says where these belong in a `$translate` result, so this
/// is our own design: `product` keeps them beside the target they qualify.
const PRODUCT_FIELDS: [&str; 6] = [
    "mapGroup",
    "mapPriority",
    "mapRule",
    "mapAdvice",
    "correlationId",
    "mapCategoryId",
];

/// One map reference set: its concept id and the code system its `mapTarget`
/// codes belong to.
struct MapScheme {
    /// The reference set concept id.
    refset: u64,
    /// The FHIR URI of the code system the targets are codes of.
    system: &'static str,
}

/// The map reference sets this server names a target code system for, each
/// with the URI FHIR publishes for the scheme it maps to.
///
/// `ConceptMap.group.target` is "An absolute URI that identifies the target
/// system that the concepts will be mapped to", and R4B says it "is not needed
/// if the target value set is specified and it contains concepts from only a
/// single system" or if "all of the target element equivalence values are
/// 'unmatched'"
/// (<https://hl7.org/fhir/R4B/conceptmap-definitions.html#ConceptMap.group.target>).
/// An implicit map states no target value set and its targets are real codes,
/// so neither case applies and the group states the system. RF2 records the
/// scheme nowhere on a member row, so the reference set says which one it is.
const MAP_SCHEMES: [MapScheme; 5] = [
    // `447562003`, the ICD-10 extended map; its target is WHO ICD-10 (SNOMED CT to
    // ICD-10 Map Specification §2, §7), whose URI is
    // <https://hl7.org/fhir/R4B/icd.html> §Summary.
    MapScheme {
        refset: 447_562_003,
        system: "http://hl7.org/fhir/sid/icd-10",
    },
    // `447563008 |SNOMED CT to ICD-9-CM equivalence complex map reference set|` (RF2
    // §5.2.3.3); the ICD-9-CM URI of <https://hl7.org/fhir/R4B/icd.html> §Summary.
    MapScheme {
        refset: 447_563_008,
        system: "http://hl7.org/fhir/sid/icd-9-cm",
    },
    // `446608001`, the ICD-O map (SNOMED CT Terminology Services Guide §4.12); the
    // preferred `uri` of the active `icd-o-3` NamingSystem of `hl7.terminology`.
    MapScheme {
        refset: 446_608_001,
        system: "http://terminology.hl7.org/CodeSystem/icd-o-3",
    },
    // `900000000000497000`, the map to NHS Clinical Terms Version 3 (SNOMED CT
    // Terminology Services Guide §4.12); the preferred `uri` of the `read-Codes`
    // NamingSystem of `hl7.terminology`, defined as Clinical Terms Version 3.
    MapScheme {
        refset: 900_000_000_000_497_000,
        system: "http://terminology.hl7.org/CodeSystem/read-Codes",
    },
    // `6011000124106 |ICD-10-CM complex map reference set|` of the US Edition (NLM,
    // "SNOMED CT to ICD-10-CM Map"); the ICD-10-CM URI of
    // <https://hl7.org/fhir/R4B/icd.html> §ICD-10 variants.
    MapScheme {
        refset: 6_011_000_124_106,
        system: "http://hl7.org/fhir/sid/icd-10-cm",
    },
];

/// One historical association reference set: its concept id and the
/// `ConceptMap` relationship its members carry.
struct Association {
    /// The reference set concept id.
    refset: u64,
    /// The relationship a member of it asserts.
    relationship: Relationship,
}

/// The association reference sets the FHIR SNOMED CT page lists as implicit
/// concept maps, each with the relationship that page gives it
/// (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Concept Maps").
const ASSOCIATIONS: [Association; 4] = [
    // `900000000000523009 |POSSIBLY EQUIVALENT TO association reference set|`.
    Association {
        refset: 900_000_000_000_523_009,
        relationship: Relationship::Inexact,
    },
    // `900000000000526001 |REPLACED BY association reference set|`.
    Association {
        refset: 900_000_000_000_526_001,
        relationship: Relationship::Equivalent,
    },
    // `900000000000527005 |SAME AS association reference set|`.
    Association {
        refset: 900_000_000_000_527_005,
        relationship: Relationship::Equal,
    },
    // `900000000000530003 |ALTERNATIVE association reference set|`.
    Association {
        refset: 900_000_000_000_530_003,
        relationship: Relationship::Inexact,
    },
];

/// The reference sets a translation of an inactive concept falls back to: the
/// concept a `SAME AS` names is the same concept, and the one `REPLACED BY`
/// names supersedes it.
const SUCCESSORS: [u64; 2] = [sct_ecl::eval::SAME_AS, sct_ecl::eval::REPLACED_BY];

/// The relationship the members of `refset` assert, when it is a historical
/// association reference set.
fn association(refset: u64) -> Option<Relationship> {
    ASSOCIATIONS
        .iter()
        .find(|held| held.refset == refset)
        .map(|held| held.relationship)
}

/// The code system the `mapTarget` codes of `refset` belong to, when this
/// server names one for it.
fn scheme(refset: u64) -> Option<&'static str> {
    MAP_SCHEMES
        .iter()
        .find(|held| held.refset == refset)
        .map(|held| held.system)
}

/// The concepts this edition's `SAME AS` and `REPLACED BY` reference sets name
/// in place of `concept`.
///
/// # Errors
///
/// Returns the storage error of a concept that does not read.
pub(crate) fn successors(
    edition: &SnomedProvider,
    concept: Concept,
) -> Result<Vec<Successor>, ProviderError> {
    let ordinal = Ordinal::new(concept.index());
    let mut out = Vec::new();
    for refset in SUCCESSORS {
        let Some(table) = edition.member_tables().table(refset) else {
            continue;
        };
        if !table.members().contains(ordinal.index()) {
            continue;
        }
        let Some(field) = table.field(TARGET_COMPONENT) else {
            continue;
        };
        let relationship = association(refset).unwrap_or(Relationship::RelatedTo);
        for row in table.rows_of(ordinal) {
            let Some(target) = association_target(edition, table, row, field, Some(relationship))?
            else {
                continue;
            };
            let Some(code) = target.code else {
                continue;
            };
            out.push(Successor {
                code,
                display: target.display,
                relationship,
                map: format!("{SYSTEM}?{FHIR_CM}={refset}"),
            });
        }
    }
    Ok(out)
}

/// The `ConceptMap` the implicit URI `url` denotes, over the reference set
/// `refset` of `edition`, carrying the elements `selection` asks for.
///
/// The map is built from the reference set on every call, so a narrowed
/// selection reads only the rows its own elements come from: the cost of a
/// translation is the size of its answer, never the size of the release. The
/// selection picks the rows and nothing else, so a narrowed map is the whole
/// map with the other elements left out.
///
/// # Errors
///
/// Returns [`ProviderError::UnknownImplicitConceptMap`] when the edition holds
/// no such reference set, [`ProviderError::UnnamedConceptMapTarget`] when it is
/// a map reference set whose scheme [`MAP_SCHEMES`] does not name, and the
/// storage error of a concept that does not read.
pub(crate) fn concept_map(
    edition: &SnomedProvider,
    url: &str,
    refset: u64,
    selection: MapSelection<'_>,
) -> Result<ConceptMapModel, ProviderError> {
    let Some(table) = edition.member_tables().table(refset) else {
        return Err(ProviderError::UnknownImplicitConceptMap {
            url: url.to_owned(),
        });
    };
    let relationship = association(refset);
    let target_component = table.field(TARGET_COMPONENT);
    let map_target = table.field(MAP_TARGET);
    // NOTE: a reference set with neither column maps nothing, so the URI names no
    // concept map (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Concept Maps").
    if target_component.is_none() && map_target.is_none() {
        return Err(ProviderError::UnknownImplicitConceptMap {
            url: url.to_owned(),
        });
    }
    // NOTE: a bare `http://snomed.info/sct` base means an unspecified edition, and the
    // server SHALL answer from the edition it serves, so the map states that version
    // either way (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Concept Maps").
    let base = edition.identity().version.clone();
    // A member points at another SNOMED concept through `targetComponentId`, and at a
    // code of the reference set's own scheme through `mapTarget`.
    let (target_system, target_version) = if target_component.is_some() {
        (SYSTEM.to_owned(), Some(base.clone()))
    } else {
        let named = scheme(refset).ok_or_else(|| ProviderError::UnnamedConceptMapTarget {
            url: url.to_owned(),
            reason: format!("no code system URI is recorded for map reference set `{refset}`"),
        })?;
        // NOTE: RF2 records no version of the scheme a `mapTarget` code comes from, so
        // the group states the system alone (RF2 §Map Reference Sets).
        (named.to_owned(), None)
    };
    // NOTE: no FHIR version orders `group.element`, so the map states them in the
    // edition's own concept order, which a selection of them reproduces exactly
    // (<https://hl7.org/fhir/R4B/conceptmap.html>).
    let mut elements: BTreeMap<u32, Element> = BTreeMap::new();
    for row in selected_rows(edition, table, target_component, map_target, selection)? {
        let Some(source) = table.concept(row) else {
            continue;
        };
        let target = match (target_component, map_target) {
            (Some(field), _) => association_target(edition, table, row, field, relationship)?,
            (None, Some(field)) => map(edition, table, row, field),
            (None, None) => None,
        };
        let Some(target) = target else {
            continue;
        };
        match elements.entry(source.index()) {
            Entry::Occupied(mut held) => held.get_mut().targets.push(target),
            Entry::Vacant(slot) => {
                let Some(code) = code_of(edition, source)? else {
                    continue;
                };
                slot.insert(Element {
                    code: Some(code),
                    display: edition.display(Concept::new(source.index()), None)?,
                    no_map: false,
                    comment: None,
                    targets: vec![target],
                });
            }
        }
    }
    let elements: Vec<Element> = elements.into_values().collect();
    // NOTE: the page's template names the map after the reference set and scopes it
    // to the edition's own implicit value set
    // (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Concept Maps").
    let scope = Some(format!("{base}?{FHIR_VS}"));
    let refset_name = name_of(edition, refset);
    Ok(ConceptMapModel {
        // NOTE: an implicit concept map has no resource, and a logical id
        // belongs to one (<https://hl7.org/fhir/R4B/resource.html#id>).
        id: None,
        url: url.to_owned(),
        version: Some(base.clone()),
        name: refset_name
            .as_ref()
            .map(|name| format!("SNOMED CT {name} Concept Map")),
        title: None,
        status: String::from("active"),
        experimental: None,
        date: None,
        publisher: None,
        description: None,
        source_scope: scope.clone(),
        target_scope: scope,
        groups: vec![Group {
            source: Some(SYSTEM.to_owned()),
            source_version: Some(base),
            target: Some(target_system),
            target_version,
            elements,
            unmapped: None,
        }],
    })
}

/// The rows `selection` admits, in the order the reference set holds them.
///
/// A row is one mapping, so restricting the rows restricts the mappings and
/// nothing else: the elements built from them are the elements the whole map
/// would carry. The row scan compares an integer or an interned string per
/// row and reads no concept, so it costs the reference set's shape rather than
/// its content.
fn selected_rows(
    edition: &SnomedProvider,
    table: &Table,
    target_component: Option<usize>,
    map_target: Option<usize>,
    selection: MapSelection<'_>,
) -> Result<Vec<usize>, ProviderError> {
    match selection {
        MapSelection::Whole => Ok((0..table.len()).collect()),
        MapSelection::Source(code) => {
            let Some(located) = edition.locate(code)? else {
                return Ok(Vec::new());
            };
            let ordinal = Ordinal::new(located.concept.index());
            if !table.members().contains(ordinal.index()) {
                return Ok(Vec::new());
            }
            Ok(table.rows_of(ordinal).collect())
        }
        MapSelection::Target(code) => {
            let Some(field) = target_component.or(map_target) else {
                return Ok(Vec::new());
            };
            let wanted = target_values(edition, target_component.is_some(), code)?;
            Ok((0..table.len())
                .filter(|&row| {
                    table
                        .value(row, field)
                        .is_some_and(|held| wanted.contains(&held))
                })
                .collect())
        }
    }
}

/// The row values a target of `code` reads as, for the column the map targets
/// through.
///
/// An association names another component, which is a concept of this edition
/// or, when the edition does not hold it, the bare component id
/// (RF2 §Association Reference Set); a map row names a code of its own scheme
/// as text.
fn target_values<'a>(
    edition: &SnomedProvider,
    association: bool,
    code: &'a str,
) -> Result<Vec<ValueRef<'a>>, ProviderError> {
    if !association {
        return Ok(match code {
            "" => Vec::new(),
            text => vec![ValueRef::String(text)],
        });
    }
    let mut wanted = Vec::new();
    if let Some(located) = edition.locate(code)? {
        wanted.push(ValueRef::Concept(Ordinal::new(located.concept.index())));
    }
    if let Ok(id) = code.parse::<u64>() {
        wanted.push(ValueRef::Component(id));
    }
    Ok(wanted)
}

/// The name of the reference set concept `refset`, for the map's `name`.
fn name_of(edition: &SnomedProvider, refset: u64) -> Option<String> {
    let located = edition.locate(&refset.to_string()).ok()??;
    edition.display(located.concept, None).ok()?
}

/// The target of an association member: the concept `targetComponentId` names.
fn association_target(
    edition: &SnomedProvider,
    table: &Table,
    row: usize,
    field: usize,
    relationship: Option<Relationship>,
) -> Result<Option<Target>, ProviderError> {
    let (code, display) = match table.value(row, field) {
        Some(ValueRef::Concept(ordinal)) => (
            code_of(edition, ordinal)?,
            edition.display(Concept::new(ordinal.index()), None)?,
        ),
        // NOTE: a target outside the edition keeps its component id as the code, the
        // only identity the row carries (RF2 §Association Reference Set).
        Some(ValueRef::Component(id)) => (Some(id.to_string()), None),
        _ => (None, None),
    };
    Ok(code.map(|code| Target {
        code: Some(code),
        display,
        relationship: relationship.unwrap_or(Relationship::RelatedTo),
        comment: None,
        depends_on: Vec::new(),
        product: Vec::new(),
    }))
}

/// The target of a map member: the code `mapTarget` carries, with the complex
/// and extended columns as `product` parts.
///
/// No specification says which `equivalence` an RF2 map row asserts, so the
/// target is `relatedto`, the code for concepts that overlap in meaning by an
/// unstated relationship
/// (<https://hl7.org/fhir/R4B/valueset-concept-map-equivalence.html>); the
/// row's own `correlationId` travels with it as a `product` part.
fn map(edition: &SnomedProvider, table: &Table, row: usize, field: usize) -> Option<Target> {
    let code = match table.value(row, field)? {
        ValueRef::String(text) if !text.is_empty() => text.to_owned(),
        _ => return None,
    };
    let product = PRODUCT_FIELDS
        .into_iter()
        .filter_map(|name| part(edition, table, row, name))
        .collect();
    Some(Target {
        code: Some(code),
        display: None,
        relationship: Relationship::RelatedTo,
        comment: None,
        depends_on: Vec::new(),
        product,
    })
}

/// One `product` part: the value of the column `name`, when the row has one.
fn part(edition: &SnomedProvider, table: &Table, row: usize, name: &str) -> Option<DependsOn> {
    let field = table.field(name)?;
    let (system, value) = match table.value(row, field)? {
        ValueRef::Concept(ordinal) => (
            Some(SYSTEM.to_owned()),
            code_of(edition, ordinal).ok().flatten()?,
        ),
        ValueRef::Component(id) => (Some(SYSTEM.to_owned()), id.to_string()),
        ValueRef::Integer(value) => (None, value.to_string()),
        ValueRef::String(text) if !text.is_empty() => (None, text.to_owned()),
        ValueRef::String(_) => return None,
    };
    Some(DependsOn {
        attribute: name.to_owned(),
        system,
        value,
        display: None,
    })
}

/// The code of the concept at `ordinal`.
fn code_of(edition: &SnomedProvider, ordinal: Ordinal) -> Result<Option<String>, ProviderError> {
    edition.code(Concept::new(ordinal.index()))
}
