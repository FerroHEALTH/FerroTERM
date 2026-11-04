// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The registry and grammar systems: BCP 47 (RFC 5646), BCP 13 (RFC 6838),
//! and ISO 3166-1 over the vendored IANA and CLDR data.

use std::sync::Arc;

use fhir_terminology::provider::filter::{Filter, FilterOperator};
use fhir_terminology::registries::bcp47::Bcp47Provider;
use fhir_terminology::registries::{bcp13, iso3166};
use fhir_terminology::registry::Registry;
use fhir_types::r4b::value_set::{
    ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
    ValueSetComposeIncludeFilter,
};

mod grammar_systems;
mod language_tags;
mod media_types;

fn registry() -> Registry {
    let mut registry = Registry::default();
    registry
        .register(Arc::new(Bcp47Provider::new()))
        .expect("bcp47");
    registry
        .register(Arc::new(bcp13::Bcp13Provider::new()))
        .expect("bcp13");
    registry
        .register(Arc::new(iso3166::provider().expect("cldr data")))
        .expect("iso3166");
    registry
}

fn filter(property: &str, op: FilterOperator, value: &str) -> Filter {
    Filter {
        property: property.to_owned(),
        op,
        value: value.to_owned(),
    }
}

fn enumerating(system: &str, codes: &[&str]) -> ValueSet {
    ValueSet {
        url: Some("http://example.org/enumerated".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(system.into()),
                concept: codes
                    .iter()
                    .map(|code| ValueSetComposeIncludeConcept {
                        code: (*code).into(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn inline(system: &str, filters: Vec<(&str, &str, &str)>) -> ValueSet {
    ValueSet {
        url: Some("http://example.org/inline".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(system.into()),
                filter: filters
                    .into_iter()
                    .map(|(p, op, v)| ValueSetComposeIncludeFilter {
                        property: p.into(),
                        op: op.into(),
                        value: v.into(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    }
}
