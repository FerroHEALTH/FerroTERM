// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `RxNorm` provider over the testkit's release-shaped artifact
//! (<https://hl7.org/fhir/R4B/rxnorm.html>).

use ferroterm_testkit::rxnorm::{
    ASPIRIN, ASPIRIN_SYNONYM_ATOM, ASPIRIN_TABLET, BRAND, BRANDED_TABLET, LABEL_ONLY, OLD_TABLET,
    ORAL_TABLET, VERSION, write_artifact,
};
use fhir_terminology::filter::{Filter, FilterOperator};
use fhir_terminology::provider::{
    Capability, CodeSystemProvider, Concept, ContentMode, ProviderError,
};
use fhir_terminology::rxnorm::{OpenError, RxNormProvider, SYSTEM};

fn provider() -> (tempfile::TempDir, RxNormProvider) {
    let dir = tempfile::tempdir().expect("tempdir");
    write_artifact(dir.path()).expect("builds");
    let provider = RxNormProvider::open(dir.path()).expect("opens");
    (dir, provider)
}

fn filter(property: &str, op: FilterOperator, value: &str) -> Filter {
    Filter {
        property: property.to_owned(),
        op,
        value: value.to_owned(),
    }
}

fn codes(provider: &RxNormProvider, set: impl IntoIterator<Item = u32>) -> Vec<String> {
    let mut out: Vec<String> = set
        .into_iter()
        .filter_map(|i| provider.code(Concept::new(i)).expect("reads"))
        .collect();
    out.sort();
    out
}

fn located(provider: &RxNormProvider, code: &str) -> Concept {
    provider
        .locate(code)
        .expect("reads")
        .expect("a concept with the code")
        .concept
}

#[test]
fn codes_are_the_rxnorm_cuis_and_the_display_is_the_rxnorm_string() {
    let (_dir, provider) = provider();
    assert_eq!(provider.identity().url, SYSTEM);
    assert_eq!(provider.identity().version, VERSION);
    assert_eq!(provider.declaration().content, ContentMode::NotPresent);
    assert!(provider.declaration().hierarchy_meaning.is_none());
    assert!(
        !provider
            .declaration()
            .capabilities
            .contains(&Capability::Subsumption),
        "no subsumption"
    );
    let tablet = located(&provider, ASPIRIN_TABLET);
    assert_eq!(
        provider.display(tablet, None).expect("reads").as_deref(),
        Some("aspirin 81 MG Oral Tablet")
    );
    assert!(
        provider.locate(LABEL_ONLY).expect("reads").is_none(),
        "an MTHSPL-only concept is not a code"
    );
    assert!(provider.locate("abc").expect("reads").is_none());
    let designations = provider.designations(tablet, None).expect("reads");
    assert_eq!(designations.len(), 3);
    assert!(designations.iter().any(
        |d| d.value == "ASPIRIN 81MG TABLET" && d.use_.as_ref().is_some_and(|u| u.code == "DP")
    ));
    let old = provider
        .status(located(&provider, OLD_TABLET))
        .expect("reads");
    assert!(!old.active);
    assert_eq!(old.inactive_reason.as_deref(), Some("obsolete"));
}

#[test]
fn properties_carry_the_term_types_sources_attributes_and_relationships() {
    let (_dir, provider) = provider();
    let tablet = located(&provider, ASPIRIN_TABLET);
    let props: Vec<String> = provider
        .properties(tablet)
        .expect("reads")
        .into_iter()
        .map(|p| format!("{}={}", p.code, p.value.as_text()))
        .collect();
    assert!(props.contains(&String::from("TTY=SCD")));
    assert!(props.contains(&String::from("SAB=MTHSPL")));
    assert!(props.contains(&String::from("NDC=00000000101")));
    assert!(props.contains(&String::from("RXN_AVAILABLE_STRENGTH=81 MG")));
    assert!(props.contains(&format!("has_ingredient={ASPIRIN}")));
    assert!(props.contains(&format!("RO={ASPIRIN}")));
    assert!(props.contains(&format!("has_dose_form={ORAL_TABLET}")));
    assert!(props.contains(&format!("has_tradename={BRANDED_TABLET}")));
    assert!(!props.iter().any(|p| p.starts_with("SPL_SET_ID")));
    let aspirin: Vec<String> = provider
        .properties(located(&provider, ASPIRIN))
        .expect("reads")
        .into_iter()
        .map(|p| format!("{}={}", p.code, p.value.as_text()))
        .collect();
    assert!(aspirin.contains(&String::from("STY=Organic Chemical")));
    let names: Vec<&str> = provider
        .declaration()
        .filters
        .iter()
        .map(|f| f.code.as_str())
        .collect();
    for name in [
        "STY",
        "SAB",
        "TTY",
        "RO",
        "RN",
        "RB",
        "SY",
        "has_ingredient",
        "isa",
    ] {
        assert!(names.contains(&name) || name == "isa", "{name} is a filter");
    }
}

#[test]
fn the_fhir_filters_answer_from_the_indexes_and_the_typed_edges() {
    let (_dir, provider) = provider();
    let scds = provider
        .filter(&filter("TTY", FilterOperator::Equal, "SCD"))
        .expect("filters");
    assert_eq!(codes(&provider, scds), [ASPIRIN_TABLET, OLD_TABLET]);
    let brands = provider
        .filter(&filter("TTY", FilterOperator::In, "BN,SBD"))
        .expect("filters");
    assert_eq!(codes(&provider, brands), [BRAND, BRANDED_TABLET]);
    let labelled = provider
        .filter(&filter("SAB", FilterOperator::Equal, "MTHSPL"))
        .expect("filters");
    assert_eq!(codes(&provider, labelled), [ASPIRIN, ASPIRIN_TABLET]);
    let organic = provider
        .filter(&filter("STY", FilterOperator::Equal, "Organic Chemical"))
        .expect("filters");
    assert_eq!(codes(&provider, organic), [ASPIRIN]);
    let with_aspirin = provider
        .filter(&filter(
            "has_ingredient",
            FilterOperator::Equal,
            &format!("CUI:{ASPIRIN}"),
        ))
        .expect("filters");
    assert_eq!(codes(&provider, with_aspirin), [ASPIRIN_TABLET, OLD_TABLET]);
    let by_atom = provider
        .filter(&filter(
            "has_ingredient",
            FilterOperator::Equal,
            &format!("AUI:{ASPIRIN_SYNONYM_ATOM}"),
        ))
        .expect("filters");
    assert_eq!(codes(&provider, by_atom), [ASPIRIN_TABLET, OLD_TABLET]);
    let related = provider
        .filter(&filter(
            "RO",
            FilterOperator::In,
            &format!("CUI:{ASPIRIN},CUI:{BRAND}"),
        ))
        .expect("filters");
    assert_eq!(
        codes(&provider, related),
        [ASPIRIN_TABLET, BRANDED_TABLET, OLD_TABLET]
    );
    assert!(matches!(
        provider.filter(&filter("has_ingredient", FilterOperator::Equal, ASPIRIN)),
        Err(ProviderError::InvalidFilterValue(_))
    ));
    assert!(matches!(
        provider.filter(&filter("has_ingredient", FilterOperator::Equal, "CUI:1")),
        Err(ProviderError::UnknownCode(_))
    ));
    assert!(matches!(
        provider.filter(&filter("colour", FilterOperator::Equal, "red")),
        Err(ProviderError::UnsupportedFilter { .. })
    ));
    assert!(matches!(
        provider.filter(&filter("concept", FilterOperator::IsA, ASPIRIN)),
        Err(ProviderError::UnsupportedFilter { .. })
    ));
    let ndc = provider
        .filter(&filter("NDC", FilterOperator::Equal, "00000000102"))
        .expect("filters");
    assert_eq!(codes(&provider, ndc), [ASPIRIN_TABLET]);
    assert_eq!(provider.all().expect("all").len(), 6);
    assert_eq!(
        codes(
            &provider,
            provider.search("acetylsal", None).expect("searches")
        ),
        [ASPIRIN]
    );
    assert_eq!(provider.search("81mg", None).expect("searches").len(), 1);
}

#[test]
fn the_one_implicit_value_set_is_all_codes() {
    let (_dir, provider) = provider();
    let all = provider
        .implicit_value_set(&format!("{SYSTEM}/vs"))
        .expect("implicit")
        .expect("compose");
    assert!(all.include[0].filters.is_empty() && all.include[0].concepts.is_empty());
    assert!(
        provider
            .implicit_value_set(&format!("{SYSTEM}/vs/x"))
            .expect("implicit")
            .is_err()
    );
    assert!(provider.implicit_value_set("http://loinc.org/vs").is_none());
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::loinc::write_artifact(dir.path()).expect("builds");
    assert!(matches!(
        RxNormProvider::open(dir.path()),
        Err(OpenError::NotRxNorm(_))
    ));
}
// NOTE: `displayLanguage` "Specifies the language to be used for description
// when validating the display property"
// (<https://hl7.org/fhir/R4B/codesystem-operation-validate-code.html>).
#[test]
fn a_display_in_the_releases_language_does_not_satisfy_a_request_for_another_one() {
    use std::sync::Arc;

    use fhir_terminology::operations::Invocation;
    use fhir_terminology::operations::validate_code::{ValidateCodeInput, validate_code};
    use fhir_terminology::registry::Registry;

    let (_dir, p) = provider();
    assert_eq!(p.language(), Some("en"));
    let mut registry = Registry::new();
    registry.register(Arc::new(p)).expect("registers");
    let ask = |code: &str, display: &str| {
        validate_code(
            &registry,
            &Invocation::Type,
            &ValidateCodeInput {
                url: Some(SYSTEM.to_owned()),
                code: Some(code.to_owned()),
                display: Some(display.to_owned()),
                display_language: Some(String::from("nl")),
                ..ValidateCodeInput::default()
            },
        )
        .expect("validates")
    };

    // Every atom of the release is English: the English display stands, and
    // the answer says the requested language has none.
    let none = ask(ASPIRIN_TABLET, "aspirin 81 MG Oral Tablet");
    assert!(none.result, "{none:?}");
    let issue = none.issues.first().expect("an issue");
    assert_eq!(issue.severity, "information");
    assert_eq!(issue.kind, "invalid-display");
    assert_eq!(
        issue.message_id(),
        "NO_VALID_DISPLAY_FOUND_NONE_FOR_LANG_OK"
    );
    assert!(none.message.is_some(), "{none:?}");

    // A display the release does not carry at all stays an error, and the
    // English display is named as the default.
    let wrong = ask(ASPIRIN_TABLET, "aspirine 81 MG tablet");
    assert!(!wrong.result, "{wrong:?}");
    let issue = wrong.issues.first().expect("an issue");
    assert_eq!(issue.severity, "error");
    assert_eq!(issue.kind, "invalid-display");
    assert_eq!(
        issue.message_id(),
        "NO_VALID_DISPLAY_FOUND_NONE_FOR_LANG_ERR"
    );
    assert!(
        issue.text.contains("aspirin 81 MG Oral Tablet"),
        "{}",
        issue.text
    );
}

#[test]
fn every_declared_implicit_value_set_form_resolves() {
    // The one form of <https://hl7.org/fhir/R4B/rxnorm.html>, "Implicit Value Sets".
    let (_dir, provider) = provider();
    crate::implicit_forms::every_form_resolves(
        &provider,
        &[("http://www.nlm.nih.gov/research/umls/rxnorm/vs", &[])],
    );
}
