// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The classification provider over the testkit's `ClaML` and ICD-10-CM
//! artifacts (<https://hl7.org/fhir/R4B/icd.html>).

use concept_graph::subsumption::Outcome;
use ferroterm_testkit::classification::{
    BILE_DUCT, BLOCK, CHAPTER, CHOLERA, CLAML_SYSTEM, CLAML_VERSION, CLASSICAL, CM_VAULT,
    CM_VAULT_INITIAL, CM_VERSION, LIVER, LIVER_CELL, SKULL, VAULT, VAULT_CLOSED,
    write_claml_artifact, write_icd10cm_artifact,
};
use fhir_terminology::classification::{ClassificationProvider, OpenError};
use fhir_terminology::filter::{Filter, FilterOperator};
use fhir_terminology::provider::{
    Capability, CodeSystemProvider, Concept, ContentMode, HierarchyMeaning, ProviderError,
};

fn claml() -> (tempfile::TempDir, ClassificationProvider) {
    let dir = tempfile::tempdir().expect("tempdir");
    write_claml_artifact(dir.path()).expect("builds");
    let provider = ClassificationProvider::open(dir.path()).expect("opens");
    (dir, provider)
}

fn icd10cm() -> (tempfile::TempDir, ClassificationProvider) {
    let dir = tempfile::tempdir().expect("tempdir");
    write_icd10cm_artifact(dir.path()).expect("builds");
    let provider = ClassificationProvider::open(dir.path()).expect("opens");
    (dir, provider)
}

fn filter(property: &str, op: FilterOperator, value: &str) -> Filter {
    Filter {
        property: property.to_owned(),
        op,
        value: value.to_owned(),
    }
}

fn codes(provider: &ClassificationProvider, set: impl IntoIterator<Item = u32>) -> Vec<String> {
    let mut out: Vec<String> = set
        .into_iter()
        .filter_map(|i| provider.code(Concept::new(i)).expect("reads"))
        .collect();
    out.sort();
    out
}

fn located(provider: &ClassificationProvider, code: &str) -> Concept {
    provider
        .locate(code)
        .expect("reads")
        .expect("a concept with the code")
        .concept
}

#[test]
fn the_identity_and_declaration_follow_the_manifest_and_the_icd_page() {
    let (_dir, provider) = claml();
    assert_eq!(provider.identity().url, CLAML_SYSTEM);
    assert_eq!(provider.identity().version, CLAML_VERSION);
    assert_eq!(
        provider.identity().title.as_deref(),
        Some("ICD-10 Nederlandse vertaling (synthetisch)")
    );
    let declaration = provider.declaration();
    assert_eq!(declaration.content, ContentMode::NotPresent);
    assert!(declaration.case_sensitive);
    assert_eq!(
        declaration.hierarchy_meaning,
        Some(HierarchyMeaning::ClassifiedWith)
    );
    assert_eq!(declaration.languages, ["en", "nl"]);
    assert!(declaration.capabilities.contains(&Capability::Subsumption));
    assert!(declaration.capabilities.contains(&Capability::Enumeration));
    assert!(
        declaration.implicit_forms.is_empty(),
        "the ICD page defines none"
    );
    let filters: Vec<&str> = declaration
        .filters
        .iter()
        .map(|f| f.code.as_str())
        .collect();
    assert!(filters.contains(&"kind"));
    assert!(filters.contains(&"exclusion"));
    assert!(filters.contains(&"usage"));
    assert!(
        provider
            .implicit_value_set(&format!("{CLAML_SYSTEM}/vs"))
            .is_none()
    );
}

#[test]
fn codes_locate_with_the_period_and_display_in_the_requested_language() {
    let (_dir, provider) = claml();
    let cell = located(&provider, LIVER_CELL);
    assert!(
        provider.locate("C220").expect("reads").is_none(),
        "the period is part of the code"
    );
    assert!(
        provider.locate("c22.0").expect("reads").is_none(),
        "case sensitive"
    );
    assert_eq!(
        provider.display(cell, None).expect("reads").as_deref(),
        Some("Levercelcarcinoom"),
        "the classification's language when none is asked"
    );
    assert_eq!(
        provider
            .display(cell, Some("en-GB"))
            .expect("reads")
            .as_deref(),
        Some("Liver cell carcinoma")
    );
    assert_eq!(
        provider
            .display(cell, Some("de"))
            .expect("reads")
            .as_deref(),
        Some("Levercelcarcinoom"),
        "the classification's language when the asked one is missing"
    );
    let designations = provider.designations(cell, None).expect("reads");
    assert_eq!(designations.len(), 3);
    let inclusion = designations
        .iter()
        .find(|d| d.use_.as_ref().is_some_and(|u| u.code == "inclusion"))
        .expect("inclusion term");
    assert_eq!(inclusion.value, "hepatocellulair carcinoom");
    assert_eq!(inclusion.language.as_deref(), Some("nl"));
    assert!(provider.status(cell).expect("reads").active);
    let closed = located(&provider, VAULT_CLOSED);
    assert_eq!(
        provider
            .display(closed, Some("nl"))
            .expect("reads")
            .as_deref(),
        Some("Fractuur van schedeldak, gesloten")
    );
}

#[test]
fn properties_carry_the_kind_notes_usage_and_tree() {
    let (_dir, provider) = claml();
    let liver = located(&provider, LIVER);
    let props: Vec<String> = provider
        .properties(liver)
        .expect("reads")
        .into_iter()
        .map(|p| format!("{}={}", p.code, p.value.as_text()))
        .collect();
    assert!(props.contains(&String::from("kind=category")));
    assert!(props.contains(&String::from(
        "exclusion=secundaire maligne nieuwvorming van lever (C78.7)"
    )));
    assert!(props.contains(&format!("parent={BLOCK}")));
    assert!(props.contains(&format!("child={LIVER_CELL}")));
    assert!(props.contains(&format!("child={BILE_DUCT}")));
    assert!(props.contains(&String::from("inactive=false")));
    let bile: Vec<String> = provider
        .properties(located(&provider, BILE_DUCT))
        .expect("reads")
        .into_iter()
        .map(|p| format!("{}={}", p.code, p.value.as_text()))
        .collect();
    assert!(bile.contains(&String::from("usage=dagger")));
    let closed: Vec<String> = provider
        .properties(located(&provider, VAULT_CLOSED))
        .expect("reads")
        .into_iter()
        .map(|p| format!("{}={}", p.code, p.value.as_text()))
        .collect();
    assert!(closed.contains(&String::from("modifier=0")));
}

#[test]
fn subsumption_and_the_generic_filters_answer_from_the_tree() {
    let (_dir, provider) = claml();
    let hierarchy = provider.hierarchy().expect("a tree");
    let chapter = located(&provider, CHAPTER);
    let cell = located(&provider, LIVER_CELL);
    let skull = located(&provider, SKULL);
    assert_eq!(hierarchy.subsumes(chapter, cell), Outcome::Subsumes);
    assert_eq!(hierarchy.subsumes(cell, chapter), Outcome::SubsumedBy);
    assert_eq!(hierarchy.subsumes(cell, cell), Outcome::Equivalent);
    assert_eq!(hierarchy.subsumes(skull, cell), Outcome::NotSubsumed);
    let under_liver = provider
        .filter(&filter("concept", FilterOperator::IsA, LIVER))
        .expect("filters");
    assert_eq!(
        codes(&provider, under_liver),
        [LIVER, LIVER_CELL, BILE_DUCT]
    );
    let below_liver = provider
        .filter(&filter("concept", FilterOperator::DescendentOf, LIVER))
        .expect("filters");
    assert_eq!(codes(&provider, below_liver), [LIVER_CELL, BILE_DUCT]);
    let leaves = provider
        .filter(&filter("concept", FilterOperator::DescendentLeaf, SKULL))
        .expect("filters");
    assert_eq!(codes(&provider, leaves), ["S02.00", "S02.01", "S02.1"]);
    let chapters = provider
        .filter(&filter("kind", FilterOperator::Equal, "chapter"))
        .expect("filters");
    assert_eq!(codes(&provider, chapters), ["II", "XIII", "XIX"]);
    let daggers = provider
        .filter(&filter("usage", FilterOperator::Exists, "true"))
        .expect("filters");
    assert_eq!(codes(&provider, daggers), [BILE_DUCT]);
    let with_exclusion = provider
        .filter(&filter("exclusion", FilterOperator::Regex, ".*C78.*"))
        .expect("filters");
    assert_eq!(codes(&provider, with_exclusion), [LIVER]);
    assert!(matches!(
        provider.filter(&filter("colour", FilterOperator::Equal, "red")),
        Err(ProviderError::UnsupportedFilter { .. })
    ));
    assert!(matches!(
        provider.filter(&filter("concept", FilterOperator::IsA, "Z99")),
        Err(ProviderError::UnknownCode(_))
    ));
    assert_eq!(provider.all().expect("all").len(), 19);
    let hits = provider.search("schedeldak", Some("nl")).expect("searches");
    assert_eq!(
        codes(&provider, hits),
        [VAULT, "S02.00", "S02.01"],
        "the modified titles carry the stem"
    );
    assert_eq!(
        provider
            .search("vault", Some("en"))
            .expect("searches")
            .len(),
        3
    );
}

#[test]
fn the_icd10cm_artifact_serves_valid_and_the_seventh_character_codes() {
    let (_dir, provider) = icd10cm();
    assert_eq!(provider.identity().url, "http://hl7.org/fhir/sid/icd-10-cm");
    assert_eq!(provider.identity().version, CM_VERSION);
    let cholera = located(&provider, CHOLERA);
    let props: Vec<String> = provider
        .properties(cholera)
        .expect("reads")
        .into_iter()
        .map(|p| format!("{}={}", p.code, p.value.as_text()))
        .collect();
    assert!(props.contains(&String::from("valid=false")));
    assert!(props.contains(&String::from("excludes1=cholera-like illness (A09)")));
    let valid = provider
        .filter(&filter("valid", FilterOperator::Equal, "true"))
        .expect("filters");
    assert_eq!(
        codes(&provider, valid),
        [CLASSICAL, "B10", "S02.0XXA", "S02.0XXB"]
    );
    let initial = located(&provider, CM_VAULT_INITIAL);
    let vault = located(&provider, CM_VAULT);
    assert_eq!(
        provider.hierarchy().expect("tree").subsumes(vault, initial),
        Outcome::Subsumes
    );
    assert_eq!(
        provider.display(initial, None).expect("reads").as_deref(),
        Some("Fracture of vault of skull, initial encounter for closed fracture")
    );
    let short = provider
        .designations(initial, None)
        .expect("reads")
        .into_iter()
        .find(|d| d.use_.as_ref().is_some_and(|u| u.code == "short"))
        .expect("short description");
    assert_eq!(short.value, "Fracture of vault of skull, init for clos fx");
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::loinc::write_artifact(dir.path()).expect("builds");
    assert!(matches!(
        ClassificationProvider::open(dir.path()),
        Err(OpenError::NotClassification(_))
    ));
}

#[test]
fn the_atc_artifact_serves_the_five_levels_with_ddds_as_properties() {
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::atc::write_artifact(dir.path()).expect("builds");
    let provider = ClassificationProvider::open(dir.path()).expect("opens");
    assert_eq!(provider.identity().url, "http://www.whocc.no/atc");
    assert_eq!(provider.identity().version, ferroterm_testkit::atc::VERSION);
    let substance = located(&provider, ferroterm_testkit::atc::SUBSTANCE);
    assert_eq!(
        provider.display(substance, None).expect("reads").as_deref(),
        Some("metforminoid")
    );
    let p: Vec<String> = provider
        .properties(substance)
        .expect("reads")
        .into_iter()
        .map(|p| format!("{}={}", p.code, p.value.as_text()))
        .collect();
    assert!(p.contains(&String::from("kind=chemical-substance")));
    assert!(p.contains(&String::from("ddd=2 g O")));
    assert!(p.contains(&String::from("ddd=1 g P; parenteral form")));
    assert!(p.contains(&format!("parent={}", ferroterm_testkit::atc::CHEMICAL)));
    let group = located(&provider, ferroterm_testkit::atc::GROUP);
    let hierarchy = provider.hierarchy().expect("tree");
    assert_eq!(hierarchy.subsumes(group, substance), Outcome::Subsumes);
    let substances = provider
        .filter(&filter("kind", FilterOperator::Equal, "chemical-substance"))
        .expect("filters");
    assert_eq!(substances.len(), 2);
    let with_ddd = provider
        .filter(&filter("ddd", FilterOperator::Exists, "true"))
        .expect("filters");
    assert_eq!(
        codes(&provider, with_ddd),
        [ferroterm_testkit::atc::SUBSTANCE]
    );
}

#[test]
fn the_dhd_artifact_serves_a_flat_thesaurus_with_dutch_terms() {
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::dhd::write_artifact(dir.path()).expect("builds");
    let provider = ClassificationProvider::open(dir.path()).expect("opens");
    assert_eq!(provider.identity().url, ::dhd_thesaurus::SYSTEM);
    assert_eq!(provider.identity().version, ferroterm_testkit::dhd::VERSION);
    assert!(provider.hierarchy().is_none(), "a flat table");
    let declaration = provider.declaration();
    assert!(!declaration.capabilities.contains(&Capability::Subsumption));
    assert!(
        declaration.properties.iter().all(|p| p.code != "parent"),
        "no parent property without a tree"
    );
    let fracture = located(&provider, ferroterm_testkit::dhd::FRACTURE);
    assert_eq!(
        provider.display(fracture, None).expect("reads").as_deref(),
        Some("fractuur van de synthetische knobbel")
    );
    let designations = provider.designations(fracture, None).expect("reads");
    assert!(
        designations
            .iter()
            .any(|d| d.value == "knobbelfractuur" && d.language.as_deref() == Some("nl-NL"))
    );
    assert!(
        designations
            .iter()
            .any(|d| d.value == "Fracture of synthetic knob (disorder)"
                && d.language.as_deref() == Some("en-GB"))
    );
    let p: Vec<String> = provider
        .properties(fracture)
        .expect("reads")
        .into_iter()
        .map(|p| format!("{}={}", p.code, p.value.as_text()))
        .collect();
    assert!(p.contains(&format!(
        "snomed={}",
        ferroterm_testkit::dhd::FRACTURE_SCTID
    )));
    assert!(p.contains(&String::from("icd10=Z99.0")));
    assert!(p.contains(&String::from("dbc=1234 (0305)")));
    assert!(p.contains(&format!("umbrella={}", ferroterm_testkit::dhd::INJURY)));
    let old = located(&provider, ferroterm_testkit::dhd::OLD);
    assert!(
        !provider.status(old).expect("reads").active,
        "ended concept"
    );
    let with_snomed = provider
        .filter(&filter("snomed", FilterOperator::Exists, "true"))
        .expect("filters");
    assert_eq!(
        codes(&provider, with_snomed),
        [ferroterm_testkit::dhd::FRACTURE]
    );
    assert!(matches!(
        provider.filter(&filter(
            "concept",
            FilterOperator::IsA,
            ferroterm_testkit::dhd::INJURY
        )),
        Err(ProviderError::UnsupportedFilter { .. })
    ));
    let found = provider
        .search("knobbel fract", Some("nl"))
        .expect("searches");
    assert_eq!(codes(&provider, found), [ferroterm_testkit::dhd::FRACTURE]);
}

#[test]
fn the_gstandaard_rungs_serve_flat_with_the_ladder_as_properties() {
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::gstandaard::write_artifacts(dir.path()).expect("builds");
    let articles = ClassificationProvider::open(&dir.path().join("artikel")).expect("opens");
    assert_eq!(articles.identity().url, ::gstandaard::ARTICLE_SYSTEM);
    assert_eq!(
        articles.identity().version,
        ferroterm_testkit::gstandaard::VERSION
    );
    assert!(articles.hierarchy().is_none());
    let article = located(&articles, ferroterm_testkit::gstandaard::ARTICLE);
    assert_eq!(
        articles.display(article, None).expect("reads").as_deref(),
        Some("SYNTHOMET TABLET 500MG 30 STUKS")
    );
    let p: Vec<String> = articles
        .properties(article)
        .expect("reads")
        .into_iter()
        .map(|p| format!("{}={}", p.code, p.value.as_text()))
        .collect();
    assert!(p.contains(&format!("hpk={}", ferroterm_testkit::gstandaard::HPK)));
    assert!(p.contains(&format!("gpk={}", ferroterm_testkit::gstandaard::GPK)));
    let ended = located(&articles, ferroterm_testkit::gstandaard::ENDED_ARTICLE);
    assert!(!articles.status(ended).expect("reads").active);
    assert!(
        articles
            .locate(ferroterm_testkit::gstandaard::REMOVED_ARTICLE)
            .expect("reads")
            .is_none()
    );
    let gpk = ClassificationProvider::open(&dir.path().join("gpk")).expect("opens");
    let generic = located(&gpk, ferroterm_testkit::gstandaard::GPK);
    assert!(
        gpk.designations(generic, None)
            .expect("reads")
            .iter()
            .any(|d| d.value == "metforminoide tablet 500mg")
    );
    let with_atc = gpk
        .filter(&filter(
            "atc",
            FilterOperator::Equal,
            ferroterm_testkit::gstandaard::ATC,
        ))
        .expect("filters");
    assert_eq!(codes(&gpk, with_atc), [ferroterm_testkit::gstandaard::GPK]);
    assert_eq!(
        codes(&gpk, gpk.search("metform", Some("nl")).expect("searches")),
        [ferroterm_testkit::gstandaard::GPK]
    );
}
// NOTE: `displayLanguage` "Specifies the language to be used for description
// when validating the display property"
// (<https://hl7.org/fhir/R4B/codesystem-operation-validate-code.html>).
#[test]
fn a_display_in_the_classifications_language_does_not_satisfy_a_request_for_another_one() {
    use std::sync::Arc;

    use fhir_terminology::operations::Invocation;
    use fhir_terminology::operations::validate_code::{ValidateCodeInput, validate_code};
    use fhir_terminology::registry::Registry;

    let (_dir, p) = claml();
    assert_eq!(p.language(), Some("nl"));
    let mut registry = Registry::new();
    registry.register(Arc::new(p)).expect("registers");
    let ask = |code: &str, display: &str| {
        validate_code(
            &registry,
            &Invocation::Type,
            &ValidateCodeInput {
                url: Some(CLAML_SYSTEM.to_owned()),
                code: Some(code.to_owned()),
                display: Some(display.to_owned()),
                display_language: Some(String::from("en")),
                ..ValidateCodeInput::default()
            },
        )
        .expect("validates")
    };

    // The category carries an English label beside the Dutch one, so the Dutch
    // title is not a display in English and the English one is named.
    let wrong = ask(
        LIVER,
        "Maligne nieuwvorming van lever en intrahepatische galwegen",
    );
    assert!(!wrong.result, "{wrong:?}");
    assert_eq!(
        wrong.display.as_deref(),
        Some("Malignant neoplasm of liver and intrahepatic bile ducts")
    );
    let issue = wrong.issues.first().expect("an issue");
    assert_eq!(issue.severity, "error");
    assert_eq!(issue.kind, "invalid-display");
    assert_eq!(
        issue.message_id(),
        "Display_Name_for__should_be_one_of__instead_of"
    );
    assert!(
        issue
            .text
            .contains("Malignant neoplasm of liver and intrahepatic bile ducts"),
        "{}",
        issue.text
    );
    assert_eq!(wrong.message.as_deref(), Some(issue.text.as_str()));

    // The block carries Dutch alone: the Dutch display stands, and the answer
    // says the requested language has none.
    let none = ask(BLOCK, "Maligne nieuwvormingen");
    assert!(none.result, "{none:?}");
    let issue = none.issues.first().expect("an issue");
    assert_eq!(issue.severity, "information");
    assert_eq!(issue.kind, "invalid-display");
    assert_eq!(
        issue.message_id(),
        "NO_VALID_DISPLAY_FOUND_NONE_FOR_LANG_OK"
    );
    assert!(none.message.is_some(), "{none:?}");
}

#[test]
fn a_classification_declares_no_implicit_value_set_form() {
    let (_dir, provider) = claml();
    crate::implicit_forms::declares_none(&provider, CLAML_SYSTEM);
}
