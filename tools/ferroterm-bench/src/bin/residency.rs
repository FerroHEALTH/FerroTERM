// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What one structure of a built artifact costs in memory, measured on its own.
//!
//! A served SNOMED edition holds more memory than its side files appear to
//! need, and a total says nothing about where it goes. This binary loads
//! exactly one structure and reports the resident memory that loading it cost,
//! so each figure is measured rather than inferred from a total (#322).
//!
//! One structure per process, because two structures in one process cannot be
//! told apart: the allocator does not give a structure back its own arena, and
//! a second load reuses what the first freed. The caller runs this once per
//! structure and the reader adds them up.
//!
//! The resident figure comes from `ferroterm_bench::memory`, which is how
//! every other measurement in this repository reads it; a process cannot read
//! its own memory without `unsafe` or a platform crate, and the workspace
//! forbids `unsafe`.
//!
//! `--report` answers the other half of the question. It loads everything a
//! served edition holds, in one process, and prints what each structure
//! reports it holds, from the structures' own `size_in_bytes`, beside the
//! process footprint. The difference between the sum and the footprint is the
//! residual, which the caller names rather than leaves as a gap.
#![allow(
    clippy::print_stdout,
    reason = "a measurement binary reports on stdout, which is what its caller reads"
)]

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::Parser;
use serde::Serialize;

#[derive(Parser)]
#[command(
    about = "The resident memory one structure of a built artifact costs, measured on its own"
)]
struct Cli {
    /// The artifact directory, the one a server is pointed at.
    #[arg(long)]
    artifact: PathBuf,
    /// Which structure to load, or `baseline` to load none.
    #[arg(long, required_unless_present = "report", conflicts_with = "report")]
    structure: Option<Structure>,
    /// Loads everything a served edition holds and prints what each structure
    /// reports it holds, beside the process footprint.
    #[arg(long)]
    report: bool,
    /// Loads every structure up to and including this one, in the order the
    /// provider loads them, rather than this one alone.
    ///
    /// Two peaks of two processes do not add up, because they do not happen at
    /// the same moment. Cumulative peaks do: the difference between loading
    /// through one structure and through the one before it is what that
    /// structure costs with everything before it already resident.
    #[arg(long, conflicts_with = "report")]
    cumulative: bool,
}

/// The structures a built artifact holds, each in its own file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Structure {
    /// Nothing: the cost of the process itself, which every other figure is
    /// measured against.
    Baseline,
    /// `hierarchy.bin`: the CSR adjacency and the roaring transitive closure.
    Hierarchy,
    /// `hierarchy.bin`, and the child adjacency the provider transposes from
    /// it at open. Its cost is this figure less the hierarchy's.
    Children,
    /// `text.bin`: the `fst` designation dictionary and its postings.
    Text,
    /// `members.bin`: the reference set tables the ECL evaluator reads.
    Members,
    /// `attributes.bin`: the attribute adjacency and its inverted index.
    Attributes,
    /// `refsets.bin`: the reference set membership bitmaps.
    ///
    /// The manifest names this one `refsets` and the tables `members`, which is
    /// the other way round from what the types are called.
    Refsets,
    /// `identifiers.bin`: the alternate identifier table.
    Identifiers,
    /// `store.redb` with its column files: the columns are read at open and
    /// the designation text stays in the database.
    Store,
    /// Everything a served edition loads, through the provider that serves it.
    Edition,
}

impl Structure {
    /// The file this structure is read from, when it is one file.
    fn file(self) -> Option<&'static str> {
        match self {
            Self::Baseline | Self::Edition => None,
            Self::Hierarchy | Self::Children => Some("hierarchy.bin"),
            Self::Text => Some("text.bin"),
            Self::Members => Some("members.bin"),
            Self::Attributes => Some("attributes.bin"),
            Self::Refsets => Some("refsets.bin"),
            Self::Identifiers => Some("identifiers.bin"),
            Self::Store => Some(STORE_FILE),
        }
    }

    /// The bytes on disk this structure was read from, over every file of it.
    fn serialized_bytes(self, dir: &Path) -> Option<u64> {
        let bytes = |path: PathBuf| std::fs::metadata(path).ok().map(|meta| meta.len());
        let own = bytes(dir.join(self.file()?))?;
        if self != Self::Store {
            return Some(own);
        }
        Some(
            concept_store::tables::COLUMNS
                .iter()
                .filter_map(|name| bytes(column_file(dir, name)))
                .fold(own, u64::saturating_add),
        )
    }
}

/// The store file inside an artifact directory.
const STORE_FILE: &str = "store.redb";

/// The side file of one store column inside an artifact directory.
fn column_file(dir: &Path, name: &str) -> PathBuf {
    concept_store::column::Column::file(&dir.join(STORE_FILE), name)
}

/// The order the provider loads the structures in, for the cumulative mode.
const LOAD_ORDER: [Structure; 7] = [
    Structure::Store,
    Structure::Hierarchy,
    Structure::Children,
    Structure::Text,
    Structure::Refsets,
    Structure::Attributes,
    Structure::Members,
];

/// One measurement, as the caller reads it.
#[derive(Serialize)]
struct Measurement {
    /// The structure measured.
    structure: Structure,
    /// The artifact it was read from.
    artifact: String,
    /// The size of the file on disk, where the structure is one file.
    serialized_bytes: Option<u64>,
    /// The resident memory of this process after loading it.
    resident_bytes: Option<u64>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if !cli
        .artifact
        .join(fhir_terminology::artifact::MANIFEST_FILE)
        .is_file()
    {
        bail!("{} holds no manifest.json", cli.artifact.display());
    }
    if cli.report {
        return report(&cli.artifact);
    }
    let Some(wanted) = cli.structure else {
        bail!("name a structure with --structure, or ask for --report");
    };
    // Every structure is held until the reading is taken, because a value the
    // compiler can see is dead is a value the allocator may already have back.
    let held = if cli.cumulative {
        let mut held = Vec::new();
        for structure in LOAD_ORDER {
            held.push(load(&cli.artifact, structure)?);
            if structure == wanted {
                break;
            }
        }
        held
    } else {
        vec![load(&cli.artifact, wanted)?]
    };
    let measurement = Measurement {
        structure: wanted,
        artifact: cli.artifact.display().to_string(),
        serialized_bytes: wanted.serialized_bytes(&cli.artifact),
        resident_bytes: ferroterm_bench::memory::median_of_process(std::process::id()),
    };
    println!("{}", serde_json::to_string(&measurement)?);
    drop(held);
    Ok(())
}

/// What one structure of the edition holds, as the report prints it.
#[derive(Serialize)]
struct Line {
    /// The structure, as a reader of the accounting names it.
    structure: &'static str,
    /// The file it was read from, where it is one file.
    file: Option<String>,
    /// That file's size on disk.
    serialized_bytes: Option<u64>,
    /// What the structure reports it holds in memory.
    size_in_bytes: u64,
}

/// The whole accounting of one artifact.
#[derive(Serialize)]
struct Report {
    /// The artifact measured.
    artifact: String,
    /// One line per structure, in the order the provider loads them.
    structures: Vec<Line>,
    /// The structures added up.
    total_size_in_bytes: u64,
    /// What the process holds, measured after the load.
    footprint_bytes: Option<u64>,
    /// How long opening the edition took, in seconds.
    open_seconds: f64,
    /// The footprint less the structures: the allocator's retained pages, the
    /// `redb` page cache, the binary, and the runtime.
    residual_bytes: Option<i64>,
    /// How every figure here was taken.
    method: &'static str,
}

/// How the report's figures are taken, beside every one of them.
const METHOD: &str = "size_in_bytes: each structure's own count of the heap its allocations hold, at their capacity; footprint: the median of five readings of this process after the load, from `footprint`'s phys_footprint on macOS and `ps -o rss=` elsewhere; residual: the footprint less the structures; open_seconds: the wall time of the one provider open this report measures, from a monotonic clock. No FHIR or SNOMED CT specification governs a benchmark: the accounting is this project's own design.";

/// Prints what every structure of the artifact at `dir` holds.
fn report(dir: &Path) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let provider = fhir_terminology::snomed::SnomedProvider::open(dir, "en")
        .context("cannot open the edition")?;
    let open_seconds = started.elapsed().as_secs_f64();
    let held = provider.footprint();
    let mut structures = Vec::new();
    let mut line = |structure: &'static str, file: Option<String>, bytes: usize| {
        structures.push(Line {
            serialized_bytes: file
                .as_ref()
                .and_then(|name| std::fs::metadata(dir.join(name)).ok())
                .map(|meta| meta.len()),
            structure,
            file,
            size_in_bytes: u64::try_from(bytes).unwrap_or(u64::MAX),
        });
    };
    let named = |name: &str| Some(name.to_owned());
    for (column, bytes) in held.store_columns {
        let file = column_file(dir, column);
        line(
            column,
            file.file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            bytes,
        );
    }
    line("is-a adjacency", named("hierarchy.bin"), held.is_a);
    line("closure bitmaps", named("hierarchy.bin"), held.closure);
    line("child adjacency", None, held.children);
    line("text index", named("text.bin"), held.text);
    line("member tables", named("members.bin"), held.member_tables);
    line("attribute rows", named("attributes.bin"), held.attributes);
    line("attribute inverted index", None, held.attributes_inverted);
    line("memberships", named("refsets.bin"), held.memberships);
    line("identifiers", named("identifiers.bin"), held.identifiers);
    let total: u64 = structures.iter().map(|line| line.size_in_bytes).sum();
    let measured = ferroterm_bench::memory::median_of_process(std::process::id());
    let report = Report {
        artifact: dir.display().to_string(),
        structures,
        total_size_in_bytes: total,
        footprint_bytes: measured,
        open_seconds,
        residual_bytes: measured.map(|bytes| {
            i64::try_from(bytes)
                .unwrap_or(i64::MAX)
                .saturating_sub(i64::try_from(total).unwrap_or(i64::MAX))
        }),
        method: METHOD,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    drop(provider);
    Ok(())
}

/// Whatever was loaded, kept alive until the reading is taken.
///
/// Nothing reads a variant's value, which is the point: the value exists to
/// hold the memory it allocated until the resident reading is taken.
#[expect(
    dead_code,
    reason = "each variant holds a structure alive for the measurement and is never read"
)]
enum Held {
    /// Nothing, for the baseline.
    Nothing,
    /// One structure, as the type that owns its memory.
    Hierarchy(Box<concept_graph::persist::Hierarchy>),
    Children(Box<(concept_graph::persist::Hierarchy, concept_graph::csr::Csr)>),
    Text(Box<designation_index::index::TextIndex>),
    Members(Box<concept_graph::refsets::RefsetMembers>),
    Attributes(Box<concept_graph::attributes::Attributes>),
    Refsets(Box<concept_graph::members::Memberships>),
    Identifiers(Box<concept_graph::identifiers::Identifiers>),
    Store(Box<concept_store::store::Store>),
    Edition(Box<fhir_terminology::snomed::SnomedProvider>),
}

/// Reads one structure of the artifact at `dir`.
fn load(dir: &Path, structure: Structure) -> anyhow::Result<Held> {
    let open = |name: &str| -> anyhow::Result<std::io::BufReader<std::fs::File>> {
        let path = dir.join(name);
        let file = std::fs::File::open(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        Ok(std::io::BufReader::new(file))
    };
    Ok(match structure {
        Structure::Baseline => Held::Nothing,
        Structure::Hierarchy => Held::Hierarchy(Box::new(
            concept_graph::persist::Hierarchy::read_from(&mut open("hierarchy.bin")?)
                .context("cannot read the hierarchy")?,
        )),
        Structure::Children => {
            let hierarchy =
                concept_graph::persist::Hierarchy::read_from(&mut open("hierarchy.bin")?)
                    .context("cannot read the hierarchy")?;
            let children = hierarchy
                .is_a
                .transpose()
                .context("cannot transpose the hierarchy")?;
            Held::Children(Box::new((hierarchy, children)))
        }
        Structure::Text => Held::Text(Box::new(
            designation_index::persist::read_from(&mut open("text.bin")?)
                .context("cannot read the text index")?,
        )),
        Structure::Members => Held::Members(Box::new(
            concept_graph::refsets::RefsetMembers::read_from(&mut open("members.bin")?)
                .context("cannot read the reference set tables")?,
        )),
        Structure::Attributes => Held::Attributes(Box::new(
            concept_graph::attributes::Attributes::read_from(&mut open("attributes.bin")?)
                .context("cannot read the attributes")?,
        )),
        Structure::Refsets => Held::Refsets(Box::new(
            concept_graph::members::Memberships::read_from(&mut open("refsets.bin")?)
                .context("cannot read the memberships")?,
        )),
        Structure::Identifiers => Held::Identifiers(Box::new(
            concept_graph::identifiers::Identifiers::read_from(&mut open("identifiers.bin")?)
                .context("cannot read the identifiers")?,
        )),
        Structure::Store => Held::Store(Box::new(
            concept_store::store::Store::open(&dir.join("store.redb"))
                .context("cannot open the store")?,
        )),
        Structure::Edition => Held::Edition(Box::new(
            fhir_terminology::snomed::SnomedProvider::open(dir, "en")
                .context("cannot open the edition")?,
        )),
    })
}
