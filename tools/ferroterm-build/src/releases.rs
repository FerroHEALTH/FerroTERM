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
    /// An Orphadata nomenclature file (`en_product1.xml`); the first gives the display language, each further language file adds designations (repeatable).
    #[arg(long, value_name = "XML", action = clap::ArgAction::Append, group = GROUP)]
    pub orpha: Vec<PathBuf>,
    /// The Orphanet release to record; the date the first file carries when absent.
    #[arg(long, value_name = "VERSION", requires = "orpha")]
    pub orpha_version: Option<String>,
    /// NEMA's DICOM Controlled Terminology as OWL (`dcm.owl` or `dcm.owl.zip`).
    #[arg(long, value_name = "OWL_OR_ZIP", group = GROUP)]
    pub dicom: Option<PathBuf>,
    /// The DICOM edition to record; the ontology's `owl:versionInfo` when absent.
    #[arg(long, value_name = "EDITION", requires = "dicom")]
    pub dicom_version: Option<String>,
    /// A saved EDQM Standard Terms API response (`full_data_by_class`, JSON) (repeatable).
    #[arg(long, value_name = "JSON", action = clap::ArgAction::Append, group = GROUP)]
    pub edqm: Vec<PathBuf>,
    /// The EDQM release to record; the newest modification date when absent.
    #[arg(long, value_name = "VERSION", requires = "edqm")]
    pub edqm_version: Option<String>,
    /// A saved EMA SPOR API response: RMS terms of one list (v1 XML), OMS organisations (v1 JSON), or SMS substances or PMS products (v2 FHIR `Bundle`) (repeatable, one kind per build).
    #[arg(long, value_name = "FILE", action = clap::ArgAction::Append, group = GROUP)]
    pub spor: Vec<PathBuf>,
    /// The SPOR release to record; the newest modification date when absent.
    #[arg(long, value_name = "VERSION", requires = "spor")]
    pub spor_version: Option<String>,
    /// The SNOMED CT Global Patient Set freeset (the tab-separated `.txt`); writes a value set over the loaded edition under `<out>/gps`.
    #[arg(long, value_name = "TXT", group = GROUP)]
    pub gps: Option<PathBuf>,
    /// The GPS release to record (`20260101`); the date in the file name when absent.
    #[arg(long, value_name = "RELEASE", requires = "gps")]
    pub gps_version: Option<String>,
}

/// A failure to read a release.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    /// A table does not read.
    #[error(transparent)]
    Sheet(#[from] ::classification::sheet::SheetError),
    /// The Orphadata nomenclature does not read.
    #[error(transparent)]
    Orpha(#[from] ::classification::orpha::OrphaError),
    /// The DICOM terminology does not read.
    #[error(transparent)]
    Dicom(#[from] ::classification::dicom::DicomError),
    /// The EDQM responses do not read.
    #[error(transparent)]
    Edqm(#[from] ::classification::edqm::EdqmError),
    /// The SPOR responses do not read.
    #[error(transparent)]
    Spor(#[from] ::classification::spor::SporError),
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
        } else if !self.orpha.is_empty() {
            Some("orphanet")
        } else if self.dicom.is_some() {
            Some("dicom")
        } else if !self.edqm.is_empty() {
            Some("edqm")
        } else if !self.spor.is_empty() {
            Some("spor")
        } else {
            None
        }
    }

    /// Builds the given classification source into `out`, `None` when no such
    /// source is given (the GPS writes a value set through [`crate::gps`]).
    ///
    /// # Errors
    ///
    /// Returns [`ReleaseError`] when the release does not read, lacks its
    /// version, or cannot be built.
    pub fn build(&self, out: &Path) -> Result<Option<classification::Report>, ReleaseError> {
        if !self.spor.is_empty() {
            let paths: Vec<&Path> = self.spor.iter().map(PathBuf::as_path).collect();
            let read = ::classification::spor::read(&paths, self.spor_version.as_deref())?;
            return Ok(Some(classification::build(
                &read.classification,
                &read.system,
                None,
                out,
            )?));
        }
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
        } else if !self.orpha.is_empty() {
            let paths: Vec<&Path> = self.orpha.iter().map(PathBuf::as_path).collect();
            (
                ::classification::orpha::read(&paths, self.orpha_version.as_deref())?,
                ::classification::orpha::SYSTEM,
            )
        } else if let Some(dicom) = &self.dicom {
            (
                ::classification::dicom::read(dicom, self.dicom_version.as_deref())?,
                ::classification::dicom::SYSTEM,
            )
        } else if !self.edqm.is_empty() {
            let paths: Vec<&Path> = self.edqm.iter().map(PathBuf::as_path).collect();
            (
                ::classification::edqm::read(&paths, self.edqm_version.as_deref())?,
                ::classification::edqm::SYSTEM,
            )
        } else {
            return Ok(None);
        };
        Ok(Some(classification::build(&read, system, None, out)?))
    }
}
