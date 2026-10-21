// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A loaded `CodeSystem` and `ConceptMap` keep `publisher`, `description` and
//! `experimental` on a read and a search, on every served version: each one
//! defines the three on both resources
//! (<https://hl7.org/fhir/R4/codesystem.html>, <https://hl7.org/fhir/R5/conceptmap.html>).

use http::StatusCode;
use serde_json::Value;

use crate::fixture::Server;

/// What the journey fixtures under `e2e/fixtures/codesystems/` state.
const PUBLISHER: &str = "FerroTERM";

#[tokio::test]
async fn a_loaded_code_system_and_concept_map_keep_their_descriptive_elements() {
    let server = Server::start_with_authored_ids();
    let resources = [
        (
            "CodeSystem",
            "e2e-taxonomy",
            "https://ferroterm.eu/fhir/CodeSystem/e2e-taxonomy",
        ),
        (
            "ConceptMap",
            "e2e-taxonomy-map",
            "https://ferroterm.eu/fhir/ConceptMap/e2e-taxonomy-map",
        ),
    ];
    for path in ["r4", "r4b", "r5", "r6"] {
        for (resource, id, url) in resources {
            let (status, read) = server.get(&format!("/{path}/{resource}/{id}")).await;
            assert_eq!(status, StatusCode::OK, "{read}");
            let (status, search) = server.get(&format!("/{path}/{resource}?url={url}")).await;
            assert_eq!(status, StatusCode::OK, "{search}");
            let found = &search["entry"][0]["resource"];
            for (how, body) in [("read", &read), ("search", found)] {
                assert_eq!(
                    body["publisher"], PUBLISHER,
                    "{path} {resource} {how}: {body}"
                );
                assert_eq!(
                    body["experimental"], true,
                    "{path} {resource} {how}: {body}"
                );
                assert!(
                    body["description"]
                        .as_str()
                        .is_some_and(|d| d.starts_with("Synthetic content")),
                    "{path} {resource} {how}: {body}"
                );
            }
        }
    }
}

#[test]
fn the_fixtures_state_what_the_test_reads() {
    for name in ["taxonomy", "conceptmap"] {
        let path = format!(
            "{}/../../e2e/fixtures/codesystems/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(&path).expect("the fixture reads");
        let fixture: Value = serde_json::from_str(&text).expect("the fixture parses");
        assert_eq!(fixture["publisher"], PUBLISHER, "{path}");
        assert_eq!(fixture["experimental"], true, "{path}");
    }
}
