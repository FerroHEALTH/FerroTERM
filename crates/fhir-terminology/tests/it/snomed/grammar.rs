// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! ECL and the compositional grammar on the SNOMED CT provider.

use ferroterm_testkit::snomed::{
    ANIMAL, CAT, COVERING, DOG, EDITION, FUR, VERSION, code, item, sctid,
};
use fhir_terminology::capabilities::Summary;
use fhir_terminology::operations::{Invocation, OperationError, lookup, validate_code};
use fhir_terminology::provider::declaration::Compositional;
use fhir_terminology::provider::filter::{Filter, FilterOperator};
use fhir_terminology::provider::{CodeSystemProvider, ProviderError};
use fhir_terminology::registry::Registry;
use fhir_terminology::snomed::SYSTEM;
use std::sync::Arc;

use super::provider;

#[test]
fn ecl_arrives_as_the_constraint_filter_and_the_ecl_implicit_value_set() {
    let (_dir, p) = provider();
    let animals = sctid(item(ANIMAL));
    let filter = |value: &str| Filter {
        property: String::from("constraint"),
        op: FilterOperator::Equal,
        value: value.to_owned(),
    };
    assert_eq!(
        p.filter(&filter(&format!("< {animals}")))
            .expect("evaluates")
            .iter()
            .collect::<Vec<_>>(),
        [CAT, DOG]
    );
    assert!(
        matches!(
            p.filter(&filter("<< ")),
            Err(ProviderError::InvalidFilterValue(invalid)) if invalid.reason.contains("byte 3") && invalid.position == Some(3)
        ),
        "malformed ECL names the position"
    );
    assert!(
        matches!(
            p.filter(&filter("<< 999999999")),
            Err(ProviderError::InvalidCode { code, .. }) if code == "999999999"
        ),
        "an unknown identifier in valid ECL is an invalid code"
    );
    assert!(matches!(
        p.filter(&filter("* {{ D moduleId = 900000000000207008 }}")),
        Err(ProviderError::UnsupportedFilter { .. })
    ));
    let expressions = |value: &str| Filter {
        property: String::from("expressions"),
        op: FilterOperator::Equal,
        value: value.to_owned(),
    };
    // NOTE: the filter says whether post-coordination is allowed
    // (<https://hl7.org/fhir/R4B/snomedct.html>, "Filter Properties"), so
    // either value selects every precoordinated concept of the version.
    assert_eq!(p.filter(&expressions("false")).expect("all").len(), 21);
    assert_eq!(p.filter(&expressions("true")).expect("all").len(), 21);
    assert!(matches!(
        p.filter(&expressions("maybe")),
        Err(ProviderError::InvalidFilterValue(_))
    ));
    let codes: Vec<&str> = p
        .declaration()
        .filters
        .iter()
        .map(|f| f.code.as_str())
        .collect();
    assert_eq!(codes, ["concept", "constraint", "expressions"]);

    // The implicit value set carries the URI-encoded expression
    // (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets").
    let encoded = format!("%3C%20{animals}%20%7Canimal%7C");
    for base in ["http://snomed.info/sct", EDITION, VERSION] {
        let compose = p
            .implicit_value_set(&format!("{base}?fhir_vs=ecl/{encoded}"))
            .expect("implicit")
            .expect("compose");
        let include = &compose.include[0];
        assert_eq!(include.filters.len(), 1);
        assert_eq!(include.filters[0].property, "constraint");
        assert_eq!(include.filters[0].op, FilterOperator::Equal);
        assert_eq!(include.filters[0].value, format!("< {animals} |animal|"));
        assert_eq!(
            include.system.as_ref().and_then(|s| s.version.as_deref()),
            (base != "http://snomed.info/sct").then_some(VERSION)
        );
        assert_eq!(
            p.filter(&include.filters[0])
                .expect("evaluates")
                .iter()
                .collect::<Vec<_>>(),
            [CAT, DOG]
        );
    }
    assert!(
        matches!(
            p.implicit_value_set("http://snomed.info/sct?fhir_vs=ecl/%3C%3C%20"),
            Some(Err(ProviderError::MalformedImplicitValueSet(malformed))) if malformed.reason.contains("byte")
        ),
        "malformed ECL is a malformed implicit value set"
    );
    assert!(
        matches!(
            p.implicit_value_set("http://snomed.info/sct?fhir_vs=ecl/%3"),
            Some(Err(ProviderError::MalformedImplicitValueSet(_)))
        ),
        "a stray percent is malformed"
    );
    assert!(
        matches!(
            p.implicit_value_set(&format!("http://snomed.info/sct?fhir_vs=ecl/{animals}")),
            Some(Ok(_))
        ),
        "an expression without reserved characters needs no encoding"
    );
}

/// A parse failure carries the parser's position as data and its error as the
/// cause: into the filter value for a `constraint` filter, and into the URI,
/// through its percent-encoding, for the `ecl/` implicit value set
/// (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets").
#[test]
fn a_malformed_expression_constraint_states_where_the_parser_stopped() {
    let (_dir, p) = provider();
    let refused = p.filter(&Filter {
        property: String::from("constraint"),
        op: FilterOperator::Equal,
        value: String::from("<< OR"),
    });
    let Err(ProviderError::InvalidFilterValue(invalid)) = &refused else {
        panic!("malformed ECL is an invalid filter value: {refused:?}");
    };
    assert_eq!(
        invalid.position,
        Some(3),
        "the parser stops at `OR`, byte 3"
    );
    let cause = std::error::Error::source(invalid.as_ref())
        .and_then(|cause| cause.downcast_ref::<sct_scg::front::ParseError>());
    assert_eq!(
        cause.map(sct_scg::front::ParseError::offset),
        Some(3),
        "the parse error is the cause, not a copy of its text"
    );

    let url = "http://snomed.info/sct?fhir_vs=ecl/%3C%3C%20OR";
    let refused = p.implicit_value_set(url).expect("the form is SNOMED's");
    let Err(ProviderError::MalformedImplicitValueSet(malformed)) = &refused else {
        panic!("malformed ECL is a malformed implicit value set: {refused:?}");
    };
    assert_eq!(
        malformed.position,
        url.find("OR"),
        "decoded byte 3 is the `O` after three escapes in the URI"
    );
    assert!(
        std::error::Error::source(malformed.as_ref())
            .is_some_and(<dyn std::error::Error>::is::<sct_scg::front::ParseError>),
        "the parse error is the cause"
    );
}

#[test]
fn the_defined_grammar_and_the_supported_grammar_are_two_declarations() {
    let (_dir, p) = provider();
    // `CodeSystem.compositional` is "The code system defines a compositional
    // (post-coordination) grammar"
    // (<https://hl7.org/fhir/R4B/codesystem-definitions.html#CodeSystem.compositional>),
    // which SNOMED CT does (<http://snomed.org/scg>).
    assert!(p.declaration().compositional.defined());
    // `TerminologyCapabilities.codeSystem.version.compositional` is "If the
    // compositional grammar defined by the code system is supported"
    // (<https://hl7.org/fhir/R4B/terminologycapabilities-definitions.html#TerminologyCapabilities.codeSystem.version.compositional>),
    // which this server does: an expression is a code it locates.
    assert!(p.declaration().compositional.supported());
    assert_eq!(p.declaration().compositional, Compositional::Supported);
    let mut registry = Registry::default();
    registry.register(Arc::new(p)).expect("registers");
    let summary = Summary::of(&registry);
    let date = "2026-09-05T00:00:00Z";
    let flags = [
        (
            "r4",
            summary.to_r4(date).code_system[0].version[0]
                .compositional
                .as_ref()
                .and_then(|f| f.value),
        ),
        (
            "r4b",
            summary.to_r4b(date).code_system[0].version[0]
                .compositional
                .as_ref()
                .and_then(|f| f.value),
        ),
        (
            "r5",
            summary.to_r5(date).code_system[0].version[0]
                .compositional
                .as_ref()
                .and_then(|f| f.value),
        ),
        (
            "r6",
            summary.to_r6(date).code_system[0].version[0]
                .compositional
                .as_ref()
                .and_then(|f| f.value),
        ),
    ];
    for (version, value) in flags {
        assert_eq!(
            value,
            Some(true),
            "{version} declares the grammar supported"
        );
    }
}

#[test]
fn a_post_coordinated_expression_is_refused_for_the_grammar_not_as_an_unknown_concept() {
    let (_dir, p) = provider();
    let mut registry = Registry::default();
    registry.register(Arc::new(p)).expect("registers");
    // NOTE: an expression is a valid code and "subject to the same rules as
    // precoordinated concepts" (<https://hl7.org/fhir/R4B/snomedct.html>,
    // "Code"), so a well-formed one over defined concepts locates.
    let expression = format!("{}:{}={}", code(CAT), code(COVERING), code(FUR));
    let input = lookup::LookupInput {
        system: Some(SYSTEM.to_owned()),
        code: Some(expression.clone()),
        ..lookup::LookupInput::default()
    };
    let outcome = lookup::lookup(&registry, &Invocation::Type, &input).expect("looks up");
    assert_eq!(
        outcome.code,
        format!("=== {} : {} = {}", code(CAT), code(COVERING), code(FUR)),
        "the answer is the expression in one canonical order"
    );

    // A malformed expression is refused for the grammar, with the offset the
    // parser stopped at (the Compositional Grammar specification, §7.3).
    let malformed = format!("{}:{}=", code(CAT), code(COVERING));
    let error = lookup::lookup(
        &registry,
        &Invocation::Type,
        &lookup::LookupInput {
            system: Some(SYSTEM.to_owned()),
            code: Some(malformed.clone()),
            ..lookup::LookupInput::default()
        },
    )
    .expect_err("refuses");
    assert!(
        matches!(&error, OperationError::InvalidCode { code, reason }
            if *code == malformed && reason.contains("compositional grammar")),
        "{error:?}"
    );
    assert_eq!(error.issue_code(), "code-invalid");
    assert_eq!(error.tx_issue_type(), "invalid-code");

    // `$validate-code` answers `false` for the same code, and its message says
    // which of the two failures it is.
    let request = validate_code::ValidateCodeInput {
        url: Some(SYSTEM.to_owned()),
        code: Some(malformed),
        ..validate_code::ValidateCodeInput::default()
    };
    let outcome =
        validate_code::validate_code(&registry, &Invocation::Type, &request).expect("validates");
    assert!(!outcome.result);
    let message = outcome.message.expect("a message");
    assert!(
        message.contains("not valid compositional grammar"),
        "{message}"
    );

    // A code that is no expression at all stays the unknown-code outcome.
    let unknown = lookup::LookupInput {
        system: Some(SYSTEM.to_owned()),
        code: Some(String::from("138875004999")),
        ..lookup::LookupInput::default()
    };
    let error = lookup::lookup(&registry, &Invocation::Type, &unknown).expect_err("refuses");
    assert!(
        matches!(error, OperationError::UnknownCode { .. }),
        "{error:?}"
    );
}

/// `$expand` names SNOMED CT as a system whose value sets are "unbounded due to
/// the inclusion of post-coordinated value sets", the case `valueset-unclosed`
/// marks (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>, Notes);
/// the `expressions` filter is what keeps the expressions out
/// (<https://hl7.org/fhir/R4B/snomedct.html>, "Filter Properties").
#[test]
fn a_selection_is_unclosed_unless_it_states_that_expressions_are_excluded() {
    let (_dir, p) = provider();
    let descendants = Filter {
        property: String::from("concept"),
        op: FilterOperator::DescendentOf,
        value: sctid(item(ANIMAL)),
    };
    let no_expressions = Filter {
        property: String::from("expressions"),
        op: FilterOperator::Equal,
        value: String::from("false"),
    };
    assert!(p.unclosed(std::slice::from_ref(&descendants)));
    assert!(!p.unclosed(&[descendants, no_expressions]));
}
