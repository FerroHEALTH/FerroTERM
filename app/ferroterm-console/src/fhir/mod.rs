// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR client: the one module in the console that builds a request.
//!
//! Every request is an ordinary request to the terminology server's public
//! FHIR API, written as an [`exchange::Exchange`] and sent by the console's
//! server ([`crate::upstream::Upstream`]), so no token reaches the browser.
//! No component builds a request, and no second client exists.

pub(crate) mod activity;
pub(crate) mod authoring;
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
pub(crate) mod outcome;
pub(crate) mod problem;
pub(crate) mod searchset;
pub(crate) mod sync;
pub(crate) mod terminology;
pub(crate) mod translate;
pub(crate) mod validation;
pub(crate) mod value_set;
pub mod version;
pub(crate) mod write;

use http::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::fhir::capability::CapabilityStatement;
use crate::fhir::code_system::CodeSystemSearch;
use crate::fhir::compose::Draft;
use crate::fhir::compose::Preview;
use crate::fhir::concept::ConceptQuery;
use crate::fhir::concept::LookupAnswer;
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
use crate::fhir::outcome::OperationOutcome;
use crate::fhir::searchset::SearchFilter;
use crate::fhir::searchset::SearchSet;
use crate::fhir::terminology::TerminologyCapabilities;
use crate::fhir::translate::TranslateAnswer;
use crate::fhir::translate::TranslateRequest;
use crate::fhir::validation::ParametersAnswer;
use crate::fhir::validation::SubsumesRequest;
use crate::fhir::validation::ValidateRequest;
use crate::fhir::value_set::PublishedValueSet;
use crate::fhir::version::FhirVersion;
use crate::fhir::write::History;
use crate::fhir::write::Written;
use crate::fhir::write::if_match;
use crate::locale::{Msg, tr, tr_with};
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
            locale: ferrohealth_ui::i18n::Locale::english(),
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
        RequestUrl::new()
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
        let url = RequestUrl::new().segment("health").render(&self.root);
        let answer = self.send(Method::Get, &url, PLAIN_JSON).await?;
        let status = status_of(&self.locale, &answer, &url)?;
        if status.is_success() {
            Ok(status)
        } else {
            Err(failure(&answer, status, &url))
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
        let url = RequestUrl::new()
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
        RequestUrl::new()
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
        RequestUrl::new()
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
        RequestUrl::new()
            .segment(version.segment())
            .segment("CodeSystem")
            .query("url", system)
            .render(&self.root)
    }

    /// The address of the `CodeSystem` resources one root publishes for a
    /// canonical, read with `_summary=true`: a large system's concepts are
    /// left out (<https://hl7.org/fhir/R4B/search.html#summary>).
    pub(crate) fn code_system_summary_url(&self, version: FhirVersion, system: &str) -> String {
        RequestUrl::new()
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
    ) -> Result<Document<CodeSystemSearch>, FhirError> {
        self.get_document(&self.code_system_summary_url(version, system))
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
        RequestUrl::new()
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
        self.get_json(&self.published_names_url(version, resource_type))
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
        self.get_json(&self.metadata_url(version)).await
    }

    /// Reads the `CapabilityStatement` of one served FHIR version.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn capability_statement(
        &self,
        version: FhirVersion,
    ) -> Result<CapabilityStatement, FhirError> {
        self.get_json(&self.metadata_url(version)).await
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
        self.get_json(&self.terminology_metadata_url(version)).await
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
    ) -> Result<Document<CodeSystemSearch>, FhirError> {
        self.get_document(&self.code_system_search_url(version, system))
            .await
    }

    /// The address one `ValueSet/$expand` run reads.
    ///
    /// `$expand` takes its parameters in the query of a `GET`
    /// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>), and every
    /// one of them is percent-encoded, so an implicit canonical carrying its
    /// own query string stays inside the parameter it belongs to.
    pub(crate) fn expand_url(&self, version: FhirVersion, request: &ExpandRequest) -> String {
        request
            .append(
                RequestUrl::new()
                    .segment(version.segment())
                    .segment("ValueSet")
                    .segment("$expand"),
            )
            .render(&self.root)
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
        self.get_json(&self.expand_url(version, request)).await
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
        let mut url = RequestUrl::new()
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
        RequestUrl::new()
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
    pub(crate) fn lookup_url(&self, version: FhirVersion, request: &LookupRequest) -> String {
        request
            .append(
                RequestUrl::new()
                    .segment(version.segment())
                    .segment("CodeSystem")
                    .segment("$lookup"),
            )
            .render(&self.root)
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
    ) -> Result<Document<LookupAnswer>, FhirError> {
        self.get_document(&self.lookup_url(version, request)).await
    }

    /// The address a `ValueSet/$expand` sent as a `POST` is addressed to.
    ///
    /// An operation invoked by `POST` carries every parameter in its
    /// `Parameters` body (<https://hl7.org/fhir/R4B/operations.html#request>),
    /// so the address is the operation itself and nothing else.
    pub(crate) fn expand_post_url(&self, version: FhirVersion) -> String {
        RequestUrl::new()
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
        self.post_json(&self.expand_post_url(version), &query.body())
            .await
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
    ) -> Result<SearchSet<PublishedValueSet>, FhirError> {
        self.get_json(&self.search_url(version, VALUE_SET, filter))
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
        self.get_document(&self.resource_url(version, VALUE_SET, id))
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
    ) -> Result<SearchSet<PublishedConceptMap>, FhirError> {
        self.get_json(&self.search_url(version, CONCEPT_MAP, filter))
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
        self.get_document(&self.resource_url(version, CONCEPT_MAP, id))
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
    pub(crate) fn validate_code_url(
        &self,
        version: FhirVersion,
        request: &ValidateRequest,
    ) -> String {
        let mut url = RequestUrl::new()
            .segment(version.segment())
            .segment(request.on.resource_type());
        if request.instance() {
            url = url.segment(&request.id);
        }
        request
            .append(url.segment("$validate-code"))
            .render(&self.root)
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
    ) -> Result<Document<ParametersAnswer>, FhirError> {
        self.get_document(&self.validate_code_url(version, request))
            .await
    }

    /// The address one `CodeSystem/$subsumes` run reads.
    ///
    /// `$subsumes` takes its parameters in the query of a `GET`
    /// (<https://hl7.org/fhir/R4B/codesystem-operation-subsumes.html>).
    pub(crate) fn subsumes_url(&self, version: FhirVersion, request: &SubsumesRequest) -> String {
        let mut url = RequestUrl::new()
            .segment(version.segment())
            .segment(CODE_SYSTEM);
        if request.instance() {
            url = url.segment(&request.id);
        }
        request.append(url.segment("$subsumes")).render(&self.root)
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
    ) -> Result<Document<ParametersAnswer>, FhirError> {
        self.get_document(&self.subsumes_url(version, request))
            .await
    }

    /// The address one `ConceptMap/$translate` run reads.
    ///
    /// `$translate` takes its parameters in the query of a `GET`
    /// (<https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>), under
    /// the names the target version's own `OperationDefinition` declares, and
    /// every one of them is percent-encoded.
    pub(crate) fn translate_url(&self, version: FhirVersion, request: &TranslateRequest) -> String {
        request
            .append(
                RequestUrl::new()
                    .segment(version.segment())
                    .segment(CONCEPT_MAP)
                    .segment("$translate"),
                version,
            )
            .render(&self.root)
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
        self.get_document(&self.translate_url(version, request))
            .await
    }
}

/// The reads the value set composer makes.
impl FhirClient {
    /// Reads one `ValueSet` as the document the server sent.
    ///
    /// The read is the ordinary RESTful read
    /// (<https://hl7.org/fhir/R4B/http.html#read>); the composer needs the
    /// whole `compose` and the `meta` an update states, which the listing
    /// screen's narrower reader does not carry.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong. An id
    /// this root does not hold arrives as [`FhirError::Refused`] carrying the
    /// server's own `OperationOutcome`.
    pub(crate) async fn value_set_stored(
        &self,
        version: FhirVersion,
        id: &str,
    ) -> Result<serde_json::Value, FhirError> {
        self.get_json(&self.resource_url(version, VALUE_SET, id))
            .await
    }

    /// Expands a compose that has never been saved, one page at a time.
    ///
    /// The draft travels in the `valueSet` parameter of a `POST`, which is the
    /// alternative to naming a value set by `url`
    /// (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>).
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong. A
    /// compose the server refuses, an expression it cannot parse among them,
    /// arrives as [`FhirError::Refused`] carrying its own `OperationOutcome`.
    pub(crate) async fn preview_compose(
        &self,
        version: FhirVersion,
        draft: &Draft,
        page: &Preview,
    ) -> Result<ExpandedValueSet, FhirError> {
        self.post_json(&self.expand_post_url(version), &draft.preview_body(page))
            .await
    }
}

/// The write client: create, update, delete, and the version history.
///
/// The server carries these interactions on `CodeSystem`, `ValueSet`, and
/// `ConceptMap` (<https://hl7.org/fhir/R4B/http.html>), gated on a SMART scope
/// where the deployment configured an issuer.
impl FhirClient {
    /// The address a create posts to.
    pub(crate) fn create_url(&self, version: FhirVersion, resource_type: &str) -> String {
        RequestUrl::new()
            .segment(version.segment())
            .segment(resource_type)
            .render(&self.root)
    }

    /// The address one resource's version history is read from.
    ///
    /// `_history` on an instance answers a `history` `Bundle`
    /// (<https://hl7.org/fhir/R4B/http.html#history>).
    pub(crate) fn history_url(
        &self,
        version: FhirVersion,
        resource_type: &str,
        id: &str,
    ) -> String {
        RequestUrl::new()
            .segment(version.segment())
            .segment(resource_type)
            .segment(id)
            .segment("_history")
            .render(&self.root)
    }

    /// Creates a resource (<https://hl7.org/fhir/R4B/http.html#create>).
    ///
    /// The console's server presents the signed-in reader's token; a visitor's
    /// request goes unauthenticated and a server that gates its writes answers
    /// `401`.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong. A
    /// refusal arrives as [`FhirError::Refused`] carrying the server's own
    /// `OperationOutcome`, which the screen renders verbatim.
    pub(crate) async fn create(
        &self,
        version: FhirVersion,
        resource_type: &str,
        body: &str,
    ) -> Result<Written, FhirError> {
        let url = self.create_url(version, resource_type);
        let exchange = Exchange::new(Method::Post, self.path_of(&url)?, FHIR_JSON)
            .with_body(FHIR_JSON, body.to_owned());
        self.write(exchange, &url).await
    }

    /// Updates a resource (<https://hl7.org/fhir/R4B/http.html#update>).
    ///
    /// `version_id` is the version being replaced, sent as `If-Match`, so a
    /// resource another reader has changed since is refused with `412` rather
    /// than overwritten (<https://hl7.org/fhir/R4B/http.html#concurrency>). A
    /// caller that holds no version sends none, which is an unconditional
    /// update.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn update(
        &self,
        version: FhirVersion,
        resource_type: &str,
        id: &str,
        body: &str,
        version_id: Option<&str>,
    ) -> Result<Written, FhirError> {
        let url = self.resource_url(version, resource_type, id);
        let exchange = Exchange::new(Method::Put, self.path_of(&url)?, FHIR_JSON)
            .with_body(FHIR_JSON, body.to_owned())
            .with_if_match(version_id.map(if_match));
        self.write(exchange, &url).await
    }

    /// Deletes a resource (<https://hl7.org/fhir/R4B/http.html#delete>).
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong. A
    /// server that refuses says why in an `OperationOutcome`.
    pub(crate) async fn delete(
        &self,
        version: FhirVersion,
        resource_type: &str,
        id: &str,
    ) -> Result<(), FhirError> {
        let url = self.resource_url(version, resource_type, id);
        let answer = self.send(Method::Delete, &url, FHIR_JSON).await?;
        let status = status_of(&self.locale, &answer, &url)?;
        if status.is_success() {
            Ok(())
        } else {
            Err(failure(&answer, status, &url))
        }
    }

    /// The address every version of every resource of one type is read from:
    /// the type-level `_history`
    /// (<https://hl7.org/fhir/R4B/http.html#history>).
    pub(crate) fn type_history_url(&self, version: FhirVersion, resource_type: &str) -> String {
        RequestUrl::new()
            .segment(version.segment())
            .segment(resource_type)
            .segment("_history")
            .render(&self.root)
    }

    /// Reads every version of every resource of one type.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn type_history(
        &self,
        version: FhirVersion,
        resource_type: &str,
    ) -> Result<History, FhirError> {
        self.get_json(&self.type_history_url(version, resource_type))
            .await
    }

    /// Reads one resource's version history.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn history(
        &self,
        version: FhirVersion,
        resource_type: &str,
        id: &str,
    ) -> Result<History, FhirError> {
        self.get_json(&self.history_url(version, resource_type, id))
            .await
    }

    /// Sends one write and reads what the server committed.
    async fn write(&self, exchange: Exchange, url: &str) -> Result<Written, FhirError> {
        let answer = self
            .exchange(exchange)
            .await
            .map_err(|error| transport(url, &error))?;
        let status = status_of(&self.locale, &answer, url)?;
        if !status.is_success() {
            return Err(failure(&answer, status, url));
        }
        // A server asked for a minimal return answers no body, which is a
        // success and not a decode failure
        // (<https://hl7.org/fhir/R4B/http.html#return>).
        let resource = if answer.body.trim().is_empty() {
            None
        } else {
            Some(
                serde_json::from_str(&answer.body).map_err(|error| FhirError::Decode {
                    url: url.to_owned(),
                    message: error.to_string(),
                })?,
            )
        };
        Ok(Written {
            resource,
            etag: answer.etag,
            location: answer.location,
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

/// The reads the authoring screens make, which no reading screen needs.
impl FhirClient {
    /// Reads the `CodeSystem` one root holds for a canonical, for authoring.
    ///
    /// The search is the same RESTful interaction the detail screen makes
    /// (<https://hl7.org/fhir/R4B/http.html#search>), and the resource comes
    /// back as the server sent it. An update replaces the whole resource
    /// (<https://hl7.org/fhir/R4B/http.html#update>), so an element the editor
    /// does not draw still has to reach the server again, which a typed read
    /// of the elements it does draw could not carry.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong. A root
    /// that holds no such code system answers `Ok(None)`, which is a different
    /// answer from a read that failed.
    pub(crate) async fn authored(
        &self,
        version: FhirVersion,
        system: &str,
    ) -> Result<Option<serde_json::Value>, FhirError> {
        let url = self.code_system_search_url(version, system);
        let answer: authoring::AuthoredSearch = self.get_json(&url).await?;
        Ok(answer.first())
    }

    /// Reads the `ConceptMap` one root holds for a canonical, for authoring.
    ///
    /// The resource comes back as the server sent it, for the reason
    /// [`FhirClient::authored`] records: an update replaces the whole resource
    /// (<https://hl7.org/fhir/R4B/http.html#update>), so an element the editor
    /// does not draw still has to reach the server again.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong. A root
    /// that holds no such concept map answers `Ok(None)`, which is a different
    /// answer from a read that failed.
    pub(crate) async fn authored_map(
        &self,
        version: FhirVersion,
        canonical: &str,
    ) -> Result<Option<serde_json::Value>, FhirError> {
        let filter = SearchFilter {
            url: canonical.to_owned(),
            version: String::new(),
        };
        let url = self.search_url(version, CONCEPT_MAP, &filter);
        let answer: authoring::AuthoredSearch = self.get_json(&url).await?;
        Ok(answer.first_of(CONCEPT_MAP))
    }

    /// The address one version of one resource is read from.
    ///
    /// `vread` addresses a version below the instance
    /// (<https://hl7.org/fhir/R4B/http.html#vread>), and both the id and the
    /// version are one percent-encoded path segment each.
    pub(crate) fn version_read_url(
        &self,
        version: FhirVersion,
        resource_type: &str,
        id: &str,
        version_id: &str,
    ) -> String {
        RequestUrl::new()
            .segment(version.segment())
            .segment(resource_type)
            .segment(id)
            .segment("_history")
            .segment(version_id)
            .render(&self.root)
    }

    /// Reads one resource as the server holds it now.
    ///
    /// The resource comes back as the server sent it, because a restore sends
    /// a whole resource back and an element this viewer has never heard of
    /// cannot survive a typed read.
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn resource(
        &self,
        version: FhirVersion,
        resource_type: &str,
        id: &str,
    ) -> Result<serde_json::Value, FhirError> {
        self.get_json(&self.resource_url(version, resource_type, id))
            .await
    }

    /// Reads one version of one resource (<https://hl7.org/fhir/R4B/http.html#vread>).
    ///
    /// # Errors
    ///
    /// Returns the variant of [`FhirError`] describing what went wrong.
    pub(crate) async fn version_read(
        &self,
        version: FhirVersion,
        resource_type: &str,
        id: &str,
        version_id: &str,
    ) -> Result<serde_json::Value, FhirError> {
        self.get_json(&self.version_read_url(version, resource_type, id, version_id))
            .await
    }
}

impl FhirClient {
    /// The path below the server's root that `url`, one of this client's
    /// addresses, names, as an exchange carries it.
    fn path_of(&self, url: &str) -> Result<String, FhirError> {
        let locale = &self.locale;
        url.strip_prefix(self.root.as_str())
            .filter(|path| path.starts_with('/'))
            .map(str::to_owned)
            .ok_or_else(|| FhirError::Transport {
                url: url.to_owned(),
                message: tr(locale, Msg::AddressNotBelowServerSRoot),
            })
    }

    /// Sends a request of `method` to `url` with no body, asking for
    /// `accept`, and answers what came back.
    async fn send(&self, method: Method, url: &str, accept: &str) -> Result<Answer, FhirError> {
        self.exchange(Exchange::new(method, self.path_of(url)?, accept))
            .await
            .map_err(|error| transport(url, &error))
    }

    /// Sends a FHIR JSON `GET` and keeps both the elements a screen reads and
    /// the document as the server sent it, which a screen shows as JSON.
    async fn get_document<T: DeserializeOwned>(&self, url: &str) -> Result<Document<T>, FhirError> {
        let answer = self.send(Method::Get, url, FHIR_JSON).await?;
        let value = read_json(&self.locale, &answer, url)?;
        Ok(Document {
            value,
            text: pretty(&answer.body),
        })
    }

    /// Sends a FHIR JSON `GET` and decodes the resource it answers.
    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, FhirError> {
        let answer = self.send(Method::Get, url, FHIR_JSON).await?;
        read_json(&self.locale, &answer, url)
    }

    /// Sends a FHIR JSON `POST` and decodes the resource it answers.
    async fn post_json<T: DeserializeOwned>(&self, url: &str, body: &str) -> Result<T, FhirError> {
        let exchange = Exchange::new(Method::Post, self.path_of(url)?, FHIR_JSON)
            .with_body(FHIR_JSON, body.to_owned());
        let answer = self
            .exchange(exchange)
            .await
            .map_err(|error| transport(url, &error))?;
        read_json(&self.locale, &answer, url)
    }
}

/// Turns an answer into the resource it carries, or into the refusal.
/// A JSON document indented two spaces a level, for a reader; a body that is
/// no JSON (which `read_json` has already refused) stays as it came.
fn pretty(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| body.to_owned())
}

fn read_json<T: DeserializeOwned>(
    locale: &ferrohealth_ui::i18n::Locale,
    answer: &Answer,
    url: &str,
) -> Result<T, FhirError> {
    let status = status_of(locale, answer, url)?;
    if !status.is_success() {
        return Err(failure(answer, status, url));
    }
    serde_json::from_str(&answer.body).map_err(|error| FhirError::Decode {
        url: url.to_owned(),
        message: error.to_string(),
    })
}

/// The failure of an exchange that produced no answer.
fn transport(url: &str, error: &ExchangeError) -> FhirError {
    FhirError::Transport {
        url: url.to_owned(),
        message: error.to_string(),
    }
}

/// Reads the answered status as a [`StatusCode`].
///
/// A status outside 100 to 999 is no HTTP status
/// (<https://www.rfc-editor.org/rfc/rfc9110#section-15>), so the conversion is
/// fallible and says so.
fn status_of(
    locale: &ferrohealth_ui::i18n::Locale,
    answer: &Answer,
    url: &str,
) -> Result<StatusCode, FhirError> {
    StatusCode::from_u16(answer.status).map_err(|_invalid| FhirError::Transport {
        url: url.to_owned(),
        message: tr_with(
            locale,
            Msg::ServerAnsweredStatus,
            &[("status", (answer.status).to_string().into())],
        ),
    })
}

/// Turns a non-success answer into the refusal the reader is shown.
fn failure(answer: &Answer, status: StatusCode, url: &str) -> FhirError {
    match serde_json::from_str::<OperationOutcome>(&answer.body) {
        // A body that parses but carries no issue says nothing, so the body
        // itself stays the evidence.
        Ok(outcome) if !outcome.issue.is_empty() => FhirError::Refused {
            url: url.to_owned(),
            status,
            outcome,
        },
        Ok(_) | Err(_) => FhirError::Status {
            url: url.to_owned(),
            status,
            body: excerpt(&answer.body),
        },
    }
}

/// Keeps the first part of a body, cut on a character boundary.
fn excerpt(body: &str) -> String {
    body.char_indices()
        .take_while(|(index, _)| *index < BODY_EXCERPT_BYTES)
        .map(|(_, character)| character)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_base_is_the_root_plus_the_version_segment() {
        let client = FhirClient::new("https://tx.example.org".to_owned());
        assert_eq!(
            client.version_base(FhirVersion::R4B),
            "https://tx.example.org/r4b"
        );
    }

    #[test]
    fn the_metadata_addresses_are_the_ones_a_reader_would_type() {
        let client = FhirClient::new("https://tx.example.org".to_owned());
        assert_eq!(
            client.metadata_url(FhirVersion::R5),
            "https://tx.example.org/r5/metadata"
        );
        assert_eq!(
            client.terminology_metadata_url(FhirVersion::R6),
            "https://tx.example.org/r6/metadata?mode=terminology"
        );
    }

    #[test]
    fn a_code_system_search_encodes_the_canonical_it_asks_about() {
        let client = FhirClient::new("https://tx.example.org".to_owned());
        assert_eq!(
            client.code_system_search_url(
                FhirVersion::R4B,
                "https://terminology.example/x?edition=2031"
            ),
            "https://tx.example.org/r4b/CodeSystem?url=https%3A%2F%2Fterminology.example%2Fx%3Fedition%3D2031",
            "a canonical carrying its own query string cannot truncate the search"
        );
    }

    #[test]
    fn an_expansion_address_carries_an_implicit_canonical_whole() {
        let client = FhirClient::new("https://tx.example.org".to_owned());
        let request = ExpandRequest {
            // An implicit value set canonical carries its own query string,
            // and an unencoded one would truncate the request around it.
            url: "http://snomed.info/sct?fhir_vs=isa/404684003".to_owned(),
            count: Some(20),
            offset: Some(40),
            ..ExpandRequest::default()
        };
        assert_eq!(
            client.expand_url(FhirVersion::R4B, &request),
            "https://tx.example.org/r4b/ValueSet/$expand\
             ?url=http%3A%2F%2Fsnomed.info%2Fsct%3Ffhir_vs%3Disa%2F404684003&count=20&offset=40",
            "the operation name survives the path and the canonical survives the query"
        );
    }

    #[test]
    fn a_lookup_address_is_the_one_a_reader_would_type() {
        let client = FhirClient::new("https://tx.example.org".to_owned());
        let request = LookupRequest {
            system: "http://snomed.info/sct".to_owned(),
            code: "404684003".to_owned(),
            ..LookupRequest::default()
        };
        assert_eq!(
            client.lookup_url(FhirVersion::R5, &request),
            "https://tx.example.org/r5/CodeSystem/$lookup\
             ?system=http%3A%2F%2Fsnomed.info%2Fsct&code=404684003",
            "the operation name survives the path and the canonical survives the query"
        );
    }

    #[test]
    fn an_inline_expansion_addresses_the_operation_and_carries_its_body() {
        let client = FhirClient::new("https://tx.example.org".to_owned());
        assert_eq!(
            client.expand_post_url(FhirVersion::R4B),
            "https://tx.example.org/r4b/ValueSet/$expand",
            "a POST carries its parameters in the body, so the address has no query"
        );
    }

    #[test]
    fn a_search_sends_only_the_parameters_the_reader_named() {
        let client = FhirClient::new("https://tx.example.org".to_owned());
        assert_eq!(
            client.search_url(FhirVersion::R4B, VALUE_SET, &SearchFilter::default()),
            "https://tx.example.org/r4b/ValueSet",
            "an unfiltered search asks for everything the root holds"
        );
        assert_eq!(
            client.search_url(
                FhirVersion::R5,
                CONCEPT_MAP,
                &SearchFilter {
                    url: "https://terminology.example/cm?a=b".to_owned(),
                    version: "2031".to_owned(),
                }
            ),
            "https://tx.example.org/r5/ConceptMap\
             ?url=https%3A%2F%2Fterminology.example%2Fcm%3Fa%3Db&version=2031",
            "a canonical carrying its own query string cannot truncate the search"
        );
    }

    #[test]
    fn a_resource_read_escapes_the_id_into_its_own_segment() {
        let client = FhirClient::new("https://tx.example.org".to_owned());
        assert_eq!(
            client.resource_url(FhirVersion::R6, VALUE_SET, "a/b"),
            "https://tx.example.org/r6/ValueSet/a%2Fb",
            "an id a reader typed cannot reach a route the viewer did not mean to ask for"
        );
    }

    #[test]
    fn a_translation_sends_the_names_the_target_version_declares() {
        let client = FhirClient::new("https://tx.example.org".to_owned());
        let request = TranslateRequest {
            system: "http://snomed.info/sct".to_owned(),
            code: "404684003".to_owned(),
            ..TranslateRequest::default()
        };
        assert_eq!(
            client.translate_url(FhirVersion::R4B, &request),
            "https://tx.example.org/r4b/ConceptMap/$translate\
             ?system=http%3A%2F%2Fsnomed.info%2Fsct&code=404684003"
        );
        assert_eq!(
            client.translate_url(FhirVersion::R6, &request),
            "https://tx.example.org/r6/ConceptMap/$translate\
             ?sourceSystem=http%3A%2F%2Fsnomed.info%2Fsct&sourceCode=404684003",
            "the R6 ballot renamed `system` and `code`"
        );
    }

    #[test]
    fn a_validation_addresses_the_resource_type_it_is_invoked_on() {
        let client = FhirClient::new("https://tx.example.org".to_owned());
        let request = ValidateRequest {
            url: "https://terminology.example/x?edition=2031".to_owned(),
            code: "cat".to_owned(),
            ..ValidateRequest::default()
        };
        assert_eq!(
            client.validate_code_url(FhirVersion::R4B, &request),
            "https://tx.example.org/r4b/CodeSystem/$validate-code\
             ?url=https%3A%2F%2Fterminology.example%2Fx%3Fedition%3D2031&code=cat",
            "the operation name survives the path and the canonical survives the query"
        );
        assert_eq!(
            client.validate_code_url(
                FhirVersion::R5,
                &ValidateRequest {
                    on: validation::ValidateOn::ValueSet,
                    id: "a/b".to_owned(),
                    ..request
                }
            ),
            "https://tx.example.org/r5/ValueSet/a%2Fb/$validate-code?code=cat",
            "an id a reader typed cannot reach a route the viewer did not mean to ask for"
        );
    }

    #[test]
    fn a_subsumption_addresses_the_code_system_and_carries_both_codes() {
        let client = FhirClient::new("https://tx.example.org".to_owned());
        let request = SubsumesRequest {
            system: "http://snomed.info/sct".to_owned(),
            code_a: "404684003".to_owned(),
            code_b: "64572001".to_owned(),
            ..SubsumesRequest::default()
        };
        assert_eq!(
            client.subsumes_url(FhirVersion::R6, &request),
            "https://tx.example.org/r6/CodeSystem/$subsumes\
             ?system=http%3A%2F%2Fsnomed.info%2Fsct&codeA=404684003&codeB=64572001"
        );
        assert_eq!(
            client.subsumes_url(
                FhirVersion::R4,
                &SubsumesRequest {
                    id: "animals".to_owned(),
                    system: String::new(),
                    ..request
                }
            ),
            "https://tx.example.org/r4/CodeSystem/animals/$subsumes?codeA=404684003&codeB=64572001",
            "an instance run names the system in the path and nowhere else"
        );
    }

    #[test]
    fn a_create_addresses_the_resource_type_and_a_history_the_instance() {
        let client = FhirClient::new("https://tx.example.org".to_owned());
        assert_eq!(
            client.create_url(FhirVersion::R5, CODE_SYSTEM),
            "https://tx.example.org/r5/CodeSystem",
            "a create posts to the type, which is what assigns the id"
        );
        assert_eq!(
            client.history_url(FhirVersion::R6, VALUE_SET, "a/b"),
            "https://tx.example.org/r6/ValueSet/a%2Fb/_history",
            "an id a reader typed cannot reach a route the viewer did not mean to ask for"
        );
    }

    #[test]
    fn a_long_failure_body_is_cut_on_a_character_boundary() {
        let body = "é".repeat(BODY_EXCERPT_BYTES);
        let kept = excerpt(&body);
        assert!(
            kept.len() <= BODY_EXCERPT_BYTES + 1,
            "the excerpt stops at the bound, not past it: {} bytes",
            kept.len()
        );
        assert!(
            body.starts_with(&kept),
            "the excerpt is a prefix of the body the server sent"
        );
    }
}

#[cfg(test)]
mod document_tests {
    use super::resources_text;

    #[test]
    fn a_bundle_of_one_shows_the_resource_and_of_several_an_array() {
        let one = r#"{"resourceType":"Bundle","entry":[{"resource":{"id":"a"}}]}"#;
        assert_eq!(resources_text(one), "{\n  \"id\": \"a\"\n}");
        let two = r#"{"entry":[{"resource":{"id":"a"}},{"resource":{"id":"b"}}]}"#;
        assert!(resources_text(two).starts_with('['));
        assert_eq!(resources_text("not json"), "not json");
    }
}

#[cfg(test)]
mod client_tests {
    use super::*;

    /// The root readers reach the server at, which every address is built on.
    const ROOT: &str = "https://tx.example.org";

    /// The console's state, whose upstream client reaches the FHIR server at
    /// `base`.
    fn console_at(base: &str) -> crate::server::ConsoleState {
        let mut config = crate::config::Config::default();
        config.ferroterm.base_url = base.to_owned();
        crate::server::ConsoleState::new(config.resolve().expect("the settings resolve"))
            .expect("the clients build")
    }

    /// Runs `run` with a client reaching the server through `state`, as the
    /// request carrying `cookie` does.
    async fn served<T, F: Future<Output = T>>(
        state: crate::server::ConsoleState,
        cookie: Option<&str>,
        run: impl FnOnce(FhirClient) -> F,
    ) -> T {
        let mut headers = http::HeaderMap::new();
        if let Some(cookie) = cookie {
            headers.insert(
                http::header::COOKIE,
                http::HeaderValue::from_str(cookie).expect("a cookie header"),
            );
        }
        let upstream = crate::upstream::Upstream::of(&state, &headers);
        run(FhirClient::new(ROOT.to_owned()).with_upstream(upstream)).await
    }

    /// A FHIR server answering `GET path` with `status` and `body`.
    async fn answering(path: &str, status: u16, body: &str) -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(path))
            .respond_with(
                wiremock::ResponseTemplate::new(status)
                    .set_body_raw(body.to_owned(), "application/fhir+json"),
            )
            .mount(&server)
            .await;
        server
    }

    /// The one request `server` received.
    async fn the_request(server: &wiremock::MockServer) -> wiremock::Request {
        let mut received = server.received_requests().await.expect("recording");
        assert_eq!(1, received.len(), "one request");
        received.remove(0)
    }

    /// An address no server answers at.
    const UNREACHABLE: &str = "http://127.0.0.1:9/";

    #[tokio::test]
    async fn a_healthy_server_answers_its_status_and_the_probe_reads_it() {
        let server = answering("/health", 200, "").await;
        let state = console_at(&server.uri());
        let (health, probe) = served(state, None, |client| async move {
            (client.health().await, client.probe().await)
        })
        .await;
        assert_eq!(StatusCode::OK, health.expect("healthy"));
        assert_eq!(Some(StatusCode::OK), probe);
    }

    #[tokio::test]
    async fn a_failing_server_s_status_is_a_failure_the_probe_still_reports() {
        let server = answering("/health", 503, "down").await;
        let (health, probe) = served(console_at(&server.uri()), None, |client| async move {
            (client.health().await, client.probe().await)
        })
        .await;
        let error = health.expect_err("a failure");
        assert!(
            matches!(&error, FhirError::Status { status, body, .. }
                if *status == StatusCode::SERVICE_UNAVAILABLE && body == "down"),
            "{error:?}"
        );
        assert_eq!(Some(StatusCode::SERVICE_UNAVAILABLE), probe);
    }

    #[tokio::test]
    async fn a_server_that_does_not_answer_probes_as_none() {
        let (health, probe) = served(console_at(UNREACHABLE), None, |client| async move {
            (client.health().await, client.probe().await)
        })
        .await;
        assert!(
            matches!(health, Err(FhirError::Transport { .. })),
            "{health:?}"
        );
        assert_eq!(None, probe);
    }

    #[tokio::test]
    async fn a_root_whose_smart_discovery_is_absent_serves_without_sign_in() {
        let server = answering("/r4/.well-known/smart-configuration", 404, "").await;
        assert!(
            served(console_at(&server.uri()), None, |client| async move {
                client.serves_without_sign_in(FhirVersion::R4).await
            })
            .await
        );
    }

    #[tokio::test]
    async fn a_root_that_publishes_smart_discovery_asks_for_a_sign_in() {
        let server = answering("/r5/.well-known/smart-configuration", 200, "{}").await;
        assert!(
            !served(console_at(&server.uri()), None, |client| async move {
                client.serves_without_sign_in(FhirVersion::R5).await
            })
            .await
        );
        assert!(
            !served(console_at(UNREACHABLE), None, |client| async move {
                client.serves_without_sign_in(FhirVersion::R5).await
            })
            .await,
            "a root that does not answer is not known to serve without one"
        );
    }

    #[tokio::test]
    async fn a_code_system_summary_asks_without_concepts_and_keeps_the_document() {
        let body = r#"{"resourceType":"Bundle","type":"searchset","total":1,"entry":[{"resource":{"resourceType":"CodeSystem","url":"https://terminology.example/x?e=1","name":"X"}}]}"#;
        let server = answering("/r4b/CodeSystem", 200, body).await;
        let document = served(console_at(&server.uri()), None, |client| async move {
            client
                .code_system_summary(FhirVersion::R4B, "https://terminology.example/x?e=1")
                .await
        })
        .await
        .expect("an answer");
        let request = the_request(&server).await;
        assert_eq!(
            Some("url=https%3A%2F%2Fterminology.example%2Fx%3Fe%3D1&_summary=true"),
            request.url.query()
        );
        assert_eq!(
            Some("application/fhir+json"),
            request
                .headers
                .get(http::header::ACCEPT)
                .and_then(|value| value.to_str().ok())
        );
        assert_eq!(1, document.value.published().len());
        assert!(
            document.text.contains("\n  \"resourceType\": \"Bundle\""),
            "the document is kept indented for a reader: {}",
            document.text
        );
    }

    #[tokio::test]
    async fn the_published_names_are_read_narrowed_to_what_a_picker_shows() {
        let body = r#"{"resourceType":"Bundle","entry":[{"resource":{"resourceType":"ValueSet","url":"https://terminology.example/vs","title":"Colours"}}]}"#;
        let server = answering("/r5/ValueSet", 200, body).await;
        let names = served(console_at(&server.uri()), None, |client| async move {
            client.published_names(FhirVersion::R5, "ValueSet").await
        })
        .await
        .expect("an answer");
        assert_eq!(
            Some("_elements=url%2Cname%2Ctitle%2Cversion%2Cstatus%2Cmeta"),
            the_request(&server).await.url.query()
        );
        assert_eq!(
            Some(&String::from("Colours")),
            names.names().get("https://terminology.example/vs")
        );
    }

    #[tokio::test]
    async fn the_capability_document_is_handed_back_as_the_server_sent_it() {
        let body =
            r#"{"resourceType":"CapabilityStatement","fhirVersion":"4.0.1","x-extra":[1,2]}"#;
        let server = answering("/r4/metadata", 200, body).await;
        let document = served(console_at(&server.uri()), None, |client| async move {
            client.capability_document(FhirVersion::R4).await
        })
        .await
        .expect("an answer");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(body).expect("JSON"),
            document
        );
    }

    #[tokio::test]
    async fn a_lookup_reads_the_concept_the_server_answers() {
        let body = r#"{"resourceType":"Parameters","parameter":[{"name":"name","valueString":"Colours"},{"name":"display","valueString":"Red"}]}"#;
        let server = answering("/r4/CodeSystem/$lookup", 200, body).await;
        let request = LookupRequest {
            system: "https://terminology.example/colours".to_owned(),
            code: "red".to_owned(),
            ..LookupRequest::default()
        };
        let document = served(console_at(&server.uri()), None, |client| async move {
            client.lookup(FhirVersion::R4, &request).await
        })
        .await
        .expect("an answer");
        assert_eq!(
            Some("system=https%3A%2F%2Fterminology.example%2Fcolours&code=red"),
            the_request(&server).await.url.query()
        );
        assert_eq!(Some(String::from("Red")), document.value.concept().display);
    }

    #[tokio::test]
    async fn a_refusal_with_an_operation_outcome_carries_it_to_the_reader() {
        let body = r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"not-found","diagnostics":"no such code"}]}"#;
        let server = answering("/r4/CodeSystem/$lookup", 404, body).await;
        let request = LookupRequest {
            system: "https://terminology.example/colours".to_owned(),
            code: "mauve".to_owned(),
            ..LookupRequest::default()
        };
        let error = served(console_at(&server.uri()), None, |client| async move {
            client.lookup(FhirVersion::R4, &request).await
        })
        .await
        .expect_err("refused");
        assert_eq!(Some(StatusCode::NOT_FOUND), error.status());
        assert_eq!(1, error.outcome().map_or(0, |outcome| outcome.issue.len()));
    }

    #[tokio::test]
    async fn a_success_whose_body_is_no_json_is_a_decode_failure() {
        let server = answering("/r4/CodeSystem/$lookup", 200, "<html/>").await;
        let request = LookupRequest {
            system: "s".to_owned(),
            code: "c".to_owned(),
            ..LookupRequest::default()
        };
        let error = served(console_at(&server.uri()), None, |client| async move {
            client.lookup(FhirVersion::R4, &request).await
        })
        .await
        .expect_err("refused");
        assert!(matches!(error, FhirError::Decode { .. }), "{error:?}");
    }

    #[tokio::test]
    async fn a_validation_reads_the_result_the_server_answers() {
        let body = r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":false},{"name":"message","valueString":"Unknown code"}]}"#;
        let server = answering("/r4b/ValueSet/$validate-code", 200, body).await;
        let request = ValidateRequest {
            on: validation::ValidateOn::ValueSet,
            url: "https://terminology.example/vs".to_owned(),
            system: "https://terminology.example/colours".to_owned(),
            code: "mauve".to_owned(),
            ..ValidateRequest::default()
        };
        let document = served(console_at(&server.uri()), None, |client| async move {
            client.validate_code(FhirVersion::R4B, &request).await
        })
        .await
        .expect("an answer");
        let validation = document
            .value
            .validation(&ferrohealth_ui::i18n::Locale::english());
        assert_eq!(
            Some(false),
            validation.result,
            "an unknown code is no refusal"
        );
        assert_eq!(Some(String::from("Unknown code")), validation.message);
    }

    #[tokio::test]
    async fn a_subsumption_reads_the_outcome_the_server_answers() {
        let body = r#"{"resourceType":"Parameters","parameter":[{"name":"outcome","valueCode":"subsumes"}]}"#;
        let server = answering("/r5/CodeSystem/colours/$subsumes", 200, body).await;
        let request = SubsumesRequest {
            id: "colours".to_owned(),
            code_a: "red".to_owned(),
            code_b: "crimson".to_owned(),
            ..SubsumesRequest::default()
        };
        let document = served(console_at(&server.uri()), None, |client| async move {
            client.subsumes(FhirVersion::R5, &request).await
        })
        .await
        .expect("an answer");
        assert_eq!(Some(String::from("subsumes")), document.value.subsumption());
    }

    #[tokio::test]
    async fn a_translation_reads_the_result_the_server_answers() {
        let body =
            r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":true}]}"#;
        let server = answering("/r4/ConceptMap/$translate", 200, body).await;
        let request = TranslateRequest {
            system: "https://terminology.example/colours".to_owned(),
            code: "red".to_owned(),
            ..TranslateRequest::default()
        };
        let document = served(console_at(&server.uri()), None, |client| async move {
            client.translate(FhirVersion::R4, &request).await
        })
        .await
        .expect("an answer");
        assert_eq!(Some(true), document.value.result());
    }

    #[tokio::test]
    async fn a_signed_in_reader_s_request_carries_their_access_token_and_a_visitor_s_none() {
        let server = answering("/health", 200, "").await;
        let state = console_at(&server.uri());
        let id = state
            .sessions()
            .establish(crate::session::SignedIn {
                access_token: secrecy::SecretString::from("synthetic-access-token"),
                expires_in: None,
                refresh_token: None,
                id_token: None,
                operator: None,
                scopes: Vec::new(),
            })
            .expect("room");
        let cookie = format!("__Host-ferroterm_console_session={}", id.as_str());
        served(state.clone(), Some(&cookie), |client| async move {
            client.health().await
        })
        .await
        .expect("healthy");
        served(state, None, |client| async move { client.health().await })
            .await
            .expect("healthy");
        let received = server.received_requests().await.expect("recording");
        let authorization: Vec<Option<&str>> = received
            .iter()
            .map(|request| {
                request
                    .headers
                    .get(http::header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
            })
            .collect();
        assert_eq!(
            vec![Some("Bearer synthetic-access-token"), None],
            authorization
        );
    }
}
