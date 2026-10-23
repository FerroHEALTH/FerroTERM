// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR releases this server mounts, one root each.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A FHIR release the server answers under its own path prefix.
///
/// The server nests `/r4`, `/r4b`, `/r5`, and `/r6` on one process, so the
/// version is part of every request URL rather than a build-time choice.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum FhirVersion {
    /// FHIR 4.0.1.
    R4,
    /// FHIR 4.3.0, the release the engine implemented first.
    #[default]
    R4B,
    /// FHIR 5.0.0.
    R5,
    /// FHIR 6.0.0 (ballot).
    R6,
}

impl FhirVersion {
    /// Every version, in release order, for the switcher to render.
    pub(crate) const ALL: [Self; 4] = [Self::R4, Self::R4B, Self::R5, Self::R6];

    /// The path segment the server mounts this version under.
    pub(crate) fn segment(self) -> &'static str {
        match self {
            Self::R4 => "r4",
            Self::R4B => "r4b",
            Self::R5 => "r5",
            Self::R6 => "r6",
        }
    }

    /// The release the server answers under this version's root, as its
    /// `CapabilityStatement.fhirVersion` names it.
    pub(crate) fn release(self) -> &'static str {
        match self {
            Self::R4 => "FHIR 4.0.1",
            Self::R4B => "FHIR 4.3.0",
            Self::R5 => "FHIR 5.0.0",
            Self::R6 => "FHIR 6.0.0-ballot5",
        }
    }

    /// The name a reader recognises.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::R4 => "R4",
            Self::R4B => "R4B",
            Self::R5 => "R5",
            Self::R6 => "R6",
        }
    }
}

impl fmt::Display for FhirVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// The error [`FhirVersion::from_str`] returns for an unknown name.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("`{0}` names no FHIR version this server serves")]
pub struct UnknownFhirVersion(String);

impl FromStr for FhirVersion {
    type Err = UnknownFhirVersion;

    /// Reads a version from a URL segment or a stored setting.
    ///
    /// The comparison is case-insensitive because the value arrives from a
    /// query string a reader can type.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownFhirVersion`] when the text names no served version.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let lowered = text.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|version| version.segment() == lowered)
            .ok_or_else(|| UnknownFhirVersion(text.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_version_round_trips_through_its_segment() {
        for version in FhirVersion::ALL {
            assert_eq!(
                version.segment().parse::<FhirVersion>(),
                Ok(version),
                "{version} must survive a link and a stored setting"
            );
        }
    }

    #[test]
    fn a_typed_version_is_read_case_insensitively() {
        assert_eq!("R4B".parse::<FhirVersion>(), Ok(FhirVersion::R4B));
        assert_eq!(" r5 ".parse::<FhirVersion>(), Ok(FhirVersion::R5));
    }

    #[test]
    fn an_unknown_version_is_an_error_and_not_a_silent_default() {
        let error = "r7".parse::<FhirVersion>().expect_err("r7 is not served");
        assert_eq!(
            error.to_string(),
            "`r7` names no FHIR version this server serves",
            "the reader sees what they typed"
        );
    }

    #[test]
    fn the_default_is_the_release_the_engine_implemented_first() {
        assert_eq!(FhirVersion::default(), FhirVersion::R4B);
    }
}
