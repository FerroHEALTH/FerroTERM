// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The grammar-defined systems and ISO 3166 under the operations.

use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::operations::expand::ExpandInput;
use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;
use fhir_terminology::operations::{
    Invocation, OperationError, expand, sources::Sources, subsumes, value_set_validate_code,
};
use fhir_terminology::provider::CodeSystemProvider;
use fhir_terminology::provider::filter::FilterOperator;
use fhir_terminology::registries::{bcp13, bcp47, iso3166};
use fhir_terminology::valueset::store::ValueSetStore;

use super::{filter, inline, registry};

#[test]
fn iso3166_is_a_case_insensitive_table_with_user_assigned_codes() {
    let provider = iso3166::provider().expect("cldr data");
    assert_eq!(provider.identity().url, iso3166::URL);
    assert_ne!(provider.identity().version, "");
    let nl = provider.locate("nl").expect("reads").expect("NL");
    assert_eq!(nl.code, "NL");
    assert_eq!(
        provider
            .display(nl.concept, None)
            .expect("reads")
            .as_deref(),
        Some("Netherlands")
    );
    let props: Vec<String> = provider
        .properties(nl.concept)
        .expect("reads")
        .into_iter()
        .map(|p| format!("{}={}", p.code, p.value.as_text()))
        .collect();
    assert!(props.contains(&String::from("alpha3=NLD")));
    assert!(props.contains(&String::from("numeric=528")));
    let xy = provider
        .locate("XY")
        .expect("reads")
        .expect("user-assigned");
    assert_eq!(
        provider
            .display(xy.concept, None)
            .expect("reads")
            .as_deref(),
        Some("User-assigned")
    );
    assert!(
        provider.locate("EU").expect("reads").is_none(),
        "a CLDR reservation, not ISO 3166-1"
    );
    // All three forms of a territory are codes of this system: FHIR selects
    // them from it with a `code` regex per form
    // (<https://hl7.org/fhir/R5/valueset-iso3166-1-3.html>), so an alpha-3 or
    // a numeric code the specification binds has to validate here.
    for form in ["NLD", "528"] {
        let found = provider
            .locate(form)
            .expect("reads")
            .unwrap_or_else(|| panic!("{form} is a code of ISO 3166-1"));
        assert_eq!(
            provider
                .display(found.concept, None)
                .expect("reads")
                .as_deref(),
            Some("Netherlands"),
            "{form} names the same territory as NL"
        );
    }
    assert!(
        provider.locate("ZZZ").expect("reads").is_some(),
        "the user-assigned range is assigned in all three forms"
    );
    assert!(provider.locate("NLX").expect("reads").is_none());
    let regex = provider
        .filter(&filter("code", FilterOperator::Regex, "^N[A-Z]$"))
        .expect("regex");
    assert!(regex.contains(nl.concept.index()));
    assert!(regex.len() >= 10 && regex.len() < 30, "{}", regex.len());
    assert!(
        iso3166::is_user_assigned("QM")
            && iso3166::is_user_assigned("ZZ")
            && !iso3166::is_user_assigned("QL")
    );
}

#[test]
fn the_grammar_systems_refuse_expansion_and_validate_by_membership() {
    let registry = registry();
    let value_sets = ValueSetStore::default();
    let concept_maps = ConceptMapStore::default();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let all = ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
            bcp13::URL,
            vec![],
        ))),
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&sources, &all),
        Err(OperationError::NotSupported(_))
    ));
    let base = ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
            bcp13::URL,
            vec![("base", "=", "text")],
        ))),
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&sources, &base),
        Err(OperationError::NotSupported(_))
    ));
    let registered = ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
            bcp13::URL,
            vec![("registered", "=", "true")],
        ))),
        ..ExpandInput::default()
    };
    let error = expand::expand(&sources, &registered).expect_err("too costly without count");
    assert!(matches!(error, OperationError::TooCostly(_)), "{error}");
    assert_eq!(error.issue_code(), "too-costly");
    let paged = ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
            bcp13::URL,
            vec![("registered", "=", "true")],
        ))),
        count: Some(50),
        ..ExpandInput::default()
    };
    let vs = expand::expand(&sources, &paged).expect("pages");
    assert_eq!(vs.contains.len(), 50);
    let narrow = ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
            bcp13::URL,
            vec![("registered", "=", "true"), ("base", "=", "text/plain")],
        ))),
        ..ExpandInput::default()
    };
    let vs = expand::expand(&sources, &narrow).expect("expands");
    assert_eq!(vs.total, 1);
    let languages = ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
            bcp47::URL,
            vec![("variant", "=", "1996")],
        ))),
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&sources, &languages),
        Err(OperationError::NotSupported(_))
    ));
}

#[test]
fn the_grammar_systems_validate_by_membership_and_decline_undetermined_subsumption() {
    let registry = registry();
    let value_sets = ValueSetStore::default();
    let concept_maps = ConceptMapStore::default();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let member = ValueSetValidateInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
            bcp13::URL,
            vec![("base", "=", "text")],
        ))),
        code: Some(String::from("text/plain; charset=utf-8")),
        system: Some(bcp13::URL.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&sources, &member).expect("validates");
    assert!(validation.result);
    assert_eq!(
        validation.code.as_deref(),
        Some("text/plain; charset=utf-8")
    );
    let outsider = ValueSetValidateInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
            bcp13::URL,
            vec![("base", "=", "text")],
        ))),
        code: Some(String::from("application/json")),
        system: Some(bcp13::URL.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&sources, &outsider).expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.issues[0].kind, "not-in-vs");
    let english = ValueSetValidateInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
            bcp47::URL,
            vec![("language", "=", "en"), ("region", "=", "US")],
        ))),
        code: Some(String::from("en-us")),
        system: Some(bcp47::URL.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&sources, &english).expect("validates");
    assert!(validation.result);
    assert_eq!(
        validation.code.as_deref(),
        Some("en-us"),
        "the request's spelling"
    );
    assert_eq!(validation.normalized_code.as_deref(), Some("en-US"));
    assert_eq!(
        validation.display.as_deref(),
        Some("English (United States)")
    );
    let subsumes_request = subsumes::SubsumesInput {
        system: Some(bcp13::URL.to_owned()),
        code_a: Some(String::from("text/plain")),
        code_b: Some(String::from("text/plain; foo=bar")),
        ..subsumes::SubsumesInput::default()
    };
    let error = subsumes::subsumes(&registry, &Invocation::Type, &subsumes_request)
        .expect_err("undetermined");
    assert!(matches!(error, OperationError::CannotDetermine(_)));
    assert_eq!(error.tx_issue_type(), "cannot-determine");
}
