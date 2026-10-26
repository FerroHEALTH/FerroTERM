// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One canonical resource per `url` and `version`.
//!
//! The `url` and `version` of a canonical resource identify one resource
//! (<https://hl7.org/fhir/R4B/resource.html#canonical>,
//! <https://hl7.org/fhir/R4B/references.html#canonical>), so a write whose
//! pair another resource of the type already carries is refused with 409 and
//! a `duplicate` issue (<https://hl7.org/fhir/R4B/valueset-issue-type.html>).

use ferroterm_server::config::{Config, TerminologySection};
use ferroterm_server::persistence::{Record, ResourceStore};
use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::{self, Server};

/// Every served FHIR base.
const BASES: [&str; 4] = ["r4", "r4b", "r5", "r6"];

/// A complete `CodeSystem` with `url`, `version`, and one concept `code`.
fn code_system(url: &str, version: &str, code: &str) -> Value {
    json!({
        "resourceType": "CodeSystem",
        "url": url,
        "version": version,
        "status": "active",
        "content": "complete",
        "concept": [{"code": code, "display": code}]
    })
}

/// The canonical this test writes on `base`, so the four bases do not share
/// one.
fn url_on(base: &str) -> String {
    format!("http://ferroterm.test/CodeSystem/shared-{base}")
}

/// Asserts that `body` is the `duplicate` refusal naming `holder`.
fn assert_duplicate(status: StatusCode, body: &Value, holder: &str) {
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["resourceType"], "OperationOutcome", "{body}");
    assert_eq!(
        body.pointer("/issue/0/code").and_then(Value::as_str),
        Some("duplicate"),
        "{body}"
    );
    let diagnostics = body
        .pointer("/issue/0/diagnostics")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        diagnostics.contains(holder),
        "the refusal names the resource that holds the canonical: {body}"
    );
}

#[tokio::test]
async fn a_create_onto_a_persisted_canonical_is_refused() {
    let server = Server::start_persisting();
    for base in BASES {
        let url = url_on(base);
        let holder = format!("holder-{base}");
        let response = server
            .put(
                &format!("/{base}/CodeSystem/{holder}"),
                &code_system(&url, "1", "a"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED, "{base}");

        let (status, body) = server
            .post(&format!("/{base}/CodeSystem"), &code_system(&url, "1", "b"))
            .await;
        assert_duplicate(status, &body, &holder);
    }
}

#[tokio::test]
async fn an_update_onto_a_persisted_canonical_is_refused() {
    let server = Server::start_persisting();
    for base in BASES {
        let url = url_on(base);
        let holder = format!("holder-{base}");
        let response = server
            .put(
                &format!("/{base}/CodeSystem/{holder}"),
                &code_system(&url, "1", "a"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED, "{base}");

        // A new id and an existing one are both refused.
        let other = format!("other-{base}");
        let (status, body) = fixture::json(
            server
                .put(
                    &format!("/{base}/CodeSystem/{other}"),
                    &code_system(&url, "1", "b"),
                )
                .await,
        )
        .await;
        assert_duplicate(status, &body, &holder);

        let response = server
            .put(
                &format!("/{base}/CodeSystem/{other}"),
                &code_system(&url, "2", "b"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED, "{base}");
        let (status, body) = fixture::json(
            server
                .put(
                    &format!("/{base}/CodeSystem/{other}"),
                    &code_system(&url, "1", "b"),
                )
                .await,
        )
        .await;
        assert_duplicate(status, &body, &holder);

        let (status, body) = server.get(&format!("/{base}/CodeSystem/{holder}")).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["concept"][0]["code"], "a",
            "the holder is untouched: {body}"
        );
    }
}

#[tokio::test]
async fn the_holder_updates_and_another_version_is_written() {
    let server = Server::start_persisting();
    for base in BASES {
        let url = url_on(base);
        let holder = format!("holder-{base}");
        let path = format!("/{base}/CodeSystem/{holder}");
        assert_eq!(
            server
                .put(&path, &code_system(&url, "1", "a"))
                .await
                .status(),
            StatusCode::CREATED,
            "{base}"
        );
        assert_eq!(
            server
                .put(&path, &code_system(&url, "1", "b"))
                .await
                .status(),
            StatusCode::OK,
            "{base}: the resource that holds the canonical updates it"
        );
        let (status, body) = server
            .post(&format!("/{base}/CodeSystem"), &code_system(&url, "2", "c"))
            .await;
        assert_eq!(
            status,
            StatusCode::CREATED,
            "{base}: the same url under another version is another resource: {body}"
        );
    }
}

#[tokio::test]
async fn a_write_onto_a_loaded_canonical_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let resources = dir.path().to_path_buf();
    std::fs::write(
        resources.join("loaded.json"),
        json!({
            "resourceType": "ValueSet",
            "id": "loaded",
            "url": "https://a.example/vs",
            "version": "1",
            "status": "active",
        })
        .to_string(),
    )
    .expect("writes");
    let server = Server::start_with_config(
        dir,
        Config {
            terminology: TerminologySection {
                code_systems: vec![resources.clone()],
                resources: Some(resources.join("resources.redb")),
                ..TerminologySection::default()
            },
            ..Config::default()
        },
    );
    for base in BASES {
        let written = json!({
            "resourceType": "ValueSet",
            "url": "https://a.example/vs",
            "version": "1",
            "status": "active",
        });
        let (status, body) = server.post(&format!("/{base}/ValueSet"), &written).await;
        assert_duplicate(status, &body, "loaded");
        let (status, body) = fixture::json(
            server
                .put(&format!("/{base}/ValueSet/mine"), &written)
                .await,
        )
        .await;
        assert_duplicate(status, &body, "loaded");

        let mut other = written.clone();
        other["version"] = json!(format!("2-{base}"));
        let (status, body) = fixture::json(
            server
                .put(&format!("/{base}/ValueSet/mine-{base}"), &other)
                .await,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CREATED,
            "{base}: another version of a loaded url is written: {body}"
        );
    }
}

#[tokio::test]
async fn a_stored_duplicate_answers_from_the_most_recent_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let database = dir.path().join("resources.redb");
    let url = "http://ferroterm.test/CodeSystem/stored-twice";
    let store = ResourceStore::open(&database).expect("opens");
    // The older write carries the id that sorts last, so an id order would
    // answer from it.
    for (id, code, written) in [
        ("zz-older", "older", "2026-01-01T00:00:00Z"),
        ("aa-newer", "newer", "2026-02-01T00:00:00Z"),
    ] {
        let mut resource = match fixture::document(&code_system(url, "1", code)) {
            fhir_types::codec::Value::Object(object) => object,
            other => panic!("the body is a resource: {other:?}"),
        };
        resource.insert(
            String::from("id"),
            fhir_types::codec::Value::String(id.to_owned()),
        );
        store
            .put(&Record {
                resource_type: String::from("CodeSystem"),
                id: id.to_owned(),
                url: Some(url.to_owned()),
                version: Some(String::from("1")),
                fhir_version: String::from("4.3.0"),
                version_id: 1,
                last_modified: written.to_owned(),
                resource,
            })
            .expect("writes");
    }
    // `redb` holds the database file for as long as its handle lives
    // (<https://docs.rs/redb/latest/redb/struct.Database.html>).
    drop(store);

    let server = Server::start_with_config(
        dir,
        Config {
            terminology: TerminologySection {
                resources: Some(database),
                ..TerminologySection::default()
            },
            ..Config::default()
        },
    );
    for base in BASES {
        let (status, body) = server
            .get(&format!(
                "/{base}/CodeSystem/$validate-code?url={url}&version=1&code=newer"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(
            parameter(&body, "result")["valueBoolean"],
            true,
            "{base}: the most recent write answers for the canonical: {body}"
        );
        let (status, body) = server
            .get(&format!(
                "/{base}/CodeSystem/$validate-code?url={url}&version=1&code=older"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(
            parameter(&body, "result")["valueBoolean"],
            false,
            "{base}: {body}"
        );
    }
}

/// The `Parameters.parameter` of `name`, or `Value::Null`.
fn parameter(body: &Value, name: &str) -> Value {
    body.get("parameter")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|part| part.get("name").and_then(Value::as_str) == Some(name))
        .cloned()
        .unwrap_or(Value::Null)
}

/// Every served FHIR base with the `version` each resource of its core
/// terminology carries, the version of the package that defines it.
const CORE: [(&str, &str); 4] = [
    ("r4", "4.0.1"),
    ("r4b", "4.3.0"),
    ("r5", "5.0.0"),
    ("r6", "6.0.0-ballot5"),
];

/// A code system the core terminology of every served version defines
/// (<https://hl7.org/fhir/R4B/codesystem-administrative-gender.html>).
const CORE_SYSTEM: &str = "http://hl7.org/fhir/administrative-gender";

/// The value set over it
/// (<https://hl7.org/fhir/R4B/valueset-administrative-gender.html>).
const CORE_VALUE_SET: &str = "http://hl7.org/fhir/ValueSet/administrative-gender";

/// Asserts that `body` is the `duplicate` refusal naming the core terminology
/// of `fhir_version`.
fn assert_core_duplicate(status: StatusCode, body: &Value, fhir_version: &str) {
    assert_duplicate(
        status,
        body,
        &format!("FHIR {fhir_version} core terminology"),
    );
}

#[tokio::test]
async fn a_code_system_onto_a_core_canonical_is_refused_on_every_version() {
    let server = Server::start_persisting();
    for base in BASES {
        // The persisted layer sits over the core layer of every version, so the
        // canonical of any version's core is refused on each base.
        for (_, core_version) in CORE {
            let written = code_system(CORE_SYSTEM, core_version, "local");
            let (status, body) = server.post(&format!("/{base}/CodeSystem"), &written).await;
            assert_core_duplicate(status, &body, core_version);
            let (status, body) = fixture::json(
                server
                    .put(&format!("/{base}/CodeSystem/mine-{base}"), &written)
                    .await,
            )
            .await;
            assert_core_duplicate(status, &body, core_version);
        }
    }
}

#[tokio::test]
async fn a_value_set_onto_a_core_canonical_is_refused_on_every_version() {
    let server = Server::start_persisting();
    for base in BASES {
        for (_, core_version) in CORE {
            let written = json!({
                "resourceType": "ValueSet",
                "url": CORE_VALUE_SET,
                "version": core_version,
                "status": "active",
            });
            let (status, body) = server.post(&format!("/{base}/ValueSet"), &written).await;
            assert_core_duplicate(status, &body, core_version);
            let (status, body) = fixture::json(
                server
                    .put(&format!("/{base}/ValueSet/mine-{base}"), &written)
                    .await,
            )
            .await;
            assert_core_duplicate(status, &body, core_version);
        }
    }
}

#[tokio::test]
async fn a_refused_core_canonical_leaves_the_core_answering() {
    let server = Server::start_persisting();
    for (base, core_version) in CORE {
        let (status, body) = server
            .post(
                &format!("/{base}/CodeSystem"),
                &code_system(CORE_SYSTEM, core_version, "local"),
            )
            .await;
        assert_core_duplicate(status, &body, core_version);
        let (status, body) = server
            .get(&format!(
                "/{base}/CodeSystem/$validate-code?url={CORE_SYSTEM}&code=male"
            ))
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(
            parameter(&body, "result")["valueBoolean"],
            true,
            "{base}: the core code system answers: {body}"
        );
        assert_eq!(
            parameter(&body, "version")["valueString"],
            core_version,
            "{base}: {body}"
        );
    }
}

#[tokio::test]
async fn a_stored_core_canonical_is_not_served_and_is_logged_at_startup() {
    crate::telemetry::admit_every_level();
    let dir = tempfile::tempdir().expect("tempdir");
    let database = dir.path().join("resources.redb");
    let store = ResourceStore::open(&database).expect("opens");
    // One record per served version's core canonical, each defining only a
    // local code, as a store written before the refusal may hold them.
    for (base, core_version) in CORE {
        let id = format!("stored-{base}");
        let mut resource = match fixture::document(&code_system(CORE_SYSTEM, core_version, "local"))
        {
            fhir_types::codec::Value::Object(object) => object,
            other => panic!("the body is a resource: {other:?}"),
        };
        resource.insert(
            String::from("id"),
            fhir_types::codec::Value::String(id.clone()),
        );
        store
            .put(&Record {
                resource_type: String::from("CodeSystem"),
                id,
                url: Some(CORE_SYSTEM.to_owned()),
                version: Some(core_version.to_owned()),
                fhir_version: String::from("4.3.0"),
                version_id: 1,
                last_modified: String::from("2026-01-01T00:00:00Z"),
                resource,
            })
            .expect("writes");
    }
    // `redb` holds the database file for as long as its handle lives
    // (<https://docs.rs/redb/latest/redb/struct.Database.html>).
    drop(store);

    let capture = crate::telemetry::Capture::default();
    let guard = tracing::subscriber::set_default(ferroterm_server::telemetry::subscriber(
        ferroterm_server::telemetry::ResolvedFormat::Json,
        "warn",
        false,
        capture.clone(),
    ));
    let server = Server::start_with_config(
        dir,
        Config {
            terminology: TerminologySection {
                resources: Some(database),
                ..TerminologySection::default()
            },
            ..Config::default()
        },
    );
    drop(guard);

    let lines = capture.lines();
    for (base, core_version) in CORE {
        let id = format!("stored-{base}");
        let canonical = format!("{CORE_SYSTEM}|{core_version}");
        let logged = lines.iter().any(|line| {
            line["level"] == "WARN"
                && line["id"] == id.as_str()
                && line["canonical"] == canonical.as_str()
        });
        assert!(logged, "{base}: the startup log names {id}: {lines:?}");
    }

    for (base, core_version) in CORE {
        for (code, answers) in [("male", true), ("local", false)] {
            let (status, body) = server
                .get(&format!(
                    "/{base}/CodeSystem/$validate-code?url={CORE_SYSTEM}&version={core_version}&code={code}"
                ))
                .await;
            assert_eq!(status, StatusCode::OK, "{base}: {body}");
            assert_eq!(
                parameter(&body, "result")["valueBoolean"],
                answers,
                "{base}: the core code system answers for its canonical, the stored record does not: {body}"
            );
        }
    }
}

/// The `result` and `version` a `CodeSystem/$validate-code` on `base`
/// answers for `code`, at `version` when one is given.
async fn validate_code(
    server: &Server,
    base: &str,
    version: Option<&str>,
    code: &str,
) -> (bool, String) {
    let version = version.map(|v| format!("&version={v}")).unwrap_or_default();
    let (status, body) = server
        .get(&format!(
            "/{base}/CodeSystem/$validate-code?url={CORE_SYSTEM}{version}&code={code}"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{base}: {body}");
    (
        parameter(&body, "result")
            .get("valueBoolean")
            .and_then(Value::as_bool)
            == Some(true),
        parameter(&body, "version")
            .get("valueString")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    )
}

/// A persisted `CodeSystem` under a core `url` with its own `version` is
/// another resource (<https://hl7.org/fhir/R4B/references.html#canonical>):
/// it answers at its version and leaves the core version answering on every
/// base. A version-less request takes the greatest version of either layer,
/// the default one registry holding both would pick: `1.0.0` sorts below
/// every core version and leaves the core answering, `local-1` sorts above.
#[tokio::test]
async fn a_persisted_version_of_a_core_url_leaves_the_core_version_answering() {
    let server = Server::start_persisting();
    let response = server
        .put(
            "/r4/CodeSystem/gender-lower",
            &code_system(CORE_SYSTEM, "1.0.0", "lower-only"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    for (base, core) in CORE {
        assert_eq!(
            validate_code(&server, base, None, "male").await,
            (true, core.to_owned()),
            "{base}: a lower persisted version leaves the core default"
        );
        assert_eq!(
            validate_code(&server, base, Some("1.0.0"), "lower-only").await,
            (true, String::from("1.0.0")),
            "{base}"
        );
    }

    let response = server
        .put(
            "/r4/CodeSystem/gender-local",
            &code_system(CORE_SYSTEM, "local-1", "local-only"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    for (base, core) in CORE {
        assert_eq!(
            validate_code(&server, base, Some("local-1"), "local-only").await,
            (true, String::from("local-1")),
            "{base}: the persisted version answers at its version"
        );
        assert!(
            !validate_code(&server, base, Some("local-1"), "male")
                .await
                .0,
            "{base}"
        );
        assert_eq!(
            validate_code(&server, base, Some(core), "male").await,
            (true, core.to_owned()),
            "{base}: the core version still answers from the core"
        );
        assert!(
            !validate_code(&server, base, Some(core), "local-only")
                .await
                .0,
            "{base}"
        );
        assert_eq!(
            validate_code(&server, base, None, "local-only").await,
            (true, String::from("local-1")),
            "{base}: `local-1` is the greatest version of either layer"
        );
    }
}

/// The `result` a `ValueSet/$validate-code` on `base` answers for the
/// animals `code` in the value set `url`, at `version` when one is given.
async fn in_value_set(
    server: &Server,
    base: &str,
    url: &str,
    version: Option<&str>,
    code: &str,
) -> bool {
    let version = version
        .map(|v| format!("&valueSetVersion={v}"))
        .unwrap_or_default();
    let system = ferroterm_testkit::fhir::ANIMALS;
    let (status, body) = server
        .get(&format!(
            "/{base}/ValueSet/$validate-code?url={url}{version}&system={system}&code={code}"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{base}: {body}");
    parameter(&body, "result")
        .get("valueBoolean")
        .and_then(Value::as_bool)
        == Some(true)
}

/// A `ValueSet` enumerating one animals `code`, at `url` and `version`.
fn enumerated(url: &str, version: &str, code: &str) -> Value {
    json!({
        "resourceType": "ValueSet",
        "url": url,
        "version": version,
        "status": "active",
        "compose": {"include": [{
            "system": ferroterm_testkit::fhir::ANIMALS,
            "concept": [{"code": code}]
        }]}
    })
}

/// The same per-version layering over a value set the deployment loaded: the
/// testkit's pets value set is `1.0`, the pets by `is-a`.
#[tokio::test]
async fn a_persisted_version_of_a_loaded_url_leaves_the_loaded_version_answering() {
    let pets = ferroterm_testkit::fhir::VS_PETS;
    let server = Server::start_persisting();
    let response = server
        .put("/r4/ValueSet/pets-lower", &enumerated(pets, "0.1", "dog"))
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    for base in BASES {
        assert!(
            in_value_set(&server, base, pets, None, "kitten").await,
            "{base}: `0.1` sorts below the loaded `1.0`, which stays the default"
        );
        assert!(
            !in_value_set(&server, base, pets, None, "dog").await,
            "{base}"
        );
        assert!(
            in_value_set(&server, base, pets, Some("0.1"), "dog").await,
            "{base}"
        );
    }

    let response = server
        .put(
            "/r4/ValueSet/pets-local",
            &enumerated(pets, "local-1", "fish"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    for base in BASES {
        assert!(
            in_value_set(&server, base, pets, Some("local-1"), "fish").await,
            "{base}: the persisted version answers at its version"
        );
        assert!(
            !in_value_set(&server, base, pets, Some("local-1"), "kitten").await,
            "{base}"
        );
        assert!(
            in_value_set(&server, base, pets, Some("1.0"), "kitten").await,
            "{base}: the loaded version still answers"
        );
        assert!(
            !in_value_set(&server, base, pets, Some("1.0"), "fish").await,
            "{base}"
        );
        assert!(
            in_value_set(&server, base, pets, None, "fish").await,
            "{base}: `local-1` is the greatest version of either"
        );
    }
}
