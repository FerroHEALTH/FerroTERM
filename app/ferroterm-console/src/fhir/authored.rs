// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reads and writes the authoring screens make: the stored resources
//! the editors open, the value set composer's preview, create, update,
//! delete, and the version history.

use ferrobridge_term::request::{Expansion, InlineValueSet, Request, ValueSetSource};

use super::answer::{decoded, failure, status_of, transport};
use super::{CONCEPT_MAP, FHIR_JSON, FhirClient, VALUE_SET};
use crate::fhir::compose::stored::StoredValueSet;
use crate::fhir::compose::{Draft, Preview};
use crate::fhir::error::FhirError;
use crate::fhir::exchange::{Exchange, Method};
use crate::fhir::expansion::ExpandedValueSet;
use crate::fhir::searchset::{Found, SearchFilter};
use crate::fhir::version::FhirVersion;
use crate::fhir::write::{History, Stored, Written, if_match};
use crate::url::RequestUrl;

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
    ) -> Result<(StoredValueSet, serde_json::Value), FhirError> {
        let url = self.resource_url(version, VALUE_SET, id);
        let answer = self.send(Method::Get, &url, FHIR_JSON).await?;
        let stored = decoded(&self.locale, version, &answer, &url)?;
        let document = serde_json::from_str(&answer.body).map_err(|error| FhirError::Decode {
            url: url.clone(),
            message: error.to_string(),
        })?;
        Ok((stored, document))
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
        let url = self.expand_post_url(version);
        let body = draft
            .preview_body(version, page)
            .map_err(|error| FhirError::undecoded(&url, version, &error))?;
        let exchange =
            Exchange::new(Method::Post, self.path_of(&url)?, FHIR_JSON).with_body(FHIR_JSON, body);
        let answer = self
            .exchange(exchange)
            .await
            .map_err(|error| transport(&url, &error))?;
        // NOTE: no specification governs this: our own design; the preview's value set
        // carries more than an inline request can, and its answer reads alike.
        let request = Request::Expand(Expansion::of(ValueSetSource::Inline(
            InlineValueSet::default(),
        )));
        let document = self.answered(version, &request, &url, &answer)?;
        ExpandedValueSet::read(document.value, &url, version)
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
        RequestUrl::default()
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
        RequestUrl::default()
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
        self.write(version, exchange, &url).await
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
        self.write(version, exchange, &url).await
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
            Err(failure(version, &answer, status, &url))
        }
    }

    /// The address every version of every resource of one type is read from:
    /// the type-level `_history`
    /// (<https://hl7.org/fhir/R4B/http.html#history>).
    pub(crate) fn type_history_url(&self, version: FhirVersion, resource_type: &str) -> String {
        RequestUrl::default()
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
        self.get_answer(version, &self.type_history_url(version, resource_type))
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
        self.get_answer(version, &self.history_url(version, resource_type, id))
            .await
    }

    /// Sends one write and reads what the server committed.
    async fn write(
        &self,
        version: FhirVersion,
        exchange: Exchange,
        url: &str,
    ) -> Result<Written, FhirError> {
        let answer = self
            .exchange(exchange)
            .await
            .map_err(|error| transport(url, &error))?;
        let status = status_of(&self.locale, &answer, url)?;
        if !status.is_success() {
            return Err(failure(version, &answer, status, url));
        }
        // A server asked for a minimal return answers no body, which is a
        // success and not a decode failure
        // (<https://hl7.org/fhir/R4B/http.html#return>).
        let resource = if answer.body.trim().is_empty() {
            None
        } else {
            Some(
                version
                    .decode(&answer.body)
                    .map_err(|error| FhirError::undecoded(url, version, &error))?,
            )
        };
        Ok(Written {
            resource,
            etag: answer.etag,
            location: answer.location,
        })
    }
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
    ) -> Result<Option<crate::authoring::code_system::Draft>, FhirError> {
        let url = self.code_system_search_url(version, system);
        let found: Found<crate::authoring::code_system::Draft> =
            self.get_answer(version, &url).await?;
        Ok(found.found().into_iter().next().cloned())
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
    ) -> Result<Option<crate::authoring::concept_map::MapDraft>, FhirError> {
        let filter = SearchFilter {
            url: canonical.to_owned(),
            version: String::new(),
        };
        let url = self.search_url(version, CONCEPT_MAP, &filter);
        let found: Found<crate::authoring::concept_map::MapDraft> =
            self.get_answer(version, &url).await?;
        Ok(found.found().into_iter().next().cloned())
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
        RequestUrl::default()
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
    ) -> Result<Stored, FhirError> {
        self.get_answer(version, &self.resource_url(version, resource_type, id))
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
    ) -> Result<Stored, FhirError> {
        self.get_answer(
            version,
            &self.version_read_url(version, resource_type, id, version_id),
        )
        .await
    }
}
