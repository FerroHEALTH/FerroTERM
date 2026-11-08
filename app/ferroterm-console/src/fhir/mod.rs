// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR client: the one module in the console that builds a request.
//!
//! Every request is an ordinary request to the terminology server's public
//! FHIR API, written as an [`exchange::Exchange`] and sent by the console's
//! server ([`crate::upstream::Upstream`]), so no token reaches the browser.
//! No component builds a request, and no second client exists.

pub(crate) mod activity;
pub(crate) mod capability;
pub(crate) mod code_system;
pub(crate) mod compose;
pub(crate) mod concept;
pub(crate) mod concept_map;
pub(crate) mod error;
pub mod exchange;
pub(crate) mod expansion;
pub(crate) mod facts;
pub(crate) mod named;
pub(crate) mod operation;
pub(crate) mod outcome;
pub(crate) mod problem;
pub(crate) mod release;
pub(crate) mod searchset;
pub(crate) mod sync;
pub(crate) mod terminology;
pub(crate) mod translate;
pub(crate) mod validation;
pub(crate) mod value_set;
pub mod version;
pub(crate) mod write;

use http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::fhir::capability::Capabilities;
use crate::fhir::code_system::PublishedCodeSystem;
use crate::fhir::concept::Concept;
use crate::fhir::concept::ConceptQuery;
use crate::fhir::concept::LookupRequest;
use crate::fhir::concept_map::PublishedConceptMap;
use crate::fhir::error::FhirError;
use crate::fhir::exchange::Answer;
use crate::fhir::exchange::Exchange;
use crate::fhir::exchange::ExchangeError;
use crate::fhir::exchange::Method;
use crate::fhir::expansion::ExpandRequest;
use crate::fhir::expansion::ExpandedValueSet;
use crate::fhir::named::NamedSearch;
use crate::fhir::searchset::Found;
use crate::fhir::searchset::SearchFilter;
use crate::fhir::terminology::TerminologyCapabilities;
use crate::fhir::translate::TranslateAnswer;
use crate::fhir::translate::TranslateRequest;
use crate::fhir::validation::SubsumesAnswer;
use crate::fhir::validation::SubsumesRequest;
use crate::fhir::validation::ValidateRequest;
use crate::fhir::validation::ValidationAnswer;
use crate::fhir::value_set::PublishedValueSet;
use crate::fhir::version::FhirVersion;
use crate::url::RequestUrl;

/// The media type a FHIR JSON request asks for.
///
/// The RESTful API defines `application/fhir+json` as the JSON representation
/// (<https://hl7.org/fhir/R4B/http.html#mime-type>).
const FHIR_JSON: &str = "application/fhir+json";

/// The media type of everything the console reads that is not FHIR: the
/// health route and the sync service's admin listener answer ordinary JSON.
const PLAIN_JSON: &str = "application/json";

/// How much of an unparseable failure body is kept as evidence.
const BODY_EXCERPT_BYTES: usize = 2_000;

/// The `ValueSet` resource type, as it appears in a request path.
pub(crate) const VALUE_SET: &str = "ValueSet";

/// The `CodeSystem` resource type, as it appears in a request path.
pub(crate) const CODE_SYSTEM: &str = "CodeSystem";

/// The `ConceptMap` resource type, as it appears in a request path.
pub(crate) const CONCEPT_MAP: &str = "ConceptMap";

/// A client for the FerroTERM server the console is configured with.
///
/// `root` is the address the server's readers reach it at, which every
/// request address a screen discloses starts with; the console's server
/// decides where a request actually goes.
#[derive(Clone, Debug, Default)]
pub(crate) struct FhirClient {
    root: String,
    upstream: Option<crate::upstream::Upstream>,
    locale: ferrohealth_ui::i18n::Locale,
}

impl FhirClient {
    /// A client whose disclosed addresses start with `root`, the server's
    /// public root without a final `/`, with no way to the server: each
    /// exchange is answered [`ExchangeError::Unavailable`].
    pub(crate) fn new(root: String) -> Self {
        Self {
            root,
            upstream: None,
            locale: ferrohealth_ui::i18n::Locale::default(),
        }
    }

    /// The same client saying what went wrong in `locale`.
    #[must_use]
    pub(crate) fn with_locale(self, locale: ferrohealth_ui::i18n::Locale) -> Self {
        Self { locale, ..self }
    }

    /// The same client sending its exchanges through `upstream`.
    #[must_use]
    pub(crate) fn with_upstream(self, upstream: crate::upstream::Upstream) -> Self {
        Self {
            upstream: Some(upstream),
            ..self
        }
    }

    /// Sends `exchange` through the client's way to the server.
    pub(crate) async fn exchange(&self, exchange: Exchange) -> Result<Answer, ExchangeError> {
        match &self.upstream {
            Some(upstream) => upstream.send(exchange).await,
            None => Err(ExchangeError::Unavailable),
        }
    }

    /// The FHIR base of one served version, as a reader would type it.
    pub(crate) fn version_base(&self, version: FhirVersion) -> String {
        RequestUrl::default()
            .segment(version.segment())
            .render(&self.root)
    }

    /// Probes `GET /health` and returns the status the server answered.
    ///
    /// `/health` sits outside the FHIR base, so no content negotiation applies
    /// and the answer carries no body to read.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError::Transport`] when the browser could not reach the
    /// server, and [`FhirError::Status`] when it answered a failure.
    pub(crate) async fn health(&self) -> Result<StatusCode, FhirError> {
        let url = RequestUrl::default().segment("health").render(&self.root);
        let answer = self.send(Method::Get, &url, PLAIN_JSON).await?;
        let status = status_of(&self.locale, &answer, &url)?;
        if status.is_success() {
            Ok(status)
        } else {
            Err(unexplained(&answer, status, &url))
        }
    }

    /// The status `GET /health` answers, or `None` when the server did not
    /// answer at all.
    pub(crate) async fn probe(&self) -> Option<StatusCode> {
        match self.health().await {
            Ok(status) => Some(status),
            Err(error) => error.status(),
        }
    }

    /// Whether the root of `version` serves with no sign-in: it answers its
    /// SMART discovery document `404`, as a deployment with authentication
    /// switched off does
    /// (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
    pub(crate) async fn serves_without_sign_in(&self, version: FhirVersion) -> bool {
        let url = RequestUrl::default()
            .segment(version.segment())
            .segment(".well-known")
            .segment("smart-configuration")
            .render(&self.root);
        let Ok(answer) = self.send(Method::Get, &url, PLAIN_JSON).await else {
            return false;
        };
        status_of(&self.locale, &answer, &url).is_ok_and(|status| status == StatusCode::NOT_FOUND)
    }

    /// The address of the `CapabilityStatement` of one served FHIR version.
    ///
    /// Every read has a paired builder, so a screen can disclose the exact
    /// request it issued without rebuilding the URL from parts of its own.
    pub(crate) fn metadata_url(&self, version: FhirVersion) -> String {
        RequestUrl::default()
            .segment(version.segment())
            .segment("metadata")
            .render(&self.root)
    }

    /// The address of the `TerminologyCapabilities` of one served version.
    ///
    /// `mode=terminology` selects the terminology capabilities of the same
    /// `metadata` interaction
    /// (<https://hl7.org/fhir/R4B/terminologycapabilities.html>).
    pub(crate) fn terminology_metadata_url(&self, version: FhirVersion) -> String {
        RequestUrl::default()
            .segment(version.segment())
            .segment("metadata")
            .query("mode", "terminology")
            .render(&self.root)
    }

    /// The address of the `CodeSystem` resources one root publishes for a
    /// canonical.
    ///
    /// `url` is the search parameter every definitional resource carries
    /// (<https://hl7.org/fhir/R4B/codesystem.html#search>), so this is the
    /// RESTful search interaction rather than an operation. The canonical is
    /// percent-encoded into the query, which matters because a code system URI
    /// can carry its own query string.
    pub(crate) fn code_system_search_url(&self, version: FhirVersion, system: &str) -> String {
        RequestUrl::default()
            .segment(version.segment())
            .segment("CodeSystem")
            .query("url", system)
            .render(&self.root)
    }

    /// The address of the `CodeSystem` resources one root publishes for a
    /// canonical, read with `_summary=true`: a large system's concepts are
    /// left out (<https://hl7.org/fhir/R4B/search.html#summary>).
    pub(crate) fn code_system_summary_url(&self, version: FhirVersion, system: &str) -> String {
        RequestUrl::default()
            .segment(version.segment())
            .segment("CodeSystem")
            .query("url", system)
            .query("_summary", "true")
            .render(&self.root)
    }

    /// Reads the `CodeSystem` resources one root publishes for a canonical,
    /// their concepts left out.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn code_system_summary(
        &self,
        version: FhirVersion,
        system: &str,
    ) -> Result<Document<Found<PublishedCodeSystem>>, FhirError> {
        self.get_answer_document(version, &self.code_system_summary_url(version, system))
            .await
    }

    /// The address the name of every published resource of one type is read
    /// from.
    ///
    /// `_elements` narrows the answer to the four fields a picker needs. A
    /// published resource may carry its whole content inline, so the whole
    /// search is a quarter of a megabyte where this is four kilobytes
    /// (<https://hl7.org/fhir/R5/search.html#elements>).
    pub(crate) fn published_names_url(&self, version: FhirVersion, resource_type: &str) -> String {
        RequestUrl::default()
            .segment(version.segment())
            .segment(resource_type)
            .query("_elements", "url,name,title,version,status,meta")
            .render(&self.root)
    }

    /// Reads the name every published resource of one type carries.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn published_names(
        &self,
        version: FhirVersion,
        resource_type: &str,
    ) -> Result<NamedSearch, FhirError> {
        self.get_answer(version, &self.published_names_url(version, resource_type))
            .await
    }

    /// Reads the `CapabilityStatement` of one served FHIR version as the
    /// server sent it, for a screen that shows the whole document.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn capability_document(
        &self,
        version: FhirVersion,
    ) -> Result<serde_json::Value, FhirError> {
        self.get_json(version, &self.metadata_url(version)).await
    }

    /// Reads the `CapabilityStatement` of one served FHIR version.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn capability_statement(
        &self,
        version: FhirVersion,
    ) -> Result<Capabilities, FhirError> {
        self.get_answer(version, &self.metadata_url(version)).await
    }

    /// Reads the `TerminologyCapabilities` of one served FHIR version.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn terminology_capabilities(
        &self,
        version: FhirVersion,
    ) -> Result<TerminologyCapabilities, FhirError> {
        self.get_answer(version, &self.terminology_metadata_url(version))
            .await
    }

    /// Reads the `CodeSystem` resources one root publishes for a canonical.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn code_system_search(
        &self,
        version: FhirVersion,
        system: &str,
    ) -> Result<Document<Found<PublishedCodeSystem>>, FhirError> {
        self.get_answer_document(version, &self.code_system_search_url(version, system))
            .await
    }

    /// The address one `ValueSet/$expand` run reads.
    ///
    /// `$expand` takes its parameters in the query of a `GET`
    /// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>), and every
    /// one of them is percent-encoded, so an implicit canonical carrying its
    /// own query string stays inside the parameter it belongs to.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError::Unwritten`] when `version` cannot carry the run.
    pub(crate) fn expand_url(
        &self,
        version: FhirVersion,
        request: &ExpandRequest,
    ) -> Result<String, FhirError> {
        self.operation(version, request.request())
            .map(|operation| operation.url().to_owned())
    }

    /// Runs `ValueSet/$expand` and reads the expansion it answers.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong. A
    /// selection the server refuses to expand, `too-costly` among them,
    /// arrives as [`FhirError::Refused`] carrying the server's own
    /// `OperationOutcome`.
    pub(crate) async fn expand(
        &self,
        version: FhirVersion,
        request: &ExpandRequest,
    ) -> Result<ExpandedValueSet, FhirError> {
        let operation = self.operation(version, request.request())?;
        let document = self.operate(version, &operation).await?;
        ExpandedValueSet::read(document.value, operation.url(), version)
    }

    /// The address of a RESTful search over one resource type.
    ///
    /// `url` and `version` are the two search parameters every definitional
    /// resource carries (<https://hl7.org/fhir/R4B/valueset.html#search>), and
    /// each is sent only when the reader named it, so an unfiltered search
    /// asks for everything the root holds. Both are percent-encoded, because a
    /// canonical can carry its own query string.
    pub(crate) fn search_url(
        &self,
        version: FhirVersion,
        resource_type: &str,
        filter: &SearchFilter,
    ) -> String {
        let mut url = RequestUrl::default()
            .segment(version.segment())
            .segment(resource_type);
        if !filter.url.is_empty() {
            url = url.query("url", &filter.url);
        }
        if !filter.version.is_empty() {
            url = url.query("version", &filter.version);
        }
        url.render(&self.root)
    }

    /// The address of one stored resource, as the read interaction takes it.
    ///
    /// The id is one percent-encoded path segment
    /// (<https://hl7.org/fhir/R4B/http.html#read>), so an address a reader
    /// typed cannot reach a route the viewer did not mean to ask for.
    pub(crate) fn resource_url(
        &self,
        version: FhirVersion,
        resource_type: &str,
        id: &str,
    ) -> String {
        RequestUrl::default()
            .segment(version.segment())
            .segment(resource_type)
            .segment(id)
            .render(&self.root)
    }

    /// The address one `CodeSystem/$lookup` reads.
    ///
    /// `$lookup` takes its parameters in the query of a `GET`
    /// (<https://hl7.org/fhir/R4B/codesystem-operation-lookup.html>), and each
    /// one is percent-encoded, so a code carrying a separator stays inside the
    /// parameter it belongs to.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError::Unwritten`] when `version` cannot carry the run.
    pub(crate) fn lookup_url(
        &self,
        version: FhirVersion,
        request: &LookupRequest,
    ) -> Result<String, FhirError> {
        self.operation(version, request.request())
            .map(|operation| operation.url().to_owned())
    }

    /// Reads one concept with `CodeSystem/$lookup`.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong. A code
    /// the system does not hold arrives as [`FhirError::Refused`] carrying the
    /// server's own `OperationOutcome`.
    pub(crate) async fn lookup(
        &self,
        version: FhirVersion,
        request: &LookupRequest,
    ) -> Result<Document<Concept>, FhirError> {
        let operation = self.operation(version, request.request())?;
        let document = self.operate(version, &operation).await?;
        Ok(Document {
            value: Concept::read(document.value, operation.url(), version)?,
            text: document.text,
        })
    }

    /// The address a `ValueSet/$expand` sent as a `POST` is addressed to.
    ///
    /// An operation invoked by `POST` carries every parameter in its
    /// `Parameters` body (<https://hl7.org/fhir/R4B/operations.html#request>),
    /// so the address is the operation itself and nothing else.
    pub(crate) fn expand_post_url(&self, version: FhirVersion) -> String {
        RequestUrl::default()
            .segment(version.segment())
            .segment("ValueSet")
            .segment("$expand")
            .render(&self.root)
    }

    /// Expands a value set the browser sends inline, and reads the answer.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn expand_inline(
        &self,
        version: FhirVersion,
        query: &ConceptQuery,
    ) -> Result<ExpandedValueSet, FhirError> {
        let operation = self.operation(version, query.request())?;
        let document = self.operate(version, &operation).await?;
        ExpandedValueSet::read(document.value, operation.url(), version)
    }

    /// Searches the `ValueSet` resources this root holds.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn value_set_search(
        &self,
        version: FhirVersion,
        filter: &SearchFilter,
    ) -> Result<Found<PublishedValueSet>, FhirError> {
        self.get_answer(version, &self.search_url(version, VALUE_SET, filter))
            .await
    }

    /// Reads one `ValueSet` by its id.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong. An id
    /// this root does not hold arrives as [`FhirError::Refused`] carrying the
    /// server's own `OperationOutcome`.
    pub(crate) async fn value_set_read(
        &self,
        version: FhirVersion,
        id: &str,
    ) -> Result<Document<PublishedValueSet>, FhirError> {
        self.get_answer_document(version, &self.resource_url(version, VALUE_SET, id))
            .await
    }

    /// Searches the `ConceptMap` resources this root holds.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn concept_map_search(
        &self,
        version: FhirVersion,
        filter: &SearchFilter,
    ) -> Result<Found<PublishedConceptMap>, FhirError> {
        self.get_answer(version, &self.search_url(version, CONCEPT_MAP, filter))
            .await
    }

    /// Reads one `ConceptMap` by its id.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong. An id
    /// this root does not hold arrives as [`FhirError::Refused`] carrying the
    /// server's own `OperationOutcome`.
    pub(crate) async fn concept_map_read(
        &self,
        version: FhirVersion,
        id: &str,
    ) -> Result<Document<PublishedConceptMap>, FhirError> {
        self.get_answer_document(version, &self.resource_url(version, CONCEPT_MAP, id))
            .await
    }

    /// The address one `$validate-code` run reads.
    ///
    /// `$validate-code` takes its parameters in the query of a `GET`
    /// (<https://hl7.org/fhir/R4B/codesystem-operation-validate-code.html>),
    /// and each one is percent-encoded, so a canonical carrying its own query
    /// string stays inside the parameter it belongs to. An instance-level run
    /// puts the resource id in the path as one encoded segment, which is what
    /// the read interaction does with an id
    /// (<https://hl7.org/fhir/R4B/http.html#read>).
    ///
    /// # Errors
    ///
    /// Returns [`FhirError::Unwritten`] when `version` cannot carry the run,
    /// or a value set run names no system for its code.
    pub(crate) fn validate_code_url(
        &self,
        version: FhirVersion,
        request: &ValidateRequest,
    ) -> Result<String, FhirError> {
        self.validation(version, request)
            .map(|operation| operation.url().to_owned())
    }

    /// Runs `$validate-code` and reads the `Parameters` it answers.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong. A
    /// refusal arrives as [`FhirError::Refused`] carrying the server's own
    /// `OperationOutcome`. A code the system does not hold is not a refusal:
    /// the operation answers `result` false and says why.
    pub(crate) async fn validate_code(
        &self,
        version: FhirVersion,
        request: &ValidateRequest,
    ) -> Result<Document<ValidationAnswer>, FhirError> {
        let operation = self.validation(version, request)?;
        let document = self.operate(version, &operation).await?;
        Ok(Document {
            value: ValidationAnswer::read(document.value, operation.url(), version)?,
            text: document.text,
        })
    }

    /// The address one `CodeSystem/$subsumes` run reads.
    ///
    /// `$subsumes` takes its parameters in the query of a `GET`
    /// (<https://hl7.org/fhir/R4B/codesystem-operation-subsumes.html>).
    ///
    /// # Errors
    ///
    /// Returns [`FhirError::Unwritten`] when `version` cannot carry the run.
    pub(crate) fn subsumes_url(
        &self,
        version: FhirVersion,
        request: &SubsumesRequest,
    ) -> Result<String, FhirError> {
        self.operation(version, request.request())
            .map(|operation| operation.url().to_owned())
    }

    /// Runs `CodeSystem/$subsumes` and reads the `Parameters` it answers.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong. A
    /// system that answers no subsumption arrives as [`FhirError::Refused`]
    /// carrying the server's own `OperationOutcome`.
    pub(crate) async fn subsumes(
        &self,
        version: FhirVersion,
        request: &SubsumesRequest,
    ) -> Result<Document<SubsumesAnswer>, FhirError> {
        let operation = self.operation(version, request.request())?;
        let document = self.operate(version, &operation).await?;
        Ok(Document {
            value: SubsumesAnswer::read(&document.value, operation.url(), version)?,
            text: document.text,
        })
    }

    /// The address one `ConceptMap/$translate` run reads.
    ///
    /// `$translate` takes its parameters in the query of a `GET`
    /// (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>), under
    /// the names the target version's own `OperationDefinition` declares, and
    /// every one of them is percent-encoded.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError::Unwritten`] when `version` cannot carry the run.
    pub(crate) fn translate_url(
        &self,
        version: FhirVersion,
        request: &TranslateRequest,
    ) -> Result<String, FhirError> {
        self.operation(version, request.request())
            .map(|operation| operation.url().to_owned())
    }

    /// Runs `ConceptMap/$translate` and reads the `Parameters` it answers.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong. A
    /// refusal arrives as [`FhirError::Refused`] carrying the server's own
    /// `OperationOutcome`.
    pub(crate) async fn translate(
        &self,
        version: FhirVersion,
        request: &TranslateRequest,
    ) -> Result<Document<TranslateAnswer>, FhirError> {
        let operation = self.operation(version, request.request())?;
        let document = self.operate(version, &operation).await?;
        Ok(Document {
            value: TranslateAnswer::read(document.value, operation.url(), version)?,
            text: document.text,
        })
    }
}

/// The resources a `searchset` `Bundle` text carries, indented: the one
/// resource when it carries one, an array of them otherwise, and the text as
/// it came when it is no `Bundle`
/// (<https://hl7.org/fhir/R4B/bundle.html>).
pub(crate) fn resources_text(bundle: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(bundle) else {
        return bundle.to_owned();
    };
    let Some(entries) = value.get("entry").and_then(serde_json::Value::as_array) else {
        return bundle.to_owned();
    };
    let resources: Vec<&serde_json::Value> = entries
        .iter()
        .filter_map(|entry| entry.get("resource"))
        .collect();
    let shown = match resources.as_slice() {
        [one] => serde_json::to_string_pretty(one),
        many => serde_json::to_string_pretty(many),
    };
    shown.unwrap_or_else(|_unwritable| bundle.to_owned())
}

/// An answer as a screen reads it and as the server sent it.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Document<T> {
    /// The elements the screen reads.
    pub(crate) value: T,
    /// The whole document, indented for a reader.
    pub(crate) text: String,
}

mod answer;
mod authored;

use answer::{status_of, unexplained};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod client_tests;
