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

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::authoring::code_system;
use crate::authoring::concept_map::MapDraft;
use crate::fhir::capability::Capabilities;
use crate::fhir::code_system::PublishedCodeSystem;
use crate::fhir::compose;
use crate::fhir::compose::stored::StoredValueSet;
use crate::fhir::concept::{Concept, ConceptQuery};
use crate::fhir::concept_map::PublishedConceptMap;
use crate::fhir::expansion::ExpandedValueSet;
use crate::fhir::named::NamedSearch;
use crate::fhir::outcome::Outcome;
use crate::fhir::parameters::Parameter;
use crate::fhir::searchset::Found;
use crate::fhir::terminology::TerminologyCapabilities;
use crate::fhir::translate::TranslateAnswer;
use crate::fhir::validation::{SubsumesAnswer, ValidationAnswer};
use crate::fhir::value_set::PublishedValueSet;
use crate::fhir::version::FhirVersion;
use crate::fhir::write::{History, Stamp, Stored};

/// The resources the console reads at one FHIR release.
pub(crate) trait Release {
    /// The release's `CapabilityStatement`.
    type CapabilityStatement: DeserializeOwned + Reads<Capabilities>;
    /// The release's `TerminologyCapabilities`.
    type TerminologyCapabilities: DeserializeOwned + Reads<TerminologyCapabilities>;
    /// The release's `ValueSet`, read as published and as `$expand`
    /// answered it.
    type ValueSet: DeserializeOwned
        + Serialize
        + Default
        + Reads<PublishedValueSet>
        + Reads<ExpandedValueSet>
        + Reads<StoredValueSet>
        + Rewrites<compose::Draft>;
    /// The release's `Bundle`, read for the resources a `searchset` carries.
    type Bundle: DeserializeOwned
        + Reads<Found<PublishedValueSet>>
        + Reads<Found<PublishedConceptMap>>
        + Reads<Found<code_system::Draft>>
        + Reads<Found<MapDraft>>
        + Reads<History>
        + Reads<NamedSearch>
        + Reads<Found<PublishedCodeSystem>>;
    /// Any resource of the release, read for the stamp a write answers and a
    /// read of one version carries.
    type Resource: DeserializeOwned + Serialize + Reads<Stamp>;
    /// The release's `ConceptMap`.
    type ConceptMap: DeserializeOwned
        + Serialize
        + Default
        + Reads<PublishedConceptMap>
        + Reads<MapDraft>
        + Rewrites<MapDraft>;
    /// The release's `Parameters`, walked as an operation answered them and
    /// written as the concept browser's inline `$expand` and the composer's
    /// preview send them.
    type Parameters: DeserializeOwned
        + Serialize
        + Reads<Vec<Parameter>>
        + Writes<ConceptQuery>
        + Previews<ValueSet = Self::ValueSet>;
    /// The release's `CodeSystem`, opened in the code system editor and
    /// written over by its save.
    type CodeSystem: DeserializeOwned
        + Serialize
        + Default
        + Reads<code_system::Draft>
        + Reads<PublishedCodeSystem>
        + Rewrites<code_system::Draft>;
    /// The release's `OperationOutcome`, read as a refusal carries it.
    type OperationOutcome: DeserializeOwned + Reads<Outcome>;
}

/// A resource of one release, read into what a page draws of it.
pub(crate) trait Reads<V> {
    /// What a page draws of this resource.
    fn read(&self) -> V;
}

/// A resource of one release, written from what a page sends.
pub(crate) trait Writes<V> {
    /// `value`, as this resource.
    fn written(value: &V) -> Self;
}

/// A resource of one release, with what a page authored written over it.
pub(crate) trait Rewrites<D> {
    /// This resource, with `draft` written over it.
    fn rewritten(self, draft: &D) -> Self;
}

/// What a page authors over one resource, written at the release that
/// serves it.
pub(crate) trait Rewritable {
    /// This draft, written over its stored resource as the release `R`
    /// defines it.
    ///
    /// # Errors
    /// Returns the decoder's error for a stored resource that is not this
    /// resource at `R`.
    fn rewrite<R: Release>(&self) -> Result<String, serde_json::Error>;
}

/// A release's `Parameters`, written as the composer's preview posts them.
pub(crate) trait Previews {
    /// The release's `ValueSet`, which the preview carries inline.
    type ValueSet;

    /// The `Parameters` that preview `page` of `value_set`.
    fn previewed(value_set: Self::ValueSet, page: &compose::Preview) -> Self;
}

/// `draft` written over `base`, the resource as the server sent it, as the
/// resource `T` of one release; a draft no server holds yet is written over
/// an empty one.
///
/// # Errors
/// Returns the decoder's error for a `base` that is not a `T`.
fn rewritten_resource<T, D>(base: &Value, draft: &D) -> Result<T, serde_json::Error>
where
    T: DeserializeOwned + Default + Rewrites<D>,
{
    let stored = if base.is_null() {
        T::default()
    } else {
        T::deserialize(base)?
    };
    Ok(stored.rewritten(draft))
}

/// `draft` written over `base` as the resource `T` of one release, as the
/// document a save sends.
///
/// # Errors
/// Returns the decoder's error for a `base` that is not a `T`.
pub(crate) fn rewritten<T, D>(base: &Value, draft: &D) -> Result<String, serde_json::Error>
where
    T: DeserializeOwned + Serialize + Default + Rewrites<D>,
{
    serde_json::to_string(&rewritten_resource::<T, D>(base, draft)?)
}

/// The `Parameters` of the release `R` that preview `page` of `draft`,
/// written over `base`.
///
/// # Errors
/// Returns the decoder's error for a `base` that is not a `ValueSet` at `R`.
fn previewed<R: Release>(
    base: &Value,
    draft: &compose::Draft,
    page: &compose::Preview,
) -> Result<String, serde_json::Error> {
    let value_set = rewritten_resource::<R::ValueSet, compose::Draft>(base, draft)?;
    serde_json::to_string(&R::Parameters::previewed(value_set, page))
}

/// `resource`, a resource a release decoded, as the JSON document it is.
#[expect(
    clippy::expect_used,
    reason = "fhir_types::codec::EncodeError names a decimal with no JSON form and an unknown \
              resource whose body is no object; a resource the decoder read holds neither"
)]
pub(crate) fn document<T: Serialize>(resource: &T) -> Value {
    serde_json::to_value(resource).expect("a decoded resource should always have a JSON form")
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

impl Decodes for PublishedValueSet {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::ValueSet>(body).map(|value_set| value_set.read())
    }
}

impl Decodes for ExpandedValueSet {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::ValueSet>(body).map(|value_set| value_set.read())
    }
}

impl Decodes for StoredValueSet {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::ValueSet>(body).map(|value_set| value_set.read())
    }
}

impl Decodes for Found<PublishedValueSet> {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::Bundle>(body).map(|bundle| bundle.read())
    }
}

impl Decodes for PublishedConceptMap {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::ConceptMap>(body).map(|map| map.read())
    }
}

impl Decodes for Outcome {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::OperationOutcome>(body).map(|outcome| outcome.read())
    }
}

/// Implements [`Decodes`] for `$answer`, an operation's answer read from the
/// parameters of the release's `Parameters`.
macro_rules! decodes_parameters {
    ($answer:ty) => {
        impl Decodes for $answer {
            fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
                serde_json::from_str::<R::Parameters>(body)
                    .map(|parameters| Self::from(parameters.read()))
            }
        }
    };
}

impl Decodes for code_system::Draft {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::CodeSystem>(body).map(|code_system| code_system.read())
    }
}

impl Decodes for Found<code_system::Draft> {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::Bundle>(body).map(|bundle| bundle.read())
    }
}

impl Decodes for NamedSearch {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::Bundle>(body).map(|bundle| bundle.read())
    }
}

impl Decodes for Found<PublishedCodeSystem> {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::Bundle>(body).map(|bundle| bundle.read())
    }
}

impl Decodes for Stamp {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::Resource>(body).map(|resource| resource.read())
    }
}

impl Decodes for Stored {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::Resource>(body).map(|resource| Stored::of(&resource))
    }
}

impl Decodes for History {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::Bundle>(body).map(|bundle| bundle.read())
    }
}

impl Decodes for MapDraft {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::ConceptMap>(body).map(|map| map.read())
    }
}

impl Decodes for Found<MapDraft> {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::Bundle>(body).map(|bundle| bundle.read())
    }
}

decodes_parameters!(TranslateAnswer);
decodes_parameters!(Concept);
decodes_parameters!(ValidationAnswer);
decodes_parameters!(SubsumesAnswer);

impl Decodes for Found<PublishedConceptMap> {
    fn decode<R: Release>(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<R::Bundle>(body).map(|bundle| bundle.read())
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
            type ValueSet = fhir_types::$module::value_set::ValueSet;
            type Bundle = fhir_types::$module::bundle::Bundle;
            type ConceptMap = fhir_types::$module::concept_map::ConceptMap;
            type Parameters = fhir_types::$module::parameters::Parameters;
            type CodeSystem = fhir_types::$module::code_system::CodeSystem;
            type Resource = fhir_types::$module::resource::Resource;
            type OperationOutcome = fhir_types::$module::operation_outcome::OperationOutcome;
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

    /// `draft`, written over its stored resource as this version defines it.
    ///
    /// # Errors
    /// Returns the decoder's error for a stored resource that is not that
    /// resource at this version.
    pub(crate) fn rewrite<D: Rewritable>(self, draft: &D) -> Result<String, serde_json::Error> {
        match self {
            Self::R4 => draft.rewrite::<R4>(),
            Self::R4B => draft.rewrite::<R4B>(),
            Self::R5 => draft.rewrite::<R5>(),
            Self::R6 => draft.rewrite::<R6>(),
        }
    }

    /// The `Parameters` that preview `page` of `draft`, written over `base`
    /// as this version defines them.
    ///
    /// # Errors
    /// Returns the decoder's error for a `base` that is not a `ValueSet` at
    /// this version.
    pub(crate) fn preview(
        self,
        base: &Value,
        draft: &compose::Draft,
        page: &compose::Preview,
    ) -> Result<String, serde_json::Error> {
        match self {
            Self::R4 => previewed::<R4>(base, draft, page),
            Self::R4B => previewed::<R4B>(base, draft, page),
            Self::R5 => previewed::<R5>(base, draft, page),
            Self::R6 => previewed::<R6>(base, draft, page),
        }
    }

    /// `query`, written as the `Parameters` of this version.
    pub(crate) fn encode(self, query: &ConceptQuery) -> String {
        match self {
            Self::R4 => encoded::<R4>(query),
            Self::R4B => encoded::<R4B>(query),
            Self::R5 => encoded::<R5>(query),
            Self::R6 => encoded::<R6>(query),
        }
    }
}

/// `query`, written as the `Parameters` of the release `R`.
#[expect(
    clippy::expect_used,
    reason = "fhir_types::codec::EncodeError names a decimal with no JSON form and an unknown \
              resource; the selection's Parameters hold neither, and its map keys are strings"
)]
fn encoded<R: Release>(query: &ConceptQuery) -> String {
    serde_json::to_string(&R::Parameters::written(query))
        .expect("a selection's Parameters should always have a JSON form")
}

/// What the readers' tests share: a fixture written with only the elements
/// a test is about, completed with the elements its resource requires.
#[cfg(test)]
pub(crate) mod fixture {
    use serde_json::{Map, Value};

    /// `json`, a `resource_type` written with only what a test is about,
    /// completed with the `resourceType`, the `status` and an expansion's
    /// `timestamp` its definition requires, as a document text.
    pub(crate) fn completed(resource_type: &str, json: &str) -> String {
        let mut document: Map<String, Value> =
            serde_json::from_str(json).expect("the fixture is a JSON object");
        document
            .entry("resourceType")
            .or_insert_with(|| Value::from(resource_type));
        document
            .entry("status")
            .or_insert_with(|| Value::from("active"));
        if let Some(Value::Object(expansion)) = document.get_mut("expansion") {
            expansion
                .entry("timestamp")
                .or_insert_with(|| Value::from("2026-01-01T00:00:00Z"));
        }
        Value::Object(document).to_string()
    }
}
