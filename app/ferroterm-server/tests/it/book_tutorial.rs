// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The book's tutorial, "Your first requests", run as written.
//!
//! Each fenced block after a `<!-- book-file: PATH -->` marker is a file the
//! reader writes; each `http` block after a `<!-- book-request: NAME ... -->`
//! marker is a request the reader sends, in page order, to a server loaded
//! from the page's own `ferroterm.toml` through the shipped configuration
//! loader. A request marker states the status it expects (`expect=`) and what
//! the answer holds (`holds=`): `holds=/pointer=value` reads a JSON pointer,
//! and `holds=name=value` reads the value of the named parameter of a
//! `Parameters` answer. A page edit that changes what the server answers, or
//! a server change that breaks a documented request, fails here. No
//! specification governs the marker format: our own design.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Component, Path};

use axum::body::Body;
use ferroterm_server::config::Config;
use http::{Request, StatusCode};
use serde_json::Value;

use crate::fixture::{Server, json};

/// The tutorial page.
const PAGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../website/book/src/tutorials/first-requests.md"
));

/// The configuration file the page writes.
const CONFIG_FILE: &str = "ferroterm.toml";

/// The fewest requests the page holds, one per operation and more; a page
/// that loses its steps fails.
const MIN_REQUESTS: usize = 8;

/// A file the reader writes.
struct File {
    /// The path relative to the working directory.
    path: String,
    /// The content.
    content: String,
}

/// A request the reader sends and what its answer must hold.
struct Step {
    /// The marker's name.
    name: String,
    /// The status the answer carries.
    expect: StatusCode,
    /// What the answer holds: a JSON pointer or a parameter name, each with
    /// its value as text.
    holds: Vec<(String, String)>,
    /// The `http` block: the request line, the headers and the body.
    block: String,
}

/// The page read into its files and its requests.
struct Tutorial {
    files: Vec<File>,
    steps: Vec<Step>,
}

/// Returns the content of the fenced block that opens on the next line of
/// `lines`, leaving `lines` after its closing fence.
fn fenced<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
    marker: &str,
) -> Result<String, Box<dyn Error>> {
    let open = lines
        .next()
        .ok_or_else(|| format!("{marker} is followed by no block"))?;
    if !open.starts_with("```") {
        return Err(format!("{marker} is followed by `{open}`, not a fenced block").into());
    }
    let mut content = String::new();
    for line in lines.by_ref() {
        if line.starts_with("```") {
            return Ok(content);
        }
        content.push_str(line);
        content.push('\n');
    }
    Err(format!("the block after {marker} never closes").into())
}

/// Reads one request marker's attributes.
fn step(marker: &str, block: String) -> Result<Step, Box<dyn Error>> {
    let mut words = marker.split_whitespace();
    let name = words.next().ok_or("a request marker names its request")?;
    let mut expect = None;
    let mut parsed = Step {
        name: name.to_owned(),
        expect: StatusCode::OK,
        holds: Vec::new(),
        block,
    };
    for word in words {
        let (key, value) = word
            .split_once('=')
            .ok_or_else(|| format!("{name}: `{word}` is no key=value"))?;
        match key {
            "expect" => expect = Some(StatusCode::from_u16(value.parse()?)?),
            "holds" => {
                let (at, wanted) = value
                    .split_once('=')
                    .ok_or_else(|| format!("{name}: `{value}` is no target=value"))?;
                parsed.holds.push((at.to_owned(), wanted.to_owned()));
            }
            _ => return Err(format!("{name}: unknown attribute `{key}`").into()),
        }
    }
    parsed.expect = expect.ok_or_else(|| format!("{name} states no expected status"))?;
    if parsed.holds.is_empty() {
        return Err(format!("{name} states nothing its answer holds").into());
    }
    Ok(parsed)
}

/// Reads the page into its files and its requests, in page order.
fn tutorial(page: &str) -> Result<Tutorial, Box<dyn Error>> {
    let mut files = Vec::new();
    let mut steps = Vec::new();
    let mut lines = page.lines();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if let Some(path) = line
            .strip_prefix("<!-- book-file:")
            .and_then(|rest| rest.strip_suffix("-->"))
        {
            let content = fenced(&mut lines, line)?;
            files.push(File {
                path: path.trim().to_owned(),
                content,
            });
        } else if let Some(marker) = line
            .strip_prefix("<!-- book-request:")
            .and_then(|rest| rest.strip_suffix("-->"))
        {
            let block = fenced(&mut lines, line)?;
            steps.push(step(marker, block)?);
        }
    }
    Ok(Tutorial { files, steps })
}

/// The request an `http` block spells: the request line, the headers up to
/// the first blank line, and the body after it.
fn request(block: &str) -> Result<Request<Body>, Box<dyn Error>> {
    let (head, body) = block.split_once("\n\n").unwrap_or((block, ""));
    let mut lines = head.lines();
    let line = lines
        .next()
        .ok_or("an http block opens with a request line")?;
    let (method, target) = line
        .split_once(' ')
        .ok_or_else(|| format!("`{line}` is no request line"))?;
    let mut builder = Request::builder().method(method).uri(target.trim());
    for header in lines {
        let (name, value) = header
            .split_once(':')
            .ok_or_else(|| format!("`{header}` is no header"))?;
        builder = builder.header(name.trim(), value.trim());
    }
    Ok(builder.body(Body::from(body.trim().to_owned()))?)
}

/// The text of `value`: a string as it stands, anything else as JSON.
fn text_of(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

/// What `at` reads in `body`: a JSON pointer, or the `value[x]` of the named
/// parameter of a `Parameters` body.
fn held(body: &Value, at: &str) -> Option<String> {
    if at.starts_with('/') {
        return body.pointer(at).map(text_of);
    }
    let parameter = body["parameter"]
        .as_array()?
        .iter()
        .find(|parameter| parameter["name"] == at)?;
    parameter
        .as_object()?
        .iter()
        .find(|(key, _)| key.starts_with("value"))
        .map(|(_, value)| text_of(value))
}

#[test]
fn the_page_reads_into_files_and_requests() {
    let page = tutorial(PAGE).expect("the page reads");
    assert!(
        page.files.iter().any(|file| file.path == CONFIG_FILE),
        "the page writes {CONFIG_FILE}"
    );
    assert!(
        page.steps.len() >= MIN_REQUESTS,
        "the page holds {} requests, at least {MIN_REQUESTS} expected",
        page.steps.len()
    );
    for file in &page.files {
        assert!(
            Path::new(&file.path)
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
            "{} stays inside the working directory",
            file.path
        );
    }
    for operation in [
        "$lookup",
        "$validate-code",
        "$subsumes",
        "$expand",
        "$translate",
    ] {
        assert!(
            page.steps.iter().any(|step| step.block.contains(operation)),
            "the page sends {operation}"
        );
    }
}

#[tokio::test]
async fn the_tutorial_runs_as_written() {
    let page = tutorial(PAGE).expect("the page reads");
    let dir = tempfile::tempdir().expect("tempdir");
    for file in &page.files {
        let path = dir.path().join(&file.path);
        std::fs::create_dir_all(path.parent().expect("a file has a directory"))
            .expect("creates the directory");
        std::fs::write(&path, &file.content).expect("writes the file");
    }
    let toml = &page
        .files
        .iter()
        .find(|file| file.path == CONFIG_FILE)
        .expect("the page writes its configuration")
        .content;
    let mut config =
        Config::from_sources(Some(toml), &BTreeMap::new()).expect("the page's configuration loads");
    // The reader starts the server in the working directory, where each
    // relative path of the configuration resolves.
    for paths in [
        &mut config.terminology.code_systems,
        &mut config.terminology.index,
    ] {
        for path in paths.iter_mut() {
            *path = dir.path().join(&*path);
        }
    }
    let server = Server::start_with_config(dir, config);
    for step in &page.steps {
        let request = request(&step.block).expect("the http block reads");
        let (status, body) = json(server.send(request).await).await;
        assert_eq!(
            status, step.expect,
            "{}: the page states {}, the server answered {status}: {body}",
            step.name, step.expect
        );
        for (at, wanted) in &step.holds {
            assert_eq!(
                held(&body, at).as_deref(),
                Some(wanted.as_str()),
                "{}: the answer holds `{at}={wanted}`: {body}",
                step.name
            );
        }
    }
}
