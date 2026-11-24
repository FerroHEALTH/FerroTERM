// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The publisher releases the European exchange format binds.
//!
//! Each is read by its `classification` reader and written by the
//! classification pipeline under the canonical URI its publisher or HL7
//! Terminology names.
//!
//! The operator supplies every release file; nothing here fetches one.

use std::path::{Path, PathBuf};

use crate::classification;

/// The flag group the release sources form: one source per build.
pub const GROUP: &str = "release";

/// The release sources and their versions.
#[derive(Debug, Default, Clone, clap::Args)]
#[group(skip)]
pub struct Releases {
    /// The IFCC NPU code file (`npu-codes-latest.csv`).
    #[arg(long, value_name = "CSV", group = GROUP, requires = "npu_version")]
    pub npu: Option<PathBuf>,
    /// The NPU release to record (`2026-11`); required, the file carries none.
    #[arg(long, value_name = "RELEASE", requires = "npu")]
    pub npu_version: Option<String>,
    /// An ICD-O-3 table: the topography or the morphology table, as the workbook or its delimited export (repeatable).
    #[arg(long, value_name = "FILE", action = clap::ArgAction::Append, group = GROUP, requires = "icdo_version")]
    pub icdo: Vec<PathBuf>,
    /// The ICD-O-3 revision to record (`3.2`); required, the tables carry none.
    #[arg(long, value_name = "VERSION", requires = "icdo")]
    pub icdo_version: Option<String>,
    /// The European Commission's EMDN workbook (`EMDN v<year>_EN.xlsx`).
    #[arg(long, value_name = "XLSX", group = GROUP)]
    pub emdn: Option<PathBuf>,
    /// The EMDN release to record when the sheet does not name it (`2026`).
    #[arg(long, value_name = "YEAR", requires = "emdn")]
    pub emdn_version: Option<String>,
    /// The ILO ISCO-08 structure workbook (`ISCO-08 EN Structure and definitions.xlsx`).
    #[arg(long, value_name = "XLSX", group = GROUP)]
    pub isco: Option<PathBuf>,
    /// The ISCO version to record; `2008` when absent.
    #[arg(long, value_name = "VERSION", requires = "isco")]
    pub isco_version: Option<String>,
}

/// A failure to read a release.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    /// A table does not read.
    #[error(transparent)]
    Sheet(#[from] ::classification::sheet::SheetError),
    /// A source was given without the version it needs.
    #[error("`--{0}` needs `--{0}-version`")]
    NoVersion(&'static str),
    /// The classification build failed.
    #[error(transparent)]
    Build(#[from] classification::Error),
}

impl Releases {
    /// The child of the index root the given source builds into.
    #[must_use]
    pub fn child(&self) -> Option<&'static str> {
        if self.npu.is_some() {
            Some("npu")
        } else if !self.icdo.is_empty() {
            Some("icd-o-3")
        } else if self.emdn.is_some() {
            Some("emdn")
        } else if self.isco.is_some() {
            Some("isco-08")
        } else {
            None
        }
    }

    /// Builds the given source into `out`, `None` when no release source is given.
    ///
    /// # Errors
    ///
    /// Returns [`ReleaseError`] when the release does not read, lacks its
    /// version, or cannot be built.
    pub fn build(&self, out: &Path) -> Result<Option<classification::Report>, ReleaseError> {
        let (read, system) = if let Some(npu) = &self.npu {
            let version = self
                .npu_version
                .as_deref()
                .ok_or(ReleaseError::NoVersion("npu"))?;
            (
                ::classification::npu::read(npu, version)?,
                ::classification::npu::SYSTEM,
            )
        } else if !self.icdo.is_empty() {
            let version = self
                .icdo_version
                .as_deref()
                .ok_or(ReleaseError::NoVersion("icdo"))?;
            let paths: Vec<&Path> = self.icdo.iter().map(PathBuf::as_path).collect();
            (
                ::classification::icdo::read(&paths, version)?,
                ::classification::icdo::SYSTEM,
            )
        } else if let Some(emdn) = &self.emdn {
            (
                ::classification::emdn::read(emdn, self.emdn_version.as_deref())?,
                ::classification::emdn::SYSTEM,
            )
        } else if let Some(isco) = &self.isco {
            (
                ::classification::isco::read(isco, self.isco_version.as_deref())?,
                ::classification::isco::SYSTEM,
            )
        } else {
            return Ok(None);
        };
        Ok(Some(classification::build(&read, system, None, out)?))
    }
}
