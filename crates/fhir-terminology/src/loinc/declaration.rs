// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the LOINC provider declares: the FHIR page's properties, every table
//! field as a filter, `copyright`, `parent`, `ancestor`, and the implicit
//! value set forms.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    ANSWER_LIST_KEY, ANSWERS_FOR_FILTER, ANSWERS_KEY, COPYRIGHT_KEY, DOCUMENT_AXES,
    FHIR_PROPERTIES, KIND_KEY, LIST_FILTER, PART_TYPE_KEY,
};
use crate::provider::declaration::{
    Capability, Compositional, ContentMode, Declaration, FilterDefinition, HierarchyMeaning,
    ImplicitArgument, ImplicitForm, PropertyDefinition, PropertyKind,
};
use crate::provider::filter::FilterOperator;

/// What the provider declares: the FHIR page's properties, every table field
/// as a filter, `copyright`, `parent`, and `ancestor`.
pub(super) fn declaration(keys: &BTreeMap<u32, String>, languages: Vec<String>) -> Declaration {
    // NOTE: a property the loaded release does not carry is not declared: the FHIR
    // page still lists `DOCUMENT_SECTION`, which `Loinc.csv` dropped (#278).
    let mut properties: Vec<PropertyDefinition> = FHIR_PROPERTIES
        .iter()
        .filter(|(code, _)| keys.values().any(|key| key == code))
        .map(|(code, kind)| PropertyDefinition {
            code: (*code).to_owned(),
            uri: None,
            description: None,
            kind: *kind,
        })
        .collect();
    for (code, kind) in [
        ("parent", PropertyKind::Code),
        ("child", PropertyKind::Code),
        (COPYRIGHT_KEY, PropertyKind::Code),
        (ANSWER_LIST_KEY, PropertyKind::Code),
        (ANSWERS_KEY, PropertyKind::Code),
        (KIND_KEY, PropertyKind::Code),
    ] {
        properties.push(PropertyDefinition {
            code: code.to_owned(),
            uri: None,
            description: None,
            kind,
        });
    }
    let mut filters: Vec<FilterDefinition> = keys
        .values()
        .filter(|name| {
            !matches!(
                name.as_str(),
                COPYRIGHT_KEY | ANSWER_LIST_KEY | ANSWERS_KEY | KIND_KEY | PART_TYPE_KEY
            )
        })
        .map(|name| FilterDefinition {
            code: name.clone(),
            description: Some(format!("The `{name}` field of the term table")),
            operators: vec![FilterOperator::Equal, FilterOperator::Regex],
            value: String::from("a field value"),
        })
        .collect();
    filters.push(FilterDefinition {
        code: COPYRIGHT_KEY.to_owned(),
        description: Some(String::from("`LOINC` or `3rdParty`")),
        operators: vec![FilterOperator::Equal],
        value: String::from("LOINC | 3rdParty"),
    });
    for (code, description) in [
        ("parent", "Codes whose immediate parent is the part"),
        ("ancestor", "Codes under the part, transitively"),
    ] {
        filters.push(FilterDefinition {
            code: code.to_owned(),
            description: Some(description.to_owned()),
            operators: vec![FilterOperator::Equal, FilterOperator::In],
            value: String::from("a part code"),
        });
    }
    // NOTE: no specification defines these four; they are what tx.fhir.org
    // answers, and the FHIR LOINC page has carried a `TODO: Document Ontology`
    // placeholder since STU3 (<https://hl7.org/fhir/R4B/loinc.html>, #277).
    for (code, description) in [
        (LIST_FILTER, "The answers of the answer list"),
        (
            ANSWERS_FOR_FILTER,
            "The answers of the answer list, or of the list the term is linked to",
        ),
    ] {
        filters.push(FilterDefinition {
            code: code.to_owned(),
            description: Some(description.to_owned()),
            operators: vec![FilterOperator::Equal],
            value: String::from("an answer list code, or a term code"),
        });
    }
    for (code, key) in DOCUMENT_AXES {
        if keys.values().any(|name| name == key) {
            filters.push(FilterDefinition {
                code: code.to_owned(),
                description: Some(format!("The Document Ontology `{key}` axis of the term")),
                operators: vec![FilterOperator::Equal, FilterOperator::Exists],
                value: String::from("a part code or name, or a boolean for `exists`"),
            });
        }
    }
    Declaration {
        content: ContentMode::NotPresent,
        case_sensitive: false,
        hierarchy_meaning: Some(HierarchyMeaning::IsA),
        compositional: Compositional::None,
        languages,
        properties,
        filters,
        capabilities: BTreeSet::from([Capability::Subsumption, Capability::Enumeration]),
        // NOTE: the `/vs`, `/vs/[id]`, and `/vs/[partcode]` forms of
        // <https://hl7.org/fhir/R4B/loinc.html>, "Implicit Value Sets".
        implicit_forms: vec![
            ImplicitForm::new("http://loinc.org/vs", ImplicitArgument::None),
            ImplicitForm::new("http://loinc.org/vs/[id]", ImplicitArgument::Code),
            ImplicitForm::new("http://loinc.org/vs/[partcode]", ImplicitArgument::Code),
        ],
    }
}
