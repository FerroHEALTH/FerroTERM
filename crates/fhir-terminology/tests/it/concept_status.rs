// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The standard concept status properties on a persisted `CodeSystem`
//! (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>).

use fhir_terminology::operations::expand::{Contains, ExpandInput};
use fhir_terminology::operations::value_set_validate_code::ValueSetValidateInput;
use fhir_terminology::operations::{Invocation, expand, lookup, validate_code};
use fhir_terminology::provider::{Concept, PropertyValue};
use fhir_types::codec::Json;

use crate::value_set::World;

const LIFECYCLE: &str = "http://example.org/fhir/CodeSystem/lifecycle";
const LIFECYCLE_SET: &str = "http://example.org/fhir/ValueSet/lifecycle";

/// A date every request is behind, and one no request reaches.
const PAST: &str = "2001-06-15";
const FUTURE: &str = "2999-01-01";

/// A system whose concepts carry one status marker each, plus the two cases
/// where markers meet.
fn lifecycle() -> serde_json::Value {
    let property = |code: &str, kind: &str| {
        serde_json::json!({
            "code": code,
            "uri": format!("http://hl7.org/fhir/concept-properties#{code}"),
            "type": kind
        })
    };
    serde_json::json!({
      "resourceType": "CodeSystem",
      "url": LIFECYCLE, "version": "1.0", "name": "Lifecycle",
      "status": "active", "content": "complete", "caseSensitive": true,
      "property": [
        property("status", "code"),
        property("inactive", "boolean"),
        property("deprecated", "dateTime"),
        property("deprecationDate", "dateTime"),
        property("retirementDate", "dateTime")
      ],
      "concept": [
        {"code": "active", "display": "Active"},
        {"code": "experimental", "display": "Experimental",
         "property": [{"code": "status", "valueCode": "experimental"}]},
        {"code": "withdrawn-status", "display": "Withdrawn by status",
         "property": [{"code": "status", "valueCode": "withdrawn"}]},
        {"code": "deprecated-status", "display": "Deprecated by status",
         "property": [{"code": "status", "valueCode": "deprecated"}]},
        {"code": "deprecated-date", "display": "Deprecated by date",
         "property": [{"code": "deprecated", "valueDateTime": PAST}]},
        {"code": "deprecation-date", "display": "Deprecation date",
         "property": [{"code": "deprecationDate", "valueDateTime": PAST}]},
        {"code": "flagged", "display": "Flagged inactive",
         "property": [{"code": "inactive", "valueBoolean": true}]},
        {"code": "retired-status", "display": "Retired by status",
         "property": [{"code": "status", "valueCode": "retired"}]},
        {"code": "retired-date", "display": "Retired by date",
         "property": [{"code": "retirementDate", "valueDateTime": PAST}]},
        {"code": "retiring", "display": "Retiring later",
         "property": [{"code": "retirementDate", "valueDateTime": FUTURE}]},
        {"code": "flagged-and-retired", "display": "Flagged and retired",
         "property": [{"code": "inactive", "valueBoolean": true},
                      {"code": "status", "valueCode": "retired"}]},
        {"code": "contradiction", "display": "Flagged active and retired",
         "property": [{"code": "inactive", "valueBoolean": false},
                      {"code": "status", "valueCode": "retired"}]}
      ]
    })
}

fn lifecycle_set() -> serde_json::Value {
    serde_json::json!({
      "resourceType": "ValueSet", "url": LIFECYCLE_SET, "version": "1.0",
      "name": "Lifecycle", "status": "active",
      "compose": {"include": [{"system": LIFECYCLE}]}
    })
}

fn world() -> World {
    World::of(&[
        ("CodeSystem-lifecycle.json", lifecycle()),
        ("ValueSet-lifecycle.json", lifecycle_set()),
    ])
}

/// The codes of an expansion, in the order the page carries them.
fn codes(contains: &[Contains]) -> Vec<&str> {
    contains.iter().map(|item| item.code.as_str()).collect()
}

/// The `$expand` answer for the whole system, with `activeOnly` as given.
fn expanded(world: &World, active_only: bool) -> Vec<Contains> {
    let request = ExpandInput {
        url: Some(LIFECYCLE_SET.to_owned()),
        active_only: Some(active_only),
        exclude_nested: Some(true),
        ..ExpandInput::default()
    };
    expand::expand(&world.sources(), &request)
        .expect("expands")
        .contains
}

/// Whether the system reads `code` as inactive, and the reason it states.
fn standing(world: &World, code: &str) -> (bool, Option<String>) {
    let provider = world
        .registry()
        .resolve(LIFECYCLE, None)
        .expect("resolves")
        .provider;
    let located = provider
        .locate(code)
        .expect("reads")
        .unwrap_or_else(|| panic!("{code} is defined"));
    let status = provider.status(located.concept).expect("reads");
    (!status.active, status.inactive_reason)
}

// The `inactive` property "is not considered active - e.g. not a valid concept
// any more", and the note on it says "the status property may also be used to
// indicate that a concept is inactive", with `retired` the value that ends a
// concept's life
// (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>).
#[test]
fn each_inactivity_marker_retires_its_concept_and_names_its_reason() {
    let world = world();
    assert_eq!(
        standing(&world, "flagged"),
        (true, Some(String::from("inactive")))
    );
    assert_eq!(
        standing(&world, "retired-status"),
        (true, Some(String::from("retired")))
    );
    // The ecosystem's `status` output is the concept's status "when its code
    // system states one" (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>),
    // so a date alone names no status to report.
    assert_eq!(
        standing(&world, "retired-date"),
        (true, Some(String::from("inactive"))),
        "a retirementDate the request is behind retires the concept"
    );
    assert_eq!(
        standing(&world, "retiring"),
        (false, None),
        "a retirementDate still to come retires nothing"
    );
}

// "Concepts that are deprecated but not inactive can still be used, but their
// use is discouraged", so no deprecation marker makes a concept inactive; R4
// and R4B define `deprecated` and R5 adds `deprecationDate`
// (<https://hl7.org/fhir/R4/codesystem-concept-properties.html>,
// <https://hl7.org/fhir/R5/codesystem-concept-properties.html>).
#[test]
fn a_deprecated_concept_stays_active() {
    let world = world();
    assert_eq!(standing(&world, "deprecated-status"), (false, None));
    assert_eq!(standing(&world, "deprecated-date"), (false, None));
    assert_eq!(standing(&world, "deprecation-date"), (false, None));
    assert_eq!(standing(&world, "experimental"), (false, None));
    assert_eq!(standing(&world, "active"), (false, None));
}

// `status` carries "typical values active, experimental, deprecated, and
// retired" and binds to no value set, so no FHIR specification says what
// another value means; only `retired` ends a concept's life here
// (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>).
#[test]
fn a_status_the_specification_does_not_list_leaves_the_concept_active() {
    assert_eq!(standing(&world(), "withdrawn-status"), (false, None));
}

// No FHIR specification says which marker wins when two disagree; the strict
// reading is that any marker saying inactive wins, and a stated `retired`
// names the reason ahead of the bare flag.
#[test]
fn a_marker_that_says_inactive_outranks_one_that_says_active() {
    let world = world();
    assert_eq!(
        standing(&world, "flagged-and-retired"),
        (true, Some(String::from("retired")))
    );
    assert_eq!(
        standing(&world, "contradiction"),
        (true, Some(String::from("retired"))),
        "`inactive = false` does not undo a retired status"
    );
}

// `activeOnly` "controls whether inactive concepts are included or excluded in
// value set expansions" (<https://hl7.org/fhir/R5/valueset-operation-expand.html>);
// absent it, "generally, inactive codes would be expected to be included"
// (<https://hl7.org/fhir/R5/valueset-definitions.html>, `compose.inactive`).
#[test]
fn expand_drops_every_inactive_concept_under_active_only() {
    let world = world();
    assert_eq!(
        codes(&expanded(&world, true)),
        [
            "active",
            "deprecated-date",
            "deprecated-status",
            "deprecation-date",
            "experimental",
            "retiring",
            "withdrawn-status"
        ]
    );
}

#[test]
fn expand_keeps_every_inactive_concept_and_flags_it_when_active_only_is_off() {
    let world = world();
    let contains = expanded(&world, false);
    let flagged: Vec<&str> = contains
        .iter()
        .filter(|item| item.inactive)
        .map(|item| item.code.as_str())
        .collect();
    assert_eq!(
        flagged,
        [
            "contradiction",
            "flagged",
            "flagged-and-retired",
            "retired-date",
            "retired-status"
        ]
    );
    assert_eq!(contains.len(), 12, "{:?}", codes(&contains));
}

// "Inactive is not invalid": `$validate-code` answers `result = true` with the
// `inactive` output and a warning
// (<https://hl7.org/fhir/R5/valueset-operation-validate-code.html>).
#[test]
fn validate_code_calls_an_inactive_concept_valid_and_says_it_is_inactive() {
    let world = world();
    let validated = |code: &str| {
        let request = ValueSetValidateInput {
            url: Some(LIFECYCLE_SET.to_owned()),
            system: Some(LIFECYCLE.to_owned()),
            code: Some(code.to_owned()),
            ..ValueSetValidateInput::default()
        };
        fhir_terminology::operations::value_set_validate_code::validate_code(
            &world.sources(),
            &request,
        )
        .expect("validates")
    };
    for code in ["flagged", "retired-status", "retired-date", "contradiction"] {
        let outcome = validated(code);
        assert!(outcome.result, "{code} is valid: {outcome:?}");
        assert_eq!(outcome.inactive, Some(true), "{code}");
    }
    // The `status` output is the concept's status "when its code system states
    // one" (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>), so a
    // concept retired by a date alone reports none.
    assert_eq!(
        validated("retired-status").status.as_deref(),
        Some("retired")
    );
    assert_eq!(validated("retired-date").status, None);
    assert_eq!(validated("flagged").status, None);

    let system = validate_code::validate_code(
        world.registry(),
        &Invocation::Type,
        &validate_code::ValidateCodeInput {
            url: Some(LIFECYCLE.to_owned()),
            code: Some(String::from("retired-date")),
            ..validate_code::ValidateCodeInput::default()
        },
    )
    .expect("validates");
    assert!(system.result);
    assert_eq!(system.inactive, Some(true));
    assert_eq!(system.issues.len(), 1, "{:?}", system.issues);
    assert_eq!(system.issues[0].severity, "warning");

    let active = validate_code::validate_code(
        world.registry(),
        &Invocation::Type,
        &validate_code::ValidateCodeInput {
            url: Some(LIFECYCLE.to_owned()),
            code: Some(String::from("deprecated-date")),
            ..validate_code::ValidateCodeInput::default()
        },
    )
    .expect("validates");
    assert!(active.result);
    assert_eq!(active.inactive, None, "a deprecated concept is active");
}

// `$lookup` "returns the full details" the system states for a concept
// (<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>), so the
// lifecycle properties come back as declared beside the derived `inactive`.
#[test]
fn lookup_returns_the_declared_lifecycle_properties_as_they_were_written() {
    let world = world();
    let looked_up = |code: &str| {
        lookup::lookup(
            world.registry(),
            &Invocation::Type,
            &lookup::LookupInput {
                system: Some(LIFECYCLE.to_owned()),
                code: Some(code.to_owned()),
                properties: vec![String::from("*")],
                ..lookup::LookupInput::default()
            },
        )
        .expect("looks up")
        .properties
        .iter()
        .map(|property| (property.code.clone(), property.value.clone()))
        .collect::<Vec<_>>()
    };
    assert_eq!(
        looked_up("retired-date"),
        [
            (String::from("inactive"), PropertyValue::Boolean(true)),
            (
                String::from("retirementDate"),
                PropertyValue::DateTime(String::from(PAST))
            )
        ]
    );
    assert_eq!(
        looked_up("deprecated-status"),
        [
            (String::from("inactive"), PropertyValue::Boolean(false)),
            (
                String::from("status"),
                PropertyValue::Code(String::from("deprecated"))
            )
        ]
    );
    assert_eq!(
        looked_up("contradiction"),
        [
            (String::from("inactive"), PropertyValue::Boolean(true)),
            (
                String::from("status"),
                PropertyValue::Code(String::from("retired"))
            )
        ],
        "the derived flag replaces the one the resource stated"
    );
}

/// The concepts the system enumerates as inactive, for `activeOnly` and
/// `compose.inactive = false`.
#[test]
fn the_inactive_set_is_every_marked_concept() {
    let world = world();
    let provider = world
        .registry()
        .resolve(LIFECYCLE, None)
        .expect("resolves")
        .provider;
    let mut inactive: Vec<String> = provider
        .inactive()
        .expect("enumerates")
        .iter()
        .filter_map(|index| provider.code(Concept::new(index)).expect("reads"))
        .collect();
    inactive.sort();
    assert_eq!(
        inactive,
        [
            "contradiction",
            "flagged",
            "flagged-and-retired",
            "retired-date",
            "retired-status"
        ]
    );
}

// A concept "deprecated but not inactive can still be used, but their use is
// discouraged" (<https://hl7.org/fhir/R5/codesystem-concept-properties.html>),
// so the standard properties earn the same deprecation note the
// standards-status extension does, and nothing more.
#[test]
fn a_deprecation_stated_through_the_standard_properties_earns_the_deprecation_note() {
    let world = world();
    for code in ["deprecated-status", "deprecated-date", "deprecation-date"] {
        let request = ValueSetValidateInput {
            url: Some(LIFECYCLE_SET.to_owned()),
            system: Some(LIFECYCLE.to_owned()),
            code: Some(code.to_owned()),
            ..ValueSetValidateInput::default()
        };
        let outcome = fhir_terminology::operations::value_set_validate_code::validate_code(
            &world.sources(),
            &request,
        )
        .expect("validates");
        assert!(outcome.result, "{code} is valid: {outcome:?}");
        assert_ne!(outcome.inactive, Some(true), "{code} stays active");
        assert_eq!(outcome.status.as_deref(), Some("deprecated"), "{code}");
        assert!(
            outcome.issues.iter().any(|issue| {
                issue.message == fhir_terminology::operations::MessageId::DeprecatedConceptFound
                    && issue.severity == "warning"
            }),
            "{code} earns DEPRECATED_CONCEPT_FOUND: {:?}",
            outcome.issues
        );
    }
    let (inactive, _) = standing(&world, "active");
    assert!(!inactive);
}

/// A primitive whose `value` is absent is legal FHIR when the element carries
/// extensions (<https://hl7.org/fhir/R5/json.html#primitive>).
fn absent(kind: &str) -> serde_json::Value {
    serde_json::json!({
        "code": kind,
        format!("_value{}", kind_element(kind)): {
            "extension": [{
                "url": "http://hl7.org/fhir/StructureDefinition/data-absent-reason",
                "valueCode": "unknown"
            }]
        }
    })
}

fn kind_element(kind: &str) -> &'static str {
    match kind {
        "status" => "Code",
        "inactive" => "Boolean",
        "retirementDate" => "DateTime",
        _ => "String",
    }
}

// `CodeSystem.concept.property.value[x]` is required, and a value element
// present without a `value` states nothing: the property is absent, never an
// empty string, a zero, or `false` (`.claude/rules/reliability.md`).
#[test]
fn a_property_value_without_a_value_is_no_property_and_never_a_default() {
    let mut system = lifecycle();
    system["concept"] = serde_json::json!([
        {"code": "bare", "display": "Bare",
         "property": [absent("status"), absent("inactive"), absent("retirementDate")]}
    ]);
    let world = World::of(&[("CodeSystem-lifecycle.json", system)]);
    let provider = world
        .registry()
        .resolve(LIFECYCLE, None)
        .expect("resolves")
        .provider;
    let located = provider.locate("bare").expect("reads").expect("defined");
    let properties = provider.properties(located.concept).expect("reads");
    let codes: Vec<&str> = properties.iter().map(|p| p.code.as_str()).collect();
    assert!(
        !codes.contains(&"status") && !codes.contains(&"retirementDate"),
        "an absent value states no property: {codes:?}"
    );
    let status = provider.status(located.concept).expect("reads");
    assert!(
        status.active,
        "an absent `inactive` is not `false` read as active by accident, and not `true`"
    );
    assert!(
        !properties
            .iter()
            .any(|p| p.code == "inactive" && p.value != PropertyValue::Boolean(false)),
        "the derived `inactive` is the only one answered: {properties:?}"
    );
}

// `CodeSystem.concept.code` is 1..1 (<https://hl7.org/fhir/R4B/codesystem.html>):
// a concept whose code has no value is a defective resource, refused with a
// typed error rather than stored under an empty code.
#[test]
fn a_concept_whose_code_has_no_value_is_refused() {
    let mut system = lifecycle();
    system["concept"] = serde_json::json!([
        {"_code": {"extension": [{
            "url": "http://hl7.org/fhir/StructureDefinition/data-absent-reason",
            "valueCode": "unknown"}]},
         "display": "Nameless"}
    ]);
    let parsed: fhir_types::codec::Value =
        serde_json::from_str(&system.to_string()).expect("the document parses");
    let resource = fhir_types::r4b::code_system::CodeSystem::from_json(
        parsed.as_object().expect("object"),
        &mut fhir_types::codec::Path::root("CodeSystem"),
    )
    .expect("an R4B CodeSystem");
    let error = fhir_terminology::fhir_codesystem::convert::r4b::convert(&resource)
        .expect_err("a concept without a code is refused");
    assert!(
        matches!(
            error,
            fhir_terminology::fhir_codesystem::model::ModelError::ConceptCode { under: None }
        ),
        "{error:?}"
    );
}
