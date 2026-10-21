// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Post-coordinated SNOMED CT expressions over HTTP, on every served version.
//!
//! An expression in Compositional Grammar is a valid `code` for
//! `http://snomed.info/sct` (<https://hl7.org/fhir/R4B/snomedct.html>,
//! "Code"), so it reaches the operations as a `code` parameter. The
//! identifiers are the fixture's own: a test ships no SNOMED CT content.

use ferroterm_testkit::snomed::{ANIMAL, CAT, COVERING, FUR, Item, LEGS, item, sctid};
use http::StatusCode;
use serde_json::json;

use crate::fixture::{Server, parameter, parameters};

const SCT: &str = "http://snomed.info/sct";

/// A `Parameters` body for a code operation on one system.
fn body(code: &str, extra: &[(&str, serde_json::Value)]) -> serde_json::Value {
    let mut parts = vec![
        ("url", json!({ "valueUri": SCT })),
        ("code", json!({ "valueCode": code })),
    ];
    parts.extend_from_slice(extra);
    parameters(&parts)
}

/// `<cat> : <covering> = <fur>`, a refined focus concept.
fn refined() -> String {
    format!(
        "{} : {} = {}",
        sctid(item(CAT)),
        sctid(item(COVERING)),
        sctid(item(FUR))
    )
}

#[tokio::test]
async fn validate_code_accepts_an_expression_on_every_served_version() {
    let server = Server::start();
    for base in ["r4", "r4b", "r5", "r6"] {
        let (status, answer) = server
            .post(
                &format!("/{base}/CodeSystem/$validate-code"),
                &body(&refined(), &[]),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {answer}");
        assert_eq!(
            parameter(&answer, "result").unwrap()["valueBoolean"],
            true,
            "{base}: {answer}"
        );
    }
}

#[tokio::test]
async fn an_expression_reaches_the_operations_through_a_percent_encoded_get() {
    let server = Server::start();
    // A `GET` carries the code in the query string, where the grammar's spaces
    // and operators are percent-encoded characters
    // (<https://hl7.org/fhir/R4B/http.html#operations>).
    let encoded: String = refined()
        .bytes()
        .map(|byte| match byte {
            b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'-' | b'.' | b'_' | b'~' => {
                char::from(byte).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect();
    let (status, answer) = server
        .get(&format!(
            "/r4b/CodeSystem/$validate-code?url={SCT}&code={encoded}"
        ))
        .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(parameter(&answer, "result").unwrap()["valueBoolean"], true);
}

#[tokio::test]
async fn validate_code_refuses_a_malformed_expression_with_its_position() {
    let server = Server::start();
    let malformed = format!("{} : {} =", sctid(item(CAT)), sctid(item(COVERING)));
    for base in ["r4b", "r5"] {
        let (status, answer) = server
            .post(
                &format!("/{base}/CodeSystem/$validate-code"),
                &body(&malformed, &[]),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {answer}");
        assert_eq!(
            parameter(&answer, "result").unwrap()["valueBoolean"],
            false,
            "{base}"
        );
        let message = parameter(&answer, "message").unwrap()["valueString"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(
            message.contains("not valid compositional grammar")
                && message.contains(&format!("at byte {}", malformed.len())),
            "{base}: {message}"
        );
    }
}

#[tokio::test]
async fn validate_code_refuses_an_expression_naming_a_concept_the_version_lacks() {
    let server = Server::start();
    let absent = sctid(Item::raw(4242));
    let expression = format!(
        "{} : {} = {absent}",
        sctid(item(CAT)),
        sctid(item(COVERING))
    );
    for base in ["r4b", "r5"] {
        let (status, answer) = server
            .post(
                &format!("/{base}/CodeSystem/$validate-code"),
                &body(&expression, &[]),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {answer}");
        assert_eq!(
            parameter(&answer, "result").unwrap()["valueBoolean"],
            false,
            "{base}"
        );
        let message = parameter(&answer, "message").unwrap()["valueString"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(message.contains(&absent), "{base}: {message}");
    }
}

#[tokio::test]
async fn lookup_answers_the_canonical_expression_and_a_generated_display() {
    let server = Server::start();
    for base in ["r4b", "r5"] {
        let (status, answer) = server
            .post(
                &format!("/{base}/CodeSystem/$lookup"),
                &parameters(&[
                    ("system", json!({ "valueUri": SCT })),
                    ("code", json!({ "valueCode": refined() })),
                ]),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {answer}");
        // "If no term or description template has been published, the full
        // expression with terms embedded may be used"
        // (<https://hl7.org/fhir/R4B/snomedct.html>, "Display").
        assert_eq!(
            parameter(&answer, "display").unwrap()["valueString"],
            format!(
                "=== {} |Cat| : {} |Has covering| = {} |Fur|",
                sctid(item(CAT)),
                sctid(item(COVERING)),
                sctid(item(FUR))
            ),
            "{base}"
        );
    }
}

#[tokio::test]
async fn subsumes_accepts_an_expression_on_either_side() {
    let server = Server::start();
    let broad = format!(
        "=== {} : {} = {}",
        sctid(item(ANIMAL)),
        sctid(item(COVERING)),
        sctid(item(FUR))
    );
    let narrow = format!(
        "=== {} : {{ {} = {}, {} = #4 }}",
        sctid(item(ANIMAL)),
        sctid(item(COVERING)),
        sctid(item(FUR)),
        sctid(item(LEGS))
    );
    for base in ["r4b", "r5"] {
        let (status, answer) = server
            .post(
                &format!("/{base}/CodeSystem/$subsumes"),
                &parameters(&[
                    ("system", json!({ "valueUri": SCT })),
                    ("codeA", json!({ "valueCode": broad.clone() })),
                    ("codeB", json!({ "valueCode": narrow.clone() })),
                ]),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {answer}");
        assert_eq!(
            parameter(&answer, "outcome").unwrap()["valueCode"],
            "subsumes",
            "{base}: {answer}"
        );
        let (_, answer) = server
            .post(
                &format!("/{base}/CodeSystem/$subsumes"),
                &parameters(&[
                    ("system", json!({ "valueUri": SCT })),
                    ("codeA", json!({ "valueCode": broad.clone() })),
                    ("codeB", json!({ "valueCode": sctid(item(CAT)) })),
                ]),
            )
            .await;
        assert_eq!(
            parameter(&answer, "outcome").unwrap()["valueCode"],
            "subsumes",
            "{base}: an expression subsumes a precoordinated concept"
        );
    }
}

#[tokio::test]
async fn closure_relates_an_expression_to_a_concept_the_table_holds() {
    let server = Server::start_persisting();
    let expression = format!(
        "=== {} : {} = {}",
        sctid(item(ANIMAL)),
        sctid(item(COVERING)),
        sctid(item(FUR))
    );
    let coding = |code: String| json!({ "valueCoding": { "system": SCT, "code": code } });
    // R4B states the relationship as `equivalence` and R5 as `relationship`
    // (<https://hl7.org/fhir/R4B/valueset-concept-map-equivalence.html>,
    // <https://hl7.org/fhir/R5/valueset-concept-map-relationship.html>).
    for (base, broader) in [
        ("r4b", "specializes"),
        ("r5", "source-is-broader-than-target"),
    ] {
        let name = format!("expressions-{base}");
        let table = json!({ "valueString": name });
        let (status, answer) = server
            .post(
                &format!("/{base}/$closure"),
                &parameters(&[("name", table.clone())]),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {answer}");
        let (status, answer) = server
            .post(
                &format!("/{base}/$closure"),
                &parameters(&[
                    ("name", table.clone()),
                    ("concept", coding(sctid(item(CAT)))),
                ]),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {answer}");
        let (status, answer) = server
            .post(
                &format!("/{base}/$closure"),
                &parameters(&[("name", table), ("concept", coding(expression.clone()))]),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {answer}");
        let element = &answer["group"][0]["element"][0];
        assert_eq!(element["code"], expression, "{base}: {answer}");
        assert_eq!(element["target"][0]["code"], sctid(item(CAT)), "{base}");
        let stated = serde_json::to_string(&element["target"][0]).unwrap();
        assert!(
            stated.contains(broader),
            "{base}: the expression is broader than the cat the table holds: {stated}"
        );
    }
}
