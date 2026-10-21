// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ValueSet/$expand` and `ValueSet/$validate-code` on R4B over the compose
//! layer (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>,
//! <https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>).

use std::sync::Arc;

use ferroterm_testkit::fhir::{
    ANIMALS, ANIMALS_NL, COLOURS, VS_ALL, VS_COLOURS, VS_ENUMERATED, VS_LOOP_A, VS_PETS,
    VS_PETS_REF, write_code_systems,
};
use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::fhir_codesystem::load::{FhirVersion, load_dir};
use fhir_terminology::fhir_codesystem::provider::FhirCodeSystem;
use fhir_terminology::operations::expand::{
    Contains, ExpandInput, ExpansionOutcome, ParameterValue,
};
use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;
use fhir_terminology::operations::{CodeableConceptRef, CodingRef, Issue};
use fhir_terminology::operations::{OperationError, Sources, expand, value_set_validate_code};
use fhir_terminology::provider::PropertyValue;
use fhir_terminology::registry::Registry;
use fhir_terminology::valueset;
use fhir_terminology::valueset::model::ExpansionDefault;
use fhir_terminology::valueset::store::ValueSetStore;
use fhir_types::r4b::extension::{Extension, ExtensionValue};
use fhir_types::r4b::value_set::{
    ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
    ValueSetComposeIncludeFilter,
};

pub(crate) struct World {
    _dir: tempfile::TempDir,
    registry: Registry,
    value_sets: ValueSetStore,
    concept_maps: ConceptMapStore,
}

impl World {
    pub(crate) fn load() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        write_code_systems(dir.path()).expect("writes");
        Self::from_dir(dir)
    }

    /// A world holding only `files`, written the way a package directory
    /// holds them.
    pub(crate) fn of(files: &[(&str, serde_json::Value)]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        for (name, value) in files {
            std::fs::write(
                dir.path().join(name),
                serde_json::to_string_pretty(value).expect("serialises"),
            )
            .expect("writes");
        }
        Self::from_dir(dir)
    }

    fn from_dir(dir: tempfile::TempDir) -> Self {
        let mut registry = Registry::new();
        for model in load_dir(dir.path(), FhirVersion::R5).expect("loads") {
            if model.supplements.is_none() {
                registry
                    .register(Arc::new(FhirCodeSystem::new(model).expect("builds")))
                    .expect("registers");
            } else {
                let target = model
                    .supplements
                    .clone()
                    .expect("a supplement names its system");
                registry.register_supplement(
                    target,
                    fhir_terminology::supplement::Supplement::from_code_system(&model),
                );
            }
        }
        let mut value_sets = ValueSetStore::new();
        for model in valueset::load::load_dir(dir.path(), FhirVersion::R5).expect("loads") {
            value_sets.insert(model).expect("stores");
        }
        let mut concept_maps = ConceptMapStore::new();
        for model in fhir_terminology::conceptmap::load::load_dir(dir.path(), FhirVersion::R5)
            .expect("loads")
        {
            concept_maps.insert(model).expect("stores");
        }
        Self {
            _dir: dir,
            registry,
            value_sets,
            concept_maps,
        }
    }

    pub(crate) fn registry(&self) -> &Registry {
        &self.registry
    }

    pub(crate) fn concept_maps(&self) -> &ConceptMapStore {
        &self.concept_maps
    }

    pub(crate) fn sources(&self) -> Sources<'_> {
        Sources {
            registry: &self.registry,
            value_sets: &self.value_sets,
            concept_maps: &self.concept_maps,
        }
    }
}

/// A codeable concept of `coding`, with no text.
fn codeable(coding: Vec<CodingRef>) -> CodeableConceptRef {
    CodeableConceptRef { coding, text: None }
}

fn codes(outcome: &ExpansionOutcome) -> Vec<String> {
    outcome.contains.iter().map(|c| c.code.clone()).collect()
}

fn parameter(outcome: &ExpansionOutcome, name: &str) -> Vec<ParameterValue> {
    outcome
        .parameters
        .iter()
        .filter(|p| p.name == name)
        .map(|p| p.value.clone())
        .collect()
}

#[test]
fn the_store_loads_every_value_set_and_defaults_to_the_greatest_version() {
    let world = World::load();
    assert_eq!(world.value_sets.len(), 8);
    let default = world.value_sets.resolve(VS_ALL, None).expect("default");
    assert_eq!(default.version.as_deref(), Some("2.0"));
    let pinned = world
        .value_sets
        .resolve(&format!("{VS_ALL}|1.0"), None)
        .expect("1.0");
    assert_eq!(pinned.version.as_deref(), Some("1.0"));
    assert!(world.value_sets.resolve(VS_ALL, Some("3.0")).is_none());
}

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
    assert!(codes(&vs).is_empty());
    assert_eq!(vs.total, 8);
}

#[test]
fn expand_follows_value_set_references_and_refuses_a_cycle() {
    let world = World::load();
    let referenced = ExpandInput {
        url: Some(VS_PETS_REF.to_owned()),
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &referenced).expect("expands");
    // The referenced expansion states an order and this one keeps it: `pet` is
    // the root of the referenced hierarchy, `kitten` its child (#276).
    assert_eq!(codes(&vs), ["pet", "kitten"]);
    let looped = ExpandInput {
        url: Some(VS_LOOP_A.to_owned()),
        ..ExpandInput::default()
    };
    let error = expand::expand(&world.sources(), &looped).expect_err("cycle");
    assert!(
        matches!(error, OperationError::ValueSetCyclic(_)),
        "{error}"
    );
    // A cycle is a processing failure, "there is no point resubmitting the same
    // content unchanged" (<https://hl7.org/fhir/R4B/valueset-issue-type.html>).
    assert_eq!(error.issue_code(), "processing");
    assert_eq!(error.tx_issue_type(), "vs-invalid");
}

#[test]
fn a_compose_reaches_a_contained_value_set_through_its_fragment() {
    let world = World::load();
    let contained = ValueSet {
        id: Some(String::from("vs1")),
        url: Some("http://example.org/inline/contained".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                concept: vec![
                    ValueSetComposeIncludeConcept {
                        code: "cat".into(),
                        ..Default::default()
                    },
                    ValueSetComposeIncludeConcept {
                        code: "kitten".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let inline = ValueSet {
        url: Some("http://example.org/inline/container".into()),
        status: "active".into(),
        contained: vec![fhir_types::r4b::resource::Resource::ValueSet(Box::new(
            contained,
        ))],
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                value_set: vec!["#vs1".into(), VS_PETS.into()],
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
    // Two value sets in one include intersect, so only `kitten` survives.
    assert_eq!(codes(&vs), ["kitten"]);
    assert_eq!(
        parameter(&vs, "used-valueset"),
        [ParameterValue::Uri(format!("{VS_PETS}|1.0"))],
        "a contained value set has no canonical of its own to report"
    );
    let validate = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &validate).expect("validates");
    assert!(
        validation.result,
        "the contained value set resolves here too"
    );
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
fn one_coding_answers_for_the_concept_when_the_include_pins_another_version() {
    let world = World::load();
    let inline = ValueSet {
        url: Some("http://example.org/inline/pinned".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                version: Some("2.0".into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let request = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        codeable_concept: Some(codeable(vec![CodingRef {
            system: Some(ANIMALS.to_owned()),
            version: Some(String::from("1.0")),
            code: Some(String::from("cat")),
            ..CodingRef::default()
        }])),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &request).expect("validates");
    assert!(!validation.result);
    // "the server SHALL return the code system and code against which
    // validation was based (`system` and `code` parameters)"
    // (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
    assert_eq!(validation.code.as_deref(), Some("cat"));
    assert_eq!(validation.system.as_deref(), Some(ANIMALS));
    assert_eq!(validation.version.as_deref(), Some("2.0"));
    assert!(
        validation.issues.iter().any(|i| i.kind == "vs-invalid"),
        "the version disagreement is the reason"
    );
    assert!(
        !validation.issues.iter().any(|i| i.kind == "not-in-vs"),
        "one coding needs no summary of the codings that failed"
    );
}

#[test]
fn expand_takes_an_inline_value_set_and_pins_system_versions() {
    let world = World::load();
    let inline = ValueSet {
        url: Some("http://example.org/inline".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(COLOURS.into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let request = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        include_definition: Some(true),
        system_version: vec![format!("{COLOURS}|1")],
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &request).expect("expands");
    assert_eq!(codes(&vs), ["Green", "RED", "blue"]);
    assert!(
        vs.include_definition,
        "includeDefinition returns the compose"
    );
    let both = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        url: Some(VS_ALL.to_owned()),
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&world.sources(), &both),
        Err(OperationError::Invalid(_))
    ));
    let wrong_version = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        check_system_version: vec![format!("{COLOURS}|9")],
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&world.sources(), &wrong_version),
        Err(OperationError::UnknownVersion { .. })
    ));
    let excluded = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        exclude_system: vec![COLOURS.to_owned()],
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &excluded).expect("expands");
    assert!(codes(&vs).is_empty());
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

#[test]
fn an_unknown_filter_operator_is_refused_at_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("ValueSet-bad.json"),
        serde_json::json!({
            "resourceType": "ValueSet", "url": "http://example.org/bad", "status": "active",
            "compose": {"include": [{"system": ANIMALS, "filter": [{"property": "concept", "op": "equals", "value": "cat"}]}]}
        })
        .to_string(),
    )
    .expect("writes");
    let error = valueset::load::load_dir(dir.path(), FhirVersion::R4B).expect_err("refused");
    assert!(error.to_string().contains("ValueSet-bad.json"), "{error}");
}

#[test]
fn validate_code_answers_the_membership_the_echo_and_the_issues() {
    let world = World::load();
    let good = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &good).expect("validates");
    assert!(validation.result);
    assert_eq!(validation.display.as_deref(), Some("Kitten"));
    assert_eq!(validation.system.as_deref(), Some(ANIMALS));
    assert_eq!(validation.version.as_deref(), Some("2.0"));
    assert_eq!(validation.code.as_deref(), Some("kitten"));
    assert!(validation.issues.is_empty());
    let inferred = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &inferred).expect("validates");
    assert!(validation.result, "one system: inferred");
    let outside = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        coding: Some(CodingRef {
            system: Some(ANIMALS.to_owned()),
            code: Some(String::from("dog")),
            ..CodingRef::default()
        }),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &outside).expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.issues.len(), 1);
    assert_eq!(validation.issues[0].kind, "not-in-vs");
    assert_eq!(validation.issues[0].code, "code-invalid");
    assert_eq!(
        validation.code.as_deref(),
        Some("dog"),
        "a valid code outside the set is still echoed"
    );
    let unknown_code = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("unicorn")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &unknown_code).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "invalid-code"]);
}

#[test]
fn validate_code_names_an_unknown_system_and_an_unknown_version() {
    let world = World::load();
    let unknown_system = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(String::from("http://example.org/fhir/CodeSystem/nowhere")),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &unknown_system)
        .expect("validates");
    assert!(!validation.result);
    // NOTE: the membership issue leads, then the missing system, which the input
    // alone named (`x-unknown-system`), the ecosystem's shape (#189).
    assert_eq!(validation.issues[0].kind, "not-in-vs");
    assert_eq!(validation.issues[1].kind, "not-found");
    assert_eq!(validation.issues[1].code, "not-found");
    assert_eq!(
        validation.x_unknown_systems,
        ["http://example.org/fhir/CodeSystem/nowhere"],
        "x-unknown-system names the system"
    );
    assert_eq!(
        validation.code.as_deref(),
        Some("kitten"),
        "the code is echoed"
    );
    // NOTE: the message joins the missing system and the membership verdict, in
    // that order (the ecosystem's `bad-system` cases).
    assert_eq!(
        validation.message.as_deref(),
        Some(
            "A definition for CodeSystem 'http://example.org/fhir/CodeSystem/nowhere' could not be found, so the code cannot be validated; The provided code 'http://example.org/fhir/CodeSystem/nowhere#kitten' was not found in the value set 'http://example.org/fhir/ValueSet/pets|1.0'"
        )
    );
    let unknown_version = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        system_version: Some(String::from("9.9")),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &unknown_version)
        .expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.unknown_systems, [format!("{ANIMALS}|9.9")]);
    // NOTE: the code is still resolved against the version the value set uses, and
    // the versionless include's choice is a warning beside the not-found error
    // (the ecosystem's `version` cases, #177).
    assert_eq!(validation.version.as_deref(), Some("2.0"));
    assert_eq!(validation.display.as_deref(), Some("Kitten"));
    let shape: Vec<(&str, &str)> = validation
        .issues
        .iter()
        .map(|i| (i.severity, i.kind))
        .collect();
    assert_eq!(shape, [("error", "not-found"), ("warning", "vs-invalid")]);
    assert_eq!(validation.issues[0].expression.as_deref(), Some("system"));
    assert_eq!(validation.issues[1].expression.as_deref(), Some("version"));
    assert!(
        validation.issues[0].text.ends_with(". Valid versions: 2.0"),
        "{}",
        validation.issues[0].text
    );
    assert!(
        validation.issues[1]
            .text
            .contains("for the versionless include"),
        "{}",
        validation.issues[1].text
    );
}

// NOTE: a coding version that differs from the include's is validated against the
// include's version and itemised as vs-invalid; a check-system-version that
// forbids the include's version is a version-error, never a refusal (#177).
#[test]
fn validate_code_itemises_version_disagreements_against_the_value_set_version() {
    let world = World::load();
    let inline = |version: Option<&str>| ValueSet {
        url: Some("http://example.org/inline-pinned".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                version: version.map(Into::into),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let coding = |version: &str| CodingRef {
        system: Some(ANIMALS.to_owned()),
        version: Some(version.to_owned()),
        code: Some(String::from("cat")),
        display: None,
    };
    // An unserved coding version against a pinned include: vs-invalid, then not-found.
    let unserved = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline(Some("2.0")))),
        coding: Some(coding("9.9")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &unserved).expect("validates");
    let shape: Vec<(&str, &str)> = validation
        .issues
        .iter()
        .map(|i| (i.severity, i.kind))
        .collect();
    assert_eq!(shape, [("error", "vs-invalid"), ("error", "not-found")]);
    assert_eq!(validation.unknown_systems, [format!("{ANIMALS}|9.9")]);
    assert_eq!(validation.display.as_deref(), Some("Cat"));
    assert_eq!(
        validation.version.as_deref(),
        Some("2.0"),
        "the include's version answers"
    );
    assert_eq!(
        validation.issues[0].text,
        format!(
            "The code system '{ANIMALS}' version '2.0' in the ValueSet include is different to the one in the value ('9.9')"
        )
    );
    assert_eq!(
        validation.issues[0].expression.as_deref(),
        Some("Coding.version")
    );
    assert!(
        validation
            .message
            .as_deref()
            .unwrap()
            .starts_with("A definition for CodeSystem"),
        "the not-found text leads the message: {:?}",
        validation.message
    );
    // A check that forbids the include's version: a version-error, result false.
    let checked = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline(Some("2.0")))),
        coding: Some(coding("2.0")),
        check_system_version: vec![format!("{ANIMALS}|1.x")],
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &checked).expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.issues[0].kind, "version-error");
    assert_eq!(validation.issues[0].code, "exception");
    assert_eq!(
        validation.issues[0].text,
        format!(
            "The version '2.0' is not allowed for system '{ANIMALS}': required to be '1.x' by a version-check parameter"
        )
    );
    assert_eq!(
        validation.message.as_deref(),
        Some(validation.issues[0].text.as_str())
    );
    // On $expand the same check is refused as an exception.
    let expand = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline(Some("2.0")))),
        check_system_version: vec![format!("{ANIMALS}|1.x")],
        ..ExpandInput::default()
    };
    let error = expand::expand(&world.sources(), &expand).expect_err("refused");
    assert!(
        matches!(error, OperationError::VersionCheck(_)),
        "{error:?}"
    );
    assert_eq!(error.issue_code(), "exception");
    assert_eq!(error.tx_issue_type(), "version-error");
}

#[test]
fn validate_code_checks_display_case_and_inactive_codes() {
    let world = World::load();
    let wrong_display = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        display: Some(String::from("Hamster")),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &wrong_display)
        .expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.issues[0].kind, "invalid-display");
    assert_eq!(validation.display.as_deref(), Some("Cat"));
    let synonym = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        display: Some(String::from("domestic CAT")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &synonym).expect("validates");
    assert!(validation.result, "a designation, case folded");
    // NOTE: whitespace is not folded: a display that differs only there is wrong,
    // and says so (the ecosystem's `bad-display-ws` case).
    let spaced = ValueSetValidateInput {
        display: Some(String::from("domestic  CAT")),
        ..synonym
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &spaced).expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.issues[0].kind, "invalid-display");
    assert!(
        validation.issues[0].text.contains("the whitespace differs"),
        "{}",
        validation.issues[0].text
    );
    assert_eq!(
        validation.issues[0].message_id(),
        "Display_Name_WS_for__should_be_one_of__instead_of"
    );
    let case = ValueSetValidateInput {
        url: Some(VS_ENUMERATED.to_owned()),
        code: Some(String::from("red")),
        system: Some(COLOURS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &case).expect("validates");
    assert!(validation.result, "colours is case-insensitive");
    // The ecosystem asks for `normalized-code` on a case-insensitive system: the
    // request's spelling is echoed and the system's comes beside it.
    assert_eq!(
        validation.code.as_deref(),
        Some("red"),
        "the request's spelling is echoed"
    );
    assert_eq!(validation.normalized_code.as_deref(), Some("RED"));
    let inactive = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        value_set_version: Some(String::from("1.0")),
        code: Some(String::from("dodo")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &inactive).expect("validates");
    assert!(validation.result, "inactive is not invalid");
    assert_eq!(validation.issues[0].kind, "code-comment");
    assert_eq!(validation.issues[0].severity, "warning");
    let abstract_refused = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("pet")),
        system: Some(ANIMALS.to_owned()),
        abstract_ok: Some(false),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &abstract_refused)
        .expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.issues[0].kind, "code-rule");
}

#[test]
fn validate_code_refuses_malformed_requests() {
    let world = World::load();
    let none = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        ..ValueSetValidateInput::default()
    };
    assert!(matches!(
        value_set_validate_code::validate_code(&world.sources(), &none),
        Err(OperationError::Invalid(_))
    ));
    let two = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("cat")),
        coding: Some(CodingRef::default()),
        ..ValueSetValidateInput::default()
    };
    assert!(matches!(
        value_set_validate_code::validate_code(&world.sources(), &two),
        Err(OperationError::Invalid(_))
    ));
    let no_value_set = ValueSetValidateInput {
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    assert!(matches!(
        value_set_validate_code::validate_code(&world.sources(), &no_value_set),
        Err(OperationError::Required(_))
    ));
    let unknown = ValueSetValidateInput {
        url: Some(String::from("http://example.org/fhir/ValueSet/nowhere")),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    assert!(matches!(
        value_set_validate_code::validate_code(&world.sources(), &unknown),
        Err(OperationError::UnknownValueSet(_))
    ));
    let ambiguous = ValueSetValidateInput {
        url: Some(VS_ENUMERATED.to_owned()),
        code: Some(String::from("cat")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &ambiguous).expect("validates");
    assert!(!validation.result);
    // The ecosystem's shape: not in the value set, then the system undetermined.
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "cannot-infer"]);
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
    assert!(parameter(&by_code, "property").is_empty());
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
    assert!(none.properties.is_empty());
}

// NOTE: the value set version trio and the system trio negotiate the versions an
// operation touches, top-level and imported alike
// (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-expand.html>).
#[test]
fn expand_negotiates_value_set_versions_and_names_the_value_sets_it_used() {
    let world = World::load();
    let pinned = ExpandInput {
        url: Some(VS_PETS_REF.to_owned()),
        default_valueset_version: vec![format!("{VS_PETS}|1.0")],
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &pinned).expect("expands");
    let echoed: Vec<(&str, String)> = vs
        .parameters
        .iter()
        .filter_map(|p| match &p.value {
            ParameterValue::Uri(u) => Some((p.name.as_str(), u.clone())),
            _ => None,
        })
        .collect();
    assert!(echoed.contains(&("default-valueset-version", format!("{VS_PETS}|1.0"))));
    assert!(
        echoed.contains(&("used-valueset", format!("{VS_PETS}|1.0"))),
        "{echoed:?}"
    );
    let missing = ExpandInput {
        url: Some(VS_PETS_REF.to_owned()),
        default_valueset_version: vec![format!("{VS_PETS}|9.9")],
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&world.sources(), &missing),
        Err(OperationError::UnknownImport(url)) if url == format!("{VS_PETS}|9.9")
    ));
    let forced = ExpandInput {
        url: Some(VS_ALL.to_owned()),
        force_valueset_version: vec![format!("{VS_ALL}|1.0")],
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &forced).expect("expands");
    assert!(
        codes(&vs).iter().any(|c| c == "dodo"),
        "1.0 has no exclude, so the forced version keeps the dodo: {:?}",
        codes(&vs)
    );
    let checked = ExpandInput {
        url: Some(VS_ALL.to_owned()),
        value_set_version: Some(String::from("2.0")),
        check_valueset_version: vec![format!("{VS_ALL}|1.0")],
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&world.sources(), &checked),
        Err(OperationError::Invalid(_))
    ));
}

#[test]
fn validate_code_negotiates_system_and_value_set_versions() {
    let world = World::load();
    let defaulted = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        default_system_version: vec![format!("{ANIMALS}|2.0")],
        check_system_version: vec![format!("{ANIMALS}|2.0")],
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &defaulted).expect("validates");
    assert!(validation.result);
    assert_eq!(validation.version.as_deref(), Some("2.0"));
    let forced_away = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        force_system_version: vec![format!("{ANIMALS}|9.9")],
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &forced_away)
        .expect("a false result");
    assert!(!validation.result);
    assert_eq!(validation.unknown_systems, [format!("{ANIMALS}|9.9")]);
    let mismatch = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        coding: Some(CodingRef {
            system: Some(ANIMALS.to_owned()),
            version: Some(String::from("2.0")),
            code: Some(String::from("kitten")),
            display: None,
        }),
        check_system_version: vec![format!("{ANIMALS}|1.0")],
        ..ValueSetValidateInput::default()
    };
    // NOTE: on a versionless include the check acts as the default, so a version
    // the server lacks is the unresolvable-include shape, never a refusal (#177).
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &mismatch).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["vs-invalid", "not-found"]);
    assert_eq!(validation.unknown_systems, [format!("{ANIMALS}|1.0")]);
    let imported = ValueSetValidateInput {
        url: Some(VS_PETS_REF.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        default_valueset_version: vec![format!("{VS_PETS}|1.0")],
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &imported).expect("validates");
    assert!(validation.result);
    let unknown_import = ValueSetValidateInput {
        url: Some(VS_PETS_REF.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        default_valueset_version: vec![format!("{VS_PETS}|9.9")],
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &unknown_import)
        .expect("a false result, not an error");
    assert!(!validation.result);
    assert_eq!(validation.issues[0].kind, "not-found");
    assert_eq!(
        validation.message.as_deref(),
        Some(format!("A definition for the value Set '{VS_PETS}|9.9' could not be found").as_str())
    );
    assert_eq!(validation.code.as_deref(), Some("kitten"));
}

// NOTE: `inferSystem` finds a bare code's system by its membership in the value
// set, and `lenient-display-validation` keeps a wrong display from failing the
// result (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-validate-code.html>).
#[test]
fn validate_code_infers_the_system_by_membership_and_can_be_lenient_on_displays() {
    let world = World::load();
    let bare = |code: &str, infer: Option<bool>| ValueSetValidateInput {
        url: Some(VS_ENUMERATED.to_owned()),
        code: Some(code.to_owned()),
        infer_system: infer,
        ..ValueSetValidateInput::default()
    };
    let red = value_set_validate_code::validate_code(&world.sources(), &bare("RED", Some(true)))
        .expect("validates");
    assert!(red.result);
    assert_eq!(red.system.as_deref(), Some(COLOURS));
    let cat = value_set_validate_code::validate_code(&world.sources(), &bare("cat", Some(true)))
        .expect("validates");
    assert_eq!(cat.system.as_deref(), Some(ANIMALS));
    let dodo = value_set_validate_code::validate_code(&world.sources(), &bare("dodo", Some(true)))
        .expect("a false result");
    assert!(!dodo.result);
    let kinds: Vec<&str> = dodo.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "cannot-infer"]);
    assert_eq!(dodo.issues[1].code, "not-found");
    assert_eq!(dodo.code.as_deref(), Some("dodo"));
    assert_eq!(
        dodo.message.as_deref(),
        Some(format!("The System URI could not be determined for the code 'dodo' in the ValueSet '{VS_ENUMERATED}|1.0'; The provided code '#dodo' was not found in the value set '{VS_ENUMERATED}|1.0'").as_str())
    );
    // Without inferSystem a bare code over two systems cannot be placed.
    let ambiguous = value_set_validate_code::validate_code(&world.sources(), &bare("cat", None))
        .expect("a false result");
    assert!(!ambiguous.result);
    assert_eq!(ambiguous.issues[1].kind, "cannot-infer");
    let wrong_display = |lenient: Option<bool>| ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        display: Some(String::from("Puppy")),
        lenient_display_validation: lenient,
        ..ValueSetValidateInput::default()
    };
    let strict = value_set_validate_code::validate_code(&world.sources(), &wrong_display(None))
        .expect("validates");
    assert!(!strict.result);
    assert_eq!(strict.issues[0].severity, "error");
    let lenient =
        value_set_validate_code::validate_code(&world.sources(), &wrong_display(Some(true)))
            .expect("validates");
    assert!(lenient.result, "a warning does not fail the result");
    assert_eq!(lenient.issues[0].severity, "warning");
    assert_eq!(lenient.issues[0].kind, "invalid-display");
    assert_eq!(lenient.display.as_deref(), Some("Kitten"));
}

// NOTE: an include naming a version the server does not serve fails the
// validation as an invalid value set naming the system (the ecosystem's test
// cases, `version/coding-v10-vs1wb`); `$expand` keeps refusing it.
#[test]
fn validate_code_fails_over_an_include_the_server_cannot_resolve_and_names_the_system() {
    let world = World::load();
    let inline = ValueSet {
        url: Some("http://example.org/inline-bad-version".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                version: Some("9".into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let input = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("a false result");
    assert!(!validation.result);
    // NOTE: without a subject version only the include's unknown version is itemised
    // (the ecosystem's `coding-vnn-vs1wb`); with one, the disagreement comes first.
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-found"]);
    assert_eq!(validation.unknown_systems, [format!("{ANIMALS}|9")]);
    assert_eq!(validation.code.as_deref(), Some("kitten"));
    assert_eq!(validation.display.as_deref(), Some("Kitten"));
    assert_eq!(
        validation.version.as_deref(),
        Some("2.0"),
        "the served default answers"
    );
    let with_version = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        system_version: Some(String::from("2.0")),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &with_version)
        .expect("a false result");
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["vs-invalid", "not-found"]);
    assert_eq!(
        validation.version.as_deref(),
        Some("2.0"),
        "the subject's served version"
    );
    let expand = ExpandInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        ..ExpandInput::default()
    };
    assert!(matches!(
        expand::expand(&world.sources(), &expand),
        Err(OperationError::UnknownVersion { .. })
    ));
    // A wildcard include version names the greatest match.
    let wild = ValueSet {
        url: Some("http://example.org/inline-wild".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                version: Some("2.x".into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let input = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&wild)),
        code: Some(String::from("kitten")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(validation.result);
    assert_eq!(validation.version.as_deref(), Some("2.0"));
}

// NOTE: a CodeableConcept is judged coding by coding, the ecosystem's shape (its
// test cases): the first coding in the value set answers the code outputs, the
// input is echoed, and issues name the coding at fault by index.
#[test]
fn validate_code_judges_a_codeable_concept_coding_by_coding() {
    let world = World::load();
    let coding = |system: &str, code: &str, display: Option<&str>| CodingRef {
        system: Some(system.to_owned()),
        version: None,
        code: Some(code.to_owned()),
        display: display.map(str::to_owned),
    };
    let mixed = ValueSetValidateInput {
        url: Some(VS_ENUMERATED.to_owned()),
        codeable_concept: Some(codeable(vec![
            coding(COLOURS, "blue", None),
            coding(ANIMALS, "cat", None),
        ])),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &mixed).expect("validates");
    assert!(validation.result, "{validation:?}");
    assert_eq!(validation.code.as_deref(), Some("cat"));
    assert_eq!(validation.system.as_deref(), Some(ANIMALS));
    let echoed = validation.codeable_concept.as_ref().map(|c| c.coding.len());
    assert_eq!(echoed, Some(2), "echoed");
    let kinds: Vec<(&str, &str, Option<&str>)> = validation
        .issues
        .iter()
        .map(|i| (i.severity, i.kind, i.expression.as_deref()))
        .collect();
    assert_eq!(
        kinds,
        [(
            "information",
            "this-code-not-in-vs",
            Some("CodeableConcept.coding[0].code")
        )]
    );
    let wrong_display = ValueSetValidateInput {
        url: Some(VS_ENUMERATED.to_owned()),
        codeable_concept: Some(codeable(vec![coding(ANIMALS, "cat", Some("Hamster"))])),
        ..ValueSetValidateInput::default()
    };
    let validation = value_set_validate_code::validate_code(&world.sources(), &wrong_display)
        .expect("validates");
    assert!(!validation.result);
    assert_eq!(validation.code.as_deref(), Some("cat"));
    assert_eq!(
        validation.issues[0].expression.as_deref(),
        Some("CodeableConcept.coding[0].display")
    );
    let none = ValueSetValidateInput {
        url: Some(VS_ENUMERATED.to_owned()),
        codeable_concept: Some(codeable(vec![
            coding(ANIMALS, "dodo", None),
            coding("http://example.org/none", "x", None),
        ])),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &none).expect("a false result");
    assert!(!validation.result);
    assert_eq!(
        validation.code, None,
        "no coding in the value set answers the code outputs"
    );
    assert_eq!(validation.system, None);
    let kinds: Vec<(&str, &str, Option<&str>)> = validation
        .issues
        .iter()
        .map(|i| (i.severity, i.kind, i.expression.as_deref()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("error", "not-in-vs", None),
            (
                "error",
                "not-found",
                Some("CodeableConcept.coding[1].system")
            ),
            (
                "information",
                "this-code-not-in-vs",
                Some("CodeableConcept.coding[0].code")
            ),
            (
                "information",
                "this-code-not-in-vs",
                Some("CodeableConcept.coding[1].code")
            ),
        ]
    );
    assert_eq!(validation.x_unknown_systems, ["http://example.org/none"]);
    assert!(validation.unknown_systems.is_empty());
    // The missing system speaks first, then the concept's verdict (the
    // ecosystem's `codeableconcept-bad-version2` case).
    assert_eq!(
        validation.message.as_deref(),
        Some(format!("A definition for CodeSystem 'http://example.org/none' could not be found, so the code cannot be validated; No valid coding was found for the value set '{VS_ENUMERATED}|1.0'").as_str())
    );
}

// NOTE: the issue texts follow the ecosystem's test cases (spec-silent, #183), so
// a validator shows its user the wording the reference server would.
#[test]
fn validate_code_speaks_the_ecosystems_words_for_displays_codes_and_membership() {
    let world = World::load();
    let cat = |display: &str, language: Option<&str>| ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        display: Some(display.to_owned()),
        display_language: language.map(str::to_owned),
        ..ValueSetValidateInput::default()
    };
    let run = |input: &ValueSetValidateInput| {
        value_set_validate_code::validate_code(&world.sources(), input).expect("validates")
    };
    // No displayLanguage: every display of the concept is valid, each with its language.
    let wrong = run(&cat("Hamster", None));
    assert!(!wrong.result);
    assert_eq!(
        wrong.issues[0].text,
        format!(
            "Wrong Display Name 'Hamster' for {ANIMALS}#cat. Valid display is one of 3 choices: 'Cat' (en), 'Domestic cat' (en) or 'Katze' (de) (for the language(s) '--')"
        )
    );
    assert_eq!(
        wrong.message.as_deref(),
        Some(wrong.issues[0].text.as_str())
    );
    // A displayLanguage with displays: only those count.
    assert!(run(&cat("Katze", Some("de"))).result);
    let english_in_german = run(&cat("Cat", Some("de")));
    assert!(!english_in_german.result);
    assert_eq!(
        english_in_german.issues[0].text,
        format!(
            "Wrong Display Name 'Cat' for {ANIMALS}#cat. Valid display is 'Katze' (de) (for the language(s) 'de')"
        )
    );
    // A displayLanguage without displays: the default language's are a note, others an error.
    let default_only = run(&cat("Cat", Some("nl")));
    assert!(default_only.result);
    assert_eq!(default_only.issues[0].severity, "information");
    assert_eq!(
        default_only.issues[0].text,
        format!(
            "There are no valid display names found for the code {ANIMALS}#cat for language(s) 'nl'. The display is 'Cat' which is a valid display for the default language"
        )
    );
    let nothing = run(&cat("Hamster", Some("nl")));
    assert!(!nothing.result);
    assert_eq!(
        nothing.issues[0].text,
        format!(
            "Wrong Display Name 'Hamster' for {ANIMALS}#cat. There are no valid display names found for language(s) 'nl'. Default display is 'Cat'"
        )
    );
}

#[test]
fn validate_code_speaks_the_ecosystems_words_for_codes_membership_abstract_and_case() {
    let world = World::load();
    let run = |input: &ValueSetValidateInput| {
        value_set_validate_code::validate_code(&world.sources(), input).expect("validates")
    };
    // An unknown code, and a code outside the value set with its asserted display.
    let unknown = run(&ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("unicorn")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    });
    let kinds: Vec<(&str, String)> = unknown
        .issues
        .iter()
        .map(|i| (i.kind, i.text.clone()))
        .collect();
    assert_eq!(
        kinds,
        [
            (
                "not-in-vs",
                format!(
                    "The provided code '{ANIMALS}#unicorn' was not found in the value set '{VS_ALL}|2.0'"
                )
            ),
            (
                "invalid-code",
                format!("Unknown code 'unicorn' in the CodeSystem '{ANIMALS}' version '2.0'")
            ),
        ]
    );
    let outside = run(&ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("dog")),
        system: Some(ANIMALS.to_owned()),
        display: Some(String::from("Doggo")),
        ..ValueSetValidateInput::default()
    });
    assert_eq!(
        outside.issues[0].text,
        format!(
            "The provided code '{ANIMALS}#dog ('Doggo')' was not found in the value set '{VS_PETS}|1.0'"
        )
    );
    // An abstract code refused: the rule, then the membership, the rule as message.
    let abstract_code = run(&ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("living")),
        system: Some(ANIMALS.to_owned()),
        abstract_ok: Some(false),
        ..ValueSetValidateInput::default()
    });
    let kinds: Vec<&str> = abstract_code.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["code-rule", "not-in-vs"]);
    assert_eq!(
        abstract_code.message.as_deref(),
        Some(
            format!(
                "Code '{ANIMALS}#living' is abstract, and not allowed in this context; The provided code '{ANIMALS}#living' was not found in the value set '{VS_ALL}|2.0'"
            )
            .as_str()
        )
    );
    // A case-insensitive system located the code under another spelling: an information note.
    let cased = run(&ValueSetValidateInput {
        url: Some(VS_COLOURS.to_owned()),
        code: Some(String::from("red")),
        system: Some(COLOURS.to_owned()),
        ..ValueSetValidateInput::default()
    });
    assert!(cased.result, "{cased:?}");
    assert_eq!(cased.code.as_deref(), Some("red"));
    assert_eq!(cased.normalized_code.as_deref(), Some("RED"));
    assert_eq!(cased.issues[0].kind, "code-rule");
    assert_eq!(cased.issues[0].severity, "information");
    assert_eq!(
        cased.issues[0].text,
        format!(
            "The code 'red' differs from the correct code 'RED' by case. Although the code system '{COLOURS}' is case insensitive, implementers are strongly encouraged to use the correct case anyway"
        )
    );
}

// NOTE: a loaded supplement applies only when the request names it in
// `useSupplement` or the value set asks for it (the ecosystem's requirements,
// #184); a supplement is never a code system of its own.
#[test]
fn supplements_apply_only_when_named() {
    let world = World::load();
    let kat = |supplements: Vec<String>| ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        display: Some(String::from("Kat")),
        use_supplement: supplements,
        ..ValueSetValidateInput::default()
    };
    let dormant = value_set_validate_code::validate_code(&world.sources(), &kat(Vec::new()))
        .expect("validates");
    assert!(
        !dormant.result,
        "the Dutch designation comes from the supplement: {dormant:?}"
    );
    assert_eq!(dormant.issues[0].kind, "invalid-display");
    let applied =
        value_set_validate_code::validate_code(&world.sources(), &kat(vec![ANIMALS_NL.to_owned()]))
            .expect("validates");
    assert!(applied.result, "{applied:?}");
    let versioned = value_set_validate_code::validate_code(
        &world.sources(),
        &kat(vec![format!("{ANIMALS_NL}|1")]),
    )
    .expect("validates");
    assert!(versioned.result);
    let unknown = value_set_validate_code::validate_code(
        &world.sources(),
        &kat(vec![String::from(
            "http://example.org/fhir/CodeSystem/no-such-supplement",
        )]),
    )
    .expect_err("refused");
    assert!(
        matches!(unknown, OperationError::UnknownSupplement(_)),
        "{unknown:?}"
    );
    assert_eq!(unknown.issue_code(), "not-found");
    assert_eq!(
        unknown.to_string(),
        "Required supplement not found: http://example.org/fhir/CodeSystem/no-such-supplement"
    );
}

#[test]
fn a_value_set_asks_for_its_supplement_and_a_supplement_is_no_code_system() {
    let world = World::load();
    // A value set that asks for the supplement applies it by itself.
    let asking = ValueSet {
        url: Some("http://example.org/inline-asking".into()),
        status: "draft".into(),
        extension: vec![Extension {
            url: "http://hl7.org/fhir/StructureDefinition/valueset-supplement".into(),
            value: Some(ExtensionValue::Canonical(ANIMALS_NL.into())),
            ..Default::default()
        }],
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let by_value_set = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            inline_value_set: Some(valueset::convert::r4b::convert(&asking)),
            code: Some(String::from("cat")),
            system: Some(ANIMALS.to_owned()),
            display: Some(String::from("Kat")),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("validates");
    assert!(by_value_set.result, "{by_value_set:?}");
    // The supplement's own URL is no code system.
    let as_system = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            url: Some(VS_ALL.to_owned()),
            code: Some(String::from("cat")),
            system: Some(ANIMALS_NL.to_owned()),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("a false result");
    assert!(!as_system.result);
    assert_eq!(as_system.issues[0].kind, "invalid-data");
    assert_eq!(
        as_system.issues[0].text,
        format!(
            "CodeSystem {ANIMALS_NL}|1 is a supplement, so can't be used as a value in Coding.system"
        )
    );
    assert_eq!(as_system.unknown_systems, [ANIMALS_NL.to_owned()]);
    // $expand applies a named supplement to the designations it returns.
    let kat_in = |vs: &ExpansionOutcome| {
        vs.contains
            .iter()
            .any(|c| c.designations.iter().any(|d| d.value == "Kat"))
    };
    let expanded = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_ALL.to_owned()),
            include_designations: Some(true),
            use_supplement: vec![ANIMALS_NL.to_owned()],
            ..ExpandInput::default()
        },
    )
    .expect("expands");
    assert!(
        kat_in(&expanded),
        "the named supplement's designation is in the expansion"
    );
    let plain = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_ALL.to_owned()),
            include_designations: Some(true),
            ..ExpandInput::default()
        },
    )
    .expect("expands");
    assert!(!kat_in(&plain), "a dormant supplement adds nothing");
}

// NOTE: an inactive concept validates with `inactive`, `status`, and a code-comment
// warning; a draft, experimental, deprecated, or withdrawn resource earns a
// status-check note (the ecosystem's requirements, #185).
#[test]
fn validate_code_reports_inactive_concepts_and_resource_standing() {
    let world = World::load();
    let run = |code: &str| {
        value_set_validate_code::validate_code(
            &world.sources(),
            &ValueSetValidateInput {
                url: Some(VS_ALL.to_owned()),
                value_set_version: Some(String::from("1.0")),
                coding: Some(CodingRef {
                    system: Some(ANIMALS.to_owned()),
                    version: None,
                    code: Some(code.to_owned()),
                    display: None,
                }),
                ..ValueSetValidateInput::default()
            },
        )
        .expect("validates")
    };
    let active = run("cat");
    assert_eq!((active.inactive, active.status.as_deref()), (None, None));
    assert!(active.issues.is_empty(), "{active:?}");
    let retired = run("fish");
    assert!(retired.result, "inactive is a warning");
    assert_eq!(retired.inactive, Some(true));
    assert_eq!(retired.status.as_deref(), Some("retired"));
    assert_eq!(retired.issues.len(), 1);
    assert_eq!(
        (retired.issues[0].severity, retired.issues[0].kind),
        ("warning", "code-comment")
    );
    assert_eq!(
        retired.issues[0].text,
        "The concept 'fish' has a status of retired and inactive and its use should be reviewed"
    );
    assert_eq!(retired.issues[0].expression.as_deref(), Some("Coding"));
    assert_eq!(
        retired.message.as_deref(),
        Some(retired.issues[0].text.as_str())
    );
    let flagged = run("dodo");
    assert_eq!(flagged.inactive, Some(true));
    assert_eq!(
        flagged.status, None,
        "no status property, only the inactive flag"
    );
    assert_eq!(
        flagged.issues[0].text,
        "The concept 'dodo' has a status of inactive and its use should be reviewed"
    );
}

#[test]
fn a_draft_experimental_deprecated_or_withdrawn_resource_earns_a_status_note() {
    use fhir_terminology::operations::standing_note;
    use fhir_terminology::provider::Standing;
    let world = World::load();
    let draft = Standing {
        status: String::from("draft"),
        ..Standing::default()
    };
    let note = standing_note("CodeSystem", "http://example.org/cs|1", &draft).expect("noted");
    assert_eq!((note.severity, note.kind), ("information", "status-check"));
    assert_eq!(
        note.text,
        "Reference to draft CodeSystem http://example.org/cs|1"
    );
    let experimental = Standing {
        experimental: true,
        ..Standing::default()
    };
    assert_eq!(
        standing_note("CodeSystem", "http://example.org/cs|1", &experimental)
            .expect("noted")
            .text,
        "Reference to experimental CodeSystem http://example.org/cs|1"
    );
    let deprecated = Standing {
        standards_status: Some(String::from("deprecated")),
        ..Standing::default()
    };
    assert_eq!(
        standing_note("CodeSystem", "http://example.org/cs|1", &deprecated)
            .expect("noted")
            .text,
        "Reference to deprecated CodeSystem http://example.org/cs|1"
    );
    assert!(standing_note("CodeSystem", "x", &Standing::default()).is_none());
    // A withdrawn value set is noted on every validation against it.
    let withdrawn = ValueSet {
        url: Some("http://example.org/inline-withdrawn".into()),
        version: Some("1".into()),
        status: "active".into(),
        extension: vec![Extension {
            url: "http://hl7.org/fhir/StructureDefinition/structuredefinition-standards-status"
                .into(),
            value: Some(ExtensionValue::Code("withdrawn".into())),
            ..Default::default()
        }],
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let noted = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            inline_value_set: Some(valueset::convert::r4b::convert(&withdrawn)),
            code: Some(String::from("cat")),
            system: Some(ANIMALS.to_owned()),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("validates");
    assert!(noted.result);
    let shape: Vec<(&str, &str, &str)> = noted
        .issues
        .iter()
        .map(|i| (i.severity, i.kind, i.text.as_str()))
        .collect();
    assert_eq!(
        shape,
        [(
            "information",
            "status-check",
            "Reference to withdrawn ValueSet http://example.org/inline-withdrawn|1"
        )]
    );
    assert_eq!(noted.message, None, "notes carry no message");
}

// NOTE: every issue carries the ecosystem's message id, the message joins the
// errors in the ecosystem's order, and an unknown system fails with the
// membership issue first (the ecosystem's test cases, #189).
#[test]
fn validate_code_names_its_messages_and_shapes_the_unknown_system_answer() {
    let world = World::load();
    let unknown = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            url: Some(VS_ALL.to_owned()),
            code: Some(String::from("x")),
            system: Some(String::from("http://example.org/fhir/CodeSystem/nowhere")),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("a false result");
    let shape: Vec<(&str, &str, Option<&str>)> = unknown
        .issues
        .iter()
        .map(|i| (i.kind, i.message_id(), i.expression.as_deref()))
        .collect();
    assert_eq!(
        shape,
        [
            (
                "not-in-vs",
                "None_of_the_provided_codes_are_in_the_value_set_one",
                Some("code")
            ),
            ("not-found", "UNKNOWN_CODESYSTEM", Some("system")),
        ]
    );
    assert_eq!(
        unknown.x_unknown_systems,
        ["http://example.org/fhir/CodeSystem/nowhere"]
    );
    assert!(
        unknown.unknown_systems.is_empty(),
        "the value set never named the system"
    );
    assert!(
        unknown
            .message
            .as_deref()
            .unwrap()
            .starts_with("A definition for CodeSystem"),
        "{:?}",
        unknown.message
    );
    // A system the value set does name is caused by the value set.
    let inline = ValueSet {
        url: Some("http://example.org/inline-unknown".into()),
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
    let caused = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
            code: Some(String::from("x")),
            system: Some(String::from("http://example.org/fhir/CodeSystem/nowhere")),
            ..ValueSetValidateInput::default()
        },
    )
    .expect("a false result");
    assert_eq!(
        caused.unknown_systems,
        ["http://example.org/fhir/CodeSystem/nowhere"]
    );
    assert!(caused.x_unknown_systems.is_empty());
}

#[test]
fn the_message_joins_the_errors_in_the_ecosystems_order() {
    let world = World::load();
    let pinned = ValueSet {
        url: Some("http://example.org/inline-pinned".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                version: Some("2.0".into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let checked = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            inline_value_set: Some(valueset::convert::r4b::convert(&pinned)),
            coding: Some(CodingRef {
                system: Some(ANIMALS.to_owned()),
                version: Some(String::from("9.9")),
                code: Some(String::from("cat")),
                display: None,
            }),
            check_system_version: vec![format!("{ANIMALS}|1.x")],
            ..ValueSetValidateInput::default()
        },
    )
    .expect("a false result");
    let ids: Vec<&str> = checked.issues.iter().map(Issue::message_id).collect();
    assert_eq!(
        ids,
        [
            "VALUESET_VERSION_CHECK",
            "VALUESET_VALUE_MISMATCH",
            "UNKNOWN_CODESYSTEM_VERSION"
        ]
    );
    let message = checked.message.as_deref().unwrap();
    let order = [
        message.find("A definition for CodeSystem").unwrap(),
        message.find("The code system").unwrap(),
        message.find("The version").unwrap(),
    ];
    assert!(order[0] < order[1] && order[1] < order[2], "{message}");
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
fn validate_code_flags_a_local_system_reference() {
    let world = World::load();
    let local = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(String::from("Location1")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &local).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "invalid-data", "not-found"]);
    assert_eq!(
        validation.message.as_deref(),
        Some(
            "A definition for CodeSystem 'Location1' could not be found, so the code cannot be validated; Coding.system must be an absolute reference, not a local reference; The provided code 'Location1#kitten' was not found in the value set 'http://example.org/fhir/ValueSet/pets|1.0'"
        )
    );
    assert_eq!(validation.x_unknown_systems, ["Location1"]);
}

#[test]
fn validate_code_names_a_value_set_used_as_a_system_as_bad_data() {
    let world = World::load();
    let input = ValueSetValidateInput {
        url: Some(VS_PETS.to_owned()),
        code: Some(String::from("kitten")),
        system: Some(VS_ALL.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "invalid-data"]);
    assert_eq!(
        validation.message.as_deref(),
        Some(
            format!(
                "The Coding references a value set, not a code system ('{VS_ALL}'); The provided code '{VS_ALL}#kitten' was not found in the value set '{VS_PETS}|1.0'"
            )
            .as_str()
        )
    );
    assert!(validation.x_unknown_systems.is_empty(), "no missing system");
    assert!(validation.unknown_systems.is_empty());
}

#[test]
fn validate_code_notes_a_concept_the_value_set_deprecates_without_a_message() {
    let world = World::load();
    let inline = ValueSet {
        url: Some("http://example.org/deprecating".into()),
        version: Some("3".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                concept: vec![ValueSetComposeIncludeConcept {
                    code: "cat".into(),
                    extension: vec![Extension {
                        url: "http://hl7.org/fhir/StructureDefinition/valueset-deprecated".into(),
                        value: Some(ExtensionValue::Boolean(true.into())),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let input = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        code: Some(String::from("cat")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(validation.result);
    let note = validation
        .issues
        .iter()
        .find(|i| i.kind == "code-comment")
        .expect("the deprecation note");
    assert_eq!(note.severity, "warning");
    assert_eq!(
        note.text,
        format!(
            "The presence of the concept 'cat' in the system '{ANIMALS}' in the value set http://example.org/deprecating|3 is marked with a status of deprecated and its use should be reviewed"
        )
    );
    assert_eq!(
        validation.message, None,
        "the note stays out of the message"
    );
}

#[test]
fn validate_code_notes_a_deprecated_concept_and_a_withdrawn_display() {
    let world = World::load();
    // `plant` carries a `deprecated` standards status: a warning, a `status`
    // output, and the message, while the concept stays active.
    let deprecated = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("plant")),
        system: Some(ANIMALS.to_owned()),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &deprecated).expect("validates");
    assert!(validation.result);
    assert_eq!(validation.inactive, None);
    assert_eq!(validation.status.as_deref(), Some("deprecated"));
    assert_eq!(
        validation.message.as_deref(),
        Some("The concept 'plant' is deprecated and its use should be reviewed")
    );
    // `Hound` is a withdrawn designation of `dog`: an error, or a warning under
    // lenient display validation, never a valid display.
    let withdrawn = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("dog")),
        system: Some(ANIMALS.to_owned()),
        display: Some(String::from("Hound")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &withdrawn).expect("validates");
    assert!(!validation.result);
    let issue = &validation.issues[0];
    assert_eq!((issue.severity, issue.kind), ("error", "display-comment"));
    assert_eq!(
        issue.text,
        "'Hound' is no longer considered a correct display for code 'dog' (status = withdrawn). The correct display is 'Dog'"
    );
    assert_eq!(validation.display.as_deref(), Some("Dog"));
    let lenient = ValueSetValidateInput {
        lenient_display_validation: Some(true),
        ..withdrawn
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &lenient).expect("validates");
    assert!(validation.result);
    assert_eq!(validation.issues[0].severity, "warning");
    assert_eq!(
        validation.message.as_deref(),
        Some(
            "'Hound' is no longer considered a correct display for code 'dog' (status = withdrawn). The correct display is 'Dog'"
        )
    );
}

#[test]
fn validate_code_refuses_an_inactive_concept_under_active_only_and_a_coding_without_a_system() {
    let world = World::load();
    // Version 1.0 of the value set keeps the dodo; 2.0 excludes it.
    let inactive = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        value_set_version: Some(String::from("1.0")),
        code: Some(String::from("dodo")),
        system: Some(ANIMALS.to_owned()),
        active_only: Some(true),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &inactive).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["code-rule", "not-in-vs", "code-comment"]);
    assert_eq!(validation.inactive, Some(true));
    assert_eq!(
        validation.message.as_deref(),
        Some(
            format!(
                "The concept 'dodo' has a status of inactive and its use should be reviewed; The concept 'dodo' is valid but is not active; The provided code '{ANIMALS}#dodo' was not found in the value set '{VS_ALL}|1.0'"
            )
            .as_str()
        )
    );
    let allowed = ValueSetValidateInput {
        active_only: None,
        ..inactive
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &allowed).expect("validates");
    assert!(
        validation.result,
        "inactive codes are members unless refused"
    );
    // A coding without a system is not inferred: the membership verdict and a
    // data warning, no system echo.
    let no_system = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        coding: Some(CodingRef {
            code: Some(String::from("cat")),
            ..CodingRef::default()
        }),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &no_system).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<(&str, &str)> = validation
        .issues
        .iter()
        .map(|i| (i.severity, i.kind))
        .collect();
    assert_eq!(kinds, [("error", "not-in-vs"), ("warning", "invalid-data")]);
    assert_eq!(validation.system, None);
    assert_eq!(validation.code.as_deref(), Some("cat"));
    assert_eq!(
        validation.message.as_deref(),
        Some(
            format!(
                "Coding has no system. A code with no system has no defined meaning, and it cannot be validated. A system should be provided; The provided code '#cat' was not found in the value set '{VS_ALL}|2.0'"
            )
            .as_str()
        )
    );
}

#[test]
fn validate_code_judges_displays_in_the_value_sets_language() {
    let world = World::load();
    let german = |compose_default: bool| {
        let mut inline = ValueSet {
            url: Some("http://example.org/german".into()),
            version: Some("1".into()),
            status: "draft".into(),
            compose: Some(ValueSetCompose {
                include: vec![ValueSetComposeInclude {
                    system: Some(ANIMALS.into()),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        if compose_default {
            // `valueset-expansion-parameter` displayLanguage = de on the compose.
            inline.compose.as_mut().expect("compose").extension = vec![Extension {
                url: "http://hl7.org/fhir/StructureDefinition/valueset-expansion-parameter".into(),
                extension: vec![
                    Extension {
                        url: "name".into(),
                        value: Some(ExtensionValue::Code("displayLanguage".into())),
                        ..Default::default()
                    },
                    Extension {
                        url: "value".into(),
                        value: Some(ExtensionValue::Code("de".into())),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }];
        } else {
            inline.language = Some("de".into());
        }
        valueset::convert::r4b::convert(&inline)
    };
    for compose_default in [false, true] {
        let english = ValueSetValidateInput {
            inline_value_set: Some(german(compose_default)),
            code: Some(String::from("cat")),
            system: Some(ANIMALS.to_owned()),
            display: Some(String::from("Cat")),
            ..ValueSetValidateInput::default()
        };
        // NOTE: the value set's language is the one displays are judged in, so
        // the English display is wrong here (the ecosystem's `bad-language-vs`).
        let validation =
            value_set_validate_code::validate_code(&world.sources(), &english).expect("validates");
        assert!(
            !validation.result,
            "compose default {compose_default}: {validation:?}"
        );
        assert_eq!(validation.issues[0].kind, "invalid-display");
        assert_eq!(validation.display.as_deref(), Some("Katze"));
        let german_display = ValueSetValidateInput {
            display: Some(String::from("Katze")),
            ..english
        };
        let validation = value_set_validate_code::validate_code(&world.sources(), &german_display)
            .expect("validates");
        assert!(validation.result, "compose default {compose_default}");
        assert!(validation.issues.is_empty(), "{:?}", validation.issues);
        let requested = ValueSetValidateInput {
            display_language: Some(String::from("en")),
            display: Some(String::from("Cat")),
            ..german_display
        };
        let validation = value_set_validate_code::validate_code(&world.sources(), &requested)
            .expect("validates");
        assert!(validation.result, "the request's language wins");
        assert!(validation.issues.is_empty(), "{:?}", validation.issues);
    }
}

#[test]
fn validate_code_checks_membership_only_when_asked() {
    let world = World::load();
    let input = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("dodo")),
        system: Some(ANIMALS.to_owned()),
        value_set_version: Some(String::from("1.0")),
        display: Some(String::from("Wrong")),
        membership_only: Some(true),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(validation.result);
    assert!(
        validation.issues.is_empty(),
        "no display, case, or status notes: {:?}",
        validation.issues
    );
    assert_eq!(validation.inactive, Some(true), "the outputs still tell");
    let checked = ValueSetValidateInput {
        membership_only: None,
        ..input
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &checked).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert!(kinds.contains(&"invalid-display"), "{kinds:?}");
}

#[test]
fn validate_code_fails_a_codeable_concept_on_an_include_version_the_server_lacks() {
    let world = World::load();
    let inline = ValueSet {
        url: Some("http://example.org/pinned-badly".into()),
        version: Some("1".into()),
        status: "draft".into(),
        compose: Some(ValueSetCompose {
            include: vec![ValueSetComposeInclude {
                system: Some(ANIMALS.into()),
                version: Some("9".into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let input = ValueSetValidateInput {
        inline_value_set: Some(valueset::convert::r4b::convert(&inline)),
        codeable_concept: Some(codeable(vec![CodingRef {
            system: Some(ANIMALS.to_owned()),
            code: Some(String::from("cat")),
            ..CodingRef::default()
        }])),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-found"], "the concept fails on that alone");
    assert_eq!(validation.code, None);
    assert_eq!(validation.system, None);
    assert_eq!(validation.display.as_deref(), Some("Cat"));
    assert_eq!(validation.version.as_deref(), Some("2.0"));
    assert_eq!(validation.unknown_systems, [format!("{ANIMALS}|9")]);
    assert_eq!(validation.message, Some(validation.issues[0].text.clone()));
}

#[test]
fn code_system_validate_code_refuses_a_supplement_as_the_system() {
    let world = World::load();
    let input = fhir_terminology::operations::validate_code::ValidateCodeInput {
        url: Some(ANIMALS_NL.to_owned()),
        code: Some(String::from("cat")),
        ..fhir_terminology::operations::validate_code::ValidateCodeInput::default()
    };
    let outcome = fhir_terminology::operations::validate_code::validate_code(
        world.sources().registry,
        &fhir_terminology::operations::Invocation::Type,
        &input,
    )
    .expect("validates");
    assert!(!outcome.result);
    assert_eq!(outcome.issues[0].kind, "invalid-data");
    assert_eq!(
        outcome.issues[0].text,
        format!(
            "CodeSystem {ANIMALS_NL}|1 is a supplement, so can't be used as a value in Coding.system"
        )
    );
    assert_eq!(
        outcome.issues[0].message_id(),
        "CODESYSTEM_CS_NO_SUPPLEMENT"
    );
    assert_eq!(outcome.unknown_systems, [ANIMALS_NL]);
    assert_eq!(outcome.code.as_deref(), Some("cat"));
}

#[test]
fn expand_filters_designations_by_a_bcp47_language() {
    let world = World::load();
    let request = ExpandInput {
        url: Some(VS_ALL.to_owned()),
        include_designations: Some(true),
        designation: vec![String::from("urn:ietf:bcp:47|de")],
        ..ExpandInput::default()
    };
    let vs = expand::expand(&world.sources(), &request).expect("expands");
    let cat = entry(&vs.contains, "cat").expect("cat");
    let values: Vec<&str> = cat.designations.iter().map(|d| d.value.as_str()).collect();
    assert_eq!(values, ["Katze"]);
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

#[test]
fn an_invalid_display_language_is_refused_on_every_operation() {
    let world = World::load();
    for language in ["-", "zz", "en-AB", "de,zz"] {
        let expand_request = ExpandInput {
            url: Some(VS_PETS.to_owned()),
            display_language: Some(String::from(language)),
            ..ExpandInput::default()
        };
        let error = expand::expand(&world.sources(), &expand_request).expect_err(language);
        assert!(
            matches!(&error, OperationError::InvalidLanguage(_)),
            "{language}: {error}"
        );
        assert_eq!(error.status(), http::StatusCode::BAD_REQUEST);
        assert_eq!(error.issue_code(), "processing");
        assert_eq!(error.tx_issue_type(), "invalid-display");
        let validate_request = ValueSetValidateInput {
            url: Some(VS_PETS.to_owned()),
            code: Some(String::from("cat")),
            system: Some(ANIMALS.to_owned()),
            display_language: Some(String::from(language)),
            ..ValueSetValidateInput::default()
        };
        let error = value_set_validate_code::validate_code(&world.sources(), &validate_request)
            .expect_err(language);
        assert_eq!(
            error.to_string(),
            format!(
                "Invalid displayLanguage: '{}'",
                language.split(',').find(|l| *l != "de").unwrap_or(language)
            )
        );
    }
    // Valid ranges, quality values, and the wildcard pass.
    for language in ["en", "de-AT", "nl, en;q=0.5", "*", "zh-Hant-TW"] {
        let request = ExpandInput {
            url: Some(VS_PETS.to_owned()),
            display_language: Some(String::from(language)),
            ..ExpandInput::default()
        };
        expand::expand(&world.sources(), &request).expect(language);
    }
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

/// Every code of an expansion tree, in pre-order.
fn flatten(contains: &[Contains]) -> Vec<&str> {
    let mut out = Vec::new();
    for entry in contains {
        out.push(entry.code.as_str());
        out.extend(flatten(&entry.contains));
    }
    out
}

/// The entry for `code` anywhere in an expansion tree.
fn entry<'a>(contains: &'a [Contains], code: &str) -> Option<&'a Contains> {
    for item in contains {
        if item.code == code {
            return Some(item);
        }
        if let Some(found) = entry(&item.contains, code) {
            return Some(found);
        }
    }
    None
}

/// A designation the concept accepts is not always one to suggest.
///
/// A designation whose `use` is a project's own code, with no language, is a
/// term the concept carries for another purpose: it validates a display that
/// matches it, and the message that lists the valid displays leaves it out.
/// No FHIR version fixes this, and the terminology ecosystem IG says the
/// behaviour "is effectively specified by the test cases"
/// (<https://hl7.org/fhir/uv/tx-ecosystem/languages.html>), which is where the
/// rule comes from (#290).
#[test]
fn a_designation_with_its_own_use_validates_a_display_without_being_offered() {
    let world = World::load();
    let fish = |display: &str| ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        code: Some(String::from("fish")),
        system: Some(ANIMALS.to_owned()),
        display: Some(String::from(display)),
        ..ValueSetValidateInput::default()
    };
    let run = |input: &ValueSetValidateInput| {
        value_set_validate_code::validate_code(&world.sources(), input).expect("validates")
    };
    assert!(
        run(&fish("Old fish name")).result,
        "the designation still validates the display"
    );
    let wrong = run(&fish("Trout"));
    assert!(!wrong.result);
    let display_issue = wrong
        .issues
        .iter()
        .find(|issue| issue.text.starts_with("Wrong Display Name"))
        .expect("the display issue");
    assert_eq!(
        display_issue.text,
        format!(
            "Wrong Display Name 'Trout' for {ANIMALS}#fish. Valid display is 'Fish' (en) (for the language(s) '--')"
        ),
        "the suggestion names the display alone"
    );
}
/// A synthetic system in two versions whose `code2` display differs, the
/// shape the ecosystem's `overload` cases use.
const TWO_VERSION: &str = "http://example.org/fhir/CodeSystem/two-version";
/// A value set including both versions of [`TWO_VERSION`].
const VS_BOTH_VERSIONS: &str = "http://example.org/fhir/ValueSet/two-version-all";
/// A value set including the second version and excluding the first.
const VS_EXCLUDING_V1: &str = "http://example.org/fhir/ValueSet/two-version-exclude";

fn two_version_world() -> World {
    let system = |version: &str, second: &str, third: &str, third_display: &str| {
        serde_json::json!({
          "resourceType": "CodeSystem", "url": TWO_VERSION, "version": version,
          "name": "TwoVersion", "status": "active", "content": "complete",
          "caseSensitive": true,
          "concept": [
            {"code": "code1", "display": "Display 1"},
            {"code": "code2", "display": second},
            {"code": third, "display": third_display}
          ]
        })
    };
    let value_set = |url: &str, compose: serde_json::Value| {
        serde_json::json!({
          "resourceType": "ValueSet", "url": url, "version": "1.0",
          "name": "TwoVersionSelection", "status": "active", "compose": compose
        })
    };
    World::of(&[
        (
            "CodeSystem-two-version-1.json",
            system("1.0.0", "Display 2", "code3", "Display 3"),
        ),
        (
            "CodeSystem-two-version-2.json",
            system("2.0.0", "Display #2", "code4", "Display 4"),
        ),
        (
            "ValueSet-two-version-all.json",
            value_set(
                VS_BOTH_VERSIONS,
                serde_json::json!({"include": [
                    {"system": TWO_VERSION, "version": "1.0.0"},
                    {"system": TWO_VERSION, "version": "2.0.0"}
                ]}),
            ),
        ),
        (
            "ValueSet-two-version-exclude.json",
            value_set(
                VS_EXCLUDING_V1,
                serde_json::json!({
                    "include": [{"system": TWO_VERSION, "version": "2.0.0"}],
                    "exclude": [{"system": TWO_VERSION, "version": "1.0.0"}]
                }),
            ),
        ),
    ])
}

// NOTE: `version` is "the version of the system of the code that was validated",
// so a value set including one system twice picks the version the coding is valid
// in (<https://hl7.org/fhir/R5/valueset-operation-validate-code.html>).
#[test]
fn validate_code_picks_the_included_version_the_display_is_valid_in() {
    let world = two_version_world();
    let coding = |display: Option<&str>| CodingRef {
        system: Some(TWO_VERSION.to_owned()),
        version: None,
        code: Some(String::from("code2")),
        display: display.map(str::to_owned),
    };
    let run = |coding: CodingRef| {
        let input = ValueSetValidateInput {
            url: Some(VS_BOTH_VERSIONS.to_owned()),
            coding: Some(coding),
            ..ValueSetValidateInput::default()
        };
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates")
    };
    let first = run(coding(Some("Display 2")));
    assert!(first.result, "{first:?}");
    assert_eq!(first.version.as_deref(), Some("1.0.0"));
    assert_eq!(first.display.as_deref(), Some("Display 2"));
    let second = run(coding(Some("Display #2")));
    assert!(second.result, "{second:?}");
    assert_eq!(second.version.as_deref(), Some("2.0.0"));
    assert_eq!(second.display.as_deref(), Some("Display #2"));
    let none = run(coding(None));
    assert!(none.result, "{none:?}");
    assert_eq!(
        none.version.as_deref(),
        Some("2.0.0"),
        "with no display asserted the greatest included version answers"
    );
}

// NOTE: an exclude's version says which version its codes are selected from, so
// it removes the code whatever version an include contributed it at
// (<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.compose.exclude>).
#[test]
fn an_exclude_at_another_version_removes_the_code_the_include_contributed() {
    let world = two_version_world();
    let input = ValueSetValidateInput {
        url: Some(VS_EXCLUDING_V1.to_owned()),
        coding: Some(CodingRef {
            system: Some(TWO_VERSION.to_owned()),
            code: Some(String::from("code1")),
            ..CodingRef::default()
        }),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(!validation.result, "{validation:?}");
    assert_eq!(validation.version.as_deref(), Some("2.0.0"));
    assert_eq!(validation.display.as_deref(), Some("Display 1"));
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs"]);
    let kept = ValueSetValidateInput {
        url: Some(VS_EXCLUDING_V1.to_owned()),
        coding: Some(CodingRef {
            system: Some(TWO_VERSION.to_owned()),
            code: Some(String::from("code4")),
            ..CodingRef::default()
        }),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &kept).expect("validates");
    assert!(
        validation.result,
        "a code the excluded version does not have stays in: {validation:?}"
    );
}

// NOTE: the membership issue names the code the way the request stated it, the
// version included, which the ecosystem requires of every answer
// (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
#[test]
fn an_unknown_code_at_an_asserted_version_names_that_version() {
    let world = two_version_world();
    let input = ValueSetValidateInput {
        url: Some(VS_BOTH_VERSIONS.to_owned()),
        coding: Some(CodingRef {
            system: Some(TWO_VERSION.to_owned()),
            version: Some(String::from("1.0.0")),
            code: Some(String::from("code4")),
            display: None,
        }),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(!validation.result);
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "invalid-code"]);
    assert_eq!(
        validation.issues[0].text,
        format!(
            "The provided code '{TWO_VERSION}|1.0.0#code4' was not found in the value set '{VS_BOTH_VERSIONS}|1.0'"
        )
    );
}

/// The first half of a pair that reaches itself through an exclude.
const VS_CIRCLE_1: &str = "http://example.org/fhir/ValueSet/circle-1";
/// The second half, which excludes the first.
const VS_CIRCLE_2: &str = "http://example.org/fhir/ValueSet/circle-2";

/// A canonical no world below holds.
const VS_ABSENT: &str = "http://example.org/fhir/ValueSet/absent";

/// A value set excluding a value set this server does not hold.
const VS_BAD_EXCLUDE: &str = "http://example.org/fhir/ValueSet/bad-exclude";

/// A world whose one value set excludes a value set nothing defines.
fn unresolvable_exclude_world() -> World {
    World::of(&[
        (
            "CodeSystem-two-version-1.json",
            serde_json::json!({
              "resourceType": "CodeSystem", "url": TWO_VERSION, "version": "1.0.0",
              "name": "TwoVersion", "status": "active", "content": "complete",
              "caseSensitive": true,
              "concept": [
                {"code": "code1", "display": "Display 1"},
                {"code": "code2", "display": "Display 2"}
              ]
            }),
        ),
        (
            "ValueSet-bad-exclude.json",
            serde_json::json!({
              "resourceType": "ValueSet", "url": VS_BAD_EXCLUDE, "version": "1.0",
              "name": "BadExclude", "status": "active",
              "compose": {
                "include": [{"system": TWO_VERSION, "concept": [{"code": "code1"}]}],
                "exclude": [{"valueSet": [VS_ABSENT]}]
              }
            }),
        ),
    ])
}

/// A value set whose own include names it.
const VS_ITSELF: &str = "http://example.org/fhir/ValueSet/itself";

/// A world holding one value set that references itself directly.
fn self_referencing_world() -> World {
    World::of(&[
        (
            "CodeSystem-two-version-1.json",
            serde_json::json!({
              "resourceType": "CodeSystem", "url": TWO_VERSION, "version": "1.0.0",
              "name": "TwoVersion", "status": "active", "content": "complete",
              "caseSensitive": true,
              "concept": [{"code": "code1", "display": "Display 1"}]
            }),
        ),
        (
            "ValueSet-itself.json",
            serde_json::json!({
              "resourceType": "ValueSet", "url": VS_ITSELF, "version": "1.0",
              "name": "Itself", "status": "active",
              "compose": {"include": [
                {"system": TWO_VERSION},
                {"valueSet": [VS_ITSELF]}
              ]}
            }),
        ),
    ])
}

fn circular_world() -> World {
    let value_set = |url: &str, compose: serde_json::Value| {
        serde_json::json!({
          "resourceType": "ValueSet", "url": url, "version": "1.0",
          "name": "Circle", "status": "active", "compose": compose
        })
    };
    World::of(&[
        (
            "CodeSystem-two-version-1.json",
            serde_json::json!({
              "resourceType": "CodeSystem", "url": TWO_VERSION, "version": "1.0.0",
              "name": "TwoVersion", "status": "active", "content": "complete",
              "caseSensitive": true,
              "concept": [{"code": "code1", "display": "Display 1"}]
            }),
        ),
        (
            "ValueSet-circle-1.json",
            value_set(
                VS_CIRCLE_1,
                serde_json::json!({"include": [
                    {"system": TWO_VERSION},
                    {"valueSet": [VS_CIRCLE_2]}
                ]}),
            ),
        ),
        (
            "ValueSet-circle-2.json",
            value_set(
                VS_CIRCLE_2,
                serde_json::json!({
                    "include": [{"system": TWO_VERSION}],
                    "exclude": [{"valueSet": [VS_CIRCLE_1]}]
                }),
            ),
        ),
    ])
}

// NOTE: the ecosystem's `default-valueset-version` suite answers one
// unresolvable reference with an error from `$expand` and `result = false` from
// `$validate-code` (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
#[test]
fn an_unresolvable_exclude_is_named_whether_or_not_an_include_held_the_code() {
    let world = unresolvable_exclude_world();
    let error = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_BAD_EXCLUDE.to_owned()),
            ..ExpandInput::default()
        },
    )
    .expect_err("an expansion has to evaluate every exclude");
    assert!(
        error.to_string().contains(VS_ABSENT),
        "the refusal names what could not be resolved: `{error}`"
    );

    // `code1` is the code the include holds and `code2` is a code of the same
    // system that it does not, so the two runs differ only in whether an
    // include matched before the exclude was reached.
    let validate = |code: &str| {
        value_set_validate_code::validate_code(
            &world.sources(),
            &ValueSetValidateInput {
                url: Some(VS_BAD_EXCLUDE.to_owned()),
                coding: Some(CodingRef {
                    system: Some(TWO_VERSION.to_owned()),
                    code: Some(code.to_owned()),
                    ..CodingRef::default()
                }),
                ..ValueSetValidateInput::default()
            },
        )
        .expect("validate-code answers rather than refusing")
    };
    for code in ["code1", "code2"] {
        let validation = validate(code);
        assert!(
            !validation.result,
            "a definition this server cannot fully evaluate is not a membership: {validation:?}"
        );
        let named = validation
            .issues
            .iter()
            .any(|issue| issue.kind == "not-found" && issue.text.contains(VS_ABSENT));
        assert!(
            named,
            "`{code}` must name the value set the server could not resolve, so a client can tell \
             a missing resource from a code that is genuinely out: {:?}",
            validation.issues
        );
    }
}

// NOTE: a reference naming no version means the version the server resolves to
// (<https://hl7.org/fhir/R4B/references.html#canonical>), so a bare reference
// re-enters a value set stored with a version.
#[test]
fn a_value_set_that_references_itself_is_refused_and_named() {
    let world = self_referencing_world();
    let error = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_ITSELF.to_owned()),
            ..ExpandInput::default()
        },
    )
    .expect_err("a value set that reaches itself cannot be expanded");
    let OperationError::ValueSetCyclic(reported) = &error else {
        panic!("a cycle, not {error}");
    };
    assert!(
        reported.contains(VS_ITSELF),
        "the error names the value set that closed the loop: `{reported}`"
    );

    let error = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            url: Some(VS_ITSELF.to_owned()),
            coding: Some(CodingRef {
                system: Some(TWO_VERSION.to_owned()),
                code: Some(String::from("code1")),
                ..CodingRef::default()
            }),
            ..ValueSetValidateInput::default()
        },
    )
    .expect_err("a value set that reaches itself cannot be validated against");
    let OperationError::ValueSetCyclic(reported) = &error else {
        panic!("a cycle, not {error}");
    };
    assert!(
        reported.contains(VS_ITSELF),
        "both operations name the same value set: `{reported}`"
    );
    assert_eq!(error.tx_issue_type(), "vs-invalid");
}

// NOTE: a value set that reaches itself cannot be evaluated, so both operations
// refuse it with the ecosystem's `vs-invalid`
// (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
#[test]
fn a_cycle_through_an_exclude_refuses_both_expand_and_validate_code() {
    let world = circular_world();
    let error = expand::expand(
        &world.sources(),
        &ExpandInput {
            url: Some(VS_CIRCLE_1.to_owned()),
            ..ExpandInput::default()
        },
    )
    .expect_err("a cycle");
    let OperationError::ValueSetCyclic(reported) = &error else {
        panic!("a cycle, not {error}");
    };
    assert!(
        reported.contains(VS_CIRCLE_1),
        "the error names the value set the loop closed on, which is the one the \
         request asked about: `{reported}`"
    );
    assert_eq!(error.issue_code(), "processing");
    assert_eq!(error.tx_issue_type(), "vs-invalid");
    // The code is of a system the first include misses, so the membership walk
    // stops before the exclude that closes the cycle.
    let error = value_set_validate_code::validate_code(
        &world.sources(),
        &ValueSetValidateInput {
            url: Some(VS_CIRCLE_1.to_owned()),
            coding: Some(CodingRef {
                system: Some(String::from("http://example.org/fhir/CodeSystem/elsewhere")),
                code: Some(String::from("code9")),
                ..CodingRef::default()
            }),
            ..ValueSetValidateInput::default()
        },
    )
    .expect_err("a cycle");
    assert!(
        matches!(error, OperationError::ValueSetCyclic(_)),
        "{error}"
    );
    assert_eq!(error.issue_code(), "processing");
    assert_eq!(error.tx_issue_type(), "vs-invalid");
    assert_eq!(error.status(), http::StatusCode::UNPROCESSABLE_ENTITY);
}

// NOTE: `codeableConcept` is echoed as "a codeableConcept containing codings for
// all the validated codes", so the `text` the request sent stays on it
// (<https://hl7.org/fhir/R5/valueset-operation-validate-code.html>).
#[test]
fn the_codeable_concept_echo_keeps_the_text_the_request_sent() {
    let world = World::load();
    let input = ValueSetValidateInput {
        url: Some(VS_ENUMERATED.to_owned()),
        codeable_concept: Some(CodeableConceptRef {
            coding: vec![CodingRef {
                system: Some(ANIMALS.to_owned()),
                code: Some(String::from("cat")),
                ..CodingRef::default()
            }],
            text: Some(String::from("some plain text")),
        }),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(validation.result, "{validation:?}");
    let echoed = validation.codeable_concept.expect("the echo");
    assert_eq!(echoed.text.as_deref(), Some("some plain text"));
    assert_eq!(echoed.coding.len(), 1);
}
// NOTE: under `valueset-membership-only` the server performs no "validation tasks
// beyond validating membership", so an unknown code carries the membership issue
// alone (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-validate-code.html>).
#[test]
fn membership_only_reports_membership_without_the_codes_standing_in_its_system() {
    let world = two_version_world();
    let input = |membership_only: Option<bool>| ValueSetValidateInput {
        url: Some(VS_BOTH_VERSIONS.to_owned()),
        coding: Some(CodingRef {
            system: Some(TWO_VERSION.to_owned()),
            code: Some(String::from("nosuch")),
            ..CodingRef::default()
        }),
        membership_only,
        ..ValueSetValidateInput::default()
    };
    let full =
        value_set_validate_code::validate_code(&world.sources(), &input(None)).expect("validates");
    let kinds: Vec<&str> = full.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "invalid-code"]);
    let membership = value_set_validate_code::validate_code(&world.sources(), &input(Some(true)))
        .expect("validates");
    assert!(!membership.result);
    let kinds: Vec<&str> = membership.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs"]);
}

/// A code system with a Dutch translation, reached through a value set that
/// references another.
const LANGUAGES: &str = "http://example.org/fhir/CodeSystem/languages";
/// The value set that selects [`LANGUAGES`] directly.
const VS_INNER: &str = "http://example.org/fhir/ValueSet/inner";
/// The value set that reaches [`LANGUAGES`] through [`VS_INNER`].
const VS_OUTER: &str = "http://example.org/fhir/ValueSet/outer";

fn referencing_world() -> World {
    World::of(&[
        (
            "CodeSystem-languages.json",
            serde_json::json!({
              "resourceType": "CodeSystem", "language": "en",
              "url": LANGUAGES, "version": "1", "name": "Languages", "status": "active",
              "content": "complete", "caseSensitive": true,
              "concept": [{"code": "cat", "display": "Cat",
                "designation": [{"language": "nl", "value": "Kat"}]}]
            }),
        ),
        (
            "ValueSet-inner.json",
            serde_json::json!({
              "resourceType": "ValueSet", "url": VS_INNER, "version": "1", "name": "Inner",
              "status": "active", "compose": {"include": [{"system": LANGUAGES}]}
            }),
        ),
        (
            "ValueSet-outer.json",
            serde_json::json!({
              "resourceType": "ValueSet", "url": VS_OUTER, "version": "1", "name": "Outer",
              "status": "active", "compose": {"include": [{"valueSet": [VS_INNER]}]}
            }),
        ),
    ])
}

#[test]
fn a_code_reached_through_a_value_set_reference_carries_the_requested_display() {
    // `displayLanguage` is "the language to use when generating the display"
    // of an expansion's concepts
    // (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>), so the hop
    // through a referenced value set answers in it too.
    let world = referencing_world();
    let expansion = |url: &str, language: &str| {
        let request = ExpandInput {
            url: Some(url.to_owned()),
            display_language: Some(language.to_owned()),
            ..ExpandInput::default()
        };
        let vs = expand::expand(&world.sources(), &request).expect("expands");
        entry(&vs.contains, "cat")
            .expect("cat")
            .display
            .clone()
            .expect("a display")
    };
    assert_eq!(expansion(VS_INNER, "nl"), "Kat", "the first hop");
    assert_eq!(expansion(VS_OUTER, "nl"), "Kat", "through the reference");
    // A language the system holds nothing in answers the display it does
    // have, at both hops, rather than the requested tag over another language.
    assert_eq!(expansion(VS_INNER, "de"), "Cat");
    assert_eq!(expansion(VS_OUTER, "de"), "Cat");
    let validate = |url: &str| {
        let input = ValueSetValidateInput {
            url: Some(url.to_owned()),
            system: Some(LANGUAGES.to_owned()),
            code: Some(String::from("cat")),
            display_language: Some(String::from("nl")),
            ..ValueSetValidateInput::default()
        };
        let validation =
            value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
        assert!(validation.result, "{validation:?}");
        validation.display.expect("a display")
    };
    assert_eq!(validate(VS_INNER), "Kat");
    assert_eq!(validate(VS_OUTER), "Kat");
}

/// A code system whose resource carries a fragment of the system.
const FRAGMENT: &str = "http://example.org/fhir/CodeSystem/fragment";
/// The value set over the whole of [`FRAGMENT`].
const VS_FRAGMENT: &str = "http://example.org/fhir/ValueSet/fragment";
/// The value set enumerating two codes of [`FRAGMENT`].
const VS_FRAGMENT_ENUM: &str = "http://example.org/fhir/ValueSet/fragment-enum";

fn fragment_world() -> World {
    World::of(&[
        (
            "CodeSystem-fragment.json",
            serde_json::json!({
              "resourceType": "CodeSystem", "language": "en",
              "url": FRAGMENT, "version": "0.1.0", "name": "Fragment", "status": "active",
              "content": "fragment", "caseSensitive": true,
              "concept": [
                {"code": "code1", "display": "Display 1"},
                {"code": "code2", "display": "Display 2"}
              ]
            }),
        ),
        (
            "ValueSet-fragment.json",
            serde_json::json!({
              "resourceType": "ValueSet", "url": VS_FRAGMENT, "version": "1", "name": "Frag",
              "status": "active", "compose": {"include": [{"system": FRAGMENT}]}
            }),
        ),
        (
            "ValueSet-fragment-enum.json",
            serde_json::json!({
              "resourceType": "ValueSet", "url": VS_FRAGMENT_ENUM, "version": "1",
              "name": "FragEnum", "status": "active",
              "compose": {"include": [{"system": FRAGMENT,
                "concept": [{"code": "code1"}, {"code": "code2"}]}]}
            }),
        ),
    ])
}

#[test]
fn a_code_a_fragment_resource_does_not_carry_is_not_a_code_the_system_lacks() {
    // A `fragment` resource holds "a subset of the code system concepts"
    // (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>), and the
    // ecosystem asks a server to "reflect `content = fragment` in an error
    // message if the code is not valid against a fragment"
    // (<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).
    let world = fragment_world();
    let input = ValueSetValidateInput {
        url: Some(VS_FRAGMENT.to_owned()),
        system: Some(FRAGMENT.to_owned()),
        code: Some(String::from("code1x")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(validation.result, "{validation:?}");
    assert_eq!(validation.code.as_deref(), Some("code1x"));
    assert_eq!(validation.system.as_deref(), Some(FRAGMENT));
    assert_eq!(validation.version.as_deref(), Some("0.1.0"));
    assert_eq!(validation.display, None, "the system has no display for it");
    assert_eq!(validation.message, None, "a result of true says nothing");
    assert_eq!(validation.issues.len(), 1, "{:?}", validation.issues);
    let issue = &validation.issues[0];
    assert_eq!(
        (issue.severity, issue.code, issue.kind),
        ("warning", "code-invalid", "invalid-code")
    );
    assert_eq!(
        issue.text,
        format!(
            "Unknown Code 'code1x' in the CodeSystem '{FRAGMENT}' version '0.1.0' - note that \
the code system is labeled as a fragment, so the code may be valid in some other fragment"
        )
    );
    assert_eq!(issue.expression.as_deref(), Some("code"));
}

#[test]
fn a_codeable_concept_over_a_fragment_holds_on_a_code_the_resource_lacks() {
    // A `CodeableConcept` is judged coding by coding, and a coding the fragment
    // leaves open is not one the value set can be said to refuse, so the
    // warning travels with a `result` of true.
    let world = fragment_world();
    let input = ValueSetValidateInput {
        url: Some(VS_FRAGMENT.to_owned()),
        codeable_concept: Some(codeable(vec![CodingRef {
            system: Some(FRAGMENT.to_owned()),
            code: Some(String::from("code1x")),
            ..CodingRef::default()
        }])),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(validation.result, "{validation:?}");
    assert_eq!(validation.issues.len(), 1, "{:?}", validation.issues);
    assert_eq!(validation.issues[0].severity, "warning");
    assert_eq!(
        validation.issues[0].expression.as_deref(),
        Some("CodeableConcept.coding[0].code")
    );
    assert!(
        validation.codeable_concept.is_some(),
        "the concept is echoed"
    );
}

#[test]
fn an_include_that_enumerates_a_fragments_codes_still_decides_membership() {
    // The include lists the codes it selects, so one it does not list is
    // outside the value set whatever else the system defines.
    let world = fragment_world();
    let input = ValueSetValidateInput {
        url: Some(VS_FRAGMENT_ENUM.to_owned()),
        system: Some(FRAGMENT.to_owned()),
        code: Some(String::from("code1x")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(!validation.result, "{validation:?}");
    let kinds: Vec<&str> = validation.issues.iter().map(|i| i.kind).collect();
    assert_eq!(kinds, ["not-in-vs", "invalid-code"]);
}

#[test]
fn a_complete_resource_keeps_answering_an_absent_code_as_one_the_system_lacks() {
    // `complete` means "all the concepts defined by the code system are
    // included in the code system resource"
    // (<https://hl7.org/fhir/R4B/codesystem-content-mode.html>), so absence
    // from the resource is absence from the system.
    let world = World::load();
    let input = ValueSetValidateInput {
        url: Some(VS_ALL.to_owned()),
        system: Some(ANIMALS.to_owned()),
        code: Some(String::from("zebra")),
        ..ValueSetValidateInput::default()
    };
    let validation =
        value_set_validate_code::validate_code(&world.sources(), &input).expect("validates");
    assert!(!validation.result, "{validation:?}");
    let severities: Vec<&str> = validation.issues.iter().map(|i| i.severity).collect();
    assert_eq!(severities, ["error", "error"]);
}
