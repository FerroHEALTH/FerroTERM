// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology the FHIR specification defines, per served version
//! (<https://hl7.org/fhir/R4B/terminologies-systems.html>).

use std::sync::Arc;

use fhir_terminology::compose::{Compose, Include, SystemRef};
use fhir_terminology::fhir_codesystem::load::FhirVersion;
use fhir_terminology::fhir_core::CoreTerminology;
use fhir_terminology::registry::Registry;
use fhir_terminology::valueset::store::ValueSetStore;

const VERSIONS: [FhirVersion; 4] = [
    FhirVersion::R4,
    FhirVersion::R4B,
    FhirVersion::R5,
    FhirVersion::R6,
];

/// The `CodeSystem.version` each package stamps on the terminology it defines.
const FHIR_VERSIONS: [(FhirVersion, &str); 4] = [
    (FhirVersion::R4, "4.0.1"),
    (FhirVersion::R4B, "4.3.0"),
    (FhirVersion::R5, "5.0.0"),
    (FhirVersion::R6, "6.0.0-ballot5"),
];

const GENDER: &str = "http://hl7.org/fhir/administrative-gender";
const GENDER_VALUE_SET: &str = "http://hl7.org/fhir/ValueSet/administrative-gender";
const PUBLICATION_STATUS: &str = "http://hl7.org/fhir/publication-status";

#[test]
fn every_version_bundle_loads() {
    for version in VERSIONS {
        let core = CoreTerminology::load(version).expect("the bundle loads");
        assert!(
            core.code_systems().systems().count() > 200,
            "{version:?} carries the code systems its package defines"
        );
        assert!(
            core.value_sets().len() > 200,
            "{version:?} carries the value sets its package defines"
        );
    }
}

#[test]
fn each_version_answers_at_its_own_version() {
    for (version, stamped) in FHIR_VERSIONS {
        let core = CoreTerminology::load(version).expect("the bundle loads");
        let resolved = core
            .code_systems()
            .resolve(GENDER, None)
            .expect("administrative-gender resolves");
        assert_eq!(
            resolved.provider.identity().version,
            stamped,
            "{version:?} answers at the version its package stamps"
        );
    }
}

#[test]
fn the_gender_value_set_selects_from_the_gender_system() {
    let core = CoreTerminology::load(FhirVersion::R4B).expect("the bundle loads");
    let model = core
        .value_sets()
        .resolve(GENDER_VALUE_SET, None)
        .expect("the value set resolves");
    let systems: Vec<&str> = model
        .compose
        .include
        .iter()
        .filter_map(|criterion| criterion.system.as_ref().map(|s| s.url.as_str()))
        .collect();
    assert_eq!(systems, vec![GENDER]);
}

/// A deployment's own `CodeSystem` shadows the specification's, and a system
/// the deployment does not hold resolves in the bundle beneath.
#[test]
fn a_deployment_registry_shadows_the_bundle() {
    let core = CoreTerminology::load(FhirVersion::R4B).expect("the bundle loads");
    let layered = Registry::new().with_beneath(core.code_systems());
    assert!(
        layered.resolve(PUBLICATION_STATUS, None).is_ok(),
        "a system only the bundle holds resolves beneath"
    );
    assert_eq!(
        layered.systems().count(),
        0,
        "the bundle is not a system this deployment publishes"
    );
    assert!(
        layered.canonicals().contains(&PUBLICATION_STATUS),
        "the bundle's systems are still reachable canonicals"
    );
}

/// The same layering over the value sets: a `url` only the bundle holds
/// resolves, and the bundle stays out of what the deployment enumerates.
#[test]
fn a_deployment_store_shadows_the_bundle() {
    let core = CoreTerminology::load(FhirVersion::R4B).expect("the bundle loads");
    let layered = ValueSetStore::new().with_beneath(core.value_sets());
    assert!(layered.resolve(GENDER_VALUE_SET, None).is_some());
    assert_eq!(layered.len(), 0);
}

/// Nothing in a bundle names a code system the FHIR specification does not
/// define, which is what keeps licensed content out of the repository.
#[test]
fn no_bundle_reaches_outside_the_specification() {
    for version in VERSIONS {
        let core = CoreTerminology::load(version).expect("the bundle loads");
        let systems = core.code_systems();
        for url in systems.canonicals() {
            assert!(
                url.starts_with("http://hl7.org/fhir/"),
                "{version:?} bundles the code system {url}"
            );
        }
        for model in core.value_sets().iter() {
            for criterion in model.compose.include.iter().chain(&model.compose.exclude) {
                assert_named_system(version, criterion, &systems);
            }
        }
    }
}

fn assert_named_system(version: FhirVersion, criterion: &Include, systems: &Arc<Registry>) {
    let Some(SystemRef { url, .. }) = &criterion.system else {
        return;
    };
    assert!(
        systems.canonicals().contains(&url.as_str()),
        "the {version:?} bundle selects from {url}, which it does not carry"
    );
}

/// The compose of a bundled value set is the one the package publishes.
#[test]
fn a_bundled_value_set_keeps_its_compose() {
    let core = CoreTerminology::load(FhirVersion::R5).expect("the bundle loads");
    let model = core
        .value_sets()
        .resolve(GENDER_VALUE_SET, None)
        .expect("the value set resolves");
    assert_eq!(model.status, "active");
    assert_eq!(model.version.as_deref(), Some("5.0.0"));
    assert!(matches!(&model.compose, Compose { include, .. } if include.len() == 1));
}
