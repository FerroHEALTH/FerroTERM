// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1
//! Emits the evidence the console ships, from the repository's own committed
//! files.
//!
//! The evidence screen states figures about this build: how many cases of the
//! HL7 terminology ecosystem suite each mode passes, the latency bars the
//! project claims, and the newest committed benchmark run. None of them is
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

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use serde_json::Value;

/// The committed pass lists and the README that records what each mode ran.
const CONFORMANCE_DIRECTORY: &str = "conformance/tx-ecosystem";

/// The latency claims and the machine that measured them.
const BARS_FILE: &str = "bench/bars.json";

/// The directory holding one subdirectory per committed benchmark run.
const RECORDS_DIRECTORY: &str = "bench/records";

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
         \x20   conformance: Conformance {{ suite_total: 0, total_source: \"\", table_source: \"\", modes: &[] }},\n\
         \x20   latency: Latency {{ machine: \"\", source: \"\", bars: &[] }},\n\
         \x20   run: Run {{ name: \"\", source: \"\", systems: &[] }},\n\
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
    let run = newest_run(&root)?;
    Ok(format!(
        "/// The evidence this build read out of the repository's committed files.\n\
         const EVIDENCE: Evidence = Evidence {{\n\
         \x20   release: {release:?},\n\
         {conformance}{latency}{run}}};\n"
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
    line(&mut out, &format!("        suite_total: {total},"));
    line(
        &mut out,
        &format!("        total_source: {total_source:?},"),
    );
    line(
        &mut out,
        &format!("        table_source: {table_source:?},"),
    );
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
    Ok(format!(
        "            Mode {{ name: {name:?}, surface: {surface:?}, passed: {passed}, ran: {ran}, source: {source:?} }},"
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
    line(&mut out, &format!("        source: {BARS_FILE:?},"));
    line(&mut out, "        bars: &[");
    for bar in listed {
        let bench = string(bar, "bench", BARS_FILE)?;
        let max = number(bar, "max_us", BARS_FILE)?;
        let measured = number(bar, "measured_us", BARS_FILE)?;
        let claim = string(bar, "claim", BARS_FILE)?;
        line(
            &mut out,
            &format!(
                "            LatencyBar {{ bench: {bench:?}, max_us: {max}, measured_us: {measured}, claim: {claim:?} }},"
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

/// A fractional field of a committed JSON document, rounded for display.
///
/// The rounding happens here rather than in the browser. Formatting one `f64`
/// pulls the whole floating-point formatter into a WebAssembly bundle that
/// otherwise holds no float at all, which the concept browser measured at 11 KB
/// gzipped (`docs/console.md` section 13). The screen shows these figures and
/// computes nothing from them, so they cross as text.
fn rounded(value: &Value, field: &str, path: &str, places: usize) -> Result<String, String> {
    let number = value
        .get(field)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("{path} carries no number `{field}`"))?;
    Ok(format!("{number:.places$}"))
}

/// A whole number with a separator every three digits, for reading.
fn grouped(number: u64) -> String {
    let digits = number.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len().div_ceil(3));
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// A byte count in mebibytes, for a resident-memory figure.
fn mebibytes(bytes: u64) -> String {
    let whole = bytes.checked_div(1024 * 1024).unwrap_or_default();
    format!("{} MiB", grouped(whole))
}

/// The newest committed benchmark run, system by system.
///
/// A run directory is named by the date it was taken, so the newest is the
/// last in name order and the choice a build makes is deterministic.
fn newest_run(root: &Path) -> Result<String, String> {
    let directory = root.join(RECORDS_DIRECTORY);
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut runs: Vec<String> = Vec::new();
    let entries = fs::read_dir(&directory)
        .map_err(|error| format!("{RECORDS_DIRECTORY} did not read: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{RECORDS_DIRECTORY}: {error}"))?;
        if entry.path().is_dir() {
            runs.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    runs.sort();
    let newest = runs
        .last()
        .ok_or_else(|| format!("{RECORDS_DIRECTORY} holds no committed run"))?;

    let run_directory = directory.join(newest);
    println!("cargo:rerun-if-changed={}", run_directory.display());
    // A BTreeMap keyed by file name, so the systems are drawn in one order
    // whatever order the directory listing arrives in.
    let mut records: BTreeMap<String, String> = BTreeMap::new();
    let entries = fs::read_dir(&run_directory)
        .map_err(|error| format!("{RECORDS_DIRECTORY}/{newest} did not read: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{RECORDS_DIRECTORY}/{newest}: {error}"))?;
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let source = format!("{RECORDS_DIRECTORY}/{newest}/{name}");
        let text = read(root, &source)?;
        let record: Value =
            serde_json::from_str(&text).map_err(|error| format!("{source} is no JSON: {error}"))?;
        records.insert(name, system(&record, &source)?);
    }
    if records.is_empty() {
        return Err(format!(
            "{RECORDS_DIRECTORY}/{newest} holds no record, so the screen would state nothing"
        ));
    }
    let source = format!("{RECORDS_DIRECTORY}/{newest}");
    let mut out = String::new();
    line(&mut out, "    run: Run {");
    line(&mut out, &format!("        name: {newest:?},"));
    line(&mut out, &format!("        source: {source:?},"));
    line(&mut out, "        systems: &[");
    for record in records.into_values() {
        line(&mut out, &record);
    }
    line(&mut out, "        ],");
    line(&mut out, "    },");
    Ok(out)
}

/// One system's record, reduced to the figures the screen draws.
fn system(record: &Value, path: &str) -> Result<String, String> {
    let machine = record
        .get("machine")
        .ok_or_else(|| format!("{path} carries no `machine`"))?;
    // A record whose artifact was built elsewhere states `ingest: null`, and
    // the screen says the run did not time the build rather than inventing a
    // figure for it.
    let ingest = record.get("ingest").filter(|ingest| !ingest.is_null());
    let (ingest_seconds, release) = match ingest {
        Some(ingest) => (
            format!("Some({:?})", rounded(ingest, "seconds", path, 1)?),
            format!("Some({:?})", string(ingest, "release", path)?),
        ),
        None => ("None".to_owned(), "None".to_owned()),
    };
    let listed = record
        .get("latency")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{path} carries no `latency` array"))?;
    let mut operations = String::new();
    for entry in listed {
        line(&mut operations, &operation(entry, path)?);
    }

    let system = string(record, "system", path)?;
    let uri = string(record, "system_uri", path)?;
    let version = string(record, "system_version", path)?;
    let fhir = string(record, "fhir", path)?;
    let taken_at = string(record, "taken_at", path)?;
    let built_by = string(record, "ferroterm_version", path)?;
    let machine = machine_line(machine, path)?;
    let concepts = grouped(number(record, "concepts", path)?);
    let ready = rounded(record, "ready_seconds", path, 2)?;
    let memory = mebibytes(number(record, "rss_warm_bytes", path)?);

    let mut out = String::new();
    line(&mut out, "            SystemRun {");
    line(&mut out, &format!("                system: {system:?},"));
    line(&mut out, &format!("                system_uri: {uri:?},"));
    line(
        &mut out,
        &format!("                system_version: {version:?},"),
    );
    line(&mut out, &format!("                fhir: {fhir:?},"));
    line(
        &mut out,
        &format!("                taken_at: {taken_at:?},"),
    );
    line(
        &mut out,
        &format!("                built_by: {built_by:?},"),
    );
    line(&mut out, &format!("                machine: {machine:?},"));
    line(
        &mut out,
        &format!("                concepts: {concepts:?},"),
    );
    line(
        &mut out,
        &format!("                ingest_seconds: {ingest_seconds},"),
    );
    line(&mut out, &format!("                release: {release},"));
    line(
        &mut out,
        &format!("                ready_seconds: {ready:?},"),
    );
    line(
        &mut out,
        &format!("                resident_memory: {memory:?},"),
    );
    line(&mut out, "                operations: &[");
    out.push_str(&operations);
    line(&mut out, "                ],");
    line(&mut out, &format!("                source: {path:?},"));
    out.push_str("            },");
    Ok(out)
}

/// The machine a record was taken on, as one line.
fn machine_line(machine: &Value, path: &str) -> Result<String, String> {
    let cpu = string(machine, "cpu", path)?;
    let os = string(machine, "os", path)?;
    let arch = string(machine, "arch", path)?;
    let gibibytes = number(machine, "memory_bytes", path)?
        .checked_div(1024 * 1024 * 1024)
        .unwrap_or_default();
    let container = machine
        .get("container")
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{path} carries no `machine.container`"))?;
    let housing = if container { ", in a container" } else { "" };
    Ok(format!("{cpu}, {os} {arch}, {gibibytes} GiB{housing}"))
}

/// One operation's timings out of a record's `latency` pair.
fn operation(entry: &Value, path: &str) -> Result<String, String> {
    let pair = entry
        .as_array()
        .ok_or_else(|| format!("{path}: a latency entry is no [name, timings] pair"))?;
    let name = pair
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{path}: a latency entry names no operation"))?;
    let timings = pair
        .get(1)
        .ok_or_else(|| format!("{path}: `{name}` carries no timings"))?;
    let status = number(timings, "status", path)?;
    let cold = rounded(timings, "cold_ms", path, 3)?;
    let median = rounded(timings, "p50_ms", path, 3)?;
    let ninety_five = rounded(timings, "p95_ms", path, 3)?;
    let ninety_nine = rounded(timings, "p99_ms", path, 3)?;
    let warm = number(timings, "warm_requests", path)?;
    Ok(format!(
        "                    Timing {{ operation: {name:?}, status: {status}, cold_ms: {cold:?}, \
         p50_ms: {median:?}, p95_ms: {ninety_five:?}, p99_ms: {ninety_nine:?}, warm_requests: {warm} }},"
    ))
}
