// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR model of each release the server mounts, from `fhir-types`.
//!
//! An answer decodes through the module of the version that served it, so
//! a member the version does not define, or a required element it lacks,
//! refuses the answer by its element path (FHIR R4 JSON representation,
//! <https://hl7.org/fhir/R4/json.html>, and its counterpart in each later
//! release). A [`Release`] names the resources the console reads at one
//! version; a [`Decodes`] is what a page reads of one of them, the same for
//! every version. No specification governs this split: our own design.

use serde::de::DeserializeOwned;

use crate::fhir::capability::Capabilities;
use crate::fhir::terminology::TerminologyCapabilities;
use crate::fhir::version::FhirVersion;

/// The resources the console reads at one FHIR release.
pub(crate) trait Release {
    /// The release's `CapabilityStatement`.
    type CapabilityStatement: DeserializeOwned + Reads<Capabilities>;
    /// The release's `TerminologyCapabilities`.
    type TerminologyCapabilities: DeserializeOwned + Reads<TerminologyCapabilities>;
}

/// A resource of one release, read into what a page draws of it.
pub(crate) trait Reads<V> {
    /// What a page draws of this resource.
    fn read(&self) -> V;
}

/// What a page reads of one resource, decoded at the release that served it.
pub(crate) trait Decodes: Sized {
    /// Decodes `body` as this resource of the release `R`.
    ///
    /// # Errors
    /// Returns the decoder's error, which names the element path, for a
    /// body that is not this resource at `R`.
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error>;
}

impl Decodes for Capabilities {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::CapabilityStatement>(body).map(|statement| statement.read())
    }
}

impl Decodes for TerminologyCapabilities {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::TerminologyCapabilities>(body)
            .map(|capabilities| capabilities.read())
    }
}

/// FHIR 4.0.1.
#[derive(Debug)]
pub(crate) struct R4;

/// FHIR 4.3.0.
#[derive(Debug)]
pub(crate) struct R4B;

/// FHIR 5.0.0.
#[derive(Debug)]
pub(crate) struct R5;

/// FHIR 6.0.0 (ballot).
#[derive(Debug)]
pub(crate) struct R6;

/// Implements [`Release`] for `$release` over the `fhir-types` module
/// `$module`.
macro_rules! release {
    ($release:ident, $module:ident) => {
        impl Release for $release {
            type CapabilityStatement =
                fhir_types::$module::capability_statement::CapabilityStatement;
            type TerminologyCapabilities =
                fhir_types::$module::terminology_capabilities::TerminologyCapabilities;
        }
    };
}

release!(R4, r4);
release!(R4B, r4b);
release!(R5, r5);
release!(R6, r6);

impl FhirVersion {
    /// Decodes `body` as the resource `A` reads, at this version.
    ///
    /// # Errors
    /// Returns the decoder's error for a body that is not that resource at
    /// this version.
    pub(crate) fn decode<A: Decodes>(self, body: &str) -> Result<A, serde_json::Error> {
        match self {
            Self::R4 => A::decode::<R4>(body),
            Self::R4B => A::decode::<R4B>(body),
            Self::R5 => A::decode::<R5>(body),
            Self::R6 => A::decode::<R6>(body),
        }
    }
}
