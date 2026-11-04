// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The SNOMED CT implicit value sets and the reference sets they serve.

use ferroterm_testkit::snomed;
use ferroterm_testkit::snomed::{
    ANIMAL, CAT, DOG, EDITION, FUR, Item, MODULE_CONCEPT, PETS, VERSION, item, sctid,
};
use fhir_terminology::provider::filter::{Filter, FilterOperator};
use fhir_terminology::provider::{CodeSystemProvider, ProviderError};
use fhir_terminology::snomed::SYSTEM;

use super::provider;

#[test]
fn the_implicit_value_sets_follow_the_snomed_ct_page() {
    let (_dir, p) = provider();
    let base = "http://snomed.info/sct";
    let all = p
        .implicit_value_set(&format!("{base}?fhir_vs"))
        .expect("implicit")
        .expect("compose");
    assert_eq!(all.include.len(), 1);
    assert!(all.include[0].filters.is_empty() && all.include[0].concepts.is_empty());
    assert_eq!(
        all.include[0]
            .system
            .as_ref()
            .and_then(|s| s.version.as_deref()),
        None,
        "the bare URI leaves the version to the registry"
    );
    let isa = p
        .implicit_value_set(&format!("{EDITION}?fhir_vs=isa/{}", sctid(item(ANIMAL))))
        .expect("implicit")
        .expect("compose");
    assert_eq!(isa.include[0].filters[0].op, FilterOperator::IsA);
    assert_eq!(isa.include[0].filters[0].value, sctid(item(ANIMAL)));
    assert_eq!(
        isa.include[0]
            .system
            .as_ref()
            .and_then(|s| s.version.as_deref()),
        Some(VERSION),
        "an edition base pins the served version"
    );
    assert!(
        p.implicit_value_set(&format!("{VERSION}?fhir_vs=isa/{}", sctid(item(ANIMAL))))
            .expect("implicit")
            .is_ok(),
        "the version URI is a base too"
    );
    let refsets = p
        .implicit_value_set(&format!("{base}?fhir_vs=refset"))
        .expect("implicit")
        .expect("compose");
    assert_eq!(
        refsets.include[0].concepts.len(),
        10,
        "every reference set the edition defines"
    );
    assert!(
        refsets.include[0]
            .concepts
            .iter()
            .any(|c| c.code == sctid(item(PETS)))
    );
    // The FHIR SNOMED CT page defines the set as "all concept ids that
    // correspond to reference sets that are explicitly defined in the specified
    // SNOMED CT edition" (<https://hl7.org/fhir/R4B/snomedct.html>), with no
    // category excluded, so a metadata reference set is listed like any other.
    assert!(
        refsets.include[0]
            .concepts
            .iter()
            .any(|c| c.code == snomed::MODULE_DEPENDENCY_SCTID),
        "the Module Dependency reference set is a reference set of the edition"
    );
    // A language reference set is defined in the edition too, and the same
    // sentence excludes no category, so it is listed (#363).
    for language in [snomed::GB_REFSET, snomed::NL_REFSET] {
        assert!(
            refsets.include[0]
                .concepts
                .iter()
                .any(|c| c.code == language),
            "the language reference set {language} is a reference set of the edition"
        );
    }
    let members = p
        .implicit_value_set(&format!("{base}?fhir_vs=refset/{}", sctid(item(PETS))))
        .expect("implicit")
        .expect("compose");
    assert_eq!(members.include[0].filters[0].op, FilterOperator::In);
    let selected = p.filter(&members.include[0].filters[0]).expect("filters");
    assert_eq!(selected.iter().collect::<Vec<_>>(), [CAT, DOG]);
}

/// The FHIR SNOMED CT page prints a template per implicit value set form and
/// says "the content of the resource must conform to the template provided"
/// (<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets"). Each
/// template carries `version`, `name`, `description`, and `copyright`.
#[test]
fn an_implicit_value_set_carries_the_page_s_template() {
    let (_dir, p) = provider();
    let animal = sctid(item(ANIMAL));
    let pets = sctid(item(PETS));
    let forms = [
        (
            format!("{EDITION}?fhir_vs=isa/{animal}"),
            Some(format!("SNOMED CT Concept {animal} and descendants")),
            Some(String::from("All SNOMED CT concepts for Animal")),
        ),
        (
            format!("{EDITION}?fhir_vs=refset"),
            Some(String::from("SNOMED CT Reference Sets")),
            Some(String::from(
                "All SNOMED CT concepts associated with a reference set",
            )),
        ),
        (
            format!("{EDITION}?fhir_vs=refset/{pets}"),
            Some(format!("SNOMED CT Reference Set {pets}")),
            Some(String::from(
                "All SNOMED CT concepts in the reference set Pets reference set",
            )),
        ),
        (
            format!("{EDITION}?fhir_vs=ecl/%3C%3C%20{animal}"),
            Some(format!("SNOMED CT Concepts matching << {animal}")),
            Some(format!(
                "All SNOMED CT concepts that match the expression constraint << {animal}"
            )),
        ),
    ];
    for (url, name, description) in forms {
        let metadata = p.implicit_metadata(&url);
        assert_eq!(metadata.version.as_deref(), Some(VERSION), "{url}");
        assert_eq!(metadata.name, name, "{url}");
        assert_eq!(metadata.description, description, "{url}");
        assert_eq!(
            metadata.copyright.as_deref(),
            Some(fhir_terminology::snomed::TEMPLATE_COPYRIGHT),
            "{url}"
        );
    }
    // The page prints no template for the bare `?fhir_vs`, so it carries only
    // the fields every template shares.
    let all = p.implicit_metadata(&format!("{EDITION}?fhir_vs"));
    assert_eq!(all.version.as_deref(), Some(VERSION));
    assert_eq!(all.name, None);
    assert_eq!(all.description, None);
    assert_eq!(
        all.copyright.as_deref(),
        Some(fhir_terminology::snomed::TEMPLATE_COPYRIGHT)
    );
}

/// A language reference set is a reference set the edition defines, so it is
/// listed by `?fhir_vs=refset`, but its members are descriptions, so
/// `?fhir_vs=refset/[sctid]` ("all concept ids in the specified reference
/// set", <https://hl7.org/fhir/R4B/snomedct.html>) and the ECL `^` operator
/// over it are refused rather than answered as an empty set.
#[test]
fn a_language_reference_set_is_listed_and_its_member_forms_are_refused() {
    let (_dir, p) = provider();
    let base = "http://snomed.info/sct";
    assert!(
        matches!(
            p.implicit_value_set(&format!("{base}?fhir_vs=refset/{}", snomed::GB_REFSET))
                .expect("implicit"),
            Err(ProviderError::InvalidFilterValue(ref invalid)) if invalid.value == snomed::GB_REFSET
        ),
        "the members of a language reference set are descriptions"
    );
    assert!(matches!(
        p.filter(&Filter {
            property: String::from("constraint"),
            op: FilterOperator::Equal,
            value: format!("^ {}", snomed::NL_REFSET),
        }),
        Err(ProviderError::InvalidCode { .. })
    ));
}

#[test]
fn malformed_and_unknown_implicit_value_sets_are_refused() {
    let (_dir, p) = provider();
    let base = "http://snomed.info/sct";
    assert!(
        matches!(
            p.implicit_value_set(&format!("{base}?fhir_vs=refset/{}", sctid(item(FUR))))
                .expect("implicit"),
            Err(ProviderError::UnknownCode(_))
        ),
        "a concept that is not a reference set"
    );
    assert!(matches!(
        p.implicit_value_set(&format!("{base}?fhir_vs=isa/{}", sctid(Item::raw(77))))
            .expect("implicit"),
        Err(ProviderError::UnknownCode(_))
    ));
    assert!(matches!(
        p.implicit_value_set(&format!("{base}?fhir_vs=isa/abc"))
            .expect("implicit"),
        Err(ProviderError::MalformedImplicitValueSet(_))
    ));
    assert!(matches!(
        p.implicit_value_set(&format!("{base}?fhir_vs=ecl/%3C%3C%20"))
            .expect("implicit"),
        Err(ProviderError::MalformedImplicitValueSet(_))
    ));
    assert!(matches!(
        p.implicit_value_set(&format!("{base}?fhir_vs=nope"))
            .expect("implicit"),
        Err(ProviderError::MalformedImplicitValueSet(_))
    ));
    // The base may be any edition version (<https://hl7.org/fhir/R4B/snomedct.html>,
    // "Implicit Value Sets"), so another edition is a version this provider does
    // not serve, which the registry asks its other editions about.
    assert!(
        matches!(
            p.implicit_value_set("http://snomed.info/sct/999?fhir_vs")
                .expect("implicit"),
            Err(ProviderError::UnservedImplicitVersion { ref url, ref version })
                if url == SYSTEM && version == "http://snomed.info/sct/999"
        ),
        "another edition"
    );
    assert!(p.implicit_value_set(&format!("{base}?fhir_cm=1")).is_none());
    assert!(matches!(
        p.filter(&Filter {
            property: String::from("concept"),
            op: FilterOperator::In,
            value: String::from("abc"),
        }),
        Err(ProviderError::InvalidFilterValue(_))
    ));
    let declared = p
        .declaration()
        .filters
        .iter()
        .find(|f| f.code == "concept")
        .expect("the concept filter is declared");
    assert!(declared.operators.contains(&FilterOperator::In));
}

/// A metadata reference set is served like any other reference set.
///
/// The FHIR SNOMED CT page defines `?fhir_vs=refset` as "all concept ids that
/// correspond to reference sets that are explicitly defined in the specified
/// SNOMED CT edition" and `?fhir_vs=refset/[sctid]` as "all concept ids in the
/// specified reference set" (<https://hl7.org/fhir/R4B/snomedct.html>). Neither
/// excludes a category, so the Module Dependency reference set, whose members
/// are the edition's modules, is listed and expands to them (#272).
#[test]
fn the_module_dependency_reference_set_is_served_like_any_other() {
    let (_dir, p) = provider();
    let members = p
        .implicit_value_set(&format!(
            "{SYSTEM}?fhir_vs=refset/{}",
            snomed::MODULE_DEPENDENCY_SCTID
        ))
        .expect("implicit")
        .expect("compose");
    assert_eq!(members.include[0].filters[0].op, FilterOperator::In);
    let selected = p.filter(&members.include[0].filters[0]).expect("filters");
    assert_eq!(
        selected.iter().collect::<Vec<_>>(),
        [MODULE_CONCEPT],
        "the members are the edition's modules"
    );
}

#[test]
fn every_declared_implicit_value_set_form_resolves() {
    // The five forms of <https://hl7.org/fhir/R4B/snomedct.html>, "Implicit
    // Value Sets"; the ECL argument is URI-encoded, as the page requires.
    let (_dir, p) = provider();
    let animal = sctid(item(ANIMAL));
    let pets = sctid(item(PETS));
    let ecl = format!("%3C%3C%20{animal}");
    crate::implicit_forms::every_form_resolves(
        &p,
        &[
            ("http://snomed.info/sct?fhir_vs", &[]),
            ("http://snomed.info/sct?fhir_vs=isa/[sctid]", &[&animal]),
            ("http://snomed.info/sct?fhir_vs=refset", &[]),
            ("http://snomed.info/sct?fhir_vs=refset/[sctid]", &[&pets]),
            ("http://snomed.info/sct?fhir_vs=ecl/[ecl]", &[&ecl]),
        ],
    );
}
