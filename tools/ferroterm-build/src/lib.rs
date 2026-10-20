//! The offline build: an RF2 release in, the served artifacts out.
//!
//! Runs once per SNOMED CT edition, outside the server process. It reads the
//! RF2 Snapshot through `rf2` and writes one `redb` store holding the
//! concepts, designations, acceptabilities, and properties, with the hierarchy
//! (`concept-graph`) and the designation index (`designation-index`) in its blob
//! slots, plus a manifest naming the edition the store was built from. Two runs
//! over the same release write byte-identical files: every collection is
//! sorted by identifier before it is numbered, and nothing records a clock.
#![doc(test(attr(deny(warnings))))]

pub mod archive;
pub mod classification;
mod common;
pub mod dhd;
pub mod icd11;
pub mod labcodeset;
pub mod loinc;
pub mod output;
pub mod pipeline;
pub mod rxnorm;

use std::path::{Path, PathBuf};

use clap::Parser;

/// The command line of `ferroterm-build`.
#[derive(Debug, Parser)]
#[command(name = "ferroterm-build", version, about)]
pub struct Cli {
    /// The SNOMED CT RF2 release: the directory holding `Snapshot/`, or the release zip.
    #[arg(
        long,
        value_name = "DIR_OR_ZIP",
        conflicts_with_all = ["loinc", "claml", "icd10cm", "rxnorm", "icd11", "atc", "dhd", "gstandaard"],
        required_unless_present_any = ["loinc", "claml", "icd10cm", "rxnorm", "icd11", "atc", "dhd", "gstandaard"]
    )]
    pub rf2: Option<PathBuf>,
    /// A refset-only RF2 package layered onto the edition `--rf2` names (repeatable).
    #[arg(long, value_name = "DIR_OR_ZIP", action = clap::ArgAction::Append, requires = "rf2")]
    pub rf2_refset: Vec<PathBuf>,
    /// The LOINC release: the unpacked `Loinc_<version>` directory, or the release zip.
    #[arg(long, value_name = "DIR_OR_ZIP", conflicts_with_all = ["claml", "icd10cm", "rxnorm"])]
    pub loinc: Option<PathBuf>,
    /// The LOINC version to record when the release does not say (`2.82`).
    #[arg(long, value_name = "VERSION", requires = "loinc")]
    pub loinc_version: Option<String>,
    /// A `ClaML` classification (WHO ICD-10, a national ICD-10, ICPC-2): the XML file, or a zip holding it.
    #[arg(long, value_name = "XML_OR_ZIP", conflicts_with_all = ["icd10cm", "rxnorm"], requires = "system")]
    pub claml: Option<PathBuf>,
    /// The code system URI the `ClaML` classification is served as (`http://hl7.org/fhir/sid/icd-10`).
    #[arg(long, value_name = "URI", requires = "claml")]
    pub system: Option<String>,
    /// The version to record when the `ClaML` title does not say (`2021`).
    #[arg(long, value_name = "VERSION", requires = "claml")]
    pub claml_version: Option<String>,
    /// The ICD-10-CM release: the directories or zips holding the tabular XML and the order file (repeatable).
    #[arg(long, value_name = "DIR_OR_ZIP", action = clap::ArgAction::Append, conflicts_with = "rxnorm")]
    pub icd10cm: Vec<PathBuf>,
    /// The `RxNorm` release: the unpacked directory holding `rrf/`, or the release zip (the full release or the Current Prescribable Content).
    #[arg(long, value_name = "DIR_OR_ZIP")]
    pub rxnorm: Option<PathBuf>,
    /// The release date to record when the release does not say (`09082026`).
    #[arg(long, value_name = "MMDDYYYY", requires = "rxnorm")]
    pub rxnorm_version: Option<String>,
    /// The ICD-11 cache directory: the entity JSON a local ICD-API deployment served, per code system and language; built into `<out>/mms`, `<out>/icf`, `<out>/entity`.
    #[arg(long, value_name = "DIR", conflicts_with_all = ["loinc", "claml", "icd10cm", "rxnorm"])]
    pub icd11: Option<PathBuf>,
    /// A local ICD-API deployment (`http://127.0.0.1:80`) to fill the cache from before building.
    #[arg(long, value_name = "URL", requires = "icd11")]
    pub icd11_api: Option<String>,
    /// The ICD-11 release to fetch and record (`2026-01`); the deployment's latest when absent.
    #[arg(long, value_name = "RELEASE", requires = "icd11")]
    pub icd11_release: Option<String>,
    /// The languages to fetch (`en,fr`); English when absent.
    #[arg(long, value_name = "LANG", value_delimiter = ',', action = clap::ArgAction::Append, requires = "icd11_api")]
    pub icd11_languages: Vec<String>,
    /// The WHO ATC/DDD index as a CSV export (`ATC code`, `ATC level name`, `DDD`, `U`, `Adm.R`, `Note`), or the G-Standaard file `BST801T`.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["loinc", "claml", "icd10cm", "rxnorm", "icd11"])]
    pub atc: Option<PathBuf>,
    /// The ATC index year to record (`2026`); required, the files carry none.
    #[arg(long, value_name = "YEAR", requires = "atc")]
    pub atc_version: Option<String>,
    /// A DHD Diagnosethesaurus or Verrichtingenthesaurus delivery: the zip, or the unpacked directory of CSV tables.
    #[arg(long, value_name = "DIR_OR_ZIP", conflicts_with_all = ["loinc", "claml", "icd10cm", "rxnorm", "icd11", "atc"])]
    pub dhd: Option<PathBuf>,
    /// The thesaurus version to record when the delivery name does not carry it (`2.27`).
    #[arg(long, value_name = "VERSION", requires = "dhd")]
    pub dhd_version: Option<String>,
    /// A G-Standaard release directory (the `BSTnnnT` files); builds the GPK, PRK, HPK, and article systems under `<out>/{gpk,prk,hpk,artikel}`.
    #[arg(long, value_name = "DIR", requires = "gstandaard_version", conflicts_with_all = ["loinc", "claml", "icd10cm", "rxnorm", "icd11", "atc", "dhd"])]
    pub gstandaard: Option<PathBuf>,
    /// The G-Standaard release to record (`202609`); required, the files carry none.
    #[arg(long, value_name = "RELEASE", requires = "gstandaard")]
    pub gstandaard_version: Option<String>,
    /// A Nederlandse Labcodeset publication: the release zip, or the `labconcepts-*.xml` document or its directory; writes FHIR resources under `<out>/labcodeset`.
    #[arg(long, value_name = "FILE_DIR_OR_ZIP", conflicts_with_all = ["loinc", "claml", "icd10cm", "rxnorm", "icd11", "atc", "dhd", "gstandaard"])]
    pub labcodeset: Option<PathBuf>,
    /// The `RxNorm` sources (`SAB`) whose names are kept beside the unrestricted `RXNORM` and `MTHSPL` (a full release under a UMLS licence).
    #[arg(long, value_name = "SAB", value_delimiter = ',', action = clap::ArgAction::Append, requires = "rxnorm")]
    pub rxnorm_sources: Vec<String>,
    /// The directory to write the artifacts into; a child of `--index-root` named for the source when absent.
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// The index root the server reads: without `--out` the build writes under it, and an `--out` outside it is refused.
    #[arg(long, value_name = "DIR", env = output::INDEX_ROOT_ENV)]
    pub index_root: Option<PathBuf>,
}

/// Runs the build the CLI describes.
///
/// A zip is unpacked (the Snapshot tree of an RF2 release and of every
/// refset-only package layered onto it, the tables of a LOINC release, the XML
/// of a `ClaML` classification, the two files of an ICD-10-CM release, the
/// `RRF` tables of an `RxNorm` release) to a temporary directory that is
/// removed when the build ends; a directory is read in place.
///
/// The artifacts land in the directory [`output::output`] settles.
///
/// # Errors
///
/// Returns [`RunError::Output`] when no output directory can be settled, and
/// the errors of [`run_into`].
pub fn run(cli: &Cli) -> Result<Report, RunError> {
    let out = output::output(cli)?;
    run_into(cli, &out)
}

/// Runs the build the CLI describes into the directory `out`, ignoring the
/// `--out` and `--index-root` of `cli`.
///
/// # Errors
///
/// Returns [`RunError`] when the zip does not unpack, the release does not
/// read, the edition cannot be identified, a layered package's module
/// dependency is unmet, or an artifact cannot be written.
pub fn run_into(cli: &Cli, out: &Path) -> Result<Report, RunError> {
    if let Some(report) = run_labcodeset(cli, out)? {
        return Ok(Report::Labcodeset(report));
    }
    if let Some(reports) = run_gstandaard(cli, out)? {
        return Ok(Report::Classifications(reports));
    }
    if let Some(report) = run_classification(cli, out)? {
        return Ok(Report::Classification(report));
    }
    if let Some(reports) = run_icd11(cli, out)? {
        return Ok(Report::Icd11(reports));
    }
    if let Some(report) = run_rxnorm(cli, out)? {
        return Ok(Report::RxNorm(report));
    }
    if let Some(report) = run_loinc(cli, out)? {
        return Ok(Report::Loinc(report));
    }
    Ok(Report::Snomed(run_snomed(cli, out)?))
}

/// The Nederlandse Labcodeset build (`--labcodeset`), when the command line
/// asks for one.
fn run_labcodeset(cli: &Cli, out: &Path) -> Result<Option<labcodeset::Report>, RunError> {
    let Some(labcodeset) = &cli.labcodeset else {
        return Ok(None);
    };
    let scratch;
    let root = if labcodeset
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        scratch = tempfile::tempdir().map_err(RunError::Scratch)?;
        archive::unpack_labcodeset(labcodeset, scratch.path())?
    } else {
        labcodeset.clone()
    };
    let publication = ::labcodeset::read(&root)?;
    Ok(Some(labcodeset::build(&publication, out)?))
}

/// The G-Standaard product ladder (`--gstandaard`), one report per rung, when
/// the command line asks for one.
fn run_gstandaard(cli: &Cli, out: &Path) -> Result<Option<Vec<classification::Report>>, RunError> {
    let (Some(dir), Some(version)) = (&cli.gstandaard, &cli.gstandaard_version) else {
        return Ok(None);
    };
    let ladder = ::gstandaard::read(dir, version)?;
    let mut reports = Vec::new();
    for (name, system, classification) in ladder.rungs() {
        reports.push(classification::build(
            classification,
            system,
            Some(version),
            &out.join(name),
        )?);
    }
    Ok(Some(reports))
}

/// The ICD-11 builds (`--icd11`), one report per code system the cache holds,
/// when the command line asks for one.
fn run_icd11(cli: &Cli, out: &Path) -> Result<Option<Vec<icd11::Report>>, RunError> {
    let Some(cache) = &cli.icd11 else {
        return Ok(None);
    };
    if let Some(api) = &cli.icd11_api {
        fetch_icd11(cache, api, cli)?;
    }
    Ok(Some(icd11::build_all(
        cache,
        cli.icd11_release.as_deref(),
        out,
    )?))
}

/// The `RxNorm` build (`--rxnorm`), when the command line asks for one.
fn run_rxnorm(cli: &Cli, out: &Path) -> Result<Option<rxnorm::Report>, RunError> {
    let Some(rxnorm) = &cli.rxnorm else {
        return Ok(None);
    };
    let scratch;
    let root = if rxnorm.is_file() {
        scratch = tempfile::tempdir().map_err(RunError::Scratch)?;
        archive::unpack_rxnorm(rxnorm, scratch.path())?
    } else {
        rxnorm.clone()
    };
    let version = cli.rxnorm_version.clone().or_else(|| {
        rxnorm
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .map(|n| n.trim_end_matches(".zip"))
            .and_then(|n| n.rsplit_once('_').map(|(_, tail)| tail.to_owned()))
            .filter(|tail| tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_digit()))
    });
    Ok(Some(rxnorm::build(
        &root,
        version.as_deref(),
        &cli.rxnorm_sources,
        out,
    )?))
}

/// The LOINC build (`--loinc`), when the command line asks for one.
fn run_loinc(cli: &Cli, out: &Path) -> Result<Option<loinc::Report>, RunError> {
    let Some(loinc) = &cli.loinc else {
        return Ok(None);
    };
    let scratch;
    let root = if loinc.is_file() {
        scratch = tempfile::tempdir().map_err(RunError::Scratch)?;
        archive::unpack_loinc(loinc, scratch.path())?
    } else {
        loinc.clone()
    };
    let version = cli
        .loinc_version
        .clone()
        .or_else(|| loinc::version_from_name(loinc));
    Ok(Some(loinc::build(&root, version.as_deref(), out)?))
}

/// The SNOMED CT build: the edition `--rf2` names with every `--rf2-refset`
/// package layered onto it.
fn run_snomed(cli: &Cli, out: &Path) -> Result<pipeline::Report, RunError> {
    let Some(rf2) = &cli.rf2 else {
        return Err(RunError::NoInput);
    };
    // The scratch directories hold the unpacked releases; they are removed
    // when this function returns, so they outlive the build.
    let mut scratch = Vec::new();
    let mut unpack = |release: &PathBuf| -> Result<PathBuf, RunError> {
        if !release.is_file() {
            return Ok(release.clone());
        }
        let directory = tempfile::tempdir().map_err(RunError::Scratch)?;
        let root = archive::unpack_snapshot(release, directory.path())?;
        scratch.push(directory);
        Ok(root)
    };
    let root = unpack(rf2)?;
    let mut refsets = Vec::with_capacity(cli.rf2_refset.len());
    for package in &cli.rf2_refset {
        refsets.push(unpack(package)?);
    }
    Ok(pipeline::build(&root, &refsets, out)?)
}

/// The classification builds (`--claml`, `--atc`, `--icd10cm`), when the
/// command line asks for one.
fn run_classification(cli: &Cli, out: &Path) -> Result<Option<classification::Report>, RunError> {
    if let Some(claml) = &cli.claml {
        let system = cli.system.as_deref().ok_or(RunError::NoSystem)?;
        let scratch;
        let file = if claml
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
        {
            scratch = tempfile::tempdir().map_err(RunError::Scratch)?;
            archive::unpack_claml(claml, scratch.path())?
        } else {
            claml.clone()
        };
        let classification = ::classification::claml::read_file(&file)?;
        return Ok(Some(classification::build(
            &classification,
            system,
            cli.claml_version.as_deref(),
            out,
        )?));
    }
    if let Some(dhd) = &cli.dhd {
        let scratch;
        let root = if dhd.is_file() {
            scratch = tempfile::tempdir().map_err(RunError::Scratch)?;
            let name = dhd
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .map_or_else(
                    || String::from("delivery"),
                    |n| n.trim_end_matches(".zip").to_owned(),
                );
            archive::unpack_dhd(dhd, &scratch.path().join(name))?
        } else {
            dhd.clone()
        };
        let thesaurus = ::dhd_thesaurus::read(&root, cli.dhd_version.as_deref())?;
        let report = classification::build(
            &thesaurus.classification,
            ::dhd_thesaurus::SYSTEM,
            cli.dhd_version.as_deref(),
            out,
        )?;
        dhd::write_concept_maps(&thesaurus, &report.version, out)?;
        return Ok(Some(report));
    }
    if let Some(atc) = &cli.atc {
        let version = cli.atc_version.as_deref().ok_or(RunError::NoAtcVersion)?;
        let classification = ::classification::atc::read(atc, Some(version))?;
        return Ok(Some(classification::build(
            &classification,
            ::classification::atc::SYSTEM,
            Some(version),
            out,
        )?));
    }
    if cli.icd10cm.is_empty() {
        return Ok(None);
    }
    let scratch = tempfile::tempdir().map_err(RunError::Scratch)?;
    let mut roots = Vec::new();
    for (i, path) in cli.icd10cm.iter().enumerate() {
        if path.is_file() {
            let into = scratch.path().join(i.to_string());
            roots.push(archive::unpack_icd10cm(path, &into)?);
        } else {
            roots.push(path.clone());
        }
    }
    let files = ::classification::icd10cm::locate(&roots)?;
    let classification = ::classification::icd10cm::read(&files)?;
    Ok(Some(classification::build(
        &classification,
        classification::ICD10CM_SYSTEM,
        None,
        out,
    )?))
}

/// Fills the ICD-11 `cache` from the deployment at `api`: every code system
/// the deployment serves, in the languages asked for.
fn fetch_icd11(cache: &Path, api: &str, cli: &Cli) -> Result<(), RunError> {
    let release = if let Some(release) = &cli.icd11_release {
        release.clone()
    } else {
        let probe = ::icd11::api::Client::new(api, "")?;
        let root = probe.get(
            &format!("{}/icd/release/11/mms", api.trim_end_matches('/')),
            "en",
        )?;
        root.get("latestRelease")
            .and_then(serde_json::Value::as_str)
            .and_then(|uri| uri.rsplit('/').nth(1).map(str::to_owned))
            .ok_or(RunError::NoRelease)?
    };
    let client = ::icd11::api::Client::new(api, &release)?;
    let languages: Vec<String> = if cli.icd11_languages.is_empty() {
        vec![String::from("en")]
    } else {
        cli.icd11_languages.clone()
    };
    for linearization in ::icd11::Linearization::ALL {
        let ids = client.ids(linearization)?;
        client.download(cache, linearization, &ids, &languages, 8)?;
    }
    Ok(())
}

/// What a build wrote.
#[derive(Debug)]
pub enum Report {
    /// A SNOMED CT edition.
    Snomed(pipeline::Report),
    /// A LOINC release.
    Loinc(loinc::Report),
    /// A classification: a `ClaML` document or the ICD-10-CM release.
    Classification(classification::Report),
    /// The G-Standaard product ladder: GPK, PRK, HPK, and article reports.
    Classifications(Vec<classification::Report>),
    /// An `RxNorm` release.
    RxNorm(rxnorm::Report),
    /// The ICD-11 code systems, one report each.
    Icd11(Vec<icd11::Report>),
    /// The Nederlandse Labcodeset as FHIR resources.
    Labcodeset(labcodeset::Report),
}

/// A failure of the command as a whole.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// No input was given.
    #[error(
        "give `--rf2`, `--loinc`, `--claml`, `--icd10cm`, `--rxnorm`, `--icd11`, `--atc`, `--dhd`, or `--gstandaard`"
    )]
    NoInput,
    /// `--atc` without `--atc-version`.
    #[error("`--atc` needs `--atc-version` (the index year)")]
    NoAtcVersion,
    /// The ICD-API names no latest release and none was given.
    #[error("the ICD-API names no latest release; pass `--icd11-release`")]
    NoRelease,
    /// `--claml` without `--system`.
    #[error("`--claml` needs `--system`")]
    NoSystem,
    /// No output directory could be settled.
    #[error(transparent)]
    Output(#[from] output::OutputError),
    /// The release zip does not unpack.
    #[error(transparent)]
    Archive(#[from] archive::ArchiveError),
    /// No temporary directory for the unpacked release.
    #[error("cannot create a temporary directory for the release")]
    Scratch(#[source] std::io::Error),
    /// The SNOMED CT build failed.
    #[error(transparent)]
    Build(#[from] pipeline::Error),
    /// The LOINC build failed.
    #[error(transparent)]
    Loinc(#[from] loinc::Error),
    /// The `ClaML` document does not read.
    #[error(transparent)]
    Claml(#[from] ::classification::claml::ClamlError),
    /// The ICD-10-CM release does not read.
    #[error(transparent)]
    Icd10cm(#[from] ::classification::icd10cm::Icd10cmError),
    /// The ATC table does not read.
    #[error(transparent)]
    Atc(#[from] ::classification::atc::AtcError),
    /// The DHD delivery does not read.
    #[error(transparent)]
    Dhd(#[from] ::dhd_thesaurus::DhdError),
    /// The Labcodeset publication does not read.
    #[error(transparent)]
    Labcodeset(#[from] ::labcodeset::LabcodesetError),
    /// The Labcodeset resources cannot be written.
    #[error(transparent)]
    LabcodesetWrite(#[from] labcodeset::WriteError),
    /// The DHD concept maps cannot be written.
    #[error(transparent)]
    DhdMaps(#[from] dhd::MapError),
    /// The G-Standaard files do not read.
    #[error(transparent)]
    GStandaard(#[from] ::gstandaard::GStandaardError),
    /// The classification build failed.
    #[error(transparent)]
    Classification(#[from] classification::Error),
    /// The `RxNorm` build failed.
    #[error(transparent)]
    RxNorm(#[from] rxnorm::Error),
    /// The ICD-API could not be walked.
    #[error(transparent)]
    Icd11Api(#[from] ::icd11::api::ApiError),
    /// The ICD-11 build failed.
    #[error(transparent)]
    Icd11(#[from] icd11::Error),
}
