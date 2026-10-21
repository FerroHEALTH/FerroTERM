//! Loading the artifacts named by the configuration.

use ferroterm_server::config::Config;
use ferroterm_server::state::{AppState, LoadError, instance_id};
use ferroterm_testkit::snomed::VERSION;

#[test]
fn the_state_loads_every_named_artifact_and_names_its_instances() {
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::snomed::write(dir.path()).expect("writes");
    let config = Config {
        index: vec![dir.path().to_path_buf()],
        ..Config::default()
    };
    let state = AppState::load(&config).expect("loads");
    let instances: Vec<(String, String, String)> = state
        .instances()
        .filter(|(_, url, _)| !url.starts_with("urn:") && *url != "http://unitsofmeasure.org")
        .map(|(a, b, c)| (a.to_owned(), b.to_owned(), c.to_owned()))
        .collect();
    assert_eq!(
        instances,
        [(
            instance_id("http://snomed.info/sct", VERSION),
            String::from("http://snomed.info/sct"),
            String::from(VERSION)
        )]
    );
    assert!(
        state
            .instance(&instance_id("http://snomed.info/sct", VERSION))
            .is_some()
    );
    assert!(state.instance("nope").is_none());
    let summaries = state.summaries().expect("summarises");
    assert_eq!(
        summaries.len(),
        5,
        "the edition and the four registry systems"
    );
    let snomed = summaries
        .iter()
        .find(|s| s.url == "http://snomed.info/sct")
        .expect("snomed");
    assert_eq!(snomed.concepts, Some(21));
    assert_eq!(snomed.languages, ["en", "nl"]);
    assert_eq!(snomed.path.as_deref(), Some(dir.path()));
    let registries: Vec<&str> = summaries
        .iter()
        .filter(|s| s.url.starts_with("urn:"))
        .map(|s| s.url.as_str())
        .collect();
    assert_eq!(
        registries,
        ["urn:ietf:bcp:13", "urn:ietf:bcp:47", "urn:iso:std:iso:3166"]
    );
}

#[test]
fn a_missing_or_duplicate_artifact_refuses_to_start() {
    let missing = Config {
        index: vec![std::path::PathBuf::from("/nonexistent/ferroterm")],
        ..Config::default()
    };
    assert!(matches!(
        AppState::load(&missing),
        Err(LoadError::Artifact { .. })
    ));
    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::snomed::write(dir.path()).expect("writes");
    let twice = Config {
        index: vec![dir.path().to_path_buf(), dir.path().to_path_buf()],
        ..Config::default()
    };
    assert!(matches!(
        AppState::load(&twice),
        Err(LoadError::Register(_))
    ));
    let empty = AppState::load(&Config::default()).expect("no artifacts is a valid server");
    assert!(
        empty
            .instances()
            .all(|(_, url, _)| url.starts_with("urn:") || url == "http://unitsofmeasure.org"),
        "only the registry systems are served without an index"
    );
}

#[test]
fn the_environment_fills_the_config() {
    let config = Config::default();
    assert_eq!(config.listen, "127.0.0.1:8080");
    assert_eq!(config.default_language, "en");
    assert_eq!(config.index, Vec::<std::path::PathBuf>::new());
}

#[test]
fn code_system_directories_load_and_supplements_apply() {
    use ferroterm_testkit::fhir::{ANIMALS, ANIMALS_NL, COLOURS, SKETCH};

    let dir = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::fhir::write_code_systems(dir.path()).expect("writes");
    let config = Config {
        code_systems: vec![dir.path().to_path_buf()],
        ..Config::default()
    };
    let state = AppState::load(&config).expect("loads");
    let urls: Vec<&str> = state
        .instances()
        .map(|(_, url, _)| url)
        .filter(|url| !url.starts_with("urn:") && *url != "http://unitsofmeasure.org")
        .collect();
    assert_eq!(
        urls,
        [ANIMALS, COLOURS, SKETCH],
        "the supplement is no instance"
    );
    assert!(
        state
            .instances()
            .any(|(id, url, version)| url == ANIMALS && id == instance_id(ANIMALS, version)),
        "an instance id carries the system, not only the version"
    );
    let animals = state.provider(ANIMALS).expect("animals");
    let cat = animals.locate("cat").expect("reads").expect("cat").concept;
    // NOTE: a loaded supplement stays dormant until a request names it (#184).
    assert_eq!(
        animals.display(cat, Some("nl")).expect("reads").as_deref(),
        Some("Cat"),
        "the supplement {ANIMALS_NL} is loaded but dormant"
    );
    let layer = state.layer();
    let named = layer
        .registry()
        .with_supplements(&[ANIMALS_NL.to_owned()])
        .expect("the supplement is loaded");
    let supplemented = named.resolve(ANIMALS, None).expect("animals").provider;
    assert_eq!(
        supplemented
            .display(cat, Some("nl"))
            .expect("reads")
            .as_deref(),
        Some("Kat"),
        "named, the supplement {ANIMALS_NL} applies"
    );
    let summaries = state.summaries().expect("summarises");
    let sketch = summaries.iter().find(|s| s.url == SKETCH).expect("sketch");
    assert_eq!(sketch.concepts, None, "example content does not enumerate");
    assert_eq!(sketch.path.as_deref(), Some(dir.path()));
}

#[test]
fn a_supplement_without_its_system_refuses_to_start() {
    let dir = tempfile::tempdir().expect("tempdir");
    let supplement = serde_json::json!({
        "resourceType": "CodeSystem",
        "url": "http://example.org/orphan-nl",
        "status": "active",
        "content": "supplement",
        "supplements": "http://example.org/nowhere",
        "concept": [{"code": "a", "designation": [{"language": "nl", "value": "A"}]}]
    });
    std::fs::write(
        dir.path().join("CodeSystem-orphan.json"),
        supplement.to_string(),
    )
    .expect("writes");
    let config = Config {
        code_systems: vec![dir.path().to_path_buf()],
        ..Config::default()
    };
    assert!(matches!(
        AppState::load(&config),
        Err(LoadError::SupplementTarget { .. })
    ));
}

#[test]
fn a_loinc_artifact_is_served_beside_the_edition() {
    let snomed = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::snomed::write(snomed.path()).expect("writes");
    let loinc = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::loinc::write_artifact(loinc.path()).expect("builds");
    let config = Config {
        index: vec![snomed.path().to_path_buf(), loinc.path().to_path_buf()],
        ..Config::default()
    };
    let state = AppState::load(&config).expect("loads");
    let loinc_summary = state
        .summaries()
        .expect("summarises")
        .into_iter()
        .find(|s| s.url == "http://loinc.org")
        .expect("loinc");
    assert_eq!(loinc_summary.version, ferroterm_testkit::loinc::VERSION);
    assert_eq!(loinc_summary.concepts, Some(11));
    assert_eq!(loinc_summary.path.as_deref(), Some(loinc.path()));
    let empty = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        empty.path().join("manifest.json"),
        r#"{"manifest":2,"system":"http://example.org/other","version":"1"}"#,
    )
    .expect("writes");
    assert!(matches!(
        AppState::load(&Config {
            index: vec![empty.path().to_path_buf()],
            ..Config::default()
        }),
        Err(LoadError::UnknownArtifact { .. })
    ));
}

#[test]
fn a_classification_artifact_is_served_by_its_manifest_kind() {
    let claml = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::classification::write_claml_artifact(claml.path()).expect("builds");
    let cm = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::classification::write_icd10cm_artifact(cm.path()).expect("builds");
    let config = Config {
        index: vec![claml.path().to_path_buf(), cm.path().to_path_buf()],
        ..Config::default()
    };
    let state = AppState::load(&config).expect("loads");
    let summaries = state.summaries().expect("summarises");
    let nl = summaries
        .iter()
        .find(|s| s.url == ferroterm_testkit::classification::CLAML_SYSTEM)
        .expect("icd-10-nl");
    assert_eq!(nl.version, ferroterm_testkit::classification::CLAML_VERSION);
    assert_eq!(nl.concepts, Some(19));
    let cm_summary = summaries
        .iter()
        .find(|s| s.url == "http://hl7.org/fhir/sid/icd-10-cm")
        .expect("icd-10-cm");
    assert_eq!(
        cm_summary.version,
        ferroterm_testkit::classification::CM_VERSION
    );
    assert_eq!(cm_summary.concepts, Some(12));
}

#[test]
fn an_rxnorm_artifact_is_served_beside_the_others() {
    let rxnorm = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::rxnorm::write_artifact(rxnorm.path()).expect("builds");
    let config = Config {
        index: vec![rxnorm.path().to_path_buf()],
        ..Config::default()
    };
    let state = AppState::load(&config).expect("loads");
    let summary = state
        .summaries()
        .expect("summarises")
        .into_iter()
        .find(|s| s.url == "http://www.nlm.nih.gov/research/umls/rxnorm")
        .expect("rxnorm");
    assert_eq!(summary.version, ferroterm_testkit::rxnorm::VERSION);
    assert_eq!(summary.concepts, Some(6));
}

#[test]
fn the_three_icd11_artifacts_are_served_by_their_manifest_kind() {
    let out = tempfile::tempdir().expect("tempdir");
    ferroterm_testkit::icd11::write_artifacts(out.path()).expect("builds");
    let config = Config {
        index: vec![
            out.path().join("mms"),
            out.path().join("icf"),
            out.path().join("entity"),
        ],
        ..Config::default()
    };
    let state = AppState::load(&config).expect("loads");
    let summaries = state.summaries().expect("summarises");
    for (url, concepts) in [
        ("http://id.who.int/icd/release/11/mms", 12),
        ("http://id.who.int/icd/release/11/icf", 5),
        ("http://id.who.int/icd/entity", 3),
    ] {
        let summary = summaries.iter().find(|s| s.url == url).expect(url);
        assert_eq!(summary.version, ferroterm_testkit::icd11::RELEASE);
        assert_eq!(summary.concepts, Some(concepts));
    }
}
