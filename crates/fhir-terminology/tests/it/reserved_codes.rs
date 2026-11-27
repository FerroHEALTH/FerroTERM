// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reserved code spaces, extension-codes supplements and global language
//! packs, as the terminology ecosystem requires them at SHALL
//! (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>, `CodeSystem`
//! supplements; `HL7/fhir-tx-ecosystem-ig` at `9c2bbce`,
//! `input/pagecontent/requirements.md`).

use std::sync::Arc;

use fhir_terminology::fhir_codesystem::load::{FhirVersion, load_file};
use fhir_terminology::fhir_codesystem::provider::FhirCodeSystem;
use fhir_terminology::operations::expand::ExpandInput;
use fhir_terminology::operations::lookup::LookupInput;
use fhir_terminology::operations::sources::Sources;
use fhir_terminology::operations::validate_code::{ValidateCodeInput, ValidationOutcome};
use fhir_terminology::operations::{Invocation, OperationError, expand, lookup, validate_code};
use fhir_terminology::registry::Registry;
use fhir_terminology::supplement::Supplement;
use fhir_terminology::valueset::expansion::{ExpansionOutcome, ParameterValue};
use fhir_types::r4b::value_set::{
    ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeConcept,
    ValueSetComposeIncludeFilter,
};
use serde_json::{Value, json};

const BASE: &str = "http://example.org/fhir/CodeSystem/workshop";
const GLOBAL: &str = "http://example.org/fhir/CodeSystem/workshop-global";
const LOCAL: &str = "http://example.org/fhir/CodeSystem/workshop-local";
const PACK: &str = "http://example.org/fhir/CodeSystem/workshop-fr";
const PACK_LOCAL: &str = "http://example.org/fhir/CodeSystem/workshop-it";
const OTHER: &str = "http://example.org/fhir/CodeSystem/garage";
const SUPPLEMENT_TYPE: &str = "http://hl7.org/fhir/StructureDefinition/codesystem-supplement-type";
const SUPPLEMENT_GLOBAL: &str =
    "http://hl7.org/fhir/StructureDefinition/codesystem-supplement-global";

/// A synthetic system with two tools of its own and two reserved spaces:
/// `U…` with no default display, `L…` with the default display `Local tool`.
fn base() -> Value {
    json!({
        "resourceType": "CodeSystem",
        "url": BASE,
        "version": "2.0.0",
        "name": "Workshop",
        "status": "active",
        "language": "en",
        "caseSensitive": true,
        "content": "complete",
        "extension": [
            {"url": "http://hl7.org/fhir/StructureDefinition/codesystem-reservedCodeSpace",
             "extension": [{"url": "reservedCodeSpace", "valueString": "U[0-9]+"}]},
            {"url": "http://hl7.org/fhir/StructureDefinition/codesystem-reservedCodeSpace",
             "extension": [{"url": "reservedCodeSpace", "valueString": "L[0-9]+"},
                           {"url": "default-display", "valueString": "Local tool"}]}
        ],
        "concept": [
            {"code": "hammer", "display": "Hammer"},
            {"code": "saw", "display": "Saw"}
        ]
    })
}

/// A supplement of `BASE` of `kind`, global or not, defining `concepts`.
fn supplement(url: &str, kind: &str, global: bool, language: &str, concepts: &Value) -> Value {
    let mut extension = vec![json!({"url": SUPPLEMENT_TYPE, "valueCode": kind})];
    if global {
        extension.push(json!({"url": SUPPLEMENT_GLOBAL, "valueBoolean": true}));
    }
    json!({
        "resourceType": "CodeSystem",
        "url": url,
        "version": "1.0.0",
        "status": "active",
        "language": language,
        "content": "supplement",
        "supplements": format!("{BASE}|2.0.0"),
        "extension": extension,
        "concept": concepts
    })
}

fn model(
    dir: &tempfile::TempDir,
    name: &str,
    resource: &Value,
) -> fhir_terminology::provider::code_system::CodeSystemModel {
    let path = dir.path().join(format!("CodeSystem-{name}.json"));
    std::fs::write(&path, resource.to_string()).expect("writes");
    load_file(&path, FhirVersion::R5).expect("loads the resource")
}

/// The workshop, a second system with a `hammer` of its own, and the four
/// supplements: a global and a referenced extension-codes supplement, a global
/// French language pack and an Italian one that is not global.
fn registry() -> Registry {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut registry = Registry::default();
    registry
        .register(Arc::new(
            FhirCodeSystem::new(model(&dir, "base", &base())).expect("builds"),
        ))
        .expect("registers");
    let other = json!({
        "resourceType": "CodeSystem", "url": OTHER, "version": "1.0.0", "status": "active",
        "language": "en", "content": "complete",
        "concept": [{"code": "hammer", "display": "Garage hammer"}]
    });
    registry
        .register(Arc::new(
            FhirCodeSystem::new(model(&dir, "other", &other)).expect("builds"),
        ))
        .expect("registers");
    let supplements = [
        supplement(
            GLOBAL,
            "extension-codes",
            true,
            "en",
            &json!([
                {"code": "U1", "display": "Torque wrench"},
                {"code": "saw", "display": "Hand saw"}
            ]),
        ),
        supplement(
            LOCAL,
            "extension-codes",
            false,
            "en",
            &json!([
                {"code": "U2", "display": "Spirit level"}
            ]),
        ),
        supplement(
            PACK,
            "lang-pack",
            true,
            "fr",
            &json!([
                {"code": "hammer", "designation": [{"language": "fr", "value": "Marteau"}]}
            ]),
        ),
        supplement(
            PACK_LOCAL,
            "lang-pack",
            false,
            "it",
            &json!([
                {"code": "hammer", "designation": [{"language": "it", "value": "Martello"}]}
            ]),
        ),
    ];
    for (i, resource) in supplements.iter().enumerate() {
        let model = model(&dir, &format!("supplement-{i}"), resource);
        let target = model.supplements.clone().expect("a target");
        registry.register_supplement(target, Supplement::from_code_system(&model));
    }
    registry
}

fn validated(registry: &Registry, code: &str, display: Option<&str>) -> ValidationOutcome {
    validated_with(registry, code, display, |input| input)
}

fn validated_with(
    registry: &Registry,
    code: &str,
    display: Option<&str>,
    adjust: impl FnOnce(ValidateCodeInput) -> ValidateCodeInput,
) -> ValidationOutcome {
    let input = adjust(ValidateCodeInput {
        url: Some(BASE.to_owned()),
        code: Some(code.to_owned()),
        display: display.map(str::to_owned),
        ..ValidateCodeInput::default()
    });
    validate_code::validate_code(registry, &Invocation::Type, &input).expect("validates")
}

fn looked_up(
    registry: &Registry,
    system: &str,
    code: &str,
    language: Option<&str>,
    use_supplement: &[&str],
) -> Result<lookup::LookupOutcome, OperationError> {
    lookup::lookup(
        registry,
        &Invocation::Type,
        &LookupInput {
            system: Some(system.to_owned()),
            code: Some(code.to_owned()),
            display_language: language.map(str::to_owned),
            use_supplement: use_supplement.iter().map(|s| (*s).to_owned()).collect(),
            ..LookupInput::default()
        },
    )
}

fn expanded(
    registry: &Registry,
    include: ValueSetComposeInclude,
    use_supplement: &[&str],
) -> ExpansionOutcome {
    let value_sets = fhir_terminology::valueset::store::ValueSetStore::default();
    let concept_maps = fhir_terminology::conceptmap::store::ConceptMapStore::default();
    let sources = Sources {
        registry,
        value_sets: &value_sets,
        concept_maps: &concept_maps,
    };
    let inline = ValueSet {
        url: Some("http://example.org/fhir/ValueSet/workshop-tools".into()),
        status: "active".into(),
        compose: Some(ValueSetCompose {
            include: vec![include],
            ..Default::default()
        }),
        ..Default::default()
    };
    let request = ExpandInput {
        inline_value_set: Some(fhir_terminology::valueset::convert::r4b::convert(&inline)),
        use_supplement: use_supplement.iter().map(|s| (*s).to_owned()).collect(),
        ..Default::default()
    };
    expand::expand(&sources, &request).expect("expands")
}

fn listed(codes: &[&str]) -> ValueSetComposeInclude {
    ValueSetComposeInclude {
        system: Some(BASE.into()),
        concept: codes
            .iter()
            .map(|code| ValueSetComposeIncludeConcept {
                code: (*code).into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn displays(outcome: &ExpansionOutcome) -> Vec<(String, Option<String>)> {
    outcome
        .contains
        .iter()
        .map(|c| (c.code.clone(), c.display.clone()))
        .collect()
}

fn used_supplements(outcome: &ExpansionOutcome) -> Vec<String> {
    outcome
        .parameters
        .iter()
        .filter(|p| p.name == "used-supplement")
        .filter_map(|p| match &p.value {
            ParameterValue::Uri(uri) => Some(uri.clone()),
            _ => None,
        })
        .collect()
}

/// "A code in a reserved code space is a valid code in the code system,
/// whether or not anything in scope defines it. When nothing does, its display
/// is the code space's default display ... (`User-assigned` if it doesn't give
/// one)".
#[test]
fn a_reserved_code_nothing_defines_is_valid_with_the_default_display() {
    let registry = registry();
    let undefined = validated(&registry, "U77", None);
    assert!(undefined.result, "{undefined:?}");
    assert_eq!(undefined.display.as_deref(), Some("User-assigned"));
    let local = validated(&registry, "L5", None);
    assert!(local.result, "{local:?}");
    assert_eq!(local.display.as_deref(), Some("Local tool"));
    let outcome = looked_up(&registry, BASE, "U77", None, &[]).expect("looks up");
    assert_eq!(outcome.display, "User-assigned");
    let outside = validated(&registry, "X1", None);
    assert!(
        !outside.result,
        "a code in no reserved space is unknown: {outside:?}"
    );
    assert!(
        validated(&registry, "U1x", None).result.eq(&false),
        "the pattern matches the whole code"
    );
}

/// The default display "is a placeholder: another display is not an error or a
/// warning", and a supplement that is not global is "only used when it is
/// referenced".
#[test]
fn the_default_display_is_a_placeholder_and_an_unreferenced_supplement_is_unused() {
    let registry = registry();
    let outcome = validated(&registry, "U2", Some("Anything at all"));
    assert!(outcome.result, "{outcome:?}");
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    assert_eq!(outcome.display.as_deref(), Some("User-assigned"));
}

/// A global extension-codes supplement "is used for the code system it
/// supplements whether or not it is referenced, as if the code system defined
/// its codes: its display is the code's display, and a different display is an
/// error (a warning with `lenient-display-validation`)".
#[test]
fn a_global_extension_codes_supplement_defines_its_codes_unreferenced() {
    let registry = registry();
    let right = validated(&registry, "U1", Some("Torque wrench"));
    assert!(right.result, "{right:?}");
    assert_eq!(right.display.as_deref(), Some("Torque wrench"));
    let wrong = validated(&registry, "U1", Some("Spanner"));
    assert!(!wrong.result, "{wrong:?}");
    assert_eq!(wrong.issues.first().map(|i| i.severity), Some("error"));
    let lenient = validated_with(&registry, "U1", Some("Spanner"), |input| {
        ValidateCodeInput {
            lenient_display: true,
            ..input
        }
    });
    assert!(lenient.result, "{lenient:?}");
    assert_eq!(lenient.issues.first().map(|i| i.severity), Some("warning"));
    let outcome = looked_up(&registry, BASE, "U1", None, &[]).expect("looks up");
    assert_eq!(outcome.display, "Torque wrench");
    assert!(
        outcome
            .used_supplements
            .contains(&format!("{GLOBAL}|1.0.0")),
        "{:?}",
        outcome.used_supplements
    );
}

/// "An `extension-codes` supplement that is not global is only used when it is
/// referenced. Its display is then the code's display, but not the only
/// correct one: a different display is a warning".
#[test]
fn a_referenced_extension_codes_supplement_suggests_its_display() {
    let loaded = registry();
    let registry = loaded
        .with_supplements(&[LOCAL.to_owned()])
        .expect("layers");
    let right = validated(&registry, "U2", None);
    assert!(right.result, "{right:?}");
    assert_eq!(right.display.as_deref(), Some("Spirit level"));
    let other = validated(&registry, "U2", Some("Bubble level"));
    assert!(other.result, "{other:?}");
    assert_eq!(other.issues.first().map(|i| i.severity), Some("warning"));
    let outcome = looked_up(&loaded, BASE, "U2", None, &[LOCAL]).expect("looks up");
    assert_eq!(outcome.display, "Spirit level");
}

/// An extension-codes supplement's display for a code the system defines "acts
/// as a language pack does for that code": a valid display beside the
/// system's own, which stays the one returned.
#[test]
fn an_extension_code_display_for_a_defined_code_is_one_more_valid_display() {
    let registry = registry();
    let outcome = validated(&registry, "saw", Some("Hand saw"));
    assert!(outcome.result, "{outcome:?}");
    assert_eq!(outcome.display.as_deref(), Some("Saw"));
}

/// "The codes an `extension-codes` supplement defines are only in an expansion
/// when they are listed in the value set", and "an expansion of a value set
/// that includes all of a code system with reserved code spaces ... SHALL be
/// marked as unclosed".
#[test]
fn an_expansion_lists_reserved_codes_only_when_the_value_set_does() {
    let registry = registry();
    let all = expanded(
        &registry,
        ValueSetComposeInclude {
            system: Some(BASE.into()),
            ..Default::default()
        },
        &[],
    );
    assert!(all.unclosed, "an include of all of it is unclosed");
    assert_eq!(
        displays(&all),
        [
            (String::from("hammer"), Some(String::from("Hammer"))),
            (String::from("saw"), Some(String::from("Saw"))),
        ]
    );
    let enumerated = expanded(&registry, listed(&["U1", "U2", "U9", "L3"]), &[]);
    assert!(
        !enumerated.unclosed,
        "an include that lists its codes is closed"
    );
    assert_eq!(
        displays(&enumerated),
        [
            (String::from("U1"), Some(String::from("Torque wrench"))),
            (String::from("U2"), Some(String::from("User-assigned"))),
            (String::from("U9"), Some(String::from("User-assigned"))),
            (String::from("L3"), Some(String::from("Local tool"))),
        ]
    );
    let referenced = expanded(&registry, listed(&["U2"]), &[LOCAL]);
    assert_eq!(
        displays(&referenced),
        [(String::from("U2"), Some(String::from("Spirit level")))]
    );
    assert_eq!(
        used_supplements(&referenced),
        [
            format!("{LOCAL}|1.0.0"),
            format!("{PACK}|1.0.0"),
            format!("{GLOBAL}|1.0.0")
        ]
    );
}

/// "A filter never selects a code in a reserved code space - not even a
/// `concept in` or `code in` filter that lists it ... and the filter does not
/// make the expansion unclosed".
#[test]
fn a_filter_never_selects_a_reserved_code() {
    let registry = registry();
    let filtered = expanded(
        &registry,
        ValueSetComposeInclude {
            system: Some(BASE.into()),
            filter: vec![ValueSetComposeIncludeFilter {
                property: "concept".into(),
                op: "in".into(),
                value: "hammer,U1,U9".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
        &[],
    );
    assert!(!filtered.unclosed);
    assert_eq!(
        displays(&filtered),
        [(String::from("hammer"), Some(String::from("Hammer")))]
    );
}

/// "A server SHALL use a global language pack ... whether or not the request
/// or the value set refers to it", "It only applies to the code system (and
/// versions) it supplements", and "Any other supplement SHALL only be used when
/// it is referenced".
#[test]
fn a_global_language_pack_applies_to_its_own_system_unreferenced() {
    let registry = registry();
    let french = looked_up(&registry, BASE, "hammer", Some("fr"), &[]).expect("looks up");
    assert_eq!(french.display, "Marteau");
    assert!(
        french.used_supplements.contains(&format!("{PACK}|1.0.0")),
        "{:?}",
        french.used_supplements
    );
    let italian = looked_up(&registry, BASE, "hammer", Some("it"), &[]).expect("looks up");
    assert_eq!(italian.display, "Hammer", "the Italian pack is not global");
    assert!(
        !italian
            .used_supplements
            .iter()
            .any(|s| s.starts_with(PACK_LOCAL))
    );
    let referenced =
        looked_up(&registry, BASE, "hammer", Some("it"), &[PACK_LOCAL]).expect("looks up");
    assert_eq!(referenced.display, "Martello");
    let other = looked_up(&registry, OTHER, "hammer", Some("fr"), &[]).expect("looks up");
    assert_eq!(other.display, "Garage hammer");
    assert!(
        other.used_supplements.is_empty(),
        "{:?}",
        other.used_supplements
    );
    let expansion = expanded(
        &registry,
        ValueSetComposeInclude {
            system: Some(BASE.into()),
            ..Default::default()
        },
        &[],
    );
    assert!(used_supplements(&expansion).contains(&format!("{PACK}|1.0.0")));
}

/// A global supplement also named in `useSupplement` applies once.
#[test]
fn a_referenced_global_supplement_applies_once() {
    let registry = registry();
    let outcome = looked_up(&registry, BASE, "U1", None, &[GLOBAL]).expect("looks up");
    let torque = outcome
        .designations
        .iter()
        .filter(|d| d.value == "Torque wrench")
        .count();
    assert_eq!(torque, 1, "{:?}", outcome.designations);
    assert_eq!(
        outcome
            .used_supplements
            .iter()
            .filter(|s| s.starts_with(GLOBAL))
            .count(),
        1
    );
}

/// A system registered after its global supplement still receives it.
#[test]
fn a_system_registered_after_its_global_supplement_receives_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut registry = Registry::default();
    let pack = model(
        &dir,
        "pack",
        &supplement(
            PACK,
            "lang-pack",
            true,
            "fr",
            &json!([
                {"code": "saw", "designation": [{"language": "fr", "value": "Scie"}]}
            ]),
        ),
    );
    registry.register_supplement(format!("{BASE}|2.0.0"), Supplement::from_code_system(&pack));
    registry.register_or_replace(Arc::new(
        FhirCodeSystem::new(model(&dir, "base", &base())).expect("builds"),
    ));
    let outcome = looked_up(&registry, BASE, "saw", Some("fr"), &[]).expect("looks up");
    assert_eq!(outcome.display, "Scie");
}
