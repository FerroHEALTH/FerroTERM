// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ValueSet/$expand`: the listing, paging, filters, designations, properties and nesting.

use ferroterm_testkit::fhir::{ANIMALS, VS_ALL, VS_PETS};
use fhir_terminology::operations::expand::ExpandInput;
use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;
use fhir_terminology::operations::{OperationError, expand, value_set_validate_code};
use fhir_terminology::provider::PropertyValue;
use fhir_terminology::valueset;
use fhir_terminology::valueset::expansion::ParameterValue;
use fhir_terminology::valueset::model::ExpansionDefault;
use fhir_types::r4b::extension::{Extension, ExtensionValue};
use fhir_types::r4b::value_set::{
    ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
    ValueSetComposeIncludeFilter,
};

use super::{World, codes, entry, flatten, parameter};

#[test]
fn expand_lists_the_system_flat_with_the_parameter_echo_and_used_codesystem() {
    let world = World::load();
    let request = ExpandInput {
        url: Some(VS_ALL.to_owned()),
        value_set_version: Some(String::from("1.0")),
        exclude_nested: Some(true),
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &request).expect("expands");
    assert_eq!(vs.model.version.as_deref(), Some("1.0"));
    assert!(
        !vs.include_definition,
        "includeDefinition defaults to false"
    );
    assert_eq!(vs.total, 9);
    assert!(vs.identifier.starts_with("urn:uuid:"));
    assert_eq!(
        codes(&vs),
        [
            "animal", "cat", "dodo", "dog", "fish", "kitten", "living", "pet", "plant"
        ]
    );
    let dodo = entry(&vs.contains, "dodo").expect("dodo");
    assert!(dodo.inactive);
    let pet = entry(&vs.contains, "pet").expect("pet");
    assert!(pet.abstract_concept);
    assert_eq!(parameter(&vs, "excludeNested").len(), 1);
    assert_eq!(
        parameter(&vs, "used-codesystem"),
        [ParameterValue::Uri(format!("{ANIMALS}|2.0"))]
    );
}

#[test]
fn expand_pages_filters_and_honours_active_only() {
    let world = World::load();
    let page = ExpandInput {
        url: Some(VS_ALL.to_owned()),
        offset: Some(2),
        count: Some(3),
        active_only: Some(true),
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &page).expect("expands");
    assert_eq!(vs.total, 7, "2.0 drops dodo; activeOnly drops fish");
    assert_eq!(vs.offset, Some(2));
    assert_eq!(codes(&vs), ["dog", "kitten", "living"]);
    let text = ExpandInput {
        url: Some(VS_ALL.to_owned()),
        filter: Some(String::from("kat")),
        display_language: Some(String::from("de")),
        include_designations: Some(true),
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &text).expect("expands");
    assert_eq!(codes(&vs), ["cat"]);
    let cat = &vs.contains[0];
    assert_eq!(cat.display.as_deref(), Some("Katze"));
    // NOTE: `includeDesignations` lists every designation; `displayLanguage` chooses
    // the display alone (the ecosystem's language cases, #190).
    assert_eq!(
        cat.designations.len(),
        2,
        "every designation, whatever the display language"
    );
    let zero = ExpandInput {
        url: Some(VS_ALL.to_owned()),
        count: Some(0),
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &zero).expect("expands");
    assert_eq!(codes(&vs), Vec::<String>::new());
    assert_eq!(vs.total, 8);
}

#[test]
fn an_inactive_concept_the_value_set_refuses_still_carries_its_status() {
    let world = World::load();
    let inline = ValueSet {
        url: Some("http://example.org/inline/active-only".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            inactive: Some(false.into()),
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let request = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        code: Some(String::from("dodo")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &request).expect("validates");
    assert!(!validation.result);
    // "the return value from $validate-code for the concept SHALL include a
    // warning that the code is invalid, and the parameters 'inactive' and
    // 'status' SHALL be populated"
    // (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
    assert_eq!(validation.inactive, Some(true));
    assert_eq!(validation.code.as_deref(), Some("dodo"));
    assert_eq!(validation.display.as_deref(), Some("Dodo"));
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["code-rule", "not-in-vs", "code-comment"]);
}

#[test]
fn expand_refuses_what_it_cannot_answer() {
    let world = World::load();
    let unknown = ExpandInput {
        url: Some(String::from("http://example.org/fhir/ValueSet/nowhere")),
        ..ExpandInput::default()
    };
    let error = expand::expand(&world.sources(), &unknown).expect_err("unknown");
    assert!(matches!(error, OperationError::UnknownValueSet(_)));
    assert_eq!(error.status(), http::StatusCode::NOT_FOUND);
    let none = ExpandInput::default();
    assert!(matches!(
        expand::expand(&world.sources(), &none),
        Err(OperationError::Required(_))
    ));
    let context = ExpandInput {
        url: Some(VS_ALL.to_owned()),
        context: true,
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&world.sources(), &context),
        Err(OperationError::NotSupported(_))
    ));
    let negative = ExpandInput {
        url: Some(VS_ALL.to_owned()),
        offset: Some(-1),
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&world.sources(), &negative),
        Err(OperationError::Invalid(_))
    ));
    let unknown_system = ValueSet {
        url: Some("http://example.org/inline".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some("http://example.org/fhir/CodeSystem/nowhere".into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let request = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&unknown_system)),
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&world.sources(), &request),
        Err(OperationError::UnknownSystem(_))
    ));
    let no_system = ValueSet {
        url: Some("http://example.org/inline".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude::default()],
            ..Default::default()
        }),
        ..Default::default()
    };
    let request = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&no_system)),
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&world.sources(), &request),
        Err(OperationError::ValueSetInvalid { .. })
    ));
}

// NOTE: R5 `$expand` `property` asks for concept properties on each `contains`,
// by code or by the property's URI, `*` for all
// (<https://hl7.org/fhir/R5/valueset-operation-expand.html>).
#[test]
fn expand_returns_the_properties_asked_for() {
    let world = World::load();
    let by_code = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_PETS.to_owned()),
            property: vec![String::from("legs")],
            ..ExpandInput::default()
        },
    )
    .expect("expands");
    let kitten = entry(&by_code.contains, "kitten").expect("kitten");
    assert_eq!(kitten.properties.len(), 1);
    assert_eq!(kitten.properties[0].code, "legs");
    assert_eq!(kitten.properties[0].value, PropertyValue::Integer(4));
    let pet = entry(&by_code.contains, "pet").expect("pet");
    assert!(pet.properties.is_empty(), "pet declares no leg count");
    assert_eq!(by_code.properties.len(), 1);
    assert_eq!(by_code.properties[0].code, "legs");
    assert_eq!(
        by_code.properties[0].uri.as_deref(),
        Some("http://example.org/legs")
    );
    // NOTE: the ecosystem does not echo `property` among the expansion parameters (#190).
    assert_eq!(
        parameter(&by_code, "property"),
        Vec::<ParameterValue>::new()
    );
    let by_uri = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_PETS.to_owned()),
            property: vec![String::from("http://example.org/legs")],
            ..ExpandInput::default()
        },
    )
    .expect("expands");
    assert_eq!(
        by_uri.contains[0].properties,
        by_code.contains[0].properties
    );
    let all = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_PETS.to_owned()),
            property: vec![String::from("*")],
            ..ExpandInput::default()
        },
    )
    .expect("expands");
    let kitten = entry(&all.contains, "kitten").expect("kitten");
    let codes: Vec<&str> = kitten.properties.iter().map(|p| p.code.as_str()).collect();
    assert!(
        codes.contains(&"legs") && codes.contains(&"parent"),
        "{codes:?}"
    );
    let none = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_PETS.to_owned()),
            ..ExpandInput::default()
        },
    )
    .expect("expands");
    assert!(none.contains.iter().all(|c| c.properties.is_empty()));
    assert_eq!(
        none.properties,
        Vec::<valueset::expansion::ExpansionProperty>::new()
    );
}

// NOTE: an expansion lists every designation unless asked for some, flags an
// inactive concept with its status, and echoes neither `property` nor a
// language it was not asked for (the ecosystem's `$expand` cases, #190).
#[test]
fn expand_lists_every_designation_and_flags_inactive_concepts() {
    let world = World::load();
    let vs = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_ALL.to_owned()),
            value_set_version: Some(String::from("1.0")),
            include_designations: Some(true),
            property: vec![String::from("legs")],
            ..ExpandInput::default()
        },
    )
    .expect("expands");
    let names: Vec<&str> = vs.parameters.iter().map(|p| p.name.as_str()).collect();
    assert!(!names.contains(&"property"), "{names:?}");
    assert!(
        !names.contains(&"displayLanguage"),
        "no language is echoed when none was asked for: {names:?}"
    );
    // The compose is one whole-system include, so the expansion nests and `fish`
    // sits under `living` > `animal`.
    let fish = entry(&vs.contains, "fish").expect("fish");
    assert!(fish.inactive);
    let codes: Vec<&str> = fish.properties.iter().map(|p| p.code.as_str()).collect();
    assert_eq!(
        codes,
        ["legs", "status"],
        "the status property rides along unasked"
    );
    assert_eq!(
        fish.properties[1].value,
        PropertyValue::Code(String::from("retired"))
    );
    let cat = entry(&vs.contains, "cat").expect("cat");
    let mut languages: Vec<Option<&str>> = cat
        .designations
        .iter()
        .map(|d| d.language.as_deref())
        .collect();
    languages.sort_unstable();
    assert_eq!(
        languages,
        [Some("de"), Some("en")],
        "every designation, whatever the display language"
    );
    let german = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_ALL.to_owned()),
            value_set_version: Some(String::from("1.0")),
            include_designations: Some(true),
            display_language: Some(String::from("de")),
            ..ExpandInput::default()
        },
    )
    .expect("expands");
    let cat = entry(&german.contains, "cat").expect("cat");
    assert_eq!(
        cat.designations.len(),
        2,
        "displayLanguage narrows nothing: {:?}",
        cat.designations
    );
    let names: Vec<&str> = german.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names.iter().filter(|n| **n == "displayLanguage").count(),
        1,
        "the requested language once"
    );
}

#[test]
fn expand_anchors_a_regex_filter_to_the_whole_code() {
    let world = World::load();
    let inline = ValueSet {
        url: Some("http://example.org/three-letters".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                filter: vec![ValueSetComposeIncludeFilter {
                    property: "code".into(),
                    op: "regex".into(),
                    value: "[a-z]{3}".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let request = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &request).expect("expands");
    // NOTE: `regex` matches the whole value (<https://hl7.org/fhir/R4B/codesystem-filter-operator.html>);
    // an unanchored match would also list `animal` and `kitten`.
    assert_eq!(codes(&vs), ["cat", "dog", "pet"]);
}

#[test]
fn expand_applies_the_value_sets_default_expansion_parameters() {
    let world = World::load();
    let default = |name: &str, value: &str| Extension {
        url: "http://hl7.org/fhir/StructureDefinition/valueset-expansion-parameter".into(),
        extension: vec![
            Extension {
                url: "name".into(),
                value: Some(ExtensionValue::Code(name.into())),
                ..Default::default()
            },
            Extension {
                url: "value".into(),
                value: Some(ExtensionValue::Code(value.into())),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let inline = ValueSet {
        url: Some("http://example.org/german-cats".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            extension: vec![default("displayLanguage", "de")],
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                concept: vec![ValueSetComposeIncludeConcept {
                    code: "cat".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let model = valueset::convert::r4b::convert(&inline).expect("converts");
    assert_eq!(
        model.expansion_parameters,
        [ExpansionDefault {
            name: String::from("displayLanguage"),
            value: String::from("de"),
        }]
    );
    let request = ExpandInput {
        inline_value_set: Some(Ok(model)),
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &request).expect("expands");
    assert_eq!(
        parameter(&vs, "displayLanguage"),
        [ParameterValue::Code(String::from("de"))],
        "the default is applied and echoed"
    );
    assert_eq!(vs.contains[0].display.as_deref(), Some("Katze"));
    let english = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        display_language: Some(String::from("en")),
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &english).expect("expands");
    assert_eq!(
        parameter(&vs, "displayLanguage"),
        [ParameterValue::Code(String::from("en"))],
        "the client's language wins over the default"
    );
}

#[test]
fn an_enumerated_code_the_system_lacks_leaves_the_expansion() {
    let world = World::load();
    let inline = ValueSet {
        url: Some("http://example.org/with-a-stranger".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                concept: vec![
                    ValueSetComposeIncludeConcept {
                        code: "cat".into(),
                        ..Default::default()
                    },
                    ValueSetComposeIncludeConcept {
                        code: "unicorn".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let request = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        ..ExpandInput::default()
    };
    // NOTE: the ecosystem expands such a value set to the codes the system has
    // (its `expand-enum-bad` cases); the specification is silent.
    let vs = expand::expand(&world.sources(), &request).expect("expands");
    assert_eq!(codes(&vs), ["cat"]);
    assert_eq!(vs.total, 1);
}

// NOTE: `excludeNotForUI` leaves out the abstract groupers a user interface would
// show for navigation (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>).
#[test]
fn exclude_not_for_ui_drops_the_abstract_groupers_and_the_total_follows() {
    let world = World::load();
    let request = ExpandInput {
        url: Some(VS_ALL.to_owned()),
        value_set_version: Some(String::from("1.0")),
        exclude_not_for_ui: Some(true),
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &request).expect("expands");
    let every = flatten(&vs.contains);
    assert!(
        !every.contains(&"pet") && !every.contains(&"living"),
        "the abstract `pet` and `living` are left out: {every:?}"
    );
    assert!(entry(&vs.contains, "cat").is_some_and(|c| !c.abstract_concept));
    assert_eq!(vs.total, 7);
    assert_eq!(parameter(&vs, "excludeNotForUI").len(), 1);
}

// NOTE: one hierarchical include expands as the system's own tree
// (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.expansion.contains>);
// the ecosystem's `parameters-expand-*-hierarchy` cases pin which composes nest.
#[test]
fn a_single_hierarchical_include_nests_and_pages_over_the_flattening() {
    let world = World::load();
    let nested = |input: ExpandInput| expand::expand(&world.sources(), &input).expect("expands");

    let whole = nested(ExpandInput {
        url: Some(VS_ALL.to_owned()),
        value_set_version: Some(String::from("1.0")),
        ..ExpandInput::default()
    });
    assert_eq!(
        whole.total, 9,
        "the total counts every concept, not the roots"
    );
    // The roots are the concepts no other selected concept subsumes; the fixture
    // nests `animal` and `plant` under `living`, and `kitten` under `cat` and `pet`.
    let roots: Vec<&str> = whole.contains.iter().map(|c| c.code.as_str()).collect();
    assert_eq!(roots, ["dodo", "living", "pet"], "{roots:?}");
    assert_eq!(
        flatten(&whole.contains).len(),
        9,
        "every concept appears once in the tree"
    );
    let living = entry(&whole.contains, "living").expect("living");
    let below: Vec<&str> = living.contains.iter().map(|c| c.code.as_str()).collect();
    assert_eq!(below, ["animal", "plant"], "{below:?}");
    let animal = living.contains.first().expect("animal");
    let under_animal: Vec<&str> = animal.contains.iter().map(|c| c.code.as_str()).collect();
    assert_eq!(under_animal, ["cat", "dog", "fish"], "{under_animal:?}");

    // `excludeNested` keeps the same concepts flat, and the echo says so.
    let flat = nested(ExpandInput {
        url: Some(VS_ALL.to_owned()),
        value_set_version: Some(String::from("1.0")),
        exclude_nested: Some(true),
        ..ExpandInput::default()
    });
    assert_eq!(flat.total, whole.total);
    assert_eq!(flat.contains.len(), 9, "every concept is a root");
    assert!(flat.contains.iter().all(|c| c.contains.is_empty()));
    assert_eq!(
        parameter(&flat, "excludeNested"),
        [ParameterValue::Boolean(true)]
    );

    // The page is the pre-order flattening, so paging cuts the tree in order.
    let page = nested(ExpandInput {
        url: Some(VS_ALL.to_owned()),
        value_set_version: Some(String::from("1.0")),
        offset: Some(1),
        count: Some(3),
        ..ExpandInput::default()
    });
    assert_eq!(page.total, 9, "the total is the whole expansion");
    let paged = flatten(&page.contains);
    assert_eq!(paged.len(), 3, "the page carries three concepts: {paged:?}");
    let whole_order = flatten(&whole.contains);
    assert_eq!(
        paged,
        whole_order.get(1..4).expect("the same flattening"),
        "the page is the slice the offset and count name"
    );
}

// The walk that produces the flattening stops at the end of the page (#309), so
// every page is asserted against the whole, not only the first one.
#[test]
fn every_page_of_a_nested_expansion_is_the_slice_of_the_whole_flattening() {
    let world = World::load();
    let expand = |input: ExpandInput| expand::expand(&world.sources(), &input).expect("expands");
    let whole = expand(ExpandInput {
        url: Some(VS_ALL.to_owned()),
        value_set_version: Some(String::from("1.0")),
        ..ExpandInput::default()
    });
    let order = flatten(&whole.contains);
    assert_eq!(order.len(), 9);
    for offset in 0..=order.len() {
        for count in 1..=order.len() {
            let asked_offset = i64::try_from(offset).expect("the fixture is small");
            let asked_count = i64::try_from(count).expect("the fixture is small");
            let page = expand(ExpandInput {
                url: Some(VS_ALL.to_owned()),
                value_set_version: Some(String::from("1.0")),
                offset: Some(asked_offset),
                count: Some(asked_count),
                ..ExpandInput::default()
            });
            let end = offset.saturating_add(count).min(order.len());
            let expected = order.get(offset.min(order.len())..end).unwrap_or_default();
            assert_eq!(
                flatten(&page.contains),
                expected,
                "offset {offset}, count {count}"
            );
            assert_eq!(page.total, 9, "offset {offset}, count {count}");
        }
    }
}
