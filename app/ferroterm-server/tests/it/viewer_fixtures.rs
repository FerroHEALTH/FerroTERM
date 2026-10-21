// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The guard over `app/ferroterm-viewer/fixtures`, and the recorder that
//! writes them.
//!
//! The viewer's unit tests read those files with `include_str!`, so they pin
//! the viewer's capability reading against what this server sends. Nothing
//! else checks that the recording still matches the render, which is what this
//! module is: one test that fails when it does not, and the same test in
//! recording mode when the render changed on purpose.
//!
//! Recording is one command:
//!
//! ```text
//! FERROTERM_RECORD_VIEWER_FIXTURES=1 \
//!   cargo nextest run -p ferroterm-server -E 'test(the_viewer_capability_fixtures)'
//! ```

use std::path::Path;
use std::path::PathBuf;

use http::StatusCode;
use serde_json::Value;

use crate::fixture::Server;

/// Names the recording mode. Set, the test writes the files instead of
/// asserting on them.
const RECORD_ENV: &str = "FERROTERM_RECORD_VIEWER_FIXTURES";

/// The roots the viewer reads, as the segment each is mounted under.
const ROOTS: [&str; 4] = ["r4", "r4b", "r5", "r6"];

/// The two documents recorded per root: the file stem, and the `metadata`
/// query that answers it.
const DOCUMENTS: [(&str, &str); 2] = [
    ("capability-statement", ""),
    ("terminology-capabilities", "?mode=terminology"),
];

/// The element a recording cannot reproduce, because it is the moment of the
/// recording.
///
/// `CapabilityStatement.date` and `TerminologyCapabilities.date` are the
/// publication instant of the statement the server just rendered
/// (<https://hl7.org/fhir/R4B/capabilitystatement.html>), so they differ
/// between two recordings of an unchanged render. Nothing else here moves on
/// its own: the software name, the version, and the release date are stamped
/// from the build, and a bump is a real change to what the fixture records.
const WALL_CLOCK: &str = "date";

/// The value a normalized `date` reads as, so a diff never opens on it.
const RECORDED_AT: &str = "(the moment of the recording)";

/// Where the viewer keeps the files.
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ferroterm-viewer/fixtures")
        .canonicalize()
        .expect("the viewer's fixture directory is beside this crate")
}

/// `document` with its wall-clock element replaced by a fixed sentence.
fn normalized(document: &Value) -> Value {
    let mut document = document.clone();
    if let Some(object) = document.as_object_mut()
        && object.contains_key(WALL_CLOCK)
    {
        object.insert(WALL_CLOCK.to_owned(), Value::String(RECORDED_AT.to_owned()));
    }
    document
}

/// `document` as the file holds it: pretty-printed, one trailing newline.
fn recorded(document: &Value) -> String {
    let mut text = serde_json::to_string_pretty(document).expect("the document writes");
    text.push('\n');
    text
}

/// Every document the viewer reads, as (file name, body) in reading order.
async fn rendered(server: &Server) -> Vec<(String, Value)> {
    let mut documents = Vec::new();
    for root in ROOTS {
        for (stem, query) in DOCUMENTS {
            let uri = format!("/{root}/metadata{query}");
            let (status, body) = server.get(&uri).await;
            assert_eq!(
                status,
                StatusCode::OK,
                "{uri} answers the document the viewer records"
            );
            documents.push((format!("{stem}-{root}.json"), body));
        }
    }
    documents
}

/// The committed fixtures still are what the server renders.
///
/// With [`RECORD_ENV`] set the same run writes them instead, so an intended
/// change to the capability render is re-recorded by one command rather than
/// by a test written and deleted for the occasion.
#[tokio::test]
async fn the_viewer_capability_fixtures_match_what_the_server_renders() {
    let server = Server::start_with_every_loader();
    let dir = fixtures_dir();
    let documents = rendered(&server).await;
    let recording = std::env::var_os(RECORD_ENV).is_some();

    for (name, body) in documents {
        let path = dir.join(&name);
        let text = recorded(&body);
        if recording {
            std::fs::write(&path, &text).expect("the fixture is written");
            println!("recorded {}", path.display());
            continue;
        }
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
        let committed: Value = serde_json::from_str(&committed)
            .unwrap_or_else(|error| panic!("{} is not JSON: {error}", path.display()));
        let held = recorded(&normalized(&committed));
        let renders = recorded(&normalized(&body));
        assert!(
            held == renders,
            "app/ferroterm-viewer/fixtures/{name} is not what this server renders.\n{}\n\
             Re-record with `{RECORD_ENV}=1 cargo nextest run -p ferroterm-server \
             -E 'test(the_viewer_capability_fixtures)'`",
            first_difference(&held, &renders)
        );
    }
}

/// The first line `held` and `renders` disagree on, as two lines of report.
///
/// A capability document is a thousand lines, and a whole-document diff buries
/// the one element that moved, so the message names the line and quotes both
/// sides of it.
fn first_difference(held: &str, renders: &str) -> String {
    for (number, (committed, rendered)) in held.lines().zip(renders.lines()).enumerate() {
        if committed != rendered {
            let line = number.saturating_add(1);
            return format!(
                "line {line} of the file reads `{committed}`, and the server renders `{rendered}`"
            );
        }
    }
    format!(
        "the file holds {} lines and the server renders {}",
        held.lines().count(),
        renders.lines().count()
    )
}
