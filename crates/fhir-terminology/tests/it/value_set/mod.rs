// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ValueSet/$expand` and `ValueSet/$validate-code` on R4B over the compose
//! layer (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>,
//! <https://hl7.org/fhir/R4B/valueset-operation-validate-code.html>).

use std::sync::Arc;

use ferroterm_testkit::fhir::write_code_systems;
use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::fhir_codesystem::load::{FhirVersion, load_dir};
use fhir_terminology::fhir_codesystem::provider::FhirCodeSystem;
use fhir_terminology::operations::sources::Sources;
use fhir_terminology::operations::{CodeableConceptRef, CodingRef};
use fhir_terminology::registry::Registry;
use fhir_terminology::valueset;
use fhir_terminology::valueset::expansion::{Contains, ExpansionOutcome, ParameterValue};
use fhir_terminology::valueset::store::ValueSetStore;

mod displays;
mod expansion;
mod fragments;
mod messages;
mod references;
mod supplements;
mod versions;

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
        let mut registry = Registry::default();
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
        let mut value_sets = ValueSetStore::default();
        for model in valueset::load::load_dir(dir.path(), FhirVersion::R5).expect("loads") {
            value_sets.insert(model).expect("stores");
        }
        let mut concept_maps = ConceptMapStore::default();
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
