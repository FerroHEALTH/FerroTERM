// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A deployment's layer over the FHIR core terminology, resolved per `url`
//! and `version`.
//!
//! A version-specific canonical names one resource and a version-less one the
//! latest (<https://hl7.org/fhir/R4B/references.html#canonical>), so a layer
//! adds versions to a `url` the core terminology defines and hides none.

use std::sync::Arc;

use fhir_terminology::fhir_codesystem::load::FhirVersion;
use fhir_terminology::fhir_core::CoreTerminology;
use fhir_terminology::provider::CodeSystemProvider;
use fhir_terminology::registry::{Registry, ResolveError};
use fhir_terminology::valueset::store::ValueSetStore;

use crate::fixture::{Fixture, URL, registry};

/// The `CodeSystem.version` and `ValueSet.version` each package stamps on the
/// terminology it defines.
const CORE: [(FhirVersion, &str); 4] = [
    (FhirVersion::R4, "4.0.1"),
    (FhirVersion::R4B, "4.3.0"),
    (FhirVersion::R5, "5.0.0"),
    (FhirVersion::R6, "6.0.0-ballot5"),
];

const GENDER: &str = "http://hl7.org/fhir/administrative-gender";
const GENDER_VALUE_SET: &str = "http://hl7.org/fhir/ValueSet/administrative-gender";

/// The deployment's own registry holding `GENDER` at each of `versions`.
fn deployment(versions: &[&str]) -> Registry {
    let mut registry = Registry::new();
    for version in versions {
        registry
            .register(Arc::new(Fixture::flat_at(GENDER, version)))
            .expect("registers");
    }
    registry
}

/// Whether `provider` is the core `CodeSystem`, the one that defines `male`.
fn is_core(provider: &dyn CodeSystemProvider) -> bool {
    provider.locate("male").expect("reads").is_some()
}

#[test]
fn a_layered_version_leaves_every_core_version_answering() {
    for (fhir, core_version) in CORE {
        let core = CoreTerminology::load(fhir).expect("the bundle loads");
        let layered = deployment(&["local-1"]).with_beneath(core.code_systems());

        let local = layered.resolve(GENDER, Some("local-1")).expect("resolves");
        assert_eq!(local.provider.identity().version, "local-1", "{fhir:?}");
        assert!(!is_core(local.provider.as_ref()), "{fhir:?}");

        let spec = layered
            .resolve(GENDER, Some(core_version))
            .expect("the core version still resolves");
        assert_eq!(spec.provider.identity().version, core_version, "{fhir:?}");
        assert!(is_core(spec.provider.as_ref()), "{fhir:?}");

        let versions: Vec<String> = layered
            .versions(GENDER)
            .map(|p| p.identity().version.clone())
            .collect();
        assert_eq!(versions, [core_version, "local-1"], "{fhir:?}");
        assert_eq!(
            layered.resolve(GENDER, Some("9")).err(),
            Some(ResolveError::UnknownVersion {
                url: GENDER.to_owned(),
                version: String::from("9"),
                available: vec![core_version.to_owned(), String::from("local-1")],
            }),
            "{fhir:?}: the refusal names the versions of both layers"
        );
    }
}

/// With no configured default the greatest version string of either layer is
/// the default, the rule one registry holding both would apply: `local-1`
/// sorts above every core version, `1.0.0` below them.
#[test]
fn the_default_is_the_greatest_version_of_either_layer() {
    for (fhir, core_version) in CORE {
        let core = CoreTerminology::load(fhir).expect("the bundle loads");

        let below = deployment(&["1.0.0"]).with_beneath(core.code_systems());
        let resolved = below.resolve(GENDER, None).expect("resolves");
        assert!(resolved.defaulted);
        assert_eq!(
            resolved.provider.identity().version,
            core_version,
            "{fhir:?}: a lower layered version leaves the core default"
        );
        assert_eq!(below.default_version(GENDER), Some(core_version));

        let above = deployment(&["1.0.0", "local-1"]).with_beneath(core.code_systems());
        let resolved = above.resolve(GENDER, None).expect("resolves");
        assert_eq!(resolved.provider.identity().version, "local-1", "{fhir:?}");
        assert_eq!(above.default_version(GENDER), Some("local-1"));
    }
}

/// A default the upper layer configures wins over the greatest version,
/// exactly as a configured default does in one registry.
#[test]
fn a_configured_default_of_the_upper_layer_wins() {
    for (fhir, _) in CORE {
        let core = CoreTerminology::load(fhir).expect("the bundle loads");
        let mut upper = deployment(&["1.0.0"]);
        upper.set_default(GENDER, "1.0.0").expect("sets default");
        let layered = upper.with_beneath(core.code_systems());
        let resolved = layered.resolve(GENDER, None).expect("resolves");
        assert_eq!(resolved.provider.identity().version, "1.0.0", "{fhir:?}");
        assert_eq!(layered.default_version(GENDER), Some("1.0.0"));
    }
}

/// A default configured beneath still holds when the upper layer adds a
/// version and configures none.
#[test]
fn a_configured_default_beneath_survives_an_upper_version() {
    let mut beneath = registry();
    beneath.set_default(URL, "2024").expect("sets default");
    let mut upper = Registry::new();
    upper
        .register(Arc::new(Fixture::flat_at(URL, "2026")))
        .expect("registers");
    let layered = upper.with_beneath(Arc::new(beneath));
    assert_eq!(layered.default_version(URL), Some("2024"));
    let resolved = layered.resolve(URL, None).expect("resolves");
    assert_eq!(resolved.provider.identity().version, "2024");
    let newest = layered.resolve(URL, Some("2026")).expect("resolves");
    assert_eq!(newest.provider.identity().version, "2026");
}

/// The same version in the upper layer answers in place of the one beneath.
#[test]
fn the_same_version_answers_from_the_upper_layer() {
    let core = CoreTerminology::load(FhirVersion::R4B).expect("the bundle loads");
    let layered = deployment(&["4.3.0"]).with_beneath(core.code_systems());
    let resolved = layered.resolve(GENDER, Some("4.3.0")).expect("resolves");
    assert!(!is_core(resolved.provider.as_ref()));
    assert_eq!(layered.versions(GENDER).count(), 1);
}

/// A version pattern picks the greatest match across both layers.
#[test]
fn a_version_pattern_matches_across_the_layers() {
    let core = CoreTerminology::load(FhirVersion::R4B).expect("the bundle loads");
    let layered = deployment(&["4.2.0"]).with_beneath(core.code_systems());
    let resolved = layered.resolve(GENDER, Some("4.x.x")).expect("resolves");
    assert_eq!(resolved.provider.identity().version, "4.3.0");
    assert!(is_core(resolved.provider.as_ref()));
}

#[test]
fn a_layered_value_set_version_leaves_the_core_versions_answering() {
    for (fhir, core_version) in CORE {
        let core = CoreTerminology::load(fhir).expect("the bundle loads");
        let spec = core
            .value_sets()
            .resolve(GENDER_VALUE_SET, None)
            .expect("the core value set resolves");
        let mut upper = ValueSetStore::new();
        for version in ["1.0.0", "local-1"] {
            let mut local = (*spec).clone();
            local.version = Some(version.to_owned());
            local.title = Some(String::from("Local"));
            upper.insert(local).expect("stores");
        }
        let layered = upper.with_beneath(core.value_sets());

        let local = layered
            .resolve(GENDER_VALUE_SET, Some("local-1"))
            .expect("resolves");
        assert_eq!(local.title.as_deref(), Some("Local"), "{fhir:?}");
        let resolved = layered
            .resolve(&format!("{GENDER_VALUE_SET}|{core_version}"), None)
            .expect("the core version still resolves");
        assert_eq!(resolved.version.as_deref(), Some(core_version), "{fhir:?}");
        assert_ne!(resolved.title.as_deref(), Some("Local"), "{fhir:?}");
        // The greatest version of either layer is the default, as in one store.
        let default = layered.resolve(GENDER_VALUE_SET, None).expect("resolves");
        assert_eq!(default.version.as_deref(), Some("local-1"), "{fhir:?}");
        assert_eq!(layered.len(), 2, "the core stays out of what is published");
    }
}

#[test]
fn a_lower_layered_value_set_version_leaves_the_core_default() {
    for (fhir, core_version) in CORE {
        let core = CoreTerminology::load(fhir).expect("the bundle loads");
        let spec = core
            .value_sets()
            .resolve(GENDER_VALUE_SET, None)
            .expect("the core value set resolves");
        let mut local = (*spec).clone();
        local.version = Some(String::from("1.0.0"));
        let mut upper = ValueSetStore::new();
        upper.insert(local).expect("stores");
        let layered = upper.with_beneath(core.value_sets());
        let default = layered.resolve(GENDER_VALUE_SET, None).expect("resolves");
        assert_eq!(default.version.as_deref(), Some(core_version), "{fhir:?}");
    }
}
