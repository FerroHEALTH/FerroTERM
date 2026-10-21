// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Identity, versions, and the default-version rule.

use std::sync::Arc;

use fhir_terminology::registry::{RegisterError, ResolveError};

use crate::fixture::{FLAT_URL, Fixture, URL, registry};

#[test]
fn the_default_is_the_configured_version_else_the_greatest() {
    let mut registry = registry();
    let resolved = registry.resolve(URL, None).expect("resolves");
    assert_eq!(resolved.provider.identity().version, "2025");
    assert!(resolved.defaulted);
    registry.set_default(URL, "2024").expect("sets default");
    let resolved = registry.resolve(URL, None).expect("resolves");
    assert_eq!(resolved.provider.identity().version, "2024");
    assert_eq!(registry.default_version(URL), Some("2024"));
    let explicit = registry.resolve(URL, Some("2025")).expect("resolves");
    assert_eq!(explicit.provider.identity().version, "2025");
    assert!(!explicit.defaulted);
}

#[test]
fn unknown_systems_versions_and_duplicates_are_typed_errors() {
    let mut registry = registry();
    assert_eq!(
        registry.resolve("http://example.org/nowhere", None).err(),
        Some(ResolveError::UnknownSystem(String::from(
            "http://example.org/nowhere"
        )))
    );
    assert_eq!(
        registry.resolve(URL, Some("1999")).err(),
        // The error names the versions that would have worked, in order.
        Some(ResolveError::UnknownVersion {
            url: String::from(URL),
            version: String::from("1999"),
            available: vec![String::from("2024"), String::from("2025")],
        })
    );
    assert_eq!(
        registry.set_default(FLAT_URL, "2").err(),
        Some(ResolveError::UnknownVersion {
            url: String::from(FLAT_URL),
            version: String::from("2"),
            available: vec![String::from("1")],
        })
    );
    assert_eq!(
        registry.register(Arc::new(Fixture::flat())).err(),
        Some(RegisterError::Duplicate {
            url: String::from(FLAT_URL),
            version: String::from("1"),
        })
    );
    assert_eq!(registry.systems().collect::<Vec<_>>(), vec![URL, FLAT_URL]);
    assert_eq!(registry.versions(URL).count(), 2);
}

// NOTE: `x` segments in a requested version name the greatest matching served
// version, the ecosystem's form (spec-silent, #174); a bare prefix names nothing.
#[test]
fn a_wildcard_version_resolves_to_the_greatest_match() {
    let registry = registry();
    let latest = registry.resolve(URL, Some("x")).expect("resolves");
    assert_eq!(latest.provider.identity().version, "2025");
    assert!(!latest.defaulted, "a pattern is a request, not a default");
    let exact = registry.resolve(URL, Some("2024")).expect("resolves");
    assert_eq!(exact.provider.identity().version, "2024");
    assert!(matches!(
        registry.resolve(URL, Some("20")),
        Err(ResolveError::UnknownVersion { .. })
    ));
    assert!(matches!(
        registry.resolve(URL, Some("3.x")),
        Err(ResolveError::UnknownVersion { .. })
    ));
}
