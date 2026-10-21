// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ICD-11 provider over the testkit's API-shaped artifacts, asserting
//! what the HL7 terminology ecosystem test cases for ICD-11 assert.

use ferroterm_testkit::icd11::{BLOCK, CHOLERA, RELEASE, SEPSIS, VIBRIO, write_artifacts};
use fhir_terminology::filter::{Filter, FilterOperator};
use fhir_terminology::icd11::{Icd11Provider, OpenError};
use fhir_terminology::provider::{
    CodeSystemProvider, Compositional, Concept, HierarchyMeaning, PropertyValue, ProviderError,
};

const MMS: &str = "http://id.who.int/icd/release/11/mms";

fn providers() -> (
    tempfile::TempDir,
    Icd11Provider,
    Icd11Provider,
    Icd11Provider,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    write_artifacts(dir.path()).expect("builds");
    let mms = Icd11Provider::open(&dir.path().join("mms")).expect("opens mms");
    let icf = Icd11Provider::open(&dir.path().join("icf")).expect("opens icf");
    let foundation = Icd11Provider::open(&dir.path().join("entity")).expect("opens entity");
    (dir, mms, icf, foundation)
}

fn located(provider: &Icd11Provider, code: &str) -> Concept {
    provider
        .locate(code)
        .expect("reads")
        .expect("a concept with the code")
        .concept
}

fn props(provider: &Icd11Provider, concept: Concept) -> Vec<String> {
    provider
        .properties(concept)
        .expect("reads")
        .into_iter()
        .map(|p| {
            let parts: Vec<String> = p
                .subproperties
                .iter()
                .map(|s| {
                    let description = s
                        .description
                        .as_deref()
                        .map(|d| format!(" ({d})"))
                        .unwrap_or_default();
                    format!("{}={}{description}", s.code, s.value.as_text())
                })
                .collect();
            if parts.is_empty() {
                format!("{}={}", p.code, p.value.as_text())
            } else {
                format!("{}={} [{}]", p.code, p.value.as_text(), parts.join("; "))
            }
        })
        .collect()
}

#[test]
fn codes_and_entity_uris_in_both_forms_name_the_same_concept() {
    let (_dir, mms, _icf, foundation) = providers();
    assert_eq!(mms.identity().url, MMS);
    assert_eq!(mms.identity().version, RELEASE);
    assert_eq!(
        mms.identity().title.as_deref(),
        Some("ICD-11 for Mortality and Morbidity Statistics")
    );
    assert_eq!(
        mms.declaration().hierarchy_meaning,
        Some(HierarchyMeaning::ClassifiedWith)
    );
    assert!(mms.declaration().case_sensitive);
    assert_eq!(mms.declaration().languages, ["en", "fr"]);
    let by_code = located(&mms, "1A00");
    let by_uri = located(&mms, &format!("{MMS}/{CHOLERA}"));
    let by_versioned = located(
        &mms,
        &format!("http://id.who.int/icd/release/11/{RELEASE}/mms/{CHOLERA}"),
    );
    assert_eq!(by_code, by_uri);
    assert_eq!(by_code, by_versioned);
    assert_eq!(
        mms.locate("1A00").expect("reads").expect("located").code,
        "1A00"
    );
    assert!(
        mms.locate("1a00").expect("reads").is_none(),
        "case sensitive"
    );
    assert!(mms.locate("XXXX9").expect("reads").is_none());
    assert_eq!(
        mms.display(by_code, None).expect("reads").as_deref(),
        Some("Cholera")
    );
    assert_eq!(
        mms.display(by_code, Some("fr")).expect("reads").as_deref(),
        Some("Choléra")
    );
    assert_eq!(
        mms.display(by_code, Some("de")).expect("reads").as_deref(),
        Some("Cholera"),
        "English when the language is missing"
    );
    let p = props(&mms, by_code);
    assert!(p.contains(&String::from("code=1A00")));
    assert!(p.contains(&format!("id={MMS}/{CHOLERA}")));
    assert!(p.contains(&format!("parent={MMS}/{BLOCK}")));
    assert!(p.contains(&String::from("classKind=category")));
    assert!(p.contains(&String::from("exclusion=Vibrio vulnificus infection")));
    assert!(p.iter().any(|x| x.starts_with(
        "postcoordinationScale=http://id.who.int/icd/schema/infectiousAgent [valueSet="
    )));
    assert!(!p.iter().any(|x| x.starts_with("notSelectable")));
    assert_eq!(
        mms.definition(by_code).expect("reads").as_deref(),
        Some("An infection of the intestine by Vibrio cholerae.")
    );
    let block = located(&mms, &format!("{MMS}/{BLOCK}"));
    let block_props = props(&mms, block);
    assert!(block_props.contains(&String::from("notSelectable=true")));
    assert!(!block_props.iter().any(|x| x.starts_with("code=")));
    assert_eq!(
        mms.display(block, None).expect("reads").as_deref(),
        Some("Bacterial intestinal infections")
    );
    let residual = located(&mms, "1A0Y");
    assert!(props(&mms, residual).contains(&format!("id={MMS}/1001/other")));
    assert_eq!(located(&mms, &format!("{MMS}/1001/other")), residual);

    let entity = located(
        &foundation,
        &format!("http://id.who.int/icd/entity/{CHOLERA}"),
    );
    assert_eq!(
        foundation.display(entity, None).expect("reads").as_deref(),
        Some("Cholera")
    );
    assert!(
        props(&foundation, entity)
            .contains(&format!("parent=http://id.who.int/icd/entity/{BLOCK}"))
    );
    assert!(
        foundation.locate(CHOLERA).expect("reads").is_none(),
        "a bare number is not a Foundation code"
    );
    assert!(
        foundation
            .locate("http://id.who.int/icd/entity/1001/other")
            .expect("reads")
            .is_none(),
        "the Foundation has no residuals"
    );
    assert!(
        foundation.declaration().implicit_forms.is_empty(),
        "the Foundation defines no postcoordination scale"
    );
}

#[test]
fn postcoordination_expressions_validate_against_the_axes() {
    let (_dir, mms, icf, _foundation) = providers();
    let simple = located(&mms, "1A00&XN8P1");
    assert_eq!(
        mms.display(simple, None).expect("reads").as_deref(),
        Some("Cholera [Vibrio cholerae O1, biovar cholerae]")
    );
    assert_eq!(
        mms.code(simple).expect("reads").as_deref(),
        Some("1A00&XN8P1")
    );
    let p = props(&mms, simple);
    assert!(p.contains(&String::from("code=1A00&XN8P1")));
    assert!(p.contains(&format!("id={MMS}/{CHOLERA} & {MMS}/2001")));
    assert!(p.contains(&format!(
        "stem=1A00 [stemLabel=Cholera; stemUri={MMS}/{CHOLERA}]"
    )));
    assert!(p.contains(&format!(
        "postcoordinationValues=http://id.who.int/icd/schema/infectiousAgent [XN8P1={MMS}/2001 (Vibrio cholerae O1, biovar cholerae)]"
    )));
    let uri_form = located(&mms, &format!("{MMS}/{CHOLERA} & {MMS}/2001"));
    assert_eq!(
        mms.display(uri_form, None).expect("reads").as_deref(),
        Some("Cholera [Vibrio cholerae O1, biovar cholerae]")
    );
    assert!(props(&mms, uri_form).contains(&String::from("code=1A00&XN8P1")));
    assert!(
        mms.locate("1A00&XN8P1&XN62R").expect("reads").is_some(),
        "a second value on an AllowAlways axis"
    );
    assert!(matches!(
        mms.locate("1A00&1G41"),
        Err(ProviderError::InvalidCode { ref reason, .. }) if reason.contains("1G41")
    ));
    assert!(
        mms.locate("1A00&XXXX9").is_err(),
        "an unknown value is an error"
    );
    assert!(
        mms.locate("XXXX9&XN8P1").expect("reads").is_none(),
        "an unknown stem is not a code"
    );
    let cluster = located(&mms, "1A01/1G41/1G40");
    assert_eq!(
        mms.display(cluster, None).expect("reads").as_deref(),
        Some(
            "Intestinal infection due to other Vibrio / Sepsis with septic shock / Sepsis without septic shock"
        )
    );
    let p = props(&mms, cluster);
    assert!(p.contains(&String::from("code=1A01/1G41/1G40")));
    assert!(p.contains(&format!(
        "postcoordinationValues=http://id.who.int/icd/schema/hasCausingCondition [1G41={MMS}/3001 (Sepsis with septic shock)]"
    )), "the required axis gets the first value: {p:?}");
    assert!(p.contains(&format!(
        "postcoordinationValues=http://id.who.int/icd/schema/hasManifestation [1G40={MMS}/{SEPSIS} (Sepsis without septic shock)]"
    )));
    let two_stems = located(&mms, "1A00&XN8P1/XN62R");
    assert_eq!(
        mms.display(two_stems, None).expect("reads").as_deref(),
        Some("Cholera [Vibrio cholerae O1, biovar cholerae] / Vibrio cholerae O1, biovar eltor"),
        "the second member fits the unfilled infectious-agent axis or starts a stem; the display renders the syntax either way"
    );
    let dot = located(&icf, "d5409.qp3");
    assert_eq!(
        icf.display(dot, None).expect("reads").as_deref(),
        Some("Dressing, unspecified [SEVERE performance difficulty (high, extreme,...) 50-95 %]")
    );
    assert!(props(&icf, dot).iter().any(|x| {
        x.starts_with("postcoordinationValues=http://id.who.int/icd/schema/performance [qp3=")
    }));
    assert!(
        icf.locate("d5409.3").expect("reads").is_none(),
        "the pre-2026 qualifier syntax names no code"
    );
    assert_eq!(located(&icf, "d540"), located(&icf, "d540"));
}

#[test]
fn scales_are_implicit_value_sets_and_the_tree_answers_filters() {
    let (_dir, mms, _icf, _foundation) = providers();
    let url = format!("{MMS}/{CHOLERA}/postcoordinationScale/infectiousAgent");
    let compose = mms
        .implicit_value_set(&url)
        .expect("implicit")
        .expect("compose");
    assert_eq!(compose.include.len(), 1);
    assert_eq!(compose.include[0].filters[0].op, FilterOperator::IsA);
    assert_eq!(
        compose.include[0].filters[0].value,
        format!("{MMS}/{VIBRIO}")
    );
    assert_eq!(
        compose.include[0]
            .system
            .as_ref()
            .and_then(|s| s.version.as_deref()),
        Some(RELEASE)
    );
    assert!(
        mms.implicit_value_set(&format!(
            "http://id.who.int/icd/release/11/{RELEASE}/mms/{CHOLERA}/postcoordinationScale/infectiousAgent"
        ))
        .is_some(),
        "the versioned form"
    );
    // The scale's value set carries the release as version and date, and a
    // name and title of its own (the ecosystem's `icd-11` expansions).
    let metadata = mms.implicit_metadata(&url);
    assert_eq!(metadata.version.as_deref(), Some(RELEASE));
    assert_eq!(metadata.date.as_deref(), Some(RELEASE));
    assert_eq!(metadata.experimental, Some(false));
    assert_eq!(
        metadata.name.as_deref(),
        Some(format!("PostcoordinationScale_{CHOLERA}_infectiousAgent").as_str())
    );
    assert!(metadata.title.is_some());
    assert_eq!(
        mms.implicit_metadata(&format!("{MMS}/{CHOLERA}")),
        fhir_terminology::provider::ImplicitMetadata::default(),
        "a plain entity URI is no implicit value set"
    );
    assert!(
        mms.implicit_value_set(&format!(
            "{MMS}/999999999/postcoordinationScale/infectiousAgent"
        ))
        .is_none(),
        "an unknown entity is an unknown value set"
    );
    assert!(
        mms.implicit_value_set(&format!("{MMS}/{CHOLERA}/postcoordinationScale/laterality"))
            .is_none()
    );
    let under = mms
        .filter(&Filter {
            property: String::from("concept"),
            op: FilterOperator::IsA,
            value: format!("{MMS}/{VIBRIO}"),
        })
        .expect("filters");
    assert_eq!(under.len(), 4);
    let by_code = mms
        .filter(&Filter {
            property: String::from("concept"),
            op: FilterOperator::IsA,
            value: String::from("XN7N1"),
        })
        .expect("filters");
    assert_eq!(under, by_code);
    let hierarchy = mms.hierarchy().expect("tree");
    let chapter = located(&mms, "01");
    let cholera = located(&mms, "1A00");
    assert!(hierarchy.ancestors(cholera).contains(chapter.index()));
    assert!(
        hierarchy.ancestors(located(&mms, "1A00&XN8P1")).is_empty(),
        "an expression has no place in the tree"
    );
    assert_eq!(mms.all().expect("all").len(), 12);
    assert_eq!(mms.search("chol", Some("fr")).expect("searches").len(), 1);
    assert_eq!(mms.search("asiatic", None).expect("searches").len(), 1);
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::loinc::write_artifact(dir.path()).expect("builds");
    assert!(matches!(
        Icd11Provider::open(dir.path()),
        Err(OpenError::NotIcd11(_))
    ));
    assert_eq!(PropertyValue::Uri(String::from("u")).as_text(), "u");
}

// NOTE: `ValueSet.expansion.contains.abstract` marks an entry the user cannot select as a
// value (<https://hl7.org/fhir/R5/valueset-definitions.html#ValueSet.expansion.contains.abstract>);
// the ecosystem's `notSelectable` cases refuse such a code when `abstract = false`.
#[test]
fn a_codeless_grouper_expands_abstract_and_is_refused_when_abstract_is_not_allowed() {
    use std::sync::Arc;

    use fhir_terminology::conceptmap::store::ConceptMapStore;
    use fhir_terminology::operations::expand::ExpandInput;
    use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;
    use fhir_terminology::operations::{Sources, expand, value_set_validate_code};
    use fhir_terminology::registry::Registry;
    use fhir_terminology::valueset::store::ValueSetStore;
    use fhir_types::r4b::value_set::{
        ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    write_artifacts(dir.path()).expect("builds");
    let mut registry = Registry::new();
    registry
        .register(Arc::new(
            Icd11Provider::open(&dir.path().join("mms")).expect("opens mms"),
        ))
        .expect("registers");
    let value_sets = ValueSetStore::new();
    let concept_maps = ConceptMapStore::new();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let block_uri = format!("{MMS}/{BLOCK}");
    let inline = ValueSet {
        url: Some("http://example.org/inline".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(MMS.into()),
                concept: [block_uri.as_str(), "1A00"]
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
    };
    let inline = fhir_terminology::valueset::convert::r4b::convert(&inline);
    let vs = expand::expand(
        &sources,
        &ExpandInput {
            inline_value_set: Some(inline.clone()),
            ..ExpandInput::default()
        },
    )
    .expect("expands");
    let block = vs
        .contains
        .iter()
        .find(|c| c.code == block_uri)
        .expect("the block");
    assert!(block.abstract_concept, "a codeless grouper is abstract");
    let cholera = vs
        .contains
        .iter()
        .find(|c| c.code == "1A00")
        .expect("cholera");
    assert!(!cholera.abstract_concept);

    let refused = value_set_validate_code::validate_code(
        &sources,
        &ValueSetValidateInput {
            inline_value_set: Some(inline.clone()),
            code: Some(block_uri.clone()),
            system: Some(MMS.to_owned()),
            abstract_ok: Some(false),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("validates");
    assert!(!refused.result, "abstract = false refuses the grouper");
    assert!(
        refused
            .issues
            .iter()
            .any(|i| i.kind == "code-rule" && i.text.contains("abstract")),
        "{:?}",
        refused.issues
    );
    let allowed = value_set_validate_code::validate_code(
        &sources,
        &ValueSetValidateInput {
            inline_value_set: Some(inline),
            code: Some(block_uri),
            system: Some(MMS.to_owned()),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("validates");
    assert!(allowed.result, "abstract defaults to allowed");
}

#[test]
fn a_codeless_grouper_is_abstract_and_codeless_and_a_coded_concept_is_neither() {
    let (_dir, mms, _icf, _foundation) = providers();
    // The codeless block is abstract for expansions and validation; `$lookup`
    // answers it `notSelectable` only (the ecosystem's `lookup-mms-no-code`).
    let status = mms
        .status(located(&mms, &format!("{MMS}/{BLOCK}")))
        .expect("reads");
    assert!(status.abstract_concept);
    assert!(status.codeless);
    let status = mms.status(located(&mms, "1A00")).expect("reads");
    assert!(!status.abstract_concept);
    assert!(!status.codeless);
}

// NOTE: `excludePostCoordinated` leaves post-coordinated expressions out of an
// expansion (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>).
#[test]
fn the_exclude_flags_drop_a_postcoordinated_expression_and_a_codeless_grouper() {
    use std::sync::Arc;

    use fhir_terminology::conceptmap::store::ConceptMapStore;
    use fhir_terminology::operations::expand::ExpandInput;
    use fhir_terminology::operations::{Sources, expand};
    use fhir_terminology::registry::Registry;
    use fhir_terminology::valueset::store::ValueSetStore;
    use fhir_types::r4b::value_set::{
        ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    write_artifacts(dir.path()).expect("builds");
    let mut registry = Registry::new();
    registry
        .register(Arc::new(
            Icd11Provider::open(&dir.path().join("mms")).expect("opens mms"),
        ))
        .expect("registers");
    let value_sets = ValueSetStore::new();
    let concept_maps = ConceptMapStore::new();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let block_uri = format!("{MMS}/{BLOCK}");
    let inline = ValueSet {
        url: Some("http://example.org/inline".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(MMS.into()),
                concept: [block_uri.as_str(), "1A00", "1A00&XN8P1"]
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
    };
    let inline = fhir_terminology::valueset::convert::r4b::convert(&inline);
    let codes = |input: ExpandInput| -> Vec<String> {
        let vs = expand::expand(&sources, &input).expect("expands");
        vs.contains.iter().map(|c| c.code.clone()).collect()
    };
    let all = codes(ExpandInput {
        inline_value_set: Some(inline.clone()),
        ..ExpandInput::default()
    });
    assert_eq!(all.len(), 3, "{all:?}");
    assert!(all.contains(&String::from("1A00&XN8P1")));
    let no_expressions = codes(ExpandInput {
        inline_value_set: Some(inline.clone()),
        exclude_post_coordinated: Some(true),
        ..ExpandInput::default()
    });
    assert_eq!(no_expressions.len(), 2, "{no_expressions:?}");
    assert!(!no_expressions.iter().any(|c| c.contains('&')));
    assert!(no_expressions.contains(&block_uri));
    let selectable = codes(ExpandInput {
        inline_value_set: Some(inline),
        exclude_not_for_ui: Some(true),
        exclude_post_coordinated: Some(true),
        ..ExpandInput::default()
    });
    assert_eq!(selectable, ["1A00"]);
}

// NOTE: no FHIR specification governs which spelling `expansion.contains.code` carries when
// a system admits several; the compose's spelling is kept, in every include and through an
// `include.valueSet` import (the ecosystem's icd-11 `expand-adhoc-enum-uri` pins the direct case).
#[test]
fn the_composes_spelling_survives_an_include_merge_and_a_value_set_import() {
    use std::sync::Arc;

    use fhir_terminology::conceptmap::store::ConceptMapStore;
    use fhir_terminology::operations::expand::ExpandInput;
    use fhir_terminology::operations::{Sources, expand};
    use fhir_terminology::registry::Registry;
    use fhir_terminology::valueset::store::ValueSetStore;
    use fhir_types::r4b::value_set::{
        ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    write_artifacts(dir.path()).expect("builds");
    let mut registry = Registry::new();
    registry
        .register(Arc::new(
            Icd11Provider::open(&dir.path().join("mms")).expect("opens mms"),
        ))
        .expect("registers");
    let enumerated = |codes: &[&str]| ValueSetComposeInclude {
        system: Some(MMS.into()),
        concept: codes
            .iter()
            .map(|code| ValueSetComposeIncludeConcept {
                code: (*code).into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let cholera_uri = format!("{MMS}/{CHOLERA}");
    let vibrio_uri = format!("{MMS}/{VIBRIO}");
    let uri_set = ValueSet {
        url: Some("http://example.org/icd11-uri".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![enumerated(&[cholera_uri.as_str()])],
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut value_sets = ValueSetStore::new();
    value_sets
        .insert(fhir_terminology::valueset::convert::r4b::convert(&uri_set).expect("converts"))
        .expect("stores");
    let concept_maps = ConceptMapStore::new();
    let sources = Sources {
        registry: &registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let codes = |value_set: ValueSet| -> Vec<String> {
        let vs = expand::expand(
            &sources,
            &ExpandInput {
                inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(
                    &value_set,
                )),
                ..ExpandInput::default()
            },
        )
        .expect("expands");
        vs.contains.iter().map(|c| c.code.clone()).collect()
    };

    // Two includes over one system: the first by code, the second by URI.
    let merged = codes(ValueSet {
        url: Some("http://example.org/inline".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![enumerated(&["1A00"]), enumerated(&[vibrio_uri.as_str()])],
            ..Default::default()
        }),
        ..Default::default()
    });
    assert!(merged.contains(&String::from("1A00")), "{merged:?}");
    assert!(
        merged.contains(&vibrio_uri),
        "the second include's URI spelling: {merged:?}"
    );

    // An import through `include.valueSet` keeps the imported compose's spelling.
    let imported = codes(ValueSet {
        url: Some("http://example.org/inline-import".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                value_set: vec!["http://example.org/icd11-uri".into()],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    });
    assert_eq!(imported, [cholera_uri], "the imported URI spelling");
}

#[test]
fn a_linearizations_grammar_is_defined_and_supported_and_the_foundation_has_none() {
    let (_dir, mms, icf, foundation) = providers();
    // `CodeSystem.compositional` is "The code system defines a compositional
    // (post-coordination) grammar"
    // (<https://hl7.org/fhir/R4B/codesystem-definitions.html#CodeSystem.compositional>)
    // and `TerminologyCapabilities.codeSystem.version.compositional` is "If the
    // compositional grammar defined by the code system is supported"
    // (<https://hl7.org/fhir/R4B/terminologycapabilities-definitions.html#TerminologyCapabilities.codeSystem.version.compositional>).
    // A linearization defines postcoordination clusters and this provider
    // locates one, so both hold; the foundation defines no such grammar.
    for provider in [&mms, &icf] {
        assert_eq!(
            provider.declaration().compositional,
            Compositional::Supported
        );
        assert!(provider.declaration().compositional.defined());
        assert!(provider.declaration().compositional.supported());
    }
    assert_eq!(foundation.declaration().compositional, Compositional::None);
    assert!(!foundation.declaration().compositional.defined());
    assert!(!foundation.declaration().compositional.supported());
}

// NOTE: `displayLanguage` "Specifies the language to be used for description
// when validating the display property"
// (<https://hl7.org/fhir/R4B/codesystem-operation-validate-code.html>).
#[test]
fn a_display_in_the_base_language_does_not_satisfy_a_request_for_another_one() {
    use std::sync::Arc;

    use fhir_terminology::operations::Invocation;
    use fhir_terminology::operations::validate_code::{ValidateCodeInput, validate_code};
    use fhir_terminology::registry::Registry;

    let dir = tempfile::tempdir().expect("tempdir");
    write_artifacts(dir.path()).expect("builds");
    let mut registry = Registry::new();
    registry
        .register(Arc::new(
            Icd11Provider::open(&dir.path().join("mms")).expect("opens mms"),
        ))
        .expect("registers");
    let ask = |code: &str, display: &str| {
        validate_code(
            &registry,
            &Invocation::Type,
            &ValidateCodeInput {
                url: Some(MMS.to_owned()),
                code: Some(code.to_owned()),
                display: Some(display.to_owned()),
                display_language: Some(String::from("fr")),
                ..ValidateCodeInput::default()
            },
        )
        .expect("validates")
    };

    // The fixture carries a French title for 1A00 alone, so the English title
    // is the wrong display for a French request and the French one is named.
    let wrong = ask("1A00", "Cholera");
    assert!(!wrong.result, "{wrong:?}");
    assert_eq!(wrong.display.as_deref(), Some("Choléra"));
    let issue = wrong.issues.first().expect("an issue");
    assert_eq!(issue.severity, "error");
    assert_eq!(issue.kind, "invalid-display");
    assert_eq!(
        issue.message_id(),
        "Display_Name_for__should_be_one_of__instead_of"
    );
    assert!(issue.text.contains("Choléra"), "{}", issue.text);
    assert_eq!(wrong.message.as_deref(), Some(issue.text.as_str()));

    // 1A01 has no French title at all, which is the shape of a linearization
    // built from the English bundle alone: the English display stands, and the
    // answer says the requested language has none.
    let none = ask("1A01", "Intestinal infection due to other Vibrio");
    assert!(none.result, "{none:?}");
    let issue = none.issues.first().expect("an issue");
    assert_eq!(issue.severity, "information");
    assert_eq!(issue.kind, "invalid-display");
    assert_eq!(
        issue.message_id(),
        "NO_VALID_DISPLAY_FOUND_NONE_FOR_LANG_OK"
    );
    assert!(none.message.is_some(), "{none:?}");
}

#[test]
fn every_declared_implicit_value_set_form_resolves() {
    // No FHIR spec defines the scale form: the WHO ICD-API's URI
    // (<https://icd.who.int/icdapi>), with the entity and the axis as arguments.
    let (_dir, mms, _icf, foundation) = providers();
    let entity = format!("{MMS}/{CHOLERA}");
    crate::implicit_forms::every_form_resolves(
        &mms,
        &[(
            "[entity]/postcoordinationScale/[axis]",
            &[&entity, "infectiousAgent"],
        )],
    );
    crate::implicit_forms::declares_none(&foundation, &foundation.identity().url);
}
