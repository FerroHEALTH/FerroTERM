// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! Emits the evidence the console ships, from the repository's own committed
//! files, and the enum of the console's messages, from its English catalog.
//!
//! The evidence screen states figures about this build: how many cases of the
//! HL7 terminology ecosystem suite each mode passes, and the latency bars the
//! project claims with what one run measured. None of them is
//! written into a source file. This script reads the committed artifacts,
//! checks them against each other, and writes the constant `src/evidence.rs`
//! includes.
//!
//! A missing file, a mode table this script cannot read, or a pass list that
//! disagrees with that table stops the build, the way the server's own build
//! script refuses a bundle it cannot read: the generated source carries a
//! `compile_error!` naming what went wrong, so a stale or absent figure never
//! ships.
//!
//! The output is a `const` rather than a JSON document the browser decodes.
//! The figures are fixed when the bundle is built, so decoding them again in
//! every reader's browser costs bytes for nothing, and a constant leaves no
//! decode failure for the screen to have to render.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use serde_json::Value;

/// The committed pass lists and the README that records what each mode ran.
const CONFORMANCE_DIRECTORY: &str = "conformance/tx-ecosystem";

/// The latency claims and the machine that measured them.
const BARS_FILE: &str = "bench/bars.json";

/// The header the mode table carries, cell for cell.
///
/// The table is found by its header rather than by position, so a second
/// table in the file is never mistaken for it and a renamed column stops the
/// build instead of silently emitting nothing.
const MODE_TABLE_HEADER: [&str; 6] = [
    "mode",
    "surface",
    "pass list",
    "passed of ran",
    "needs",
    "open failures",
];

/// Reads the committed evidence and writes what the bundle carries.
fn main() {
    let Some(out_dir) = std::env::var_os("OUT_DIR") else {
        println!("cargo::warning=OUT_DIR is unset, so the evidence cannot be written");
        return;
    };
    let source = match assemble() {
        Ok(evidence) => evidence,
        Err(reason) => refusal(&reason),
    };
    let path = Path::new(&out_dir).join("evidence.rs");
    if let Err(error) = fs::write(&path, source) {
        println!("cargo::warning=cannot write {}: {error}", path.display());
    }
    let messages = Path::new(&out_dir).join("messages.rs");
    if let Err(error) = fs::write(&messages, messages_source()) {
        println!(
            "cargo::warning=cannot write {}: {error}",
            messages.display()
        );
    }
}

/// The English catalog, which names every message the console draws.
const CATALOG_FILE: &str = "locales/en/ferroterm-console.ftl";

/// The `Msg` enum of every message the English catalog declares, one variant
/// per message id, so a lookup of a message the catalog lacks fails to compile.
///
/// A message is a line that starts with its id and an equals sign (Project
/// Fluent, the syntax guide); comments, attributes and continuation lines are
/// skipped. An unreadable catalog writes a `compile_error!`.
fn messages_source() -> String {
    println!("cargo:rerun-if-changed={CATALOG_FILE}");
    let text = match fs::read_to_string(CATALOG_FILE) {
        Ok(text) => text,
        Err(error) => return format!("compile_error!(\"cannot read {CATALOG_FILE}: {error}\");"),
    };
    let ids: Vec<&str> = text
        .lines()
        .filter_map(|line| line.split_once(" =").map(|(id, _)| id))
        .filter(|id| {
            id.starts_with(|first: char| first.is_ascii_lowercase())
                && id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    if let Some(twice) = ids.iter().find(|id| !seen.insert(**id)) {
        return format!("compile_error!(\"{CATALOG_FILE} declares the message {twice} twice\");");
    }
    // NOTE: FerroUI's number formatter reads the `number-` marks by id from the
    // catalog itself, so no console code names them and they have no variant.
    let ids: Vec<&str> = ids
        .into_iter()
        .filter(|id| !id.starts_with("number-"))
        .collect();
    let mut out = String::from(
        "/// A message of the console's catalog, by its id.\n#[cfg_attr(\n    target_arch = \"wasm32\",\n    expect(\n        dead_code,\n        reason = \"the words of the server's own routes, its sign-in and its form posts, are drawn on the server alone\"\n    )\n)]\n#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub(crate) enum Msg {\n",
    );
    for id in &ids {
        line(
            &mut out,
            &format!("    /// The message `{id}`.\n    {},", variant(id)),
        );
    }
    out.push_str("}\n\nimpl Msg {\n    /// Every message, in the catalog's order.\n    #[cfg(test)]\n    pub(crate) const ALL: &[Self] = &[\n");
    for id in &ids {
        line(&mut out, &format!("        Self::{},", variant(id)));
    }
    out.push_str("    ];\n\n    /// The message's id in the catalog.\n    #[must_use]\n    #[expect(clippy::too_many_lines, reason = \"one arm per catalog message, written by build.rs\")]\n    pub(crate) const fn id(self) -> &'static str {\n        match self {\n");
    for id in &ids {
        line(
            &mut out,
            &format!("            Self::{} => {id:?},", variant(id)),
        );
    }
    out.push_str("        }\n    }\n}\n");
    out
}

/// The variant name of the message id `id`: each hyphenated word capitalised.
fn variant(id: &str) -> String {
    id.split('-')
        .map(|word| {
            let mut letters = word.chars();
            letters.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + letters.as_str()
            })
        })
        .collect()
}

/// The generated source, refusing to compile, with `reason` as the failure.
///
/// The empty constant keeps the crate's own module type-checking, so the one
/// diagnostic a reader sees is the reason this script could not read a figure.
fn refusal(reason: &str) -> String {
    format!(
        "compile_error!({reason:?});\n\
         /// The evidence this build could not read.\n\
         const EVIDENCE: Evidence = Evidence {{\n\
         \x20   release: \"\",\n\
         \x20   conformance: Conformance {{ modes: &[] }},\n\
         \x20   latency: Latency {{ machine: \"\", bars: &[] }},\n\
         }};\n"
    )
}

/// Assembles the whole constant out of the committed files.
fn assemble() -> Result<String, String> {
    let root = workspace_root()?;
    let release = std::env::var("CARGO_PKG_VERSION")
        .map_err(|error| format!("cargo sets CARGO_PKG_VERSION: {error}"))?;
    let conformance = conformance(&root)?;
    let latency = latency(&root)?;
    Ok(format!(
        "/// The evidence this build read out of the repository's committed files.\n\
         const EVIDENCE: Evidence = Evidence {{\n\
         \x20   release: {release:?},\n\
         {conformance}{latency}}};\n"
    ))
}

/// The workspace root, two levels above this crate.
fn workspace_root() -> Result<PathBuf, String> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|error| format!("cargo sets CARGO_MANIFEST_DIR: {error}"))?;
    Path::new(&manifest)
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("{manifest} does not sit two levels below the workspace root"))
}

/// Reads a committed file, and watches it for the next build.
fn read(root: &Path, relative: &str) -> Result<String, String> {
    let path = root.join(relative);
    println!("cargo:rerun-if-changed={}", path.display());
    fs::read_to_string(&path).map_err(|error| {
        format!(
            "the evidence screen states figures from {relative}, which did not read: {error}. \
             The emitter stops rather than shipping a figure with no source."
        )
    })
}

/// Appends one line to the constant being written.
fn line(out: &mut String, text: &str) {
    out.push_str(text);
    out.push('\n');
}

/// The suite modes, their pass counts, and how many cases each run covered.
fn conformance(root: &Path) -> Result<String, String> {
    let total_source = format!("{CONFORMANCE_DIRECTORY}/total.txt");
    let total = read(root, &total_source)?;
    let total: u32 = total
        .trim()
        .parse()
        .map_err(|error| format!("{total_source} records the suite's case count: {error}"))?;

    let table_source = format!("{CONFORMANCE_DIRECTORY}/README.md");
    let readme = read(root, &table_source)?;
    let rows = mode_rows(&readme, &table_source)?;
    if rows.is_empty() {
        return Err(format!(
            "{table_source} carries no mode table row, so the screen would state nothing"
        ));
    }

    let mut out = String::new();
    line(&mut out, "    conformance: Conformance {");
    line(&mut out, "        modes: &[");
    for row in rows {
        line(&mut out, &mode(root, &table_source, total, &row)?);
    }
    line(&mut out, "        ],");
    line(&mut out, "    },");
    Ok(out)
}

/// One row of the mode table.
struct ModeRow {
    /// The suite mode the run selected.
    mode: String,
    /// The served FHIR root the run drove.
    surface: String,
    /// The committed pass list, by file name.
    list: String,
    /// How many cases the run passed.
    passed: u32,
    /// How many cases the run ran.
    ran: u32,
    /// The content the mode needs beyond what the runner supplies.
    needs: String,
}

/// Reads every row of the mode table out of the README.
fn mode_rows(readme: &str, path: &str) -> Result<Vec<ModeRow>, String> {
    let header = MODE_TABLE_HEADER.to_vec();
    let mut lines = readme
        .lines()
        .skip_while(|line| cells(line).as_ref() != Some(&header));
    if lines.next().is_none() {
        return Err(format!(
            "{path} carries no table headed {}, which is where the pass counts live",
            header.join(" | ")
        ));
    }
    // The alignment row under the header.
    lines.next();
    let mut rows = Vec::new();
    for line in lines {
        let Some(cells) = cells(line) else {
            break;
        };
        if cells.len() != MODE_TABLE_HEADER.len() {
            return Err(format!(
                "{path}: a mode row has {} cells and the header has {}: {line}",
                cells.len(),
                MODE_TABLE_HEADER.len()
            ));
        }
        rows.push(mode_row(&cells, path, line)?);
    }
    Ok(rows)
}

/// The cells of a markdown table row, trimmed, or `None` for any other line.
fn cells(line: &str) -> Option<Vec<&str>> {
    let inner = line.trim().strip_prefix('|')?.strip_suffix('|')?;
    Some(inner.split('|').map(str::trim).collect())
}

/// One row, with the backticks the markdown wraps its values in removed.
fn mode_row(cells: &[&str], path: &str, line: &str) -> Result<ModeRow, String> {
    let plain = |index: usize| -> Result<String, String> {
        cells
            .get(index)
            .map(|cell| cell.trim_matches('`').to_owned())
            .ok_or_else(|| format!("{path}: {line} has no cell {index}"))
    };
    let counted = plain(3)?;
    let (passed, ran) = counted.split_once(" of ").ok_or_else(|| {
        format!("{path}: the passed-of-ran cell reads `{counted}`, not `<passed> of <ran>`: {line}")
    })?;
    let number = |text: &str| -> Result<u32, String> {
        text.trim()
            .parse()
            .map_err(|error| format!("{path}: `{text}` is no case count: {error}"))
    };
    Ok(ModeRow {
        mode: plain(0)?,
        surface: plain(1)?,
        list: plain(2)?,
        passed: number(passed)?,
        ran: number(ran)?,
        needs: plain(4)?,
    })
}

/// One mode, checked against the pass list it names.
///
/// The count the table states and the count the list holds are two records of
/// the same fact, so the screen states it only when they agree.
fn mode(root: &Path, table: &str, total: u32, row: &ModeRow) -> Result<String, String> {
    let source = format!("{CONFORMANCE_DIRECTORY}/{}", row.list);
    let list = read(root, &source)?;
    let listed = list.lines().filter(|line| !line.trim().is_empty()).count();
    let listed = u32::try_from(listed)
        .map_err(|error| format!("{source} lists more cases than fit a u32: {error}"))?;
    if listed != row.passed {
        return Err(format!(
            "{source} lists {listed} passing cases and {table} says {}. The evidence screen \
             states both, so they agree or the build stops.",
            row.passed
        ));
    }
    if row.mode == "general" && row.ran != total {
        return Err(format!(
            "{table} says the general mode ran {} cases and total.txt says {total}",
            row.ran
        ));
    }
    let name = &row.mode;
    let surface = &row.surface;
    let passed = row.passed;
    let ran = row.ran;
    let needs = &row.needs;
    Ok(format!(
        "            Mode {{ name: {name:?}, surface: {surface:?}, passed: {passed}, ran: {ran}, needs: {needs:?} }},"
    ))
}

/// The latency claims, with the measurement each one has beside it.
fn latency(root: &Path) -> Result<String, String> {
    let text = read(root, BARS_FILE)?;
    let document: Value =
        serde_json::from_str(&text).map_err(|error| format!("{BARS_FILE} is no JSON: {error}"))?;
    let machine = string(&document, "machine", BARS_FILE)?;
    let listed = document
        .get("bars")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{BARS_FILE} carries no `bars` array"))?;
    if listed.is_empty() {
        return Err(format!("{BARS_FILE} claims no latency bar"));
    }
    let mut out = String::new();
    line(&mut out, "    latency: Latency {");
    line(&mut out, &format!("        machine: {machine:?},"));
    line(&mut out, "        bars: &[");
    for bar in listed {
        let bench = string(bar, "bench", BARS_FILE)?;
        let max = number(bar, "max_us", BARS_FILE)?;
        let measured = number(bar, "measured_us", BARS_FILE)?;
        line(
            &mut out,
            &format!(
                "            LatencyBar {{ bench: {bench:?}, max_us: {max}, measured_us: {measured} }},"
            ),
        );
    }
    line(&mut out, "        ],");
    line(&mut out, "    },");
    Ok(out)
}

/// A string field of a committed JSON document.
fn string(value: &Value, field: &str, path: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("{path} carries no string `{field}`"))
}

/// A whole-number field of a committed JSON document.
fn number(value: &Value, field: &str, path: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{path} carries no whole number `{field}`"))
}
