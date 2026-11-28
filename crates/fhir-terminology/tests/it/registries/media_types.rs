// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! BCP 13 media types: the grammar, the registry and subsumption.

use concept_graph::subsumption::Outcome;
use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::operations::expand::ExpandInput;
use fhir_terminology::operations::{expand, sources::Sources};
use fhir_terminology::provider::filter::FilterOperator;
use fhir_terminology::provider::{CodeSystemProvider, error::ProviderError};
use fhir_terminology::registries::bcp13;
use fhir_terminology::valueset::store::ValueSetStore;

use super::{enumerating, filter, inline, registry};

#[test]
fn bcp13_parses_the_grammar_knows_the_registry_and_subsumes_by_parameters() {
    let provider = bcp13::Bcp13Provider::new();
    let plain = provider
        .locate("Text/Plain; CharSet=UTF-8")
        .expect("reads")
        .expect("valid");
    assert_eq!(plain.code, "text/plain; charset=utf-8");
    assert!(provider.locate("not a mime type").expect("reads").is_none());
    assert!(provider.locate("text/").expect("reads").is_none());
    assert!(
        provider
            .locate("text/plain; charset")
            .expect("reads")
            .is_none()
    );
    let unregistered = provider
        .locate("application/x-tx-ecosystem-test")
        .expect("reads")
        .expect("valid");
    let props = |c| -> Vec<String> {
        provider
            .properties(c)
            .expect("reads")
            .into_iter()
            .map(|p| {
                format!(
                    "{}={}",
                    p.code,
                    p.value.as_ref().expect("a value").as_text()
                )
            })
            .collect()
    };
    assert_eq!(
        props(plain.concept),
        ["base=text", "base=text/plain", "registered=true"]
    );
    assert!(props(unregistered.concept).contains(&String::from("registered=false")));
    assert!(
        provider
            .filter_matches(
                plain.concept,
                &filter("base", FilterOperator::Equal, "text")
            )
            .expect("answers")
    );
    assert!(
        provider
            .filter_matches(
                plain.concept,
                &filter("base", FilterOperator::Equal, "text/plain")
            )
            .expect("answers")
    );
    assert!(
        !provider
            .filter_matches(
                plain.concept,
                &filter("base", FilterOperator::Equal, "text/html")
            )
            .expect("answers")
    );
    let registered = provider
        .filter_all(&[filter("registered", FilterOperator::Equal, "true")])
        .expect("enumerates");
    assert!(
        registered.len() > 2000,
        "{} registered types",
        registered.len()
    );
    let narrow = provider
        .filter_all(&[
            filter("registered", FilterOperator::Equal, "true"),
            filter("base", FilterOperator::Equal, "text/plain"),
        ])
        .expect("enumerates");
    assert_eq!(narrow.len(), 1);
    assert!(matches!(
        provider.filter_all(&[filter("base", FilterOperator::Equal, "text")]),
        Err(ProviderError::NotEnumerable)
    ));
    assert!(matches!(
        provider.filter_all(&[filter("registered", FilterOperator::Equal, "false")]),
        Err(ProviderError::NotEnumerable)
    ));
}

#[test]
fn bcp13_subsumes_by_parameters_and_cannot_decide_unknown_ones() {
    let provider = bcp13::Bcp13Provider::new();
    let sub = |a: &str, b: &str| {
        let a = provider.locate(a).expect("reads").expect("valid").concept;
        let b = provider.locate(b).expect("reads").expect("valid").concept;
        provider.subsumes(a, b)
    };
    assert_eq!(
        sub("text/plain", "text/plain").expect("decides"),
        Some(Outcome::Equivalent)
    );
    assert_eq!(
        sub("Text/Plain; CharSet=UTF-8", "text/plain; charset=utf-8").expect("decides"),
        Some(Outcome::Equivalent)
    );
    assert_eq!(
        sub("text/plain", "application/json").expect("decides"),
        Some(Outcome::NotSubsumed)
    );
    assert_eq!(
        sub("application/xml", "application/fhir+xml").expect("decides"),
        Some(Outcome::NotSubsumed),
        "a suffix is not a hierarchy"
    );
    // Parameters narrow a type that defines no default for them.
    assert_eq!(
        sub(
            "application/fhir+json",
            "application/fhir+json; fhirversion=4.0"
        )
        .expect("decides"),
        Some(Outcome::Subsumes)
    );
    assert_eq!(
        sub(
            "application/fhir+json; fhirversion=4.0",
            "application/fhir+json"
        )
        .expect("decides"),
        Some(Outcome::SubsumedBy)
    );
    assert!(matches!(
        sub("text/plain", "text/plain; foo=bar"),
        Err(ProviderError::CannotDetermine(_))
    ));
    assert!(matches!(
        sub("text/plain; foo=bar", "text/plain; foo=baz"),
        Err(ProviderError::CannotDetermine(_))
    ));
}

#[test]
fn bcp13_reads_the_rfc_parameter_defaults_into_the_bare_type() {
    let provider = bcp13::Bcp13Provider::new();
    let sub = |a: &str, b: &str| {
        let a = provider.locate(a).expect("reads").expect("valid").concept;
        let b = provider.locate(b).expect("reads").expect("valid").concept;
        provider.subsumes(a, b)
    };
    // NOTE: "Where a parameter has a default defined by its RFC, the bare form already
    // carries that default" (ecosystem IG at `9c2bbce`, `requirements.md`, BCP 13): text/plain
    // is charset=us-ascii (RFC 6657 §4) and format=fixed (RFC 3676 §4), so these contradict.
    assert_eq!(
        sub("text/plain", "text/plain; charset=us-ascii").expect("decides"),
        Some(Outcome::Equivalent)
    );
    assert_eq!(
        sub("text/plain", "text/plain; charset=utf-8").expect("decides"),
        Some(Outcome::NotSubsumed)
    );
    assert_eq!(
        sub("text/plain; charset=utf-8", "text/plain").expect("decides"),
        Some(Outcome::NotSubsumed)
    );
    assert_eq!(
        sub("text/plain; charset=utf-8", "text/plain; charset=utf-16").expect("decides"),
        Some(Outcome::NotSubsumed)
    );
    assert_eq!(
        sub(
            "text/plain; charset=utf-8",
            "text/plain; charset=utf-8; format=fixed"
        )
        .expect("decides"),
        Some(Outcome::Equivalent)
    );
    assert_eq!(
        sub(
            "text/plain; charset=utf-8",
            "text/plain; charset=utf-8; format=flowed"
        )
        .expect("decides"),
        Some(Outcome::NotSubsumed)
    );
    assert_eq!(
        sub("text/plain; charset=utf-8", "text/plain; format=flowed").expect("decides"),
        Some(Outcome::NotSubsumed)
    );
    assert_eq!(
        sub(
            "text/plain; foo=bar",
            "text/plain; charset=us-ascii; foo=bar"
        )
        .expect("decides"),
        Some(Outcome::Equivalent),
        "a parameter both codes carry identically does not stop the server deciding"
    );
    // NOTE: RFC 6657 §3 leaves text/csv's default charset unstated, and "Where the server
    // does not know a parameter ... [it] SHALL return an error rather than guessing"
    // (ecosystem IG at `9c2bbce`, `requirements.md`, BCP 13).
    assert!(matches!(
        sub("text/csv", "text/csv; charset=utf-8"),
        Err(ProviderError::CannotDetermine(_))
    ));
    assert_eq!(
        sub("text/csv; charset=utf-8", "text/csv; charset=utf-8").expect("decides"),
        Some(Outcome::Equivalent),
        "a charset both codes state is compared like any other"
    );
}

/// The IANA registry is finite, so `registered = true` walks it, but every
/// registered type also carries the parameters of RFC 2045 §5.1, which no
/// enumeration holds: "if the value set itself is unbounded due to the
/// inclusion of post-coordinated value sets (e.g. SNOMED CT, UCUM), then the
/// extension valueset-unclosed can be used to indicate that the expansion is
/// incomplete" (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>,
/// Notes).
#[test]
fn a_filtered_media_type_expansion_is_unclosed_and_an_enumerated_one_is_not() {
    let registry = registry();
    let value_sets = ValueSetStore::default();
    let concept_maps = ConceptMapStore::default();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let narrow = ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
            bcp13::URL,
            vec![("registered", "=", "true"), ("base", "=", "text/plain")],
        ))),
        ..ExpandInput::default()
    };
    let outcome = expand::expand(&sources, &narrow).expect("expands");
    assert_eq!(outcome.total, 1);
    assert!(outcome.unclosed, "the parameterised forms are not listed");
    let rendered = fhir_terminology::valueset::render::r4b::expansion(&outcome);
    let expansion = rendered.expansion.expect("an expansion");
    assert_eq!(
        expansion.extension,
        vec![fhir_types::r4b::extension::Extension {
            url: String::from("http://hl7.org/fhir/StructureDefinition/valueset-unclosed"),
            value: Some(fhir_types::r4b::extension::ExtensionValue::Boolean(
                true.into()
            )),
            ..Default::default()
        }]
    );
    let enumerated = ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(
            &enumerating(bcp13::URL, &["text/plain", "text/plain; charset=utf-8"]),
        )),
        ..ExpandInput::default()
    };
    let outcome = expand::expand(&sources, &enumerated).expect("expands");
    assert_eq!(outcome.total, 2);
    assert!(
        !outcome.unclosed,
        "the codes an include lists are its members"
    );
    let rendered = fhir_terminology::valueset::render::r4b::expansion(&outcome);
    assert_eq!(
        rendered.expansion.expect("an expansion").extension,
        Vec::<fhir_types::r4b::extension::Extension>::new()
    );
}
