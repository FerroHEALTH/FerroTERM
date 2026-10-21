// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Synthetic FHIR `CodeSystem` resources (R5 JSON) for the generic provider:
//! invented systems under `http://example.org/`, no published content.

use std::path::Path;

/// The URL of the hierarchical animal system.
pub const ANIMALS: &str = "http://example.org/fhir/CodeSystem/animals";
/// The URL of the case-insensitive colour system.
pub const COLOURS: &str = "http://example.org/fhir/CodeSystem/colours";
/// The URL of the `example`-content system.
pub const SKETCH: &str = "http://example.org/fhir/CodeSystem/sketch";
/// The URL of the supplement to the animal system.
pub const ANIMALS_NL: &str = "http://example.org/fhir/CodeSystem/animals-nl";
/// Every animal, in two versions (`1.0` all of them, `2.0` without `dodo`).
pub const VS_ALL: &str = "http://example.org/fhir/ValueSet/animals-all";
/// The descendants of `pet`, by an `is-a` filter.
pub const VS_PETS: &str = "http://example.org/fhir/ValueSet/pets";
/// Enumerated concepts from two systems, one with its own display.
pub const VS_ENUMERATED: &str = "http://example.org/fhir/ValueSet/enumerated";
/// `include.valueSet` over the pets value set.
pub const VS_PETS_REF: &str = "http://example.org/fhir/ValueSet/pets-ref";
/// Two value sets that reference each other.
pub const VS_LOOP_A: &str = "http://example.org/fhir/ValueSet/loop-a";
/// The other half of the loop.
pub const VS_LOOP_B: &str = "http://example.org/fhir/ValueSet/loop-b";
/// Every colour.
pub const VS_COLOURS: &str = "http://example.org/fhir/ValueSet/colours-all";
/// Animals to colours: `cat` equivalent to `RED`, `dog` broader than
/// `Green`, `fish` explicitly unmapped, anything else fixed to `blue`.
pub const CM_ANIMALS_COLOURS: &str = "http://example.org/fhir/ConceptMap/animals-colours";
/// A map with one element whose `unmapped` defers to the animals map.
pub const CM_FALLBACK: &str = "http://example.org/fhir/ConceptMap/fallback";

/// The animal system: nesting for the hierarchy, `subsumedBy` for one extra
/// parent, `status`, `notSelectable`, and `inactive` properties, designations
/// in English and German, one declared property and filter.
fn animals() -> serde_json::Value {
    serde_json::json!({
      "resourceType": "CodeSystem", "language": "en",
      "url": ANIMALS, "version": "2.0", "name": "Animals", "title": "Animals (synthetic)",
      "status": "active", "content": "complete", "caseSensitive": true,
      "hierarchyMeaning": "is-a", "compositional": false, "versionNeeded": false,
      "filter": [{"code": "legs", "description": "By leg count", "operator": ["=", "in"], "value": "an integer"}],
      "property": [
        {"code": "legs", "uri": "http://example.org/legs", "description": "Leg count", "type": "integer"},
        {"code": "status", "uri": "http://hl7.org/fhir/concept-properties#status", "type": "code"},
        {"code": "notSelectable", "uri": "http://hl7.org/fhir/concept-properties#notSelectable", "type": "boolean"},
        {"code": "inactive", "uri": "http://hl7.org/fhir/concept-properties#inactive", "type": "boolean"},
        {"code": "subsumedBy", "type": "code"}
      ],
      "concept": [
        {"code": "living", "display": "Living thing", "property": [{"code": "notSelectable", "valueBoolean": true}], "concept": [
          {"code": "animal", "display": "Animal", "definition": "A living thing that is not a plant.",
           "designation": [{"language": "de", "value": "Tier"}],
           "concept": [
             {"code": "cat", "display": "Cat", "designation": [
                {"language": "en", "use": {"system": "http://snomed.info/sct", "code": "900000000000013009", "display": "Synonym"}, "value": "Domestic cat"},
                {"language": "de", "value": "Katze"}],
              "property": [{"code": "legs", "valueInteger": 4}]},
             {"code": "dog", "display": "Dog", "property": [{"code": "legs", "valueInteger": 4}],
              "designation": [{"language": "en", "value": "Hound",
                "extension": [{"url": "http://hl7.org/fhir/StructureDefinition/structuredefinition-standards-status", "valueCode": "withdrawn"}]}]},
             {"code": "fish", "display": "Fish", "property": [{"code": "legs", "valueInteger": 0}, {"code": "status", "valueCode": "retired"}],
              "designation": [{"use": {"system": "http://example.org/fhir/CodeSystem/designations", "code": "fishy"}, "value": "Old fish name"}]}
           ]},
          {"code": "plant", "display": "Plant",
           "extension": [{"url": "http://hl7.org/fhir/StructureDefinition/structuredefinition-standards-status", "valueCode": "deprecated"}]}
        ]},
        {"code": "pet", "display": "Pet", "property": [{"code": "notSelectable", "valueBoolean": true}]},
        {"code": "kitten", "display": "Kitten", "property": [{"code": "subsumedBy", "valueCode": "cat"}, {"code": "subsumedBy", "valueCode": "pet"}, {"code": "legs", "valueInteger": 4}]},
        {"code": "dodo", "display": "Dodo", "property": [{"code": "inactive", "valueBoolean": true}]}
      ]
    })
}

fn colours() -> serde_json::Value {
    serde_json::json!({
      "resourceType": "CodeSystem",
      "url": COLOURS, "version": "1", "name": "Colours", "status": "active",
      "content": "complete", "caseSensitive": false,
      "concept": [
        {"code": "RED", "display": "Red"},
        {"code": "Green", "display": "Green"},
        {"code": "blue", "display": "Blue", "designation": [{"language": "nl", "value": "Blauw"}]}
      ]
    })
}

fn sketch() -> serde_json::Value {
    serde_json::json!({
      "resourceType": "CodeSystem",
      "url": SKETCH, "version": "0.1", "name": "Sketch", "status": "draft",
      "content": "example",
      "concept": [{"code": "a", "display": "An example"}]
    })
}

fn animals_nl() -> serde_json::Value {
    serde_json::json!({
      "resourceType": "CodeSystem",
      "url": ANIMALS_NL, "version": "1", "name": "AnimalsNl", "status": "active",
      "content": "supplement", "supplements": ANIMALS,
      "property": [{"code": "colour", "type": "string"}],
      "concept": [
        {"code": "cat", "designation": [{"language": "nl", "value": "Kat"}], "property": [{"code": "colour", "valueString": "any"}]},
        {"code": "dog", "designation": [{"language": "nl", "value": "Hond"}]}
      ]
    })
}

/// The value sets over the animal and colour systems.
fn value_sets() -> Vec<(&'static str, serde_json::Value)> {
    let vs = |url: &str, version: &str, name: &str, compose: serde_json::Value| {
        serde_json::json!({
          "resourceType": "ValueSet", "url": url, "version": version, "name": name,
          "title": format!("{name} (synthetic)"), "status": "active", "experimental": false,
          "publisher": "FerroTERM tests", "compose": compose
        })
    };
    vec![
        (
            "ValueSet-animals-all-1.json",
            vs(
                VS_ALL,
                "1.0",
                "AnimalsAll",
                serde_json::json!({"include": [{"system": ANIMALS}]}),
            ),
        ),
        (
            "ValueSet-animals-all-2.json",
            vs(
                VS_ALL,
                "2.0",
                "AnimalsAll",
                serde_json::json!({"include": [{"system": ANIMALS}], "exclude": [{"system": ANIMALS, "concept": [{"code": "dodo"}]}]}),
            ),
        ),
        (
            "ValueSet-pets.json",
            vs(
                VS_PETS,
                "1.0",
                "Pets",
                serde_json::json!({"include": [{"system": ANIMALS, "filter": [{"property": "concept", "op": "is-a", "value": "pet"}]}]}),
            ),
        ),
        (
            "ValueSet-enumerated.json",
            vs(
                VS_ENUMERATED,
                "1.0",
                "Enumerated",
                serde_json::json!({"include": [
                    {"system": ANIMALS, "concept": [{"code": "cat", "display": "Kitty"}, {"code": "dog"}]},
                    {"system": COLOURS, "concept": [{"code": "RED"}]}
                ]}),
            ),
        ),
        (
            "ValueSet-pets-ref.json",
            vs(
                VS_PETS_REF,
                "1.0",
                "PetsRef",
                serde_json::json!({"include": [{"valueSet": [VS_PETS]}]}),
            ),
        ),
        (
            "ValueSet-loop-a.json",
            vs(
                VS_LOOP_A,
                "1.0",
                "LoopA",
                serde_json::json!({"include": [{"valueSet": [VS_LOOP_B]}]}),
            ),
        ),
        (
            "ValueSet-loop-b.json",
            vs(
                VS_LOOP_B,
                "1.0",
                "LoopB",
                serde_json::json!({"include": [{"valueSet": [VS_LOOP_A]}]}),
            ),
        ),
        (
            "ValueSet-colours-all.json",
            vs(
                VS_COLOURS,
                "1.0",
                "ColoursAll",
                serde_json::json!({"include": [{"system": COLOURS}]}),
            ),
        ),
    ]
}

/// Writes the code systems, value sets, concept maps, and a non-resource JSON
/// file to be skipped into `dir`, as a FHIR package's `package/` directory would
/// hold them.
///
/// # Errors
///
/// Returns the I/O error when a file cannot be written.
pub fn write_code_systems(dir: &Path) -> std::io::Result<()> {
    for (name, value) in [
        ("CodeSystem-animals.json", animals()),
        ("CodeSystem-colours.json", colours()),
        ("CodeSystem-sketch.json", sketch()),
        ("CodeSystem-animals-nl.json", animals_nl()),
        (
            "package.json",
            serde_json::json!({"name": "example.fixture", "version": "1.0.0", "fhirVersions": ["5.0.0"]}),
        ),
    ]
    .into_iter()
    .chain(value_sets())
    .chain(concept_maps())
    {
        std::fs::write(dir.join(name), serde_json::to_string_pretty(&value)?)?;
    }
    Ok(())
}

/// The concept maps over the animal and colour systems, in R5 form.
fn concept_maps() -> Vec<(&'static str, serde_json::Value)> {
    vec![
        (
            "ConceptMap-animals-colours.json",
            serde_json::json!({
              "resourceType": "ConceptMap", "url": CM_ANIMALS_COLOURS, "version": "1.0",
              "name": "AnimalsToColours", "status": "active",
              "sourceScopeUri": VS_ALL, "targetScopeUri": VS_COLOURS,
              "group": [{
                "source": ANIMALS, "target": COLOURS,
                "element": [
                  {"code": "cat", "target": [{"code": "RED", "display": "Red", "relationship": "equivalent"}]},
                  {"code": "dog", "target": [{"code": "Green", "relationship": "source-is-broader-than-target", "comment": "roughly"}]},
                  {"code": "fish", "noMap": true,
                   "extension": [{"url": "http://hl7.org/fhir/6.0/StructureDefinition/extension-ConceptMap.group.element.comment", "valueString": "fish have no colour"}]}
                ],
                "unmapped": {"mode": "fixed", "code": "blue", "display": "Blue", "relationship": "related-to"}
              }]
            }),
        ),
        (
            "ConceptMap-fallback.json",
            serde_json::json!({
              "resourceType": "ConceptMap", "url": CM_FALLBACK, "version": "1.0",
              "name": "Fallback", "status": "active",
              "group": [{
                "source": ANIMALS, "target": COLOURS,
                "element": [{"code": "plant", "target": [{"code": "Green", "relationship": "related-to"}]}],
                "unmapped": {"mode": "other-map", "otherMap": CM_ANIMALS_COLOURS}
              }]
            }),
        ),
    ]
}
