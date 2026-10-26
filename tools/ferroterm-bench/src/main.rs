// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ferroterm-bench`: one reproducible record per code system and run.
//!
//! For each system in the configuration the harness times `ferroterm-build`
//! over the release (peak memory from `/usr/bin/time`), starts the server over
//! the artifact, measures the time to ready and the resident memory (`ps`),
//! fires a fixed request set per operation over HTTP (the first request cold,
//! the rest warm, percentiles over the warm set), samples the resident memory
//! again, and writes a JSON record naming the machine, the versions, and the
//! method beside every figure. No specification governs a benchmark: our own
//! design, and every number carries its conditions.
#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line tool reports to stdout and stderr"
)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::Parser;
use serde::{Deserialize, Serialize};

/// The command line.
#[derive(Debug, Parser)]
#[command(name = "ferroterm-bench", version, about)]
struct Cli {
    /// The systems to measure (`bench/systems.json`).
    #[arg(long, value_name = "FILE", default_value = "bench/systems.json")]
    config: PathBuf,
    /// The directory the records are written into.
    #[arg(long, value_name = "DIR", default_value = "bench/results")]
    out: PathBuf,
    /// The server binary.
    #[arg(long, value_name = "FILE", default_value = "target/release/ferroterm")]
    server: PathBuf,
    /// The build binary.
    #[arg(
        long,
        value_name = "FILE",
        default_value = "target/release/ferroterm-build"
    )]
    build: PathBuf,
    /// Skip the ingest measurement (the artifact is measured as it is).
    #[arg(long)]
    skip_ingest: bool,
    /// Measure only the systems whose name contains this text.
    #[arg(long, value_name = "TEXT")]
    only: Option<String>,
    /// The port the server listens on during the run.
    #[arg(long, default_value_t = 8123)]
    port: u16,
}

/// The configuration: the FHIR version endpoint, the warm request count, and
/// the systems.
#[derive(Debug, Deserialize)]
struct Config {
    fhir: String,
    warm_requests: usize,
    systems: Vec<System>,
}

/// One system to measure: where its artifact and release are, how it builds,
/// and the fixed requests.
#[derive(Debug, Deserialize)]
struct System {
    name: String,
    artifact: PathBuf,
    release: Option<PathBuf>,
    build: Option<Vec<String>>,
    uri: String,
    lookup: String,
    validate: String,
    subsumes: Option<[String; 2]>,
    expand: Option<String>,
    expand_small: Option<String>,
    search: String,
    translate: Option<[String; 2]>,
}

/// The machine a record was taken on.
#[derive(Debug, Serialize)]
struct Machine {
    os: String,
    arch: String,
    cpu: String,
    memory_bytes: u64,
    container: bool,
    /// The one-minute load average when the run started.
    ///
    /// A busy machine gives the operating system's readings rather than the
    /// server's: with an IDE indexing and a container runtime resident, a
    /// served edition read 155 MB where a settled machine read 830 MB, and an
    /// ingest took 69 s where a settled machine took 25 s. A record states
    /// what the machine was doing so a reader can tell one from the other
    /// (#512).
    load_average_1m: Option<f64>,
}

/// The ingest measurement.
#[derive(Debug, Serialize)]
struct Ingest {
    seconds: f64,
    peak_rss_bytes: Option<u64>,
    release: PathBuf,
}

/// The latency of one operation: the first request cold, percentiles over the
/// warm requests, in milliseconds.
#[derive(Debug, Serialize)]
struct Latency {
    status: u16,
    /// The bytes the answer carried.
    ///
    /// A read costs a fixed amount plus the serialising of what it answers
    /// with, so a latency without the size of its answer cannot say which of
    /// the two moved (#512).
    answer_bytes: usize,
    cold_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    warm_requests: usize,
}

/// One record.
#[derive(Debug, Serialize)]
struct Record {
    taken_at: String,
    ferroterm_version: &'static str,
    fhir: String,
    machine: Machine,
    system: String,
    system_uri: String,
    system_version: Option<String>,
    concepts: Option<u64>,
    artifact_bytes: u64,
    ingest: Option<Ingest>,
    ready_seconds: f64,
    rss_open_bytes: Option<u64>,
    rss_warm_bytes: Option<u64>,
    latency: Vec<(String, Latency)>,
    comparison: Option<String>,
    method: &'static str,
}

const METHOD: &str = "ingest: wall time around ferroterm-build as a child process, peak resident memory from /usr/bin/time; ready: from spawning ferroterm until GET /health answers 200; rss: the median of five readings of the server process after ready and after the warm requests, taken with `footprint` on macOS (`phys_footprint`, which counts the pages the kernel compressed away) and `ps -o rss=` elsewhere; latency: HTTP round trips from this process on the same machine, the first request of an operation cold, percentiles over the warm requests that follow (nearest-rank), with the bytes the answer carried beside them; machine: the one-minute load average when the run started, because a busy machine is measured instead of the server; comparison: not run";

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let text = std::fs::read_to_string(&cli.config)
        .with_context(|| format!("cannot read {}", cli.config.display()))?;
    let config: Config = serde_json::from_str(&text)
        .with_context(|| format!("{} is not a bench configuration", cli.config.display()))?;
    std::fs::create_dir_all(&cli.out)
        .with_context(|| format!("cannot create {}", cli.out.display()))?;
    let runtime = tokio::runtime::Runtime::new().context("cannot start the runtime")?;
    let machine = machine();
    let mut failed = Vec::new();
    // Two passes, because a build is the loudest neighbour a measurement can
    // have. Every serving figure is taken while nothing else runs, and the
    // builds follow (#304).
    let wanted: Vec<&System> = config
        .systems
        .iter()
        .filter(|system| {
            cli.only
                .as_deref()
                .is_none_or(|only| system.name.contains(only))
        })
        .filter(|system| {
            let present = system
                .artifact
                .join(fhir_terminology::artifact::MANIFEST_FILE)
                .exists();
            if !present {
                eprintln!(
                    "{}: no artifact at {}; skipped",
                    system.name,
                    system.artifact.display()
                );
            }
            present
        })
        .collect();
    let mut records: Vec<(&System, Record)> = Vec::with_capacity(wanted.len());
    for system in &wanted {
        match runtime.block_on(measure(&cli, &config, system, &machine)) {
            Ok(record) => {
                println!(
                    "{}: ready {}, rss {} open / {} warm, {} operations",
                    system.name,
                    duration_text(record.ready_seconds),
                    bytes_text(record.rss_open_bytes.unwrap_or(0)),
                    bytes_text(record.rss_warm_bytes.unwrap_or(0)),
                    record.latency.len(),
                );
                records.push((system, record));
            }
            Err(error) => {
                eprintln!("{}: {error:#}; no record written", system.name);
                failed.push(system.name.clone());
            }
        }
    }
    for (system, record) in &mut records {
        if cli.skip_ingest {
            continue;
        }
        match ingest(&cli, system) {
            Ok(measured) => {
                if let Some(measured) = &measured {
                    println!(
                        "{}: built in {}, peak {}",
                        system.name,
                        duration_text(measured.seconds),
                        bytes_text(measured.peak_rss_bytes.unwrap_or(0)),
                    );
                }
                record.ingest = measured;
            }
            Err(error) => {
                eprintln!(
                    "{}: {error:#}; the record carries no build figure",
                    system.name
                );
                failed.push(system.name.clone());
            }
        }
    }
    for (system, record) in &records {
        let path = cli.out.join(format!(
            "{}-{}.json",
            slug(&system.name),
            record.taken_at.replace([':', '.'], "-")
        ));
        let json = serde_json::to_string_pretty(record)?;
        std::fs::write(&path, format!("{json}\n"))
            .with_context(|| format!("cannot write {}", path.display()))?;
        println!("{}: written to {}", system.name, path.display());
    }
    if failed.is_empty() {
        Ok(())
    } else {
        bail!("no record for: {}", failed.join(", "))
    }
}

fn slug(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Measures one system's serving figures.
///
/// The ingest is not run here. A build of a SNOMED edition peaks over 4 GB,
/// and a server started in the seconds after one reports both a resident
/// figure and a latency well above the same server on a settled machine
/// (measured on #304). Every serving figure is taken first, and the builds
/// follow in their own pass.
async fn measure(
    cli: &Cli,
    config: &Config,
    system: &System,
    machine: &Machine,
) -> anyhow::Result<Record> {
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            system
                .artifact
                .join(fhir_terminology::artifact::MANIFEST_FILE),
        )
        .with_context(|| format!("cannot read the manifest of {}", system.name))?,
    )?;
    let mut server = Server::start(&cli.server, &system.artifact, cli.port)?;
    let ready_seconds = server.wait_ready().await?;
    let rss_open_bytes = server.rss();
    let base = format!("http://127.0.0.1:{}/{}", cli.port, config.fhir);
    let client = reqwest::Client::new();
    if let Err(refused) = served(&client, &base, system).await {
        server.stop();
        return Err(refused);
    }
    let mut latency = Vec::new();
    for request in requests(&base, system) {
        let measured =
            time_requests(&client, &request.url, &request.query, config.warm_requests).await?;
        latency.push((request.operation, measured));
    }
    let rss_warm_bytes = server.rss();
    server.stop();
    Ok(Record {
        taken_at: fhir_terminology::clock::now().to_string(),
        ferroterm_version: env!("CARGO_PKG_VERSION"),
        fhir: config.fhir.clone(),
        machine: Machine {
            os: machine.os.clone(),
            arch: machine.arch.clone(),
            cpu: machine.cpu.clone(),
            memory_bytes: machine.memory_bytes,
            container: machine.container,
            // The load when the run started, not when this record is written,
            // so every system of one run carries the same reading.
            load_average_1m: machine.load_average_1m,
        },
        system: system.name.clone(),
        system_uri: system.uri.clone(),
        system_version: manifest
            .get("version")
            .or_else(|| manifest.get("release"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        concepts: manifest.get("concepts").and_then(serde_json::Value::as_u64),
        artifact_bytes: dir_size(&system.artifact)?,
        ingest: None,
        ready_seconds,
        rss_open_bytes,
        rss_warm_bytes,
        latency,
        comparison: None,
        method: METHOD,
    })
}

/// Fails unless the running server is serving the system this record is about.
///
/// The server refuses an artifact it cannot read, logs the reason, and carries
/// on serving whatever else it was given. That contract is right for an
/// operator and wrong here: every timing below would be taken against a system
/// the server is not serving, and a 404 answered in 200 microseconds is a fast
/// number about nothing (#493). Nobody reads the log of a green run, so the
/// run asks instead.
///
/// # Errors
///
/// When the root answers no `TerminologyCapabilities`, or answers one that
/// does not declare this system.
async fn served(client: &reqwest::Client, base: &str, system: &System) -> anyhow::Result<()> {
    let capabilities: serde_json::Value = client
        .get(format!("{base}/metadata"))
        .query(&[("mode", "terminology")])
        .send()
        .await
        .with_context(|| format!("asking {base} what it serves"))?
        .error_for_status()
        .with_context(|| format!("{base} answered no TerminologyCapabilities"))?
        .json()
        .await
        .with_context(|| format!("reading what {base} says it serves"))?;
    let declared: Vec<&str> = capabilities
        .get("codeSystem")
        .and_then(serde_json::Value::as_array)
        .map(|systems| {
            systems
                .iter()
                .filter_map(|entry| entry.get("uri").and_then(serde_json::Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    if declared.contains(&system.uri.as_str()) {
        return Ok(());
    }
    bail!(
        "{}: the server read {} and refused it, then carried on. It serves {} and none of \
         them is {}. Its log says why; a stale store layout is the usual reason. Rebuild the \
         artifact with ferroterm-build before measuring it.",
        system.name,
        system.artifact.display(),
        if declared.is_empty() {
            "nothing".to_owned()
        } else {
            declared.join(", ")
        },
        system.uri,
    );
}

/// One request of the fixed set: the operation name, the URL, and the query.
struct Request {
    operation: String,
    url: String,
    query: Vec<(String, String)>,
}

/// The fixed requests of a system.
fn requests(base: &str, system: &System) -> Vec<Request> {
    let q = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    };
    let mut out = vec![
        Request {
            operation: String::from("lookup"),
            url: format!("{base}/CodeSystem/$lookup"),
            query: q(&[("system", &system.uri), ("code", &system.lookup)]),
        },
        Request {
            operation: String::from("validate-code"),
            url: format!("{base}/CodeSystem/$validate-code"),
            query: q(&[("url", &system.uri), ("code", &system.validate)]),
        },
    ];
    if let Some([a, b]) = &system.subsumes {
        out.push(Request {
            operation: String::from("subsumes"),
            url: format!("{base}/CodeSystem/$subsumes"),
            query: q(&[("system", &system.uri), ("codeA", a), ("codeB", b)]),
        });
    }
    if let Some(url) = &system.expand_small {
        out.push(Request {
            operation: String::from("expand-small"),
            url: format!("{base}/ValueSet/$expand"),
            query: q(&[("url", url), ("count", "100")]),
        });
    }
    if let Some(url) = &system.expand {
        out.push(Request {
            operation: String::from("expand-large"),
            url: format!("{base}/ValueSet/$expand"),
            query: q(&[("url", url), ("count", "1000")]),
        });
        out.push(Request {
            operation: String::from("search"),
            url: format!("{base}/ValueSet/$expand"),
            query: q(&[("url", url), ("filter", &system.search), ("count", "20")]),
        });
    }
    // A translation over a map reference set with tens of thousands of members,
    // which is the read that has to cost the answer rather than the set (#369).
    if let Some([map, code]) = &system.translate {
        out.push(Request {
            operation: String::from("translate"),
            url: format!("{base}/ConceptMap/$translate"),
            query: q(&[("url", map), ("system", &system.uri), ("code", code)]),
        });
    }
    out
}

/// One cold request, then `warm` requests, timed round trip.
async fn time_requests(
    client: &reqwest::Client,
    url: &str,
    query: &[(String, String)],
    warm: usize,
) -> anyhow::Result<Latency> {
    let once = || async {
        let started = Instant::now();
        let response = client.get(url).query(query).send().await?;
        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            bail!(
                "{url} answered {status}; a record never holds an error response: {}",
                String::from_utf8_lossy(&body)
            );
        }
        Ok::<_, anyhow::Error>((status.as_u16(), body.len(), started.elapsed()))
    };
    let (status, answer_bytes, cold) = once().await?;
    let mut samples = Vec::with_capacity(warm);
    for _ in 0..warm {
        let (_, _, elapsed) = once().await?;
        samples.push(elapsed);
    }
    samples.sort();
    Ok(Latency {
        status,
        answer_bytes,
        cold_ms: millis(cold),
        p50_ms: millis(percentile(&samples, 50)),
        p95_ms: millis(percentile(&samples, 95)),
        p99_ms: millis(percentile(&samples, 99)),
        warm_requests: warm,
    })
}

/// The nearest-rank percentile of sorted `samples`.
fn percentile(samples: &[Duration], p: usize) -> Duration {
    if samples.is_empty() {
        return Duration::ZERO;
    }
    let rank = (p * samples.len()).div_ceil(100).clamp(1, samples.len());
    samples.get(rank - 1).copied().unwrap_or(Duration::ZERO)
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// A duration in the unit that fits it: seconds, milliseconds, or microseconds.
fn duration_text(seconds: f64) -> String {
    if seconds >= 1.0 {
        format!("{seconds:.2} s")
    } else if seconds >= 0.001 {
        format!("{:.2} ms", seconds * 1e3)
    } else {
        format!("{:.0} \u{b5}s", seconds * 1e6)
    }
}

/// A byte count in the unit that fits it: GB, MB, or KB.
fn bytes_text(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        let hundredths = bytes.div_euclid(10_000_000);
        format!(
            "{}.{:02} GB",
            hundredths.div_euclid(100),
            hundredths.rem_euclid(100)
        )
    } else if bytes >= 1_000_000 {
        format!("{} MB", bytes.div_euclid(1_000_000))
    } else {
        format!("{} KB", bytes.div_euclid(1_000))
    }
}

/// Times `ferroterm-build` over the release into a scratch directory and reads
/// its peak resident memory from `/usr/bin/time`.
fn ingest(cli: &Cli, system: &System) -> anyhow::Result<Option<Ingest>> {
    let Some(build) = &system.build else {
        return Ok(None);
    };
    let scratch = std::env::temp_dir().join(format!("ferroterm-bench-{}", slug(&system.name)));
    if scratch.exists() {
        std::fs::remove_dir_all(&scratch)?;
    }
    std::fs::create_dir_all(&scratch)?;
    let mut command = Command::new("/usr/bin/time");
    command.arg(if cfg!(target_os = "macos") {
        "-l"
    } else {
        "-v"
    });
    command.arg(&cli.build);
    let mut release = None;
    for (index, flag) in build.iter().enumerate() {
        command.arg(flag);
        if index == 0
            && let Some(path) = &system.release
        {
            command.arg(path);
            release = Some(path.clone());
        }
    }
    command.arg("--out").arg(&scratch);
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let started = Instant::now();
    let output = command
        .output()
        .with_context(|| format!("cannot run {}", cli.build.display()))?;
    let seconds = started.elapsed().as_secs_f64();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if std::fs::remove_dir_all(&scratch).is_err() {
        eprintln!("{}: could not remove {}", system.name, scratch.display());
    }
    if !output.status.success() {
        bail!("{}: the build failed:\n{stderr}", system.name);
    }
    Ok(Some(Ingest {
        seconds,
        peak_rss_bytes: peak_rss(&stderr),
        release: release.unwrap_or_else(|| PathBuf::from(build.join(" "))),
    }))
}

/// The peak resident set size `/usr/bin/time` reported, in bytes (macOS
/// reports bytes, GNU time kilobytes).
fn peak_rss(stderr: &str) -> Option<u64> {
    for line in stderr.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_suffix("maximum resident set size") {
            return rest.trim().parse().ok();
        }
        if let Some(rest) = line.strip_prefix("Maximum resident set size (kbytes):") {
            return rest.trim().parse::<u64>().ok().map(|kb| kb * 1024);
        }
    }
    None
}

/// The server under measurement.
struct Server {
    child: Child,
    port: u16,
    started: Instant,
    /// Where the server's own diagnostics went.
    ///
    /// A server told to serve an artifact it cannot read says why and exits.
    /// With those streams discarded the run reported only an exit status, and
    /// the one sentence naming the artifact and the reason was thrown away
    /// (#493). They go to a file so a full pipe can never block the child.
    log: PathBuf,
}

impl Server {
    fn start(binary: &Path, artifact: &Path, port: u16) -> anyhow::Result<Self> {
        let log = Path::new("target/bench").join(format!("server-{port}.log"));
        if let Some(directory) = log.parent() {
            std::fs::create_dir_all(directory)
                .with_context(|| format!("cannot make {}", directory.display()))?;
        }
        let sink = std::fs::File::create(&log)
            .with_context(|| format!("cannot write {}", log.display()))?;
        // Both streams, because `tracing_subscriber` writes to stdout and the
        // panic hook writes to stderr, and the run wants whichever spoke.
        let also = sink
            .try_clone()
            .with_context(|| format!("cannot write {}", log.display()))?;
        let child = Command::new(binary)
            .env("FERROTERM__TERMINOLOGY__INDEX", artifact)
            .env("FERROTERM__SERVER__LISTEN", format!("127.0.0.1:{port}"))
            .env("FERROTERM__TELEMETRY__FORMAT", "json")
            // A benchmark run is no installation, so it sends no usage report.
            .env("FERROTERM__USAGE_REPORT__ENABLED", "false")
            .stdout(Stdio::from(also))
            .stderr(Stdio::from(sink))
            .spawn()
            .with_context(|| format!("cannot start {}", binary.display()))?;
        Ok(Self {
            child,
            port,
            started: Instant::now(),
            log,
        })
    }

    /// What the server said before it stopped.
    fn said(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_else(|unreadable| {
            format!("(nothing readable at {}: {unreadable})", self.log.display())
        })
    }

    /// Polls `/health` until it answers; the seconds from spawn to the answer.
    async fn wait_ready(&mut self) -> anyhow::Result<f64> {
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/health", self.port);
        for _ in 0..600 {
            if let Some(status) = self.child.try_wait()? {
                bail!(
                    "the server exited before it was ready ({status}). It said:\n{}",
                    self.said().trim()
                );
            }
            if let Ok(response) = client.get(&url).send().await
                && response.status().is_success()
            {
                return Ok(self.started.elapsed().as_secs_f64());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        bail!(
            "the server did not answer /health within 60 s. It said:\n{}",
            self.said().trim()
        )
    }

    /// The memory the server process holds, in bytes: the median of five
    /// readings a fifth of a second apart.
    ///
    /// One reading is a moment, and a moment on a machine that has just
    /// finished other work is not the figure a served edition holds (#304).
    /// `ferroterm-residency` reads it the same way, from the same module, so
    /// the two instruments cannot drift apart (#322).
    fn rss(&self) -> Option<u64> {
        ferroterm_bench::memory::median_of_process(self.child.id())
    }

    fn stop(&mut self) {
        if self.child.kill().is_ok() {
            let _status = self.child.wait();
        }
    }
}

/// The machine, from `sysctl` on macOS and `/proc` on Linux.
fn machine() -> Machine {
    let sysctl = |name: &str| {
        Command::new("sysctl")
            .args(["-n", name])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    let proc_field = |path: &str, key: &str| {
        std::fs::read_to_string(path).ok().and_then(|text| {
            text.lines()
                .find(|l| l.starts_with(key))
                .and_then(|l| l.split(':').nth(1))
                .map(|v| v.trim().to_owned())
        })
    };
    let (cpu, memory_bytes) = if cfg!(target_os = "macos") {
        (
            sysctl("machdep.cpu.brand_string").unwrap_or_default(),
            sysctl("hw.memsize")
                .and_then(|m| m.parse().ok())
                .unwrap_or(0),
        )
    } else {
        (
            // An aarch64 /proc/cpuinfo names no model; the implementer and part
            // fields are what it has.
            proc_field("/proc/cpuinfo", "model name")
                .or_else(|| proc_field("/proc/cpuinfo", "Hardware"))
                .or_else(|| {
                    let implementer = proc_field("/proc/cpuinfo", "CPU implementer")?;
                    let vendor = match implementer.as_str() {
                        "0x41" => "Arm",
                        "0x51" => "Qualcomm",
                        "0x61" => "Apple",
                        "0xc0" => "Ampere",
                        _ => "unknown vendor",
                    };
                    Some(format!(
                        "{vendor} {} (implementer {implementer}, part {})",
                        std::env::consts::ARCH,
                        proc_field("/proc/cpuinfo", "CPU part")?
                    ))
                })
                .unwrap_or_else(|| "unknown".to_owned()),
            proc_field("/proc/meminfo", "MemTotal")
                .and_then(|m| {
                    m.split_whitespace()
                        .next()
                        .and_then(|kb| kb.parse::<u64>().ok())
                })
                .map_or(0, |kb| kb * 1024),
        )
    };
    Machine {
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        cpu,
        memory_bytes,
        container: Path::new("/.dockerenv").exists() || std::env::var_os("container").is_some(),
        load_average_1m: load_average(),
    }
}

/// The one-minute load average, from `uptime`.
///
/// `getloadavg` needs `unsafe` or a platform crate, and the line `uptime`
/// prints carries the same three numbers on both platforms this runs on.
fn load_average() -> Option<f64> {
    let output = Command::new("uptime").output().ok()?;
    first_load(&String::from_utf8_lossy(&output.stdout))
}

/// The first of the three load averages in a line `uptime` printed.
///
/// macOS writes `load averages: 4.63 7.33 8.36` and Linux writes
/// `load average: 0.00, 0.01, 0.05`, so the separator is a comma on one and a
/// space on the other.
fn first_load(text: &str) -> Option<f64> {
    let (_, averages) = text.split_once("load average")?;
    averages
        .trim_start_matches(['s', ':', ' '])
        .split([',', ' '])
        .find(|field| !field.is_empty())?
        .trim()
        .parse()
        .ok()
}

/// The size of every file under `dir`, in bytes.
fn dir_size(dir: &Path) -> anyhow::Result<u64> {
    let mut total = 0;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        total += if metadata.is_dir() {
            dir_size(&entry.path())?
        } else {
            metadata.len()
        };
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::first_load;

    #[test]
    fn the_load_average_is_read_on_both_platforms() {
        assert_eq!(
            first_load("7:08  up 10 days, 1 user, load averages: 4.63 7.33 8.36"),
            Some(4.63),
            "macOS separates the three with spaces"
        );
        assert_eq!(
            first_load(" 07:08:11 up 3 days,  2 users,  load average: 0.00, 0.01, 0.05"),
            Some(0.00),
            "Linux separates them with commas"
        );
        assert_eq!(
            first_load("a line that says nothing about the load"),
            None,
            "a record states no load rather than a wrong one"
        );
    }
}
