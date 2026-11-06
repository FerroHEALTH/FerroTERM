// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! BCP 47 language tags: validity, case, subsumption and expansion.

use concept_graph::subsumption::Outcome;
use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::operations::expand::ExpandInput;
use fhir_terminology::operations::{Invocation, OperationError, expand, sources::Sources};
use fhir_terminology::provider::declaration::ContentMode;
use fhir_terminology::provider::filter::FilterOperator;
use fhir_terminology::provider::{CodeSystemProvider, error::ProviderError};
use fhir_terminology::registries::bcp47::{Analysis, Bcp47Provider};
use fhir_terminology::registries::{bcp13, bcp47, subtags};
use fhir_terminology::valueset::expansion::ParameterValue;
use fhir_terminology::valueset::store::ValueSetStore;
use fhir_types::r4b::value_set::{
    ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeFilter,
};

use super::{enumerating, filter, inline, registry};

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
    let value_sets = ValueSetStore::default();
    let concept_maps = ConceptMapStore::default();
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
    let value_sets = ValueSetStore::default();
    let concept_maps = ConceptMapStore::default();
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
    let value_sets = ValueSetStore::default();
    let concept_maps = ConceptMapStore::default();
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
    let value_sets = ValueSetStore::default();
    let concept_maps = ConceptMapStore::default();
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
    let value_sets = ValueSetStore::default();
    let concept_maps = ConceptMapStore::default();
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
    let value_sets = ValueSetStore::default();
    let concept_maps = ConceptMapStore::default();
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
