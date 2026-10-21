// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The history interaction on the persisted resources, at the instance and
//! type levels (<https://hl7.org/fhir/R4B/http.html#history>).
//!
//! The answer is a `Bundle` of type `history` with one entry per version,
//! newest first, `entry.request` naming the interaction that made the version
//! and `entry.response` its status, `ETag`, and `lastModified`; the delete is an
//! entry with no resource.

use axum::body::Body;
use http::{Request, StatusCode};
use serde_json::{Value, json};

use crate::fixture::{self, Server};

/// Every served FHIR base.
const BASES: [&str; 4] = ["r4", "r4b", "r5", "r6"];

/// A complete `CodeSystem` holding one concept `code`.
fn code_system(url: &str, code: &str) -> Value {
    json!({
        "resourceType": "CodeSystem",
        "url": url,
        "version": "1.0",
        "status": "active",
        "content": "complete",
        "concept": [{"code": code, "display": code}]
    })
}

/// A `ValueSet` over one system.
fn value_set(url: &str) -> Value {
    json!({
        "resourceType": "ValueSet",
        "url": url,
        "version": "1.0",
        "status": "active",
        "compose": {"include": [{"system": "http://ferroterm.test/CodeSystem/any"}]}
    })
}

/// Sends a `DELETE` and answers its status.
async fn delete(server: &Server, uri: &str) -> StatusCode {
    let request = Request::delete(uri).body(Body::empty()).expect("request");
    server.send(request).await.status()
}

/// The entries of a Bundle.
fn entries(body: &Value) -> Vec<Value> {
    body.get("entry")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The text at `pointer` in `value`, or the empty string.
fn text<'a>(value: &'a Value, pointer: &str) -> &'a str {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// The interaction codes the capability statement declares for `resource_type`.
fn interactions(body: &Value, resource_type: &str) -> Vec<String> {
    body.pointer("/rest/0/resource")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|resource| resource.get("type").and_then(Value::as_str) == Some(resource_type))
        .flat_map(|resource| {
            resource
                .get("interaction")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|interaction| {
            interaction
                .get("code")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

#[tokio::test]
async fn an_instance_history_lists_every_version_and_the_delete_newest_first() {
    let server = Server::start_persisting();
    for base in BASES {
        let id = format!("colours-{base}");
        let url = format!("http://ferroterm.test/CodeSystem/history-{base}");
        let path = format!("/{base}/CodeSystem/{id}");
        assert_eq!(
            server.put(&path, &code_system(&url, "red")).await.status(),
            StatusCode::CREATED
        );
        assert_eq!(
            server.put(&path, &code_system(&url, "blue")).await.status(),
            StatusCode::OK
        );
        assert_eq!(delete(&server, &path).await, StatusCode::NO_CONTENT);

        let (status, body) = server.get(&format!("{path}/_history")).await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body["resourceType"], "Bundle", "{base}: {body}");
        assert_eq!(body["type"], "history", "{base}: {body}");
        assert_eq!(body["total"], 3, "{base}: {body}");
        assert_eq!(
            text(&body, "/link/0/url"),
            format!("http://{}{path}/_history", fixture::AUTHORITY),
            "{base}: the self link is the URL the history was read at: {body}"
        );
        let listed = entries(&body);
        let methods: Vec<&str> = listed
            .iter()
            .map(|entry| text(entry, "/request/method"))
            .collect();
        assert_eq!(methods, ["DELETE", "PUT", "PUT"], "{base}: {body}");
        let etags: Vec<&str> = listed
            .iter()
            .map(|entry| text(entry, "/response/etag"))
            .collect();
        assert_eq!(etags, ["W/\"3\"", "W/\"2\"", "W/\"1\""], "{base}: {body}");
        let statuses: Vec<&str> = listed
            .iter()
            .map(|entry| text(entry, "/response/status"))
            .collect();
        assert_eq!(
            statuses,
            ["204 No Content", "200 OK", "201 Created"],
            "{base}: {body}"
        );
        for entry in &listed {
            assert!(
                text(entry, "/response/lastModified").contains('T'),
                "{base}: every entry states when it was written: {entry}"
            );
            assert_eq!(
                text(entry, "/request/url"),
                format!("CodeSystem/{id}"),
                "{base}: {entry}"
            );
            assert_eq!(
                text(entry, "/fullUrl"),
                format!("http://{}/{base}/CodeSystem/{id}", fixture::AUTHORITY),
                "{base}: {entry}"
            );
        }
        let (deleted, updated, created) = (&listed[0], &listed[1], &listed[2]);
        assert!(
            deleted.get("resource").is_none(),
            "{base}: a delete carries no resource: {deleted}"
        );
        assert_eq!(updated["resource"]["concept"][0]["code"], "blue", "{base}");
        assert_eq!(updated["resource"]["meta"]["versionId"], "2", "{base}");
        assert_eq!(created["resource"]["concept"][0]["code"], "red", "{base}");

        // The delete is a version of its own, which a version read answers
        // with `410` (<https://hl7.org/fhir/R4B/http.html#vread>).
        let (status, body) = server.get(&format!("{path}/_history/3")).await;
        assert_eq!(status, StatusCode::GONE, "{base}: {body}");
    }
}

#[tokio::test]
async fn a_resource_written_again_after_a_delete_keeps_counting_its_versions() {
    let server = Server::start_persisting();
    for base in BASES {
        let path = format!("/{base}/ValueSet/again-{base}");
        let url = format!("http://ferroterm.test/ValueSet/again-{base}");
        server.put(&path, &value_set(&url)).await;
        assert_eq!(delete(&server, &path).await, StatusCode::NO_CONTENT);
        let response = server.put(&path, &value_set(&url)).await;
        assert_eq!(response.status(), StatusCode::CREATED, "{base}");
        assert_eq!(
            fixture::header(&response, http::header::ETAG).as_deref(),
            Some("W/\"3\""),
            "{base}: the write after the delete is the third version"
        );

        let (status, body) = server.get(&format!("{path}/_history")).await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        let listed = entries(&body);
        let summary: Vec<(&str, &str)> = listed
            .iter()
            .map(|entry| {
                (
                    text(entry, "/request/method"),
                    text(entry, "/response/status"),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("PUT", "201 Created"),
                ("DELETE", "204 No Content"),
                ("PUT", "201 Created")
            ],
            "{base}: {body}"
        );
        let (status, first) = server.get(&format!("{path}/_history/1")).await;
        assert_eq!(status, StatusCode::OK, "{base}: {first}");
        assert_eq!(
            first["meta"]["versionId"], "1",
            "{base}: version 1 survives"
        );
    }
}

#[tokio::test]
async fn a_create_by_post_is_listed_as_a_post_to_the_type() {
    let server = Server::start_persisting();
    for base in BASES {
        let url = format!("http://ferroterm.test/ValueSet/posted-{base}");
        let (status, created) = server
            .post(&format!("/{base}/ValueSet"), &value_set(&url))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{base}: {created}");
        let id = text(&created, "/id").to_owned();

        let (status, body) = server.get(&format!("/{base}/ValueSet/{id}/_history")).await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        let listed = entries(&body);
        assert_eq!(listed.len(), 1, "{base}: {body}");
        assert_eq!(text(&listed[0], "/request/method"), "POST", "{base}");
        assert_eq!(text(&listed[0], "/request/url"), "ValueSet", "{base}");
        assert_eq!(
            text(&listed[0], "/response/status"),
            "201 Created",
            "{base}"
        );
    }
}

#[tokio::test]
async fn a_type_history_lists_every_resource_of_the_type() {
    let server = Server::start_persisting();
    for base in BASES {
        for name in ["first", "second"] {
            let path = format!("/{base}/ConceptMap/{name}-{base}");
            let map = json!({
                "resourceType": "ConceptMap",
                "url": format!("http://ferroterm.test/ConceptMap/{name}-{base}"),
                "version": "1.0",
                "status": "active"
            });
            assert_eq!(
                server.put(&path, &map).await.status(),
                StatusCode::CREATED,
                "{base}"
            );
        }
    }
    for base in BASES {
        let (status, body) = server.get(&format!("/{base}/ConceptMap/_history")).await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body["type"], "history", "{base}");
        assert_eq!(
            body["total"], 8,
            "{base}: every map written on every base: {body}"
        );
        assert_eq!(
            text(&body, "/link/0/url"),
            format!("http://{}/{base}/ConceptMap/_history", fixture::AUTHORITY),
            "{base}: {body}"
        );
        let times: Vec<String> = entries(&body)
            .iter()
            .map(|entry| text(entry, "/response/lastModified").to_owned())
            .collect();
        let parsed: Vec<jiff::Timestamp> = times
            .iter()
            .map(|time| time.parse().expect("an instant"))
            .collect();
        assert!(
            parsed.windows(2).all(|pair| pair[0] >= pair[1]),
            "{base}: newest first: {times:?}"
        );
    }
}

#[tokio::test]
async fn since_keeps_the_versions_written_at_or_after_the_instant() {
    let server = Server::start_persisting();
    for base in BASES {
        let path = format!("/{base}/CodeSystem/since-{base}");
        let url = format!("http://ferroterm.test/CodeSystem/since-{base}");
        for code in ["a", "b", "c"] {
            server.put(&path, &code_system(&url, code)).await;
        }
        let (_, body) = server.get(&format!("{path}/_history")).await;
        let second = text(&entries(&body)[1], "/response/lastModified").to_owned();

        let (status, body) = server
            .get(&format!("{path}/_history?_since={second}"))
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        let listed = entries(&body);
        let etags: Vec<&str> = listed
            .iter()
            .map(|entry| text(entry, "/response/etag"))
            .collect();
        assert_eq!(etags, ["W/\"3\"", "W/\"2\""], "{base}: {body}");

        let (status, body) = server
            .get(&format!("/{base}/CodeSystem/_history?_since={second}"))
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        let floor: jiff::Timestamp = second.parse().expect("an instant");
        assert!(
            entries(&body).iter().all(|entry| {
                text(entry, "/response/lastModified")
                    .parse::<jiff::Timestamp>()
                    .is_ok_and(|at| at >= floor)
            }),
            "{base}: {body}"
        );
        assert!(
            entries(&body)
                .iter()
                .any(|entry| text(entry, "/response/etag") == "W/\"3\""
                    && text(entry, "/fullUrl").ends_with(&format!("since-{base}"))),
            "{base}: the type level narrows the same way: {body}"
        );
        assert!(
            !entries(&body)
                .iter()
                .any(|entry| text(entry, "/response/etag") == "W/\"1\""
                    && text(entry, "/fullUrl").ends_with(&format!("since-{base}"))),
            "{base}: the first version is before the instant: {body}"
        );
    }
}

#[tokio::test]
async fn a_loaded_resource_and_an_unpersisting_type_answer_an_empty_history() {
    let server = Server::start_persisting();
    let loaded = server.snomed_id();
    for base in BASES {
        let (status, body) = server
            .get(&format!("/{base}/CodeSystem/{loaded}/_history"))
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body["type"], "history", "{base}");
        assert_eq!(body["total"], 0, "{base}: {body}");

        let (status, body) = server
            .get(&format!("/{base}/CodeSystem/never-written/_history"))
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{base}: {body}");
    }

    let plain = Server::start_with_resources();
    for base in BASES {
        let (status, body) = plain.get(&format!("/{base}/ValueSet/_history")).await;
        assert_eq!(status, StatusCode::OK, "{base}: {body}");
        assert_eq!(body["total"], 0, "{base}: {body}");
    }
}

#[tokio::test]
async fn a_version_another_release_cannot_read_keeps_its_response_and_says_why() {
    let server = Server::start_persisting();
    let map = json!({
        "resourceType": "ConceptMap",
        "url": "http://ferroterm.test/ConceptMap/r5-only",
        "version": "1.0",
        "status": "active",
        "group": [{
            "source": "http://ferroterm.test/CodeSystem/a",
            "target": "http://ferroterm.test/CodeSystem/b",
            // R5 states a target's direction as `relationship`, which R4B
            // does not define (<https://hl7.org/fhir/R5/conceptmap.html>).
            "element": [{"code": "a", "target": [{"code": "b", "relationship": "equivalent"}]}]
        }]
    });
    let path = "/r5/ConceptMap/r5-only";
    assert_eq!(server.put(path, &map).await.status(), StatusCode::CREATED);

    let (status, body) = server.get(&format!("{path}/_history")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(text(&entries(&body)[0], "/request/method"), "PUT", "{body}");

    let (status, body) = server.get("/r4b/ConceptMap/r5-only/_history").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let listed = entries(&body);
    assert_eq!(listed.len(), 1, "{body}");
    let entry = &listed[0];
    // A `PUT` entry SHALL carry its resource
    // (<https://hl7.org/fhir/R4B/http.html#history>), so the version keeps
    // only its response, with the reason in `response.outcome`.
    assert!(entry.get("resource").is_none(), "{entry}");
    assert!(entry.get("request").is_none(), "{entry}");
    assert_eq!(text(entry, "/response/etag"), "W/\"1\"", "{entry}");
    assert_eq!(
        text(entry, "/response/outcome/issue/0/code"),
        "not-supported",
        "{entry}"
    );
    assert!(
        text(entry, "/response/outcome/issue/0/diagnostics").contains("5.0.0"),
        "the outcome names the release the version reads as: {entry}"
    );
}

#[tokio::test]
async fn a_history_parameter_this_server_does_not_answer_is_refused() {
    let server = Server::start_persisting();
    for base in BASES {
        for query in ["_count=1", "_at=2026", "_list=x", "url=x"] {
            let (status, body) = server
                .get(&format!("/{base}/ValueSet/_history?{query}"))
                .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{base} {query}: {body}");
            assert_eq!(
                text(&body, "/issue/0/code"),
                "not-supported",
                "{base} {query}: {body}"
            );
        }
        // `_since` is an `instant`, which carries a time zone
        // (<https://hl7.org/fhir/R4B/datatypes.html#instant>).
        let (status, body) = server
            .get(&format!("/{base}/ValueSet/_history?_since=2026-01-01"))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{base}: {body}");
    }
}

#[tokio::test]
async fn the_capability_statement_declares_history_exactly_where_it_declares_vread() {
    let persisting = Server::start_persisting();
    let plain = Server::start_with_resources();
    for base in BASES {
        for resource_type in ["CodeSystem", "ValueSet", "ConceptMap"] {
            for (server, persists) in [(&persisting, true), (&plain, false)] {
                let (status, body) = server.get(&format!("/{base}/metadata")).await;
                assert_eq!(status, StatusCode::OK, "{base}: {body}");
                let codes = interactions(&body, resource_type);
                assert_eq!(
                    codes.contains(&String::from("vread")),
                    persists,
                    "{base} {resource_type}: {codes:?}"
                );
                for history in ["history-instance", "history-type"] {
                    assert_eq!(
                        codes.contains(&String::from(history)),
                        persists,
                        "{base} {resource_type}: {history} beside vread: {codes:?}"
                    );
                }
            }
        }
    }
}
