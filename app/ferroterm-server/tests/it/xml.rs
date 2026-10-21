// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FHIR XML on the wire: `_format`, `Accept`, and `Content-Type` choose the
//! format on every route (<https://hl7.org/fhir/R4B/http.html#mime-type>).

use ferroterm_testkit::fhir::{ANIMALS, VS_ALL, VS_PETS};
use fhir_types::schema::Schemas;
use fhir_types::xml::{from_xml, to_xml};
use http::StatusCode;
use serde_json::{Value, json};

use crate::fixture::Server;

const XML: &str = "application/fhir+xml; charset=utf-8";
const JSON: &str = "application/fhir+json; charset=utf-8";

fn parsed(schemas: &Schemas, body: &str) -> Value {
    let object = from_xml(schemas, body).expect("well-formed FHIR XML");
    let text = serde_json::to_string(&object).expect("the document writes");
    serde_json::from_str(&text).expect("the document parses")
}

fn parameter<'a>(object: &'a Value, name: &str) -> Option<&'a Value> {
    object
        .get("parameter")?
        .as_array()?
        .iter()
        .find(|p| p["name"] == name)
}

#[tokio::test]
async fn format_and_accept_select_xml_on_every_route() {
    let server = Server::start_with_resources();
    let r4b = &fhir_types::r4b::schema::SCHEMAS;
    // `_format=xml` on an operation.
    let (status, content_type, body) = server
        .get_text(
            &format!("/r4b/CodeSystem/$lookup?system={ANIMALS}&code=cat&_format=xml"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(content_type, XML);
    let object = parsed(r4b, &body);
    assert_eq!(object["resourceType"], "Parameters");
    assert_eq!(
        parameter(&object, "display").expect("display")["valueString"],
        "Cat"
    );
    // `Accept` on the capability statement, which lists both formats.
    let (status, content_type, body) = server
        .get_text("/r5/metadata", Some("application/fhir+xml"))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(content_type, XML);
    let object = parsed(&fhir_types::r5::schema::SCHEMAS, &body);
    assert_eq!(object["resourceType"], "CapabilityStatement");
    assert_eq!(
        object["format"],
        json!([
            "application/fhir+json",
            "json",
            "application/fhir+xml",
            "xml"
        ])
    );
    // The full media type in `_format`, on the value set search and read.
    let (status, content_type, body) = server
        .get_text(
            &format!("/r4/ValueSet?url={VS_ALL}&_format=application/fhir%2Bxml"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(content_type, XML);
    let bundle = parsed(&fhir_types::r4::schema::SCHEMAS, &body);
    assert_eq!(bundle["resourceType"], "Bundle");
    assert_eq!(bundle["entry"][0]["resource"]["resourceType"], "ValueSet");
    // `$versions` and `$expand` too.
    let (status, content_type, body) = server.get_text("/r6/$versions?_format=xml", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(content_type, XML);
    assert_eq!(
        parameter(&parsed(&fhir_types::r6::schema::SCHEMAS, &body), "version").expect("version")["valueCode"],
        "6.0"
    );
    let (status, content_type, body) = server
        .get_text(
            &format!("/r5/ValueSet/$expand?url={VS_PETS}"),
            Some("text/xml"),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(content_type, XML);
    let expansion = parsed(&fhir_types::r5::schema::SCHEMAS, &body);
    assert_eq!(expansion["resourceType"], "ValueSet");
    assert!(
        expansion["expansion"]["contains"]
            .as_array()
            .is_some_and(|c| !c.is_empty())
    );
    // JSON stays the default, and `*/*` means JSON.
    let (status, content_type, _) = server
        .get_text(
            &format!("/r4b/CodeSystem/$lookup?system={ANIMALS}&code=cat"),
            Some("*/*"),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, JSON);
}

#[tokio::test]
async fn failures_and_unknown_formats_answer_in_the_negotiated_format() {
    let server = Server::start_with_resources();
    // A failure in XML, when asked for.
    let (status, content_type, body) = server
        .get_text(
            "/r4b/CodeSystem/$lookup?system=http://example.org/nowhere&code=cat&_format=xml",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(content_type, XML);
    let outcome = parsed(&fhir_types::r4b::schema::SCHEMAS, &body);
    assert_eq!(outcome["resourceType"], "OperationOutcome");
    assert_eq!(outcome["issue"][0]["code"], "not-found");
    // An unknown route, by `Accept`.
    let (status, content_type, body) = server
        .get_text("/r4b/Nothing", Some("application/xml"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(content_type, XML, "{body}");
    // A `_format` the server does not speak.
    let (status, content_type, body) = server
        .get_text(
            &format!("/r4b/CodeSystem/$lookup?system={ANIMALS}&code=cat&_format=turtle"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_ACCEPTABLE, "{body}");
    assert_eq!(content_type, JSON);
}

#[tokio::test]
async fn an_xml_parameters_body_is_accepted_and_a_malformed_one_refused() {
    let server = Server::start_with_resources();
    let r4b = &fhir_types::r4b::schema::SCHEMAS;
    let request = json!({"resourceType": "Parameters", "parameter": [
        {"name": "url", "valueUri": VS_PETS},
        {"name": "coding", "valueCoding": {"system": ANIMALS, "code": "kitten", "display": "Kitten"}}
    ]});
    let document: fhir_types::codec::Value =
        serde_json::from_str(&request.to_string()).expect("the request parses");
    let xml = to_xml(r4b, document.as_object().expect("object")).expect("XML");
    // XML in, JSON out by default.
    let (status, content_type, body) = server
        .post_text(
            "/r4b/ValueSet/$validate-code",
            "application/fhir+xml",
            &xml,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(content_type, JSON);
    let answer: Value = serde_json::from_str(&body).expect("JSON");
    assert_eq!(
        crate::fixture::parameter(&answer, "result").expect("result")["valueBoolean"],
        true
    );
    // XML in, XML out when asked; the R5 endpoint reads it through its own schema.
    let (status, content_type, body) = server
        .post_text(
            "/r5/ValueSet/$validate-code?_format=xml",
            "application/fhir+xml; charset=utf-8",
            &xml,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(content_type, XML);
    let answer = parsed(&fhir_types::r5::schema::SCHEMAS, &body);
    assert_eq!(
        parameter(&answer, "result").expect("result")["valueBoolean"],
        true
    );
    // Malformed XML: a `400` in the format `Accept` names.
    let (status, content_type, body) = server
        .post_text(
            "/r4b/ValueSet/$validate-code",
            "application/fhir+xml",
            "<Parameters xmlns=\"http://hl7.org/fhir\"><parameter>",
            Some("application/fhir+xml"),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(content_type, XML);
    let outcome = parsed(r4b, &body);
    assert_eq!(outcome["issue"][0]["code"], "structure");
    // A body of another media type is still refused.
    let (status, _, body) = server
        .post_text("/r4b/ValueSet/$validate-code", "text/plain", "hello", None)
        .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{body}");
}

/// A read, a search, an operation and the capability statement of `version`.
fn routes(server: &Server, version: &str) -> [String; 4] {
    let id = server.instance_id_of(ANIMALS);
    [
        format!("/{version}/CodeSystem/{id}"),
        format!("/{version}/ValueSet?url={VS_ALL}"),
        format!("/{version}/CodeSystem/$lookup?system={ANIMALS}&code=cat"),
        format!("/{version}/metadata"),
    ]
}

#[tokio::test]
async fn an_accept_naming_no_served_format_is_not_acceptable_on_every_route_and_version() {
    // A server that cannot serve any format the client accepts answers `406`
    // (<https://hl7.org/fhir/R4B/http.html#mime-type>), in the default format.
    let server = Server::start_with_resources();
    for version in ["r4", "r4b", "r5", "r6"] {
        for route in routes(&server, version) {
            for accept in ["text/csv", "application/pdf, text/html", "text/*"] {
                let (status, content_type, body) = server.get_text(&route, Some(accept)).await;
                assert_eq!(
                    status,
                    StatusCode::NOT_ACCEPTABLE,
                    "{route} with `Accept: {accept}`: {body}"
                );
                assert_eq!(content_type, JSON, "{route} with `Accept: {accept}`");
                let outcome: Value = serde_json::from_str(&body).expect("JSON");
                assert_eq!(outcome["resourceType"], "OperationOutcome", "{route}");
                assert_eq!(outcome["issue"][0]["code"], "not-supported", "{route}");
            }
        }
    }
}

#[tokio::test]
async fn wildcards_an_absent_accept_and_a_mixed_list_keep_json_on_every_route_and_version() {
    let server = Server::start_with_resources();
    for version in ["r4", "r4b", "r5", "r6"] {
        for route in routes(&server, version) {
            for accept in [
                None,
                Some("*/*"),
                Some("application/*"),
                Some("text/csv, application/fhir+json"),
                Some("text/html,application/xhtml+xml,*/*;q=0.8"),
            ] {
                let (status, content_type, body) = server.get_text(&route, accept).await;
                assert_eq!(
                    status,
                    StatusCode::OK,
                    "{route} with `Accept: {accept:?}`: {body}"
                );
                assert_eq!(content_type, JSON, "{route} with `Accept: {accept:?}`");
            }
        }
    }
}

#[tokio::test]
async fn the_acceptable_range_with_the_highest_quality_wins() {
    // The most specific matching range weighs each format, the heavier format
    // wins, and `q=0` excludes (<https://www.rfc-editor.org/rfc/rfc9110#section-12.5.1>).
    let server = Server::start_with_resources();
    for version in ["r4", "r4b", "r5", "r6"] {
        let route = format!("/{version}/CodeSystem/$lookup?system={ANIMALS}&code=cat");
        for (accept, expected) in [
            ("application/fhir+xml;q=0.9, application/fhir+json", JSON),
            ("application/fhir+json;q=0.5, application/fhir+xml", XML),
            (
                "application/fhir+json; q=0.1, application/fhir+xml; q=0.2",
                XML,
            ),
            (
                "application/fhir+xml;q=1.0, application/fhir+json;q=0.999",
                XML,
            ),
            ("application/fhir+json;q=0, */*", XML),
            (
                "application/fhir+json;fhirVersion=4.0;q=0.3, text/xml;q=0.4",
                XML,
            ),
            ("application/fhir+xml, application/fhir+json", JSON),
        ] {
            let (status, content_type, body) = server.get_text(&route, Some(accept)).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "{route} with `Accept: {accept}`: {body}"
            );
            assert_eq!(content_type, expected, "{route} with `Accept: {accept}`");
        }
        for accept in [
            "application/fhir+json;q=0, application/fhir+xml;q=0",
            "*/*;q=0",
            "application/fhir+json;q=2",
        ] {
            let (status, content_type, body) = server.get_text(&route, Some(accept)).await;
            assert_eq!(
                status,
                StatusCode::NOT_ACCEPTABLE,
                "{route} with `Accept: {accept}`: {body}"
            );
            assert_eq!(content_type, JSON, "{route} with `Accept: {accept}`");
        }
    }
}

#[tokio::test]
async fn format_still_wins_over_an_unacceptable_accept() {
    let server = Server::start_with_resources();
    let (status, content_type, body) = server
        .get_text(
            &format!("/r4b/CodeSystem/$lookup?system={ANIMALS}&code=cat&_format=xml"),
            Some("text/csv"),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(content_type, XML);
}
