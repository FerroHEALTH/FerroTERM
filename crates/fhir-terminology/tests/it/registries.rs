//! The registry and grammar systems: BCP 47 (RFC 5646), BCP 13 (RFC 6838),
//! and ISO 3166-1 over the vendored IANA and CLDR data.

use std::sync::Arc;

use concept_graph::subsumption::Outcome;
use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::filter::{Filter, FilterOperator};
use fhir_terminology::operations::expand::{ExpandInput, ParameterValue};
use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;
use fhir_terminology::operations::{
    Invocation, OperationError, Sources, expand, subsumes, value_set_validate_code,
};
use fhir_terminology::provider::{CodeSystemProvider, ContentMode, ProviderError};
use fhir_terminology::registries::bcp47::{Analysis, Bcp47Provider};
use fhir_terminology::registries::{bcp13, bcp47, iso3166, subtags};
use fhir_terminology::registry::Registry;
use fhir_terminology::valueset::store::ValueSetStore;
use fhir_types::r4b::value_set::{
    ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
    ValueSetComposeIncludeFilter,
};

fn registry() -> Registry {
    let mut registry = Registry::new();
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

#[test]
fn bcp47_distinguishes_malformed_well_formed_and_valid_tags() {
    let provider = Bcp47Provider::new();
    assert!(matches!(provider.analyze("en"), Analysis::Valid(_)));
    assert!(matches!(provider.analyze("EN-gb"), Analysis::Valid(_)));
    assert!(matches!(provider.analyze("zh-Hant-TW"), Analysis::Valid(_)));
    assert!(matches!(
        provider.analyze("sl-rozaj-biske"),
        Analysis::Valid(_)
    ));
    assert!(matches!(
        provider.analyze("de-DE-u-co-phonebk"),
        Analysis::Valid(_)
    ));
    assert!(matches!(
        provider.analyze("x-private-tag"),
        Analysis::Valid(_)
    ));
    assert!(matches!(provider.analyze("i-klingon"), Analysis::Valid(_)));
    // The `QM..QZ` region range registers every subtag in it (RFC 5646 §3.1.1).
    assert!(matches!(provider.analyze("en-QQ"), Analysis::Valid(_)));
    match provider.analyze("en-AB") {
        Analysis::WellFormed { unknown, .. } => assert_eq!(unknown, ["AB"]),
        other => panic!("well-formed with an unknown region, got {other:?}"),
    }
    match provider.analyze("abcd") {
        Analysis::WellFormed { unknown, .. } => {
            assert_eq!(unknown, ["abcd"], "4 letters are reserved");
        }
        other => panic!("reserved primary subtag, got {other:?}"),
    }
    assert!(matches!(provider.analyze(""), Analysis::Malformed(_)));
    assert!(matches!(provider.analyze("en--GB"), Analysis::Malformed(_)));
    assert!(matches!(
        provider.analyze("en-GB-u"),
        Analysis::Malformed(_)
    ));
    assert!(matches!(
        provider.analyze("toolongsubtag"),
        Analysis::Malformed(_)
    ));
    assert!(matches!(provider.analyze("en_GB"), Analysis::Malformed(_)));
}

#[test]
fn bcp47_locates_in_canonical_case_with_a_composed_display_and_parts() {
    let provider = Bcp47Provider::new();
    let located = provider
        .locate("EN-latn-gb")
        .expect("reads")
        .expect("valid");
    assert_eq!(located.code, "en-Latn-GB");
    assert_eq!(
        provider
            .display(located.concept, None)
            .expect("reads")
            .as_deref(),
        Some("English (Latin, United Kingdom)")
    );
    let again = provider
        .locate("en-Latn-GB")
        .expect("reads")
        .expect("valid");
    assert_eq!(
        again.concept, located.concept,
        "one ordinal per canonical tag"
    );
    let codes: Vec<String> = provider
        .properties(located.concept)
        .expect("reads")
        .into_iter()
        .map(|p| format!("{}={}", p.code, p.value.as_text()))
        .collect();
    assert_eq!(codes, ["language=en", "script=Latn", "region=GB"]);
    assert!(
        provider.locate("en-AB").expect("reads").is_none(),
        "well-formed is not valid"
    );
    assert!(provider.locate("not a tag").expect("reads").is_none());
    assert!(
        provider
            .filter_matches(
                located.concept,
                &filter("language", FilterOperator::Equal, "en")
            )
            .expect("answers")
    );
    assert!(
        provider
            .filter_matches(
                located.concept,
                &filter("region", FilterOperator::Equal, "gb")
            )
            .expect("answers")
    );
    assert!(
        !provider
            .filter_matches(
                located.concept,
                &filter("region", FilterOperator::Equal, "US")
            )
            .expect("answers")
    );
    assert!(matches!(provider.all(), Err(ProviderError::NotEnumerable)));
    assert!(matches!(
        provider.filter(&filter("variant", FilterOperator::Equal, "1996")),
        Err(ProviderError::FilterNotEnumerable)
    ));
    assert_eq!(provider.declaration().content, ContentMode::NotPresent);
    let deprecated = provider
        .locate("art-lojban")
        .expect("reads")
        .expect("grandfathered");
    assert!(!provider.status(deprecated.concept).expect("reads").active);
}

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
            .map(|p| format!("{}={}", p.code, p.value.as_text()))
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
    assert_eq!(
        sub("text/plain", "text/plain; charset=utf-8").expect("decides"),
        Some(Outcome::Subsumes)
    );
    assert_eq!(
        sub("text/plain; charset=utf-8", "text/plain").expect("decides"),
        Some(Outcome::SubsumedBy)
    );
    assert_eq!(
        sub("text/plain; charset=utf-8", "text/plain; charset=utf-16").expect("decides"),
        Some(Outcome::NotSubsumed)
    );
    assert_eq!(
        sub(
            "text/plain; charset=utf-8",
            "text/plain; charset=utf-8; format=flowed"
        )
        .expect("decides"),
        Some(Outcome::Subsumes)
    );
    assert_eq!(
        sub("text/plain; charset=utf-8", "text/plain; format=flowed").expect("decides"),
        Some(Outcome::NotSubsumed)
    );
    assert_eq!(
        sub("text/plain; foo=bar", "text/plain; charset=utf-8; foo=bar").expect("decides"),
        Some(Outcome::Subsumes)
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

/// RFC 4647 §3.3.2 extended filtering, read in both directions: the section's
/// own example has the range "de-*-DE" and "its synonym `de-DE`" matching
/// `de-Latn-DE` while `de-x-DE` fails on the singleton, and RFC 5646 §2.2.8
/// keeps a grandfathered tag whole ("each tag, in its entirety, represents a
/// language or collection of languages").
#[test]
fn bcp47_subsumes_language_tags_by_extended_filtering() {
    let provider = Bcp47Provider::new();
    let sub = |a: &str, b: &str| {
        let a = provider.locate(a).expect("reads").expect("valid").concept;
        let b = provider.locate(b).expect("reads").expect("valid").concept;
        provider.subsumes(a, b).expect("decides")
    };
    assert_eq!(sub("en-US", "en-US"), Some(Outcome::Equivalent));
    assert_eq!(
        sub("EN", "en-us"),
        Some(Outcome::Subsumes),
        "tags are compared case-insensitively"
    );
    assert_eq!(sub("en", "en-US"), Some(Outcome::Subsumes));
    assert_eq!(sub("en-US", "en"), Some(Outcome::SubsumedBy));
    assert_eq!(sub("zh", "zh-Hans-CN"), Some(Outcome::Subsumes));
    assert_eq!(sub("de", "de-1901"), Some(Outcome::Subsumes));
    assert_eq!(
        sub("en-US", "en-Latn-US"),
        Some(Outcome::Subsumes),
        "an intermediate subtag the range omits is skipped"
    );
    assert_eq!(sub("en", "fr"), Some(Outcome::NotSubsumed));
    assert_eq!(sub("en-US", "en-GB"), Some(Outcome::NotSubsumed));
    assert_eq!(sub("zh-Hant", "zh-Hans-CN"), Some(Outcome::NotSubsumed));
    assert_eq!(
        sub("en-Latn", "en-US"),
        Some(Outcome::NotSubsumed),
        "each states something the other does not"
    );
    assert_eq!(
        sub("zh", "zh-min-nan"),
        Some(Outcome::NotSubsumed),
        "a grandfathered tag is one opaque subtag"
    );
    assert_eq!(sub("en", "en-x-goethe"), Some(Outcome::Subsumes));
    assert_eq!(
        sub("en-US", "en-x-US"),
        Some(Outcome::NotSubsumed),
        "the singleton `x` stops the walk before `US`"
    );
}

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
    let value_sets = ValueSetStore::new();
    let concept_maps = ConceptMapStore::new();
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
    let value_sets = ValueSetStore::new();
    let concept_maps = ConceptMapStore::new();
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
    let value_sets = ValueSetStore::new();
    let concept_maps = ConceptMapStore::new();
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

/// A tag is valid when every subtag is registered, and the variants,
/// extensions, and private-use subtags of RFC 5646 §2.2.6 to §2.2.7 build
/// unboundedly many valid tags over the finite registry.
#[test]
fn a_language_tag_selection_is_unclosed() {
    let provider = Bcp47Provider::new();
    assert!(provider.unclosed(&[filter("language", FilterOperator::Equal, "en")]));
}

/// `region = 2ALPHA / 3DIGIT` holds one registered subtag (RFC 5646 §2.1), so
/// a fixed primary language leaves a finite list of `language ["-" region]`
/// tags, the shape RFC 5646 §4.1 recommends writing when no script is needed.
#[test]
fn a_fixed_language_expands_to_the_registered_regions() {
    let registry = registry();
    let value_sets = ValueSetStore::new();
    let concept_maps = ConceptMapStore::new();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let english = |count: Option<i64>| ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
            bcp47::URL,
            vec![("language", "=", "en")],
        ))),
        count,
        ..ExpandInput::default()
    };
    let regions = subtags::REGISTRY_DATA.of_kind(subtags::Kind::Region).len();
    assert!(regions > 300, "the vendored registry has {regions} regions");
    let outcome = expand::expand(&sources, &english(Some(5))).expect("expands");
    assert_eq!(
        outcome.total,
        u64::try_from(regions).expect("a registry count") + 1,
        "the bare tag and one per registered region"
    );
    let codes: Vec<&str> = outcome
        .contains
        .iter()
        .map(|item| item.code.as_str())
        .collect();
    assert_eq!(codes, ["en", "en-AA", "en-AC", "en-AD", "en-AE"]);
    assert_eq!(
        outcome
            .contains
            .first()
            .and_then(|item| item.display.as_deref()),
        Some("English")
    );
    assert!(outcome.unclosed, "an extension can always be added");
    let paged = expand::expand(&sources, &english(Some(5))).expect("expands");
    assert_eq!(
        paged.contains, outcome.contains,
        "the order does not move between calls"
    );
}

/// `script = 4ALPHA` holds one registered subtag (RFC 5646 §2.1), so a fixed
/// language and region leave a finite list of `language ["-" script] "-" region`
/// tags.
#[test]
fn a_fixed_language_and_region_expand_to_the_registered_scripts() {
    let registry = registry();
    let value_sets = ValueSetStore::new();
    let concept_maps = ConceptMapStore::new();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let input = ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
            bcp47::URL,
            vec![("language", "=", "en"), ("region", "=", "US")],
        ))),
        count: Some(3),
        ..ExpandInput::default()
    };
    let scripts = subtags::REGISTRY_DATA.of_kind(subtags::Kind::Script).len();
    let outcome = expand::expand(&sources, &input).expect("expands");
    assert_eq!(
        outcome.total,
        u64::try_from(scripts).expect("a registry count") + 1,
        "the tag itself and one per registered script"
    );
    let codes: Vec<&str> = outcome
        .contains
        .iter()
        .map(|item| item.code.as_str())
        .collect();
    assert_eq!(codes, ["en-US", "en-Adlm-US", "en-Afak-US"]);
}

/// `langtag = language ["-" script] ["-" region] …` (RFC 5646 §2.1): the
/// primary language is the one subtag every tag carries, so a selection that
/// leaves it open names no finite list this server enumerates.
#[test]
fn a_script_or_a_region_alone_leaves_the_language_open() {
    let registry = registry();
    let value_sets = ValueSetStore::new();
    let concept_maps = ConceptMapStore::new();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    for filters in [vec![("script", "=", "Latn")], vec![("region", "=", "US")]] {
        let input = ExpandInput {
            inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
                bcp47::URL,
                filters.clone(),
            ))),
            ..ExpandInput::default()
        };
        let error = expand::expand(&sources, &input).expect_err("no fixed language");
        assert_eq!(
            error.to_string(),
            format!("This filter on {} cannot be expanded", bcp47::URL),
            "{filters:?}: the system expands other selections"
        );
        assert!(
            matches!(error, OperationError::NotSupported(_)),
            "{filters:?}: {error}"
        );
        assert_eq!(error.issue_code(), "not-supported");
        assert_eq!(error.message_id(), "CODESYSTEM_NOT_ENUMERABLE");
    }
}

/// An unpaged expansion past [`expand::EXPANSION_LIMIT`] answers `too-costly`,
/// whatever system it came from.
#[test]
fn an_oversized_language_enumeration_is_too_costly() {
    let registry = registry();
    let value_sets = ValueSetStore::new();
    let concept_maps = ConceptMapStore::new();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let value_set = ValueSet {
        url: Some("http://example.org/many-languages".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: ["en", "fr", "de", "nl"]
                .into_iter()
                .map(|language| ValueSetComposeInclude {
                    system: Some(bcp47::URL.into()),
                    filter: vec![ValueSetComposeIncludeFilter {
                        property: "language".into(),
                        op: "=".into(),
                        value: language.into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let input = ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(
            &value_set,
        )),
        ..ExpandInput::default()
    };
    let error = expand::expand(&sources, &input).expect_err("past the limit");
    assert!(matches!(error, OperationError::TooCostly(_)), "{error}");
    assert_eq!(error.issue_code(), "too-costly");
    let paged = ExpandInput {
        count: Some(2),
        ..input
    };
    let outcome = expand::expand(&sources, &paged).expect("pages");
    assert!(outcome.total > expand::EXPANSION_LIMIT, "{}", outcome.total);
}

/// An unclosed expansion states why beside the mark: `extension.valueString`
/// on `ValueSet.expansion` (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>,
/// Notes).
#[test]
fn a_language_tag_expansion_states_why_it_is_unclosed() {
    let registry = registry();
    let value_sets = ValueSetStore::new();
    let concept_maps = ConceptMapStore::new();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let input = ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
            bcp47::URL,
            vec![("language", "=", "nl")],
        ))),
        count: Some(1),
        ..ExpandInput::default()
    };
    let outcome = expand::expand(&sources, &input).expect("expands");
    assert_eq!(
        outcome.unclosed_reasons,
        vec![String::from(
            "The code System 'urn:ietf:bcp:47' has a grammar and so has infinite members"
        )]
    );
    let rendered = fhir_terminology::valueset::render::r4b::expansion(&outcome);
    let urls: Vec<&str> = rendered
        .expansion
        .as_ref()
        .expect("an expansion")
        .extension
        .iter()
        .map(|extension| extension.url.as_str())
        .collect();
    assert_eq!(
        urls,
        [
            "http://hl7.org/fhir/StructureDefinition/valueset-unclosed",
            "http://hl7.org/fhir/StructureDefinition/valueset-unclosed-reason",
        ]
    );
}

/// R6 alone declares `handle-unclosed-expansion`: "If true this asserts that
/// you will correctly handle an unclosed expansion and the returned expansion
/// SHALL include the valueset-unclosed extension if the value set is unclosed.
/// If handle-unclosed-expansion is set to false the server SHALL return an
/// error if the value set is unclosed"
/// (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-expand.html>).
// NOTE: the ballot fixes no behaviour for the absent parameter, so an expansion that
// names none keeps the mark the R4B and R5 Notes describe: our own design.
#[test]
fn handle_unclosed_expansion_refuses_an_unclosed_expansion_only_when_it_is_false() {
    let registry = registry();
    let value_sets = ValueSetStore::new();
    let concept_maps = ConceptMapStore::new();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let unclosed = || {
        Some(fhir_terminology::valueset::convert::r4b::convert(&inline(
            bcp13::URL,
            vec![("registered", "=", "true"), ("base", "=", "text/plain")],
        )))
    };
    let absent = ExpandInput {
        inline_value_set: unclosed(),
        ..ExpandInput::default()
    };
    let outcome = expand::expand(&sources, &absent).expect("expands");
    assert!(outcome.unclosed, "the default keeps the mark");
    assert!(
        !outcome
            .parameters
            .iter()
            .any(|p| p.name == "handle-unclosed-expansion"),
        "a parameter the request did not name is not echoed"
    );
    let handled = ExpandInput {
        inline_value_set: unclosed(),
        handle_unclosed_expansion: Some(true),
        ..ExpandInput::default()
    };
    let outcome = expand::expand(&sources, &handled).expect("expands");
    assert!(outcome.unclosed);
    assert!(
        outcome
            .parameters
            .iter()
            .any(|p| p.name == "handle-unclosed-expansion"
                && p.value == ParameterValue::Boolean(true)),
        "{:?}",
        outcome.parameters
    );
    let rendered = fhir_terminology::valueset::render::r4b::expansion(&outcome);
    assert_eq!(
        rendered
            .expansion
            .expect("an expansion")
            .extension
            .first()
            .map(|e| e.url.as_str()),
        Some("http://hl7.org/fhir/StructureDefinition/valueset-unclosed")
    );
    let refused = ExpandInput {
        inline_value_set: unclosed(),
        handle_unclosed_expansion: Some(false),
        ..ExpandInput::default()
    };
    let error = expand::expand(&sources, &refused).expect_err("refused");
    assert!(matches!(error, OperationError::NotSupported(_)), "{error}");
    assert_eq!(error.issue_code(), "not-supported");
    assert_eq!(error.status(), http::StatusCode::BAD_REQUEST);
    // A closed expansion answers whatever the client asserted about unclosed ones.
    let closed = ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(
            &enumerating(bcp13::URL, &["text/plain"]),
        )),
        handle_unclosed_expansion: Some(false),
        ..ExpandInput::default()
    };
    let outcome = expand::expand(&sources, &closed).expect("expands");
    assert!(!outcome.unclosed);
    assert_eq!(outcome.total, 1);
}
/// A media type and a language tag are the same text in every language, so
/// neither system states one (RFC 6838, RFC 5646 §3.1.5) and its display
/// answers a request in any.
#[test]
fn a_registry_with_no_language_of_its_own_answers_every_display_language() {
    use fhir_terminology::operations::validate_code::{ValidateCodeInput, validate_code};

    let bcp13 = bcp13::Bcp13Provider::new();
    let bcp47 = Bcp47Provider::new();
    assert_eq!(bcp13.language(), None);
    assert_eq!(bcp47.language(), None);
    let tag = bcp47.locate("nl").expect("reads").expect("valid");
    let dutch = bcp47
        .display(tag.concept, None)
        .expect("reads")
        .expect("a display");
    let registry = registry();
    let ask = |url: &str, code: &str, display: &str| {
        validate_code(
            &registry,
            &Invocation::Type,
            &ValidateCodeInput {
                url: Some(url.to_owned()),
                code: Some(code.to_owned()),
                display: Some(display.to_owned()),
                display_language: Some(String::from("fr")),
                ..ValidateCodeInput::default()
            },
        )
        .expect("validates")
    };

    let media = ask(bcp13::URL, "text/plain", "text/plain");
    assert!(media.result, "{media:?}");
    assert!(media.issues.is_empty(), "{media:?}");
    let language = ask(bcp47::URL, "nl", &dutch);
    assert!(language.result, "{language:?}");
    assert!(language.issues.is_empty(), "{language:?}");
}

#[test]
fn the_language_registries_declare_no_implicit_value_set_form() {
    let bcp47 = Bcp47Provider::new();
    crate::implicit_forms::declares_none(&bcp47, &bcp47.identity().url);
    let bcp13 = bcp13::Bcp13Provider::new();
    crate::implicit_forms::declares_none(&bcp13, &bcp13.identity().url);
}
