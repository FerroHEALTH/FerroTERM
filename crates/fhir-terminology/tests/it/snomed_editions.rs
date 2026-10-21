// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Two SNOMED CT editions loaded at once, and the implicit forms of each.
//!
//! The FHIR SNOMED CT page says an implicit URI's base "is either
//! `http://snomed.info/sct`, or the URI for the edition version", and that the
//! edition in the base decides the membership
//! (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets"), so a
//! server holding several editions answers every one of them.

use std::sync::Arc;

use fhir_terminology::conceptmap::store::ConceptMapStore;
use fhir_terminology::operations::Invocation;
use fhir_terminology::operations::expand::{ExpandInput, expand};
use fhir_terminology::operations::lookup::{LookupInput, lookup};
use fhir_terminology::operations::translate::{TranslateInput, translate};
use fhir_terminology::operations::{OperationError, Sources};
use fhir_terminology::registry::{Registry, ResolveError};
use fhir_terminology::snomed::{SYSTEM, SnomedProvider};
use fhir_terminology::valueset::store::ValueSetStore;

use ferroterm_testkit::snomed;
use ferroterm_testkit::snomed::{
    ANIMAL, BIRD, CAT, DOG, EDITION, PETS, SAME_AS_SCTID, VERSION, code,
};

/// Both synthetic editions, in one registry, with the first as the default.
struct Editions {
    _first: tempfile::TempDir,
    _second: tempfile::TempDir,
    registry: Registry,
    value_sets: ValueSetStore,
    concept_maps: ConceptMapStore,
}

impl Editions {
    fn new() -> Self {
        let first = tempfile::tempdir().expect("tempdir");
        snomed::write(first.path()).expect("writes the first edition");
        let second = tempfile::tempdir().expect("tempdir");
        snomed::write_second(second.path()).expect("writes the second edition");
        let mut registry = Registry::new();
        for dir in [first.path(), second.path()] {
            registry
                .register(Arc::new(SnomedProvider::open(dir, "en").expect("opens")))
                .expect("registers");
        }
        registry
            .set_default(SYSTEM, VERSION)
            .expect("the first edition is the default");
        Self {
            _first: first,
            _second: second,
            registry,
            value_sets: ValueSetStore::new(),
            concept_maps: ConceptMapStore::new(),
        }
    }

    fn sources(&self) -> Sources<'_> {
        Sources {
            registry: &self.registry,
            value_sets: &self.value_sets,
            concept_maps: &self.concept_maps,
        }
    }

    /// The codes `$expand` of the implicit value set `url` returns, sorted.
    fn expand(&self, url: &str) -> Result<Vec<String>, OperationError> {
        let outcome = expand(
            &self.sources(),
            &ExpandInput {
                url: Some(url.to_owned()),
                exclude_nested: Some(true),
                ..ExpandInput::default()
            },
        )?;
        let mut codes: Vec<String> = outcome.contains.into_iter().map(|c| c.code).collect();
        codes.sort();
        Ok(codes)
    }

    /// The version every entry of the expansion of `url` states.
    fn expanded_version(&self, url: &str) -> String {
        let outcome = expand(
            &self.sources(),
            &ExpandInput {
                url: Some(url.to_owned()),
                exclude_nested: Some(true),
                ..ExpandInput::default()
            },
        )
        .expect("expands");
        let mut versions: Vec<String> = outcome.contains.into_iter().map(|c| c.version).collect();
        versions.sort();
        versions.dedup();
        match versions.as_slice() {
            [only] => only.clone(),
            other => panic!("one version, not {other:?}"),
        }
    }
}

/// Every `fhir_vs` form of the FHIR SNOMED CT page, on `base`.
fn forms(base: &str) -> [String; 5] {
    [
        format!("{base}?fhir_vs"),
        format!("{base}?fhir_vs=isa/{}", code(ANIMAL)),
        format!("{base}?fhir_vs=refset"),
        format!("{base}?fhir_vs=refset/{}", code(PETS)),
        format!("{base}?fhir_vs=ecl/%3C%3C{}", code(ANIMAL)),
    ]
}

/// Both editions are loaded, and each answers `$lookup` for its own version.
#[test]
fn two_editions_of_one_system_are_loaded_side_by_side() {
    let editions = Editions::new();
    let versions: Vec<String> = editions
        .registry
        .versions(SYSTEM)
        .map(|provider| provider.identity().version.clone())
        .collect();
    assert_eq!(versions.len(), 2, "two editions: {versions:?}");
    assert!(versions.contains(&VERSION.to_owned()));
    assert!(versions.contains(&snomed::second_version()));
    assert_eq!(
        editions.registry.default_version(SYSTEM),
        Some(VERSION),
        "the first edition is the configured default"
    );
}

/// Every `fhir_vs` form on the non-default edition's version base expands
/// against that edition (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit
/// Value Sets": the base is the bare system URI or the edition version URI).
#[test]
fn every_implicit_form_answers_on_the_non_default_edition() {
    let editions = Editions::new();
    for url in forms(&snomed::second_version()) {
        let codes = editions
            .expand(&url)
            .unwrap_or_else(|error| panic!("{url} expands: {error}"));
        assert!(!codes.is_empty(), "{url} selects concepts");
    }
    for url in forms(&snomed::second_edition()) {
        assert!(
            editions.expand(&url).is_ok(),
            "{url}: the edition URI is a base too"
        );
    }
}

/// The edition in the base decides the membership: the second edition holds a
/// concept the first does not, and every form that selects it says so.
#[test]
fn the_edition_in_the_base_decides_the_membership() {
    let editions = Editions::new();
    let bird = code(BIRD);
    let selecting = [0usize, 1, 3, 4];
    let first = forms(VERSION);
    let second = forms(&snomed::second_version());
    for index in selecting {
        let (default_url, other_url) = (&first[index], &second[index]);
        let default = editions.expand(default_url).expect("expands");
        let other = editions.expand(other_url).expect("expands");
        assert_ne!(default, other, "{other_url} differs from {default_url}");
        assert!(!default.contains(&bird), "{default_url} has no bird");
        assert!(other.contains(&bird), "{other_url} has the bird");
    }
    // `?fhir_vs=refset` lists the reference sets, which both editions share.
    assert_eq!(
        editions.expand(&first[2]).expect("expands"),
        editions.expand(&second[2]).expect("expands"),
        "both editions define the same reference sets"
    );
}

/// Each expansion states the version of the edition its base named, in
/// `ValueSet.expansion.contains.version`
/// (<https://hl7.org/fhir/R4B/valueset-definitions.html>).
#[test]
fn each_expansion_states_the_edition_its_base_named() {
    let editions = Editions::new();
    let pets = format!("?fhir_vs=refset/{}", code(PETS));
    assert_eq!(
        editions.expanded_version(&format!("{}{pets}", snomed::second_version())),
        snomed::second_version()
    );
    assert_eq!(
        editions.expanded_version(&format!("{EDITION}{pets}")),
        VERSION
    );
}

/// The bare system URI keeps answering from the default edition.
#[test]
fn the_bare_system_base_answers_from_the_default_edition() {
    let editions = Editions::new();
    let url = format!("{SYSTEM}?fhir_vs=refset/{}", code(PETS));
    let codes = editions.expand(&url).expect("expands");
    assert_eq!(codes, [code(CAT), code(DOG)].map(String::from).to_vec());
    assert_eq!(editions.expanded_version(&url), VERSION);
}

/// An edition version no loaded edition serves is that version missing, not a
/// malformed value set URI.
#[test]
fn an_edition_no_provider_serves_is_not_found() {
    let editions = Editions::new();
    let missing = "http://snomed.info/sct/999/version/20260101";
    for url in forms(missing) {
        match editions.expand(&url) {
            Err(OperationError::UnknownVersion {
                url: system,
                version,
                ..
            }) => {
                assert_eq!(system, SYSTEM);
                assert_eq!(version, missing);
            }
            other => panic!("{url}: not-found on the version, not {other:?}"),
        }
    }
}

/// The implicit concept maps resolve by the same rule as the value sets
/// (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Concept Maps").
#[test]
fn an_implicit_concept_map_resolves_on_the_non_default_edition() {
    let editions = Editions::new();
    let translate_through = |url: String| {
        translate(
            &editions.sources(),
            &TranslateInput {
                url: Some(url),
                code: Some(code(snomed::FISH)),
                system: Some(String::from(SYSTEM)),
                ..TranslateInput::default()
            },
        )
    };
    let matched = translate_through(format!(
        "{}?fhir_cm={SAME_AS_SCTID}",
        snomed::second_version()
    ))
    .expect("translates through the second edition");
    assert_eq!(matched.matches.len(), 1);
    assert!(matches!(
        translate_through(format!(
            "http://snomed.info/sct/999?fhir_cm={SAME_AS_SCTID}"
        )),
        Err(OperationError::UnknownVersion { .. })
    ));
}

/// A version URI naming only the edition resolves to the greatest loaded
/// release of that edition.
///
/// The page tells clients to send that form ("At minimum the URI SHOULD
/// contain the sctid of the SNOMED CT distribution") and lets the service
/// default ("if the date of release is not provided, the Terminology Service
/// may default to the most recent version of the named SNOMED CT
/// distribution", <https://hl7.org/fhir/R4B/snomedct.html>, "Versions"). The
/// same section keeps the date-only form an error.
#[test]
fn an_edition_uri_without_a_date_names_the_greatest_release_of_that_edition() {
    let first = tempfile::tempdir().expect("tempdir");
    snomed::write(first.path()).expect("writes the first release");
    let later = tempfile::tempdir().expect("tempdir");
    snomed::write_later(later.path()).expect("writes the later release");
    let second = tempfile::tempdir().expect("tempdir");
    snomed::write_second(second.path()).expect("writes the second edition");
    let mut registry = Registry::new();
    for dir in [first.path(), later.path(), second.path()] {
        registry
            .register(Arc::new(SnomedProvider::open(dir, "en").expect("opens")))
            .expect("registers");
    }
    registry
        .set_default(SYSTEM, VERSION)
        .expect("the first release is the default");
    let resolved = |version: &str| {
        registry
            .resolve(SYSTEM, Some(version))
            .map(|r| r.provider.identity().version.clone())
    };
    assert_eq!(
        resolved(EDITION).expect("the edition resolves"),
        snomed::later_version(),
        "the greatest loaded release of the edition, not the default one"
    );
    assert_eq!(
        resolved(&snomed::second_edition()).expect("the second edition resolves"),
        snomed::second_version(),
        "the edition in the URI picks the edition"
    );
    // A release of the named edition still resolves to itself.
    assert_eq!(resolved(VERSION).expect("the release resolves"), VERSION);
    // An edition no loaded release belongs to, and the date-only form the same
    // section says a server SHOULD refuse, both stay unknown versions.
    for refused in ["http://snomed.info/sct/999", "20260101"] {
        assert!(
            matches!(resolved(refused), Err(ResolveError::UnknownVersion { .. })),
            "`{refused}` names no loaded version"
        );
    }
    // The resolution is the operations' own, so `$lookup` reads the later
    // release through the edition URI.
    let outcome = lookup(
        &registry,
        &Invocation::Type,
        &LookupInput {
            code: Some(code(BIRD)),
            system: Some(String::from(SYSTEM)),
            version: Some(String::from(EDITION)),
            ..LookupInput::default()
        },
    )
    .expect("the later release holds the bird");
    assert_eq!(
        outcome.version.as_deref(),
        Some(snomed::later_version().as_str())
    );
}
