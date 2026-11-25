// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! UDI device identifiers (UDI-DI) of one issuing entity.
//!
//! The operator exports the device table: the pipe-delimited `device.txt` of
//! the `AccessGUDID` release
//! (<https://accessgudid.nlm.nih.gov/download/delimited>), or an EUDAMED or
//! issuing-entity export saved as a delimited file or workbook.
//!
//! Each row is one UDI-DI with its brand or trade name, description, model,
//! manufacturer, and Basic UDI-DI. The list is flat. A table names no
//! release, so the build is given one.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::sheet::{self, Column, SheetError};
use crate::{Class, Classification, PREFERRED, Rubric};

/// The designation kind of the device description.
pub const DESCRIPTION: &str = "description";
/// The property kind of the version or model number.
pub const MODEL: &str = "model";
/// The property kind of the manufacturer's name.
pub const MANUFACTURER: &str = "manufacturer";
/// The property kind of the Basic UDI-DI (Regulation (EU) 2017/745 Annex VI Part C).
pub const BASIC_UDI_DI: &str = "basic-udi-di";
/// The class kind of every UDI-DI.
pub const KIND: &str = "udi-di";

/// An issuing entity of UDIs, each served under the URI FHIR names for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issuer {
    /// GS1.
    Gs1,
    /// The Health Industry Business Communications Council.
    Hibcc,
    /// ICCBBA, for blood containers.
    IccbbaBlood,
    /// ICCBBA, for other devices.
    IccbbaOther,
    /// IFA, the Informationsstelle für Arzneispezialitäten.
    Ifa,
}

impl Issuer {
    /// Every issuing entity, by the name [`Issuer::from_str`] reads.
    pub const ALL: [(&'static str, Self); 5] = [
        ("gs1", Self::Gs1),
        ("hibcc", Self::Hibcc),
        ("iccbba-blood", Self::IccbbaBlood),
        ("iccbba-other", Self::IccbbaOther),
        ("ifa", Self::Ifa),
    ];

    /// The system URI.
    // NOTE: FHIR R5 Device.udiCarrier.issuer; HL7 Terminology registers no UDI issuer.
    #[must_use]
    pub fn system(self) -> &'static str {
        match self {
            Self::Gs1 => "http://hl7.org/fhir/NamingSystem/gs1-di",
            Self::Hibcc => "http://hl7.org/fhir/NamingSystem/hibcc-diI",
            Self::IccbbaBlood => "http://hl7.org/fhir/NamingSystem/iccbba-blood-di",
            Self::IccbbaOther => "http://hl7.org/fhir/NamingSystem/iccbba-other-di",
            Self::Ifa => "http://hl7.org/fhir/NamingSystem/ifa-gmbh-di",
        }
    }

    /// The word an issuing-agency cell names this entity by.
    fn agency(self) -> &'static str {
        match self {
            Self::Gs1 => "gs1",
            Self::Hibcc => "hibcc",
            Self::IccbbaBlood | Self::IccbbaOther => "iccbba",
            Self::Ifa => "ifa",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Gs1 => "GS1 UDI device identifiers",
            Self::Hibcc => "HIBCC UDI device identifiers",
            Self::IccbbaBlood => "ICCBBA UDI device identifiers for blood containers",
            Self::IccbbaOther => "ICCBBA UDI device identifiers for other devices",
            Self::Ifa => "IFA UDI device identifiers",
        }
    }
}

/// A name that is no issuing entity of [`Issuer::ALL`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is no UDI issuing entity; one of gs1, hibcc, iccbba-blood, iccbba-other, ifa")]
pub struct UnknownIssuer(pub String);

impl FromStr for Issuer {
    type Err = UnknownIssuer;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
            .map(|(_, issuer)| *issuer)
            .ok_or_else(|| UnknownIssuer(name.to_owned()))
    }
}

/// A failure to read a UDI-DI table.
#[derive(Debug, thiserror::Error)]
pub enum UdiError {
    /// The table does not read or has no UDI-DI column.
    #[error(transparent)]
    Sheet(#[from] SheetError),
    /// A row names another issuing entity than the one being built.
    #[error(
        "{path}: {rows} rows are issued by `{agency}`; filter the table to one issuing entity per build"
    )]
    OtherIssuer {
        /// The table.
        path: PathBuf,
        /// The first other agency named.
        agency: String,
        /// The rows naming another agency.
        rows: usize,
    },
}

const COLUMNS: [Column; 7] = [
    Column {
        name: "PrimaryDI",
        aliases: &["primarydi", "udi-di", "udi-di code", "udi_di", "udidi"],
        required: true,
    },
    Column {
        name: "brandName",
        aliases: &["brandname", "trade name", "tradename", "trade_name"],
        required: false,
    },
    Column {
        name: "deviceDescription",
        aliases: &["devicedescription", "device description", "description"],
        required: false,
    },
    Column {
        name: "versionModelNumber",
        aliases: &["versionmodelnumber", "model", "version or model"],
        required: false,
    },
    Column {
        name: "companyName",
        aliases: &["companyname", "manufacturer", "manufacturer name"],
        required: false,
    },
    Column {
        name: "Basic UDI-DI",
        aliases: &["basic udi-di", "basic udi-di code", "basic_udi_di"],
        required: false,
    },
    Column {
        name: "deviceIdIssuingAgency",
        aliases: &[
            "deviceidissuingagency",
            "issuing agency",
            "issuing entity",
            "issuing_agency",
        ],
        required: false,
    },
];

/// Reads the UDI-DIs `issuer` assigned from the table at `path` as the
/// release `version`.
///
/// The brand or trade name is the display, the description when the row has
/// none.
///
/// # Errors
///
/// Returns [`UdiError::Sheet`] when the table does not read or lacks the
/// UDI-DI column, and [`UdiError::OtherIssuer`] when a row's issuing agency
/// names another entity.
pub fn read(path: &Path, issuer: Issuer, version: &str) -> Result<Classification, UdiError> {
    let sheet = sheet::read(path)?;
    let header = sheet.header(path, &COLUMNS)?;
    let mut classes = Vec::new();
    let mut other: Option<(String, usize)> = None;
    for (code, row) in sheet.coded(&header) {
        if let Some(agency) = header.cell(row, 6) {
            let named = agency
                .to_lowercase()
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|word| word == issuer.agency());
            if !named {
                let (_, rows) = other.get_or_insert_with(|| (agency.to_owned(), 0));
                *rows = rows.saturating_add(1);
                continue;
            }
        }
        let mut rubrics = Vec::new();
        if let Some(name) = header.cell(row, 1).or_else(|| header.cell(row, 2)) {
            rubrics.push(Rubric::english(PREFERRED, name));
        }
        rubrics.extend(header.rubrics(
            row,
            &[
                (2, DESCRIPTION),
                (3, MODEL),
                (4, MANUFACTURER),
                (5, BASIC_UDI_DI),
            ],
        ));
        classes.push(Class {
            code: code.to_owned(),
            kind: KIND.to_owned(),
            rubrics,
            ..Class::default()
        });
    }
    if let Some((agency, rows)) = other {
        return Err(UdiError::OtherIssuer {
            path: path.to_path_buf(),
            agency,
            rows,
        });
    }
    Ok(Classification {
        name: String::from("UDI-DI"),
        title: issuer.title().to_owned(),
        version: Some(version.to_owned()),
        language: String::from("en"),
        kinds: vec![KIND.to_owned()],
        classes,
        designation_kinds: vec![PREFERRED.to_owned(), DESCRIPTION.to_owned()],
        hierarchy: None,
    })
}
