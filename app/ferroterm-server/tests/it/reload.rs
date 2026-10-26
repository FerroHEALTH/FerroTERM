// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reloading the served set without a restart: what a swap picks up, what a
//! refused swap leaves alone, and who may ask for one.
//!
//! No FHIR specification governs a reload, so these are our own design (#578).

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferroterm_server::config::{Config, TerminologySection};
use ferroterm_server::reload::Serving;
use ferroterm_server::state::AppState;
use ferroterm_testkit::fhir::{ANIMALS, SKETCH};
use http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use crate::fixture::json;

/// A serving over `config`, loaded the way the binary loads it.
fn serving(config: Config) -> Serving {
    let state = Arc::new(AppState::load(&config).expect("loads"));
    Serving::new(config, state)
}

async fn get(router: &Router, uri: &str) -> (StatusCode, Value) {
    let request = Request::get(uri).body(Body::empty()).expect("request");
    json(router.clone().oneshot(request).await.expect("response")).await
}

/// One admin request, answered as plain JSON: the admin surface is not FHIR.
async fn admin(router: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json")
    };
    (status, body)
}

async fn scrape(router: &Router) -> String {
    let request = Request::get("/metrics")
        .body(Body::empty())
        .expect("request");
    let response = router.clone().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK, "the scrape answers");
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

/// The systems `GET /r4b/metadata?mode=terminology` names.
async fn terminology_systems(router: &Router) -> Vec<String> {
    let (status, body) = get(router, "/r4b/metadata?mode=terminology").await;
    assert_eq!(status, StatusCode::OK, "the statement answers");
    body.get("codeSystem")
        .and_then(Value::as_array)
        .map(|systems| {
            systems
                .iter()
                .filter_map(|system| system.get("uri").and_then(Value::as_str))
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn a_resource_written_after_the_start_is_served_after_a_reload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let resources = dir.path().join("resources");
    std::fs::create_dir_all(&resources).expect("creates");
    let serving = serving(Config {
        terminology: TerminologySection {
            code_systems: vec![resources.clone()],
            ..TerminologySection::default()
        },
        ..Config::default()
    });
    // The router is built once, as the binary builds it, so what it answers
    // after the swap is what a live server answers.
    let router = ferroterm_server::router(serving.clone());
    assert!(
        !terminology_systems(&router)
            .await
            .contains(&SKETCH.to_owned()),
        "the resource directory is empty at start"
    );

    ferroterm_testkit::fhir::write_code_systems(&resources).expect("writes the resources");
    let served = serving.reload().expect("the new set builds");

    assert!(
        served.iter().any(|system| system.system == SKETCH),
        "the reload answers with the systems it now serves: {served:?}"
    );
    assert!(
        terminology_systems(&router)
            .await
            .contains(&SKETCH.to_owned()),
        "the swapped set is what metadata describes"
    );
    let (status, body) = get(
        &router,
        &format!("/r4b/CodeSystem/$lookup?system={ANIMALS}&code=cat"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        scrape(&router).await.contains(SKETCH),
        "the loaded gauge carries the swapped set"
    );
}

#[tokio::test]
async fn a_removed_resource_leaves_the_served_set_after_a_reload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let resources = dir.path().join("resources");
    std::fs::create_dir_all(&resources).expect("creates");
    ferroterm_testkit::fhir::write_code_systems(&resources).expect("writes the resources");
    let serving = serving(Config {
        terminology: TerminologySection {
            code_systems: vec![resources.clone()],
            ..TerminologySection::default()
        },
        ..Config::default()
    });
    let router = ferroterm_server::router(serving.clone());
    assert!(
        terminology_systems(&router)
            .await
            .contains(&SKETCH.to_owned())
    );

    std::fs::remove_file(resources.join("CodeSystem-sketch.json")).expect("removes");
    let served = serving.reload().expect("the new set builds");

    assert!(
        !served.iter().any(|system| system.system == SKETCH),
        "{served:?}"
    );
    assert!(
        !terminology_systems(&router)
            .await
            .contains(&SKETCH.to_owned())
    );
    assert!(
        !scrape(&router).await.contains(SKETCH),
        "a dropped system leaves the loaded gauge"
    );
    let (status, body) = get(
        &router,
        &format!("/r4b/CodeSystem/$lookup?system={ANIMALS}&code=cat"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the rest of the directory is served throughout: {body}"
    );
}

#[tokio::test]
async fn an_artifact_swapped_into_the_configured_path_is_served_after_a_reload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let index = dir.path().join("edition");
    std::fs::create_dir_all(&index).expect("creates");
    ferroterm_testkit::snomed::write(&index).expect("writes the edition");
    let serving = serving(Config {
        terminology: TerminologySection {
            index: vec![index.clone()],
            ..TerminologySection::default()
        },
        ..Config::default()
    });
    let router = ferroterm_server::router(serving.clone());
    let bird = ferroterm_testkit::snomed::sctid(ferroterm_testkit::snomed::item(
        ferroterm_testkit::snomed::BIRD,
    ));
    let lookup = format!("/r4b/CodeSystem/$lookup?system=http://snomed.info/sct&code={bird}");
    // The system resolves and the code does not, which is an invalid code
    // (<https://hl7.org/fhir/R4B/codesystem-operation-lookup.html>).
    assert_eq!(
        get(&router, &lookup).await.0,
        StatusCode::BAD_REQUEST,
        "the first release does not carry the concept"
    );

    // The later release is built beside the running server and renamed over the
    // configured path: the old directory is unlinked, so the open handles of the
    // set still answering keep reading it
    // (<https://pubs.opengroup.org/onlinepubs/9699919799/functions/rename.html>).
    let staged = dir.path().join("staged");
    std::fs::create_dir_all(&staged).expect("creates");
    ferroterm_testkit::snomed::write_later(&staged).expect("writes the later release");
    std::fs::remove_dir_all(&index).expect("removes");
    std::fs::rename(&staged, &index).expect("renames");
    let served = serving.reload().expect("the new set builds");

    assert!(
        served
            .iter()
            .any(|system| system.version == ferroterm_testkit::snomed::later_version()),
        "{served:?}"
    );
    let (status, body) = get(&router, &lookup).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn a_broken_artifact_refuses_the_reload_and_the_old_set_still_answers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let index = dir.path().join("edition");
    std::fs::create_dir_all(&index).expect("creates");
    ferroterm_testkit::snomed::write(&index).expect("writes the edition");
    let serving = serving(Config {
        terminology: TerminologySection {
            index: vec![index.clone()],
            ..TerminologySection::default()
        },
        ..Config::default()
    });
    let router = ferroterm_server::router(serving.clone());
    let cat = ferroterm_testkit::snomed::sctid(ferroterm_testkit::snomed::item(
        ferroterm_testkit::snomed::CAT,
    ));
    let lookup = format!("/r4b/CodeSystem/$lookup?system=http://snomed.info/sct&code={cat}");
    assert_eq!(get(&router, &lookup).await.0, StatusCode::OK);
    let before = terminology_systems(&router).await;

    std::fs::write(index.join("manifest.json"), b"{ not a manifest").expect("writes");
    let refused = serving
        .reload()
        .expect_err("a damaged artifact does not load");

    let reason = format!("{refused}");
    assert!(reason.contains("does not load"), "{reason}");
    assert_eq!(
        get(&router, &lookup).await.0,
        StatusCode::OK,
        "the old set keeps answering"
    );
    assert_eq!(terminology_systems(&router).await, before);
    let exposition = scrape(&router).await;
    assert!(
        exposition.contains("ferroterm_reloads_total{outcome=\"failed\"} 1"),
        "{exposition}"
    );
    assert!(
        exposition.contains("ferroterm_reloads_total{outcome=\"ok\"} 0"),
        "{exposition}"
    );
}

#[tokio::test]
async fn a_request_in_flight_during_a_swap_answers_from_the_set_it_started_on() {
    let dir = tempfile::tempdir().expect("tempdir");
    let index = dir.path().join("edition");
    let resources = dir.path().join("resources");
    std::fs::create_dir_all(&index).expect("creates");
    std::fs::create_dir_all(&resources).expect("creates");
    ferroterm_testkit::snomed::write(&index).expect("writes the edition");
    let serving = serving(Config {
        terminology: TerminologySection {
            index: vec![index],
            code_systems: vec![resources.clone()],
            ..TerminologySection::default()
        },
        ..Config::default()
    });
    let router = ferroterm_server::router(serving.clone());
    // The handle a handler holds for the length of its request.
    let in_flight = serving.current();

    ferroterm_testkit::fhir::write_code_systems(&resources).expect("writes the resources");
    let swapping = {
        let serving = serving.clone();
        tokio::task::spawn_blocking(move || serving.reload())
    };
    let cat = ferroterm_testkit::snomed::sctid(ferroterm_testkit::snomed::item(
        ferroterm_testkit::snomed::CAT,
    ));
    let lookup = format!("/r4b/CodeSystem/$lookup?system=http://snomed.info/sct&code={cat}");
    let (first, second, third) = tokio::join!(
        get(&router, &lookup),
        get(&router, &lookup),
        get(&router, &lookup)
    );
    swapping.await.expect("the task finishes").expect("swaps");

    for (status, body) in [first, second, third] {
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    assert!(
        !in_flight.instances().any(|(_, url, _)| url == SKETCH),
        "the handle taken before the swap still resolves the old set"
    );
    assert!(
        serving
            .current()
            .instances()
            .any(|(_, url, _)| url == SKETCH),
        "and the set taken after it resolves the new one"
    );
}

#[tokio::test]
async fn the_admin_listener_serves_reload_and_the_fhir_surface_does_not() {
    let dir = tempfile::tempdir().expect("tempdir");
    let index = dir.path().join("edition");
    std::fs::create_dir_all(&index).expect("creates");
    ferroterm_testkit::snomed::write(&index).expect("writes the edition");
    let serving = serving(Config {
        terminology: TerminologySection {
            index: vec![index],
            ..TerminologySection::default()
        },
        ..Config::default()
    });
    let admin_router = ferroterm_server::reload::router(serving.clone());
    let fhir = ferroterm_server::router(serving);

    let request = Request::post("/reload")
        .body(Body::empty())
        .expect("request");
    let (status, body) = admin(&admin_router, request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["outcome"], "ok");
    assert!(
        body["systems"].as_array().is_some_and(|systems| systems
            .iter()
            .any(|system| { system["system"] == "http://snomed.info/sct" })),
        "the answer names the systems now served: {body}"
    );

    let request = Request::get("/reload")
        .body(Body::empty())
        .expect("request");
    let response = admin_router.oneshot(request).await.expect("response");
    assert_eq!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "the admin listener takes a POST"
    );

    for method in [Request::post("/reload"), Request::get("/reload")] {
        let request = method.body(Body::empty()).expect("request");
        let (status, body) = json(fhir.clone().oneshot(request).await.expect("response")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["resourceType"], "OperationOutcome", "{body}");
    }
}

#[tokio::test]
async fn a_refused_reload_answers_the_admin_request_with_the_reason() {
    let dir = tempfile::tempdir().expect("tempdir");
    let index = dir.path().join("edition");
    std::fs::create_dir_all(&index).expect("creates");
    ferroterm_testkit::snomed::write(&index).expect("writes the edition");
    let serving = serving(Config {
        terminology: TerminologySection {
            index: vec![index.clone()],
            ..TerminologySection::default()
        },
        ..Config::default()
    });
    let admin_router = ferroterm_server::reload::router(serving);
    std::fs::write(index.join("manifest.json"), b"{ not a manifest").expect("writes");

    let request = Request::post("/reload")
        .body(Body::empty())
        .expect("request");
    let (status, body) = admin(&admin_router, request).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["outcome"], "failed");
    let reason = body["reason"].as_str().unwrap_or_default();
    assert!(
        reason.contains(&index.display().to_string()),
        "the reason names the artifact: {reason}"
    );
}

#[test]
fn an_unset_address_configures_no_admin_listener() {
    // The process environment is not read: `std::env::set_var` is unsafe in
    // edition 2024 and this workspace forbids unsafe
    // (<https://doc.rust-lang.org/edition-guide/rust-2024/newly-unsafe-functions.html>).
    assert_eq!(Config::default().admin_listen(), None);
    assert_eq!(
        Config::from_sources(None, &std::collections::BTreeMap::new())
            .expect("the defaults load")
            .admin_listen(),
        None,
        "an unset admin.listen means no admin listener"
    );
}
/// The SNOMED CT system every index-root case below serves.
const SNOMED: &str = "http://snomed.info/sct";

/// The versions `GET /r4b/metadata?mode=terminology` names for `system`.
async fn terminology_versions(router: &Router, system: &str) -> Vec<String> {
    let (status, body) = get(router, "/r4b/metadata?mode=terminology").await;
    assert_eq!(status, StatusCode::OK, "the statement answers");
    body.get("codeSystem")
        .and_then(Value::as_array)
        .map(|systems| {
            systems
                .iter()
                .filter(|entry| entry.get("uri").and_then(Value::as_str) == Some(system))
                .filter_map(|entry| entry.get("version").and_then(Value::as_array))
                .flatten()
                .filter_map(|version| version.get("code").and_then(Value::as_str))
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The two releases a root holds once both are in place.
fn both_releases() -> Vec<String> {
    vec![
        String::from(ferroterm_testkit::snomed::VERSION),
        ferroterm_testkit::snomed::later_version(),
    ]
}

/// `$lookup` of the concept only the later release carries.
fn bird_lookup() -> String {
    let bird = ferroterm_testkit::snomed::sctid(ferroterm_testkit::snomed::item(
        ferroterm_testkit::snomed::BIRD,
    ));
    format!("/r4b/CodeSystem/$lookup?system={SNOMED}&code={bird}")
}

/// A root under `dir` holding the first release as a child directory.
fn root_with_first_release(dir: &std::path::Path) -> std::path::PathBuf {
    let root = dir.join("index");
    let first = root.join("20260101");
    std::fs::create_dir_all(&first).expect("creates");
    ferroterm_testkit::snomed::write(&first).expect("writes the edition");
    root
}

/// The later release, written as a child of `root`.
fn later_release_under(root: &std::path::Path) -> std::path::PathBuf {
    let later = root.join("20260301");
    std::fs::create_dir_all(&later).expect("creates");
    ferroterm_testkit::snomed::write_later(&later).expect("writes the later release");
    later
}

#[tokio::test]
async fn an_index_root_serves_every_child_directory_that_holds_a_manifest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = root_with_first_release(dir.path());
    later_release_under(&root);
    // A child the sync has not finished writing, and a file beside the
    // releases: neither holds a manifest, so neither is an artifact.
    std::fs::create_dir_all(root.join("incoming")).expect("creates");
    std::fs::write(root.join("RETENTION"), b"two releases\n").expect("writes");

    let serving = serving(Config {
        terminology: TerminologySection {
            index: vec![root],
            ..TerminologySection::default()
        },
        ..Config::default()
    });
    let router = ferroterm_server::router(serving);

    assert_eq!(
        terminology_versions(&router, SNOMED).await,
        both_releases(),
        "both releases under the root are served"
    );
    let (status, body) = get(&router, &bird_lookup()).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the greatest version is still the default: {body}"
    );
}

#[tokio::test]
async fn a_child_added_under_a_root_is_served_after_a_reload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = root_with_first_release(dir.path());
    let serving = serving(Config {
        terminology: TerminologySection {
            index: vec![root.clone()],
            ..TerminologySection::default()
        },
        ..Config::default()
    });
    let router = ferroterm_server::router(serving.clone());
    assert_eq!(
        get(&router, &bird_lookup()).await.0,
        StatusCode::BAD_REQUEST,
        "the first release does not carry the concept"
    );

    // The release is written beside the root and renamed in, which is one
    // atomic step
    // (<https://pubs.opengroup.org/onlinepubs/9699919799/functions/rename.html>).
    let staged = dir.path().join("staged");
    std::fs::create_dir_all(&staged).expect("creates");
    ferroterm_testkit::snomed::write_later(&staged).expect("writes the later release");
    std::fs::rename(&staged, root.join("20260301")).expect("renames");
    let served = serving.reload().expect("the new set builds");

    assert!(
        served
            .iter()
            .any(|system| system.version == ferroterm_testkit::snomed::later_version()),
        "{served:?}"
    );
    assert_eq!(
        terminology_versions(&router, SNOMED).await,
        both_releases(),
        "the swapped set is what metadata describes"
    );
    let (status, body) = get(&router, &bird_lookup()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn a_child_removed_from_a_root_leaves_the_served_set_after_a_reload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = root_with_first_release(dir.path());
    let later = later_release_under(&root);
    let serving = serving(Config {
        terminology: TerminologySection {
            index: vec![root],
            ..TerminologySection::default()
        },
        ..Config::default()
    });
    let router = ferroterm_server::router(serving.clone());
    assert_eq!(get(&router, &bird_lookup()).await.0, StatusCode::OK);

    std::fs::remove_dir_all(&later).expect("removes");
    let served = serving.reload().expect("the new set builds");

    assert!(
        !served
            .iter()
            .any(|system| system.version == ferroterm_testkit::snomed::later_version()),
        "{served:?}"
    );
    assert_eq!(
        terminology_versions(&router, SNOMED).await,
        vec![String::from(ferroterm_testkit::snomed::VERSION)],
        "the retired release leaves the statement"
    );
    assert_eq!(
        get(&router, &bird_lookup()).await.0,
        StatusCode::BAD_REQUEST,
        "and the concept it carried is served no more"
    );
}

#[tokio::test]
async fn a_child_without_a_manifest_is_ignored_until_the_manifest_lands() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = root_with_first_release(dir.path());
    let serving = serving(Config {
        terminology: TerminologySection {
            index: vec![root.clone()],
            ..TerminologySection::default()
        },
        ..Config::default()
    });
    let router = ferroterm_server::router(serving.clone());

    // A child written in place with its manifest still to come: what a copy
    // into the root looks like halfway through.
    let half = later_release_under(&root);
    let manifest = std::fs::read(half.join("manifest.json")).expect("reads");
    std::fs::remove_file(half.join("manifest.json")).expect("removes");
    serving.reload().expect("the new set builds");

    assert_eq!(
        terminology_versions(&router, SNOMED).await,
        vec![String::from(ferroterm_testkit::snomed::VERSION)],
        "the half-written child is passed over"
    );
    assert_eq!(
        get(&router, &bird_lookup()).await.0,
        StatusCode::BAD_REQUEST
    );

    std::fs::write(half.join("manifest.json"), manifest).expect("writes");
    serving.reload().expect("the new set builds");

    assert_eq!(
        terminology_versions(&router, SNOMED).await,
        both_releases(),
        "and is served once its manifest is there"
    );
    assert_eq!(get(&router, &bird_lookup()).await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_broken_child_under_a_root_refuses_the_reload_and_the_old_set_still_answers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = root_with_first_release(dir.path());
    let later = later_release_under(&root);
    let serving = serving(Config {
        terminology: TerminologySection {
            index: vec![root],
            ..TerminologySection::default()
        },
        ..Config::default()
    });
    let router = ferroterm_server::router(serving.clone());
    let before = terminology_versions(&router, SNOMED).await;

    std::fs::write(later.join("manifest.json"), b"{ not a manifest").expect("writes");
    let refused = serving.reload().expect_err("a damaged child does not load");

    let reason = format!("{refused}");
    assert!(reason.contains("does not load"), "{reason}");
    assert_eq!(
        terminology_versions(&router, SNOMED).await,
        before,
        "the old set keeps answering"
    );
    let (status, body) = get(&router, &bird_lookup()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}
