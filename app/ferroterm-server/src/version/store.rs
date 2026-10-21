// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The REST interactions of the persisted `CodeSystem`, `ValueSet`, and
//! `ConceptMap` resources.
//!
//! The FHIR REST API defines create, update, read, version read, search,
//! and delete, their status codes, and the `ETag` and `Last-Modified` headers
//! a version-aware client works with
//! (<https://hl7.org/fhir/R4B/http.html>). One implementation serves every
//! version: a resource is stored as the JSON object it arrived as, with the
//! FHIR version it arrived in, and a read renders it through the reading
//! version's own codec. The per-version `store!` macro only binds the routes
//! to it.

use std::sync::Arc;

use axum::body::Bytes;
use axum::response::Response;
use fhir_terminology::conceptmap::model::ConceptMapModel;
use fhir_terminology::fhir_codesystem::model::CodeSystemModel;
use fhir_terminology::provider::CodeSystemProvider;
use fhir_terminology::valueset::model::ValueSetModel;
use fhir_types::codec::{Object, expect_object};
use fhir_types::schema::Schemas;
use http::header::{CONTENT_TYPE, ETAG, IF_MATCH, LAST_MODIFIED, LOCATION};
use http::{HeaderMap, HeaderValue, StatusCode};

use crate::elements::Projection;
use crate::outcome::Failure;
use crate::persistence::{HistoryEntry, Method, Record, ResourceType};
use crate::state::{AppState, PersistError};
use crate::wire::Wire;

/// The generated codec of one served version.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Surface {
    /// The FHIR version this endpoint serves.
    pub fhir_version: &'static str,
    /// The path segment this version answers under, for the absolute
    /// `Bundle.entry.fullUrl` of a searchset.
    pub segment: &'static str,
    /// The version's XML schema, for an XML request or response.
    pub schemas: &'static Schemas,
    /// Reads a stored object as a resource of this version and writes it back.
    pub round_trip: fn(&Object) -> Result<Object, String>,
    /// Renders a loaded value set, under its instance id, as a `ValueSet` of
    /// this version.
    pub render_value_set: fn(&ValueSetModel, &str) -> Result<Object, String>,
    /// Renders a loaded code system, under its instance id, as a `CodeSystem`
    /// of this version.
    pub render_code_system: fn(&CodeSystemModel, &str) -> Result<Object, String>,
    /// Renders a loaded concept map, under its instance id, as a `ConceptMap`
    /// of this version.
    pub render_concept_map: fn(&ConceptMapModel, &str) -> Result<Object, String>,
}

/// One request against the persisted resources of one type.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Request<'a> {
    /// The server state.
    pub state: &'a AppState,
    /// The version this request arrived on.
    pub surface: Surface,
    /// The resource type the route serves.
    pub resource_type: ResourceType,
    /// The request headers.
    pub headers: &'a HeaderMap,
    /// The request path, for the `Location` of a created resource.
    pub path: &'a str,
    /// The negotiated response format.
    pub wire: Wire,
}

/// `POST {type}`: stores `body` under a server-assigned id.
///
/// # Errors
///
/// A body that is not a resource of this type is a 400; a deployment that
/// persists nothing refuses with a 422.
pub(crate) fn create(request: &Request<'_>, body: &Bytes) -> Result<Response, Failure> {
    let mut object = body_object(request, body)?;
    let id = uuid::Uuid::new_v4().to_string();
    object.insert(
        "id".to_owned(),
        fhir_types::codec::Value::String(id.clone()),
    );
    let record = write(request, &id, Method::Post, object)?;
    Ok(created(request, &record))
}

/// `PUT {type}/{id}`: stores `body` as `id`, creating it when it is new.
///
/// # Errors
///
/// A body that is not a resource of this type, or one whose `id` is another,
/// is a 400; an `If-Match` that does not hold is a 412.
pub(crate) fn update(request: &Request<'_>, id: &str, body: &Bytes) -> Result<Response, Failure> {
    let known = check_id(id)?;
    let mut object = body_object(request, body)?;
    if let Some(sent) = object.get("id").and_then(fhir_types::codec::Value::as_str)
        && sent != known
    {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "invalid",
            format!("the resource id `{sent}` is not the id `{known}` of the request URL"),
        ));
    }
    let held = request.state.persisted_record(request.resource_type, known);
    check_if_match(request.headers, held.as_ref())?;
    object.insert(
        "id".to_owned(),
        fhir_types::codec::Value::String(known.to_owned()),
    );
    let record = write(request, known, Method::Put, object)?;
    if held.is_none() {
        return Ok(created(request, &record));
    }
    let object = rendered(request, &record)?;
    Ok(with_headers(
        request
            .wire
            .response(StatusCode::OK, &object, request.surface.schemas),
        &record,
        None,
    ))
}

/// `GET {type}/{id}`: the current version of the resource.
///
/// A `CodeSystem`, `ValueSet`, or `ConceptMap` id the deployment loaded reads
/// here too, rendered from the model the engine holds; it carries no `ETag`,
/// because a loaded resource has no version this server counts. `projection`
/// is the `_summary` and `_elements` of the read
/// (<https://hl7.org/fhir/R4B/http.html#read>).
///
/// # Errors
///
/// An id that was never stored is a 404, one whose current version was
/// deleted is a 410.
pub(crate) fn read(
    request: &Request<'_>,
    id: &str,
    projection: &Projection,
) -> Result<Response, Failure> {
    let known = check_id(id)?;
    let schemas = request.surface.schemas;
    let Some(record) = request.state.persisted_record(request.resource_type, known) else {
        if let Some(object) = loaded(request, known)? {
            let object = projection.apply(&object, schemas);
            return Ok(request.wire.response(StatusCode::OK, &object, schemas));
        }
        return Err(gone_or_missing(request, known));
    };
    let object = projection.apply(&rendered(request, &record)?, schemas);
    Ok(with_headers(
        request
            .wire
            .response(StatusCode::OK, &object, request.surface.schemas),
        &record,
        None,
    ))
}

/// The resource `id` names among the ones the deployment loaded, when there
/// is one.
///
/// The FHIR read is defined over any resource the server holds, and a code
/// system, value set, or concept map the deployment loaded from disk is one
/// (<https://hl7.org/fhir/R4B/http.html#read>); what comes back is the
/// definition, with the concepts only where `content` says the resource
/// carries them.
///
/// # Errors
///
/// A resource this version cannot encode is a 500.
fn loaded(request: &Request<'_>, id: &str) -> Result<Option<Object>, Failure> {
    match request.resource_type {
        ResourceType::CodeSystem => {
            if let Some(resolved) = request.state.instance(id) {
                let model =
                    fhir_terminology::fhir_codesystem::model::described(&*resolved.provider);
                return (request.surface.render_code_system)(&model, id)
                    .map(Some)
                    .map_err(|reason| rendering(&reason));
            }
            // NOTE: a supplement "defines extra properties and designations" of
            // another code system (<https://hl7.org/fhir/R4B/codesystem.html#supplements>)
            // and is a `CodeSystem` this server holds, so it reads at its own id.
            let Some(model) = request.state.supplement_instance(id) else {
                return Ok(None);
            };
            (request.surface.render_code_system)(&model, id)
                .map(Some)
                .map_err(|reason| rendering(&reason))
        }
        ResourceType::ValueSet => {
            let Some(model) = request.state.value_set_instance(id) else {
                return Ok(None);
            };
            (request.surface.render_value_set)(&model, id)
                .map(Some)
                .map_err(|reason| rendering(&reason))
        }
        ResourceType::ConceptMap => {
            let Some(model) = request.state.concept_map_instance(id) else {
                return Ok(None);
            };
            (request.surface.render_concept_map)(&model, id)
                .map(Some)
                .map_err(|reason| rendering(&reason))
        }
    }
}

/// `GET {type}/{id}/_history/{version_id}`: one version of the resource.
///
/// # Errors
///
/// A version that was never written is a 404, and the version a delete made
/// is a 410 (<https://hl7.org/fhir/R4B/http.html#vread>).
pub(crate) fn version_read(
    request: &Request<'_>,
    id: &str,
    version_id: &str,
) -> Result<Response, Failure> {
    let known = check_id(id)?;
    let wanted = version_id.parse::<u32>().map_err(|_unparsed| {
        Failure::new(
            StatusCode::NOT_FOUND,
            "not-found",
            format!("`{version_id}` is not a version this server writes"),
        )
    })?;
    let held = request
        .state
        .persisted_version(request.resource_type, known, wanted)
        .map_err(|error| persist_failure(&error))?;
    let Some(record) = held else {
        let deleted = request
            .state
            .persisted_is_delete(request.resource_type, known, wanted)
            .map_err(|error| persist_failure(&error))?;
        if deleted {
            return Err(Failure::new(
                StatusCode::GONE,
                "deleted",
                format!(
                    "version `{wanted}` of {}/{known} is its delete",
                    request.resource_type.name()
                ),
            ));
        }
        return Err(Failure::new(
            StatusCode::NOT_FOUND,
            "not-found",
            format!(
                "no version `{wanted}` of {}/{known}",
                request.resource_type.name()
            ),
        ));
    };
    let object = rendered(request, &record)?;
    Ok(with_headers(
        request
            .wire
            .response(StatusCode::OK, &object, request.surface.schemas),
        &record,
        None,
    ))
}

/// `DELETE {type}/{id}`: removes the current version.
///
/// A delete of a resource that is already deleted, or of one that never
/// existed, has no effect and answers `204` all the same
/// (<https://hl7.org/fhir/R4B/http.html#delete>).
///
/// # Errors
///
/// An `If-Match` that does not hold is a 412; a deployment that persists
/// nothing refuses with a 422.
pub(crate) fn delete(request: &Request<'_>, id: &str) -> Result<Response, Failure> {
    let known = check_id(id)?;
    let held = request.state.persisted_record(request.resource_type, known);
    check_if_match(request.headers, held.as_ref())?;
    request
        .state
        .delete_persisted(request.resource_type, known)
        .map_err(|error| persist_failure(&error))?;
    Ok(status_response(StatusCode::NO_CONTENT))
}

/// The versions a history interaction lists, newest first
/// (<https://hl7.org/fhir/R4B/http.html#history>): those of the resource `id`
/// names, or of every resource of the type when `id` is `None`.
///
/// A resource the deployment loaded has one version and no history this
/// server counts, so it answers an empty list, as does a type this deployment
/// persists nothing of.
///
/// # Errors
///
/// A parameter other than `_since` and `_format` is a 400 `not-supported`, a
/// `_since` that is no FHIR instant is a 400, an id this server has never held
/// is a 404, and a store that cannot be read is a 500.
pub(crate) fn history_entries(
    request: &Request<'_>,
    id: Option<&str>,
    query: &[(String, String)],
) -> Result<Vec<HistoryEntry>, Failure> {
    let since = history_since(query)?;
    let known = id.map(check_id).transpose()?;
    let entries = request
        .state
        .persisted_history(request.resource_type, known)
        .map_err(|error| persist_failure(&error))?;
    if let Some(known) = known
        && entries.is_empty()
        && request
            .state
            .loaded_canonical(request.resource_type, known)
            .is_none()
    {
        return Err(Failure::new(
            StatusCode::NOT_FOUND,
            "not-found",
            format!("no {} with id `{known}`", request.resource_type.name()),
        ));
    }
    let mut timed = Vec::with_capacity(entries.len());
    for entry in entries {
        let at: jiff::Timestamp = entry.last_modified.parse().map_err(|_unparsed| {
            Failure::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "exception",
                format!(
                    "the stored instant `{}` of {}/{} version {} does not read",
                    entry.last_modified,
                    request.resource_type.name(),
                    entry.id,
                    entry.version_id
                ),
            )
        })?;
        // NOTE: `_since` keeps the versions "created at or after the given instant"
        // (<https://hl7.org/fhir/R4B/http.html#history>).
        if since.is_none_or(|since| at >= since) {
            timed.push((at, entry));
        }
    }
    timed.sort_by(|(left_at, left), (right_at, right)| {
        right_at
            .cmp(left_at)
            .then_with(|| right.version_id.cmp(&left.version_id))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(timed.into_iter().map(|(_, entry)| entry).collect())
}

/// The `_since` a history request names, refusing every parameter this server
/// does not answer.
///
/// The history interaction defines `_count`, `_since`, `_at`, and `_list`
/// (<https://hl7.org/fhir/R4B/http.html#history>); this server answers `_since`
/// and refuses the others with `not-supported`, as it refuses every parameter
/// it does not implement.
fn history_since(query: &[(String, String)]) -> Result<Option<jiff::Timestamp>, Failure> {
    let mut since = None;
    for (name, value) in query {
        match name.as_str() {
            "_format" => {}
            "_since" if since.is_none() => {
                // NOTE: `_since` is an `instant`, which always carries a time zone
                // (<https://hl7.org/fhir/R4B/datatypes.html#instant>).
                let instant = value.parse::<jiff::Timestamp>().map_err(|error| {
                    Failure::new(
                        StatusCode::BAD_REQUEST,
                        "value",
                        format!("`_since={value}` is not a FHIR instant: {error}"),
                    )
                })?;
                since = Some(instant);
            }
            "_since" => {
                return Err(Failure::new(
                    StatusCode::BAD_REQUEST,
                    "value",
                    "`_since` is named more than once",
                ));
            }
            other => {
                return Err(Failure::new(
                    StatusCode::BAD_REQUEST,
                    "not-supported",
                    format!(
                        "history parameter `{other}` is not supported; use `_since` or `_format`"
                    ),
                ));
            }
        }
    }
    Ok(since)
}

/// The records a search over `url` and `version` matches, sorted by id.
///
/// # Errors
///
/// A search parameter this server does not answer is a 400.
pub(crate) fn matches(
    state: &AppState,
    resource_type: ResourceType,
    query: &[(String, String)],
) -> Result<Vec<Record>, Failure> {
    let (url, version) = criteria(query)?;
    Ok(state
        .persisted_records(resource_type)
        .into_iter()
        .filter(|record| {
            url.is_none_or(|wanted| record.url.as_deref() == Some(wanted))
                && version.is_none_or(|wanted| record.version.as_deref() == Some(wanted))
        })
        .collect())
}

/// Why `record` has no representation in the FHIR version `reading` names.
///
/// One wording serves the refusal of an instance read and the issue a search
/// carries for the same resource, so a client reads the same sentence either
/// way.
pub(crate) fn unreadable(record: &Record, reading: &str, reason: &str) -> String {
    format!(
        "{}/{} was written as FHIR {} and does not read as FHIR {reading}: {reason}",
        record.resource_type, record.id, record.fhir_version
    )
}

/// `record` as a resource of the reading version, with its `meta`.
///
/// A written resource is never converted between FHIR releases. R4B states a
/// target's direction with `concept-map-equivalence` and R5 with
/// `concept-map-relationship`; `equal`, `subsumes`, `specializes`, `inexact`,
/// and `unmatched` have no code in the R5 set
/// (<https://hl7.org/fhir/R4B/valueset-concept-map-equivalence.html>,
/// <https://hl7.org/fhir/R5/valueset-concept-map-relationship.html>), and
/// HL7's R4-to-R5 transforms live in a cross-version implementation guide that
/// is informative and under development
/// (<https://hl7.org/fhir/extensions/conversions-ConceptMap.html>), so no core
/// specification states what a conversion should produce.
///
/// # Errors
///
/// A resource stored in another FHIR version that carries an element this one
/// does not define is a 404. Serving one release per base is the strategy the
/// specification describes as one where "the same record has a different
/// identity depending on the version of FHIR in use"
/// (<https://hl7.org/fhir/R5/versioning.html>), and a read answers `404` for
/// an identity a base does not hold
/// (<https://hl7.org/fhir/R4B/http.html#read>, which names `200`, `404`, and
/// `410` and no other status). The `OperationOutcome` names the release the
/// resource does read as.
pub(crate) fn rendered(request: &Request<'_>, record: &Record) -> Result<Object, Failure> {
    (request.surface.round_trip)(&record.resource).map_err(|reason| {
        Failure::new(
            StatusCode::NOT_FOUND,
            "not-supported",
            unreadable(record, request.surface.fhir_version, &reason),
        )
    })
}

/// The `ETag` and `Last-Modified` of `record` on `response`, and its
/// `Location` when one is given.
pub(crate) fn with_headers(
    mut response: Response,
    record: &Record,
    location: Option<&str>,
) -> Response {
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(&record.etag()) {
        headers.insert(ETAG, value);
    }
    if let Some(value) = http_date(&record.last_modified) {
        headers.insert(LAST_MODIFIED, value);
    }
    if let Some(value) = location.and_then(|text| HeaderValue::from_str(text).ok()) {
        headers.insert(LOCATION, value);
    }
    response
}

/// The `201 Created` of a resource that was just written.
fn created(request: &Request<'_>, record: &Record) -> Response {
    let location = format!(
        "{}/{}/_history/{}",
        request.path.trim_end_matches('/'),
        record.id,
        record.version_id
    );
    let response = match rendered(request, record) {
        Ok(object) => request
            .wire
            .response(StatusCode::CREATED, &object, request.surface.schemas),
        // NOTE: the resource was just read through this version's codec, so it renders;
        // if it ever does not, the status and headers still tell the client what happened.
        Err(_) => status_response(StatusCode::CREATED),
    };
    with_headers(response, record, Some(&location))
}

/// Stores `object` as `id` and returns the record it became.
///
/// # Errors
///
/// A write onto an id the deployment loaded a resource under is a 409: that
/// resource answers a read at the id and is not the client's to replace. No
/// FHIR specification governs a write onto such an id: our own design, and it
/// is the refusal the server would otherwise meet at its next start.
///
/// A write whose `url` and `version` another resource of the type carries,
/// loaded or persisted, is a 409 `duplicate` naming that resource's id.
fn write(
    request: &Request<'_>,
    id: &str,
    method: Method,
    object: Object,
) -> Result<Record, Failure> {
    if let Some(canonical) = request.state.loaded_canonical(request.resource_type, id) {
        return Err(Failure::new(
            StatusCode::CONFLICT,
            "conflict",
            format!(
                "the id `{id}` is the one the loaded {} `{canonical}` is read at",
                request.resource_type.name()
            ),
        ));
    }
    request
        .state
        .put_persisted(
            request.resource_type,
            id,
            request.surface.fhir_version,
            method,
            object,
        )
        .map_err(|error| persist_failure(&error))
}

/// The body as a JSON object of the request's format, refused when it is not a
/// resource of the route's type.
fn body_object(request: &Request<'_>, body: &Bytes) -> Result<Object, Failure> {
    let structure = |text: String| Failure::new(StatusCode::BAD_REQUEST, "structure", text);
    let object = match Wire::of_body(request.headers)? {
        Wire::Json => {
            let value: fhir_types::codec::Value = serde_json::from_slice(body)
                .map_err(|error| structure(format!("the body is not JSON: {error}")))?;
            let path = fhir_types::codec::Path::root(request.resource_type.name());
            expect_object(&value, &path)
                .map_err(|error| structure(error.to_string()))?
                .clone()
        }
        Wire::Xml => {
            let text = std::str::from_utf8(body)
                .map_err(|error| structure(format!("the body is not UTF-8: {error}")))?;
            fhir_types::xml::from_xml(request.surface.schemas, text)
                .map_err(|error| structure(error.to_string()))?
        }
    };
    let sent = object
        .get("resourceType")
        .and_then(fhir_types::codec::Value::as_str)
        .unwrap_or_default();
    if sent != request.resource_type.name() {
        return Err(Failure::new(
            StatusCode::BAD_REQUEST,
            "invalid",
            format!(
                "this endpoint stores {} resources, and the body is a `{sent}`",
                request.resource_type.name()
            ),
        ));
    }
    Ok(object)
}

/// `id` when it is a FHIR id.
fn check_id(id: &str) -> Result<&str, Failure> {
    let shaped = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    if shaped {
        return Ok(id);
    }
    Err(Failure::new(
        StatusCode::BAD_REQUEST,
        "value",
        format!("`{id}` is not a FHIR id: at most 64 of A-Z, a-z, 0-9, `-`, and `.`"),
    ))
}

/// The `412` of an `If-Match` that names another version.
fn check_if_match(headers: &HeaderMap, held: Option<&Record>) -> Result<(), Failure> {
    let Some(wanted) = headers.get(IF_MATCH).and_then(|value| value.to_str().ok()) else {
        return Ok(());
    };
    let current = held.map(Record::etag);
    if current.as_deref() == Some(wanted.trim()) {
        return Ok(());
    }
    Err(Failure::new(
        StatusCode::PRECONDITION_FAILED,
        "conflict",
        match current {
            Some(current) => {
                format!("`If-Match: {wanted}` does not hold: the version is {current}")
            }
            None => format!("`If-Match: {wanted}` does not hold: there is no such resource"),
        },
    ))
}

/// The `410` of a resource whose versions were written and then deleted, and
/// the `404` of one that never existed.
fn gone_or_missing(request: &Request<'_>, id: &str) -> Failure {
    let deleted = request
        .state
        .persisted_version(request.resource_type, id, 1)
        .is_ok_and(|held| held.is_some());
    if deleted {
        return Failure::new(
            StatusCode::GONE,
            "deleted",
            format!("{}/{id} is deleted", request.resource_type.name()),
        );
    }
    Failure::new(
        StatusCode::NOT_FOUND,
        "not-found",
        format!("no {} with id `{id}`", request.resource_type.name()),
    )
}

/// The failure a store error answers with.
pub(crate) fn persist_failure(error: &PersistError) -> Failure {
    let (status, code) = match error {
        PersistError::NotConfigured => (StatusCode::UNPROCESSABLE_ENTITY, "not-supported"),
        // NOTE: a resource that "failed basic FHIR validation rules", a shape or a
        // primitive outside its lexical form, is a 400
        // (<https://hl7.org/fhir/R4B/http.html#2.21.0.10.1>); 422 is for profiles and business rules.
        PersistError::Decode { .. } | PersistError::Convert { .. } | PersistError::Layer(_) => {
            (StatusCode::BAD_REQUEST, "invalid")
        }
        PersistError::Store(_) => (StatusCode::INTERNAL_SERVER_ERROR, "exception"),
        // NOTE: `url` and `version` identify one canonical resource
        // (<https://hl7.org/fhir/R4B/resource.html#canonical>), and a write that breaks it is refused.
        PersistError::Duplicate { .. } | PersistError::CoreDuplicate { .. } => {
            (StatusCode::CONFLICT, "duplicate")
        }
    };
    let failure = Failure::new(status, code, error.to_string());
    match error {
        PersistError::Decode { source, .. } => failure.at(source.path.clone()),
        _ => failure,
    }
}

/// The ids of the value sets the deployment loaded that `query` matches, the
/// persisted ones left out because a search lists those from their records.
///
/// # Errors
///
/// A search parameter this server does not answer is a 400.
pub(crate) fn loaded_value_sets(
    state: &AppState,
    query: &[(String, String)],
) -> Result<Vec<String>, Failure> {
    let (url, version) = criteria(query)?;
    Ok(state
        .value_set_instances()
        .into_iter()
        .filter(|(id, served_url, served_version)| {
            state.persisted_record(ResourceType::ValueSet, id).is_none()
                && url.is_none_or(|wanted| served_url == wanted)
                && version.is_none_or(|wanted| served_version.as_deref() == Some(wanted))
        })
        .map(|(id, _, _)| id)
        .collect())
}

/// The ids of the concept maps the deployment loaded that `query` matches, the
/// persisted ones left out because a search lists those from their records.
///
/// `ConceptMap` search defines `url` and `version`
/// (<https://hl7.org/fhir/R4B/conceptmap.html#search>).
///
/// # Errors
///
/// A search parameter this server does not answer is a 400.
pub(crate) fn loaded_concept_maps(
    state: &AppState,
    query: &[(String, String)],
) -> Result<Vec<String>, Failure> {
    let (url, version) = criteria(query)?;
    Ok(state
        .concept_map_instances()
        .into_iter()
        .filter(|(id, served_url, served_version)| {
            state
                .persisted_record(ResourceType::ConceptMap, id)
                .is_none()
                && url.is_none_or(|wanted| served_url == wanted)
                && version.is_none_or(|wanted| served_version.as_deref() == Some(wanted))
        })
        .map(|(id, _, _)| id)
        .collect())
}

/// One loaded code system a search matched: its instance id and the provider
/// that serves it.
pub(crate) type Served = (String, Arc<dyn CodeSystemProvider>);

/// The code systems the deployment loaded that `query` matches, each with the
/// provider that serves it, the persisted ones left out because a search lists
/// those from their records.
///
/// `CodeSystem` search defines `url` and `version`
/// (<https://hl7.org/fhir/R4B/codesystem.html#search>).
///
/// # Errors
///
/// A search parameter this server does not answer is a 400.
pub(crate) fn loaded_code_systems(
    state: &AppState,
    query: &[(String, String)],
) -> Result<Vec<Served>, Failure> {
    let (url, version) = criteria(query)?;
    let mut out = Vec::new();
    for (id, served_url, served_version) in state.instances() {
        if url.is_some_and(|asked| asked != served_url)
            || version.is_some_and(|asked| asked != served_version)
            || state
                .persisted_record(ResourceType::CodeSystem, id)
                .is_some()
        {
            continue;
        }
        // NOTE: an id the served layer no longer resolves is not a resource this
        // server holds, so the search does not match it
        // (<https://hl7.org/fhir/R4B/http.html#search>).
        if let Some(resolved) = state.instance(id) {
            out.push((id.to_owned(), resolved.provider));
        }
    }
    Ok(out)
}

/// The supplements the deployment loaded that `query` matches, each with the
/// resource that describes it.
///
/// # Errors
///
/// A search parameter this server does not answer is a 400.
pub(crate) fn loaded_supplements(
    state: &AppState,
    query: &[(String, String)],
) -> Result<Vec<(String, Arc<CodeSystemModel>)>, Failure> {
    let (url, version) = criteria(query)?;
    Ok(state
        .supplement_instances()
        .into_iter()
        .filter(|(id, model)| {
            state
                .persisted_record(ResourceType::CodeSystem, id)
                .is_none()
                && url.is_none_or(|wanted| model.url == wanted)
                && version.is_none_or(|wanted| model.version == wanted)
        })
        .collect())
}

/// The FHIR base the request that `headers` and `uri` belong to reached this
/// version at.
///
/// `Bundle.entry.fullUrl` is the absolute URL of the resource
/// (<https://hl7.org/fhir/R4B/bundle-definitions.html#Bundle.entry.fullUrl>),
/// so a searchset needs the base the client used: the one the deployment
/// declares, else the authority the request itself names. A deployment behind
/// TLS or a path prefix declares `FERROTERM_BASE_URL`, because the request
/// carries no scheme and no `Forwarded` header is read here: no FHIR
/// specification governs reverse-proxy trust, and this is our own design.
pub(crate) fn base_of(
    state: &AppState,
    segment: &str,
    headers: &HeaderMap,
    uri: &http::Uri,
) -> Option<String> {
    if let Some(base) = state.base_url() {
        return Some(crate::version::endpoint(base, segment));
    }
    let authority = uri
        .authority()
        .map(http::uri::Authority::as_str)
        .or_else(|| {
            headers
                .get(http::header::HOST)
                .and_then(|value| value.to_str().ok())
        })
        .filter(|authority| !authority.is_empty())?;
    Some(crate::version::endpoint(
        &format!("http://{authority}"),
        segment,
    ))
}

/// The `url` and `version` a search names.
///
/// `_elements` and `_summary` name what to return rather than what to match,
/// so they pass through here and are applied to the answer
/// (<https://hl7.org/fhir/R4B/search.html#summary>).
fn criteria(query: &[(String, String)]) -> Result<(Option<&str>, Option<&str>), Failure> {
    let mut url = None;
    let mut version = None;
    for (name, value) in query {
        match name.as_str() {
            "url" => url = Some(value.as_str()),
            "version" => version = Some(value.as_str()),
            "_format" | crate::elements::PARAMETER | crate::elements::SUMMARY => {}
            other => {
                return Err(Failure::new(
                    StatusCode::BAD_REQUEST,
                    "not-supported",
                    format!(
                        "search parameter `{other}` is not supported; use `url`, `version`, `_elements` or `_summary`"
                    ),
                ));
            }
        }
    }
    Ok((url, version))
}

/// The `500` of a resource the server holds but cannot encode.
pub(crate) fn rendering(reason: &str) -> Failure {
    Failure::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "exception",
        format!("cannot encode the resource: {reason}"),
    )
}

/// A response that carries only a status.
fn status_response(status: StatusCode) -> Response {
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, crate::wire::FHIR_JSON)
        .body(axum::body::Body::empty())
        .unwrap_or_else(|_| axum::response::IntoResponse::into_response(status))
}

/// A stored instant as the HTTP-date `Last-Modified` carries
/// (<https://www.rfc-editor.org/rfc/rfc9110.html#section-5.6.7>).
fn http_date(instant: &str) -> Option<HeaderValue> {
    let timestamp: jiff::Timestamp = instant.parse().ok()?;
    HeaderValue::from_str(&timestamp.strftime("%a, %d %b %Y %H:%M:%S GMT").to_string()).ok()
}

/// The six axum handlers of one resource type, over the shared implementation.
///
/// The `store!` macro invokes this by path, once per type: a `macro_rules!`
/// definition nested inside another one cannot spell its own metavariables
/// (<https://doc.rust-lang.org/reference/macros-by-example.html>).
macro_rules! store_routes {
    ($create:ident, $read:ident, $update:ident, $version_read:ident, $delete:ident, $search:ident, $history:ident, $type_history:ident, $kind:expr) => {
        /// `GET {id}/_history`: every version of the resource, newest first.
        pub async fn $history(
            axum::extract::State(state): axum::extract::State<
                std::sync::Arc<crate::state::AppState>,
            >,
            axum::extract::Path(id): axum::extract::Path<String>,
            axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
            headers: http::HeaderMap,
            axum::extract::Query(query): axum::extract::Query<Vec<(String, String)>>,
        ) -> axum::response::Response {
            let wire = match Wire::negotiate(&query, &headers) {
                Ok(wire) => wire,
                Err(failure) => return failure.into_response(),
            };
            let asked = Asked {
                query: &query,
                headers: &headers,
                uri: &uri,
            };
            finish(history(&state, $kind, Some(&id), &asked, wire), wire)
        }

        /// `GET _history`: every version of every resource of the type,
        /// newest first.
        pub async fn $type_history(
            axum::extract::State(state): axum::extract::State<
                std::sync::Arc<crate::state::AppState>,
            >,
            axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
            headers: http::HeaderMap,
            axum::extract::Query(query): axum::extract::Query<Vec<(String, String)>>,
        ) -> axum::response::Response {
            let wire = match Wire::negotiate(&query, &headers) {
                Ok(wire) => wire,
                Err(failure) => return failure.into_response(),
            };
            let asked = Asked {
                query: &query,
                headers: &headers,
                uri: &uri,
            };
            finish(history(&state, $kind, None, &asked, wire), wire)
        }

        /// `POST`: stores the body under a server-assigned id.
        pub async fn $create(
            axum::extract::State(state): axum::extract::State<
                std::sync::Arc<crate::state::AppState>,
            >,
            axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
            headers: http::HeaderMap,
            axum::extract::Query(query): axum::extract::Query<Vec<(String, String)>>,
            body: axum::body::Bytes,
        ) -> axum::response::Response {
            let wire = match Wire::negotiate(&query, &headers) {
                Ok(wire) => wire,
                Err(failure) => return failure.into_response(),
            };
            let request = crate::version::store::Request {
                state: &state,
                surface: surface(),
                resource_type: $kind,
                headers: &headers,
                path: uri.path(),
                wire,
            };
            finish(crate::version::store::create(&request, &body), wire)
        }

        /// `GET {id}`: the current version of the resource.
        pub async fn $read(
            axum::extract::State(state): axum::extract::State<
                std::sync::Arc<crate::state::AppState>,
            >,
            axum::extract::Path(id): axum::extract::Path<String>,
            headers: http::HeaderMap,
            axum::extract::Query(query): axum::extract::Query<Vec<(String, String)>>,
        ) -> axum::response::Response {
            let wire = match Wire::negotiate(&query, &headers) {
                Ok(wire) => wire,
                Err(failure) => return failure.into_response(),
            };
            let request = crate::version::store::Request {
                state: &state,
                surface: surface(),
                resource_type: $kind,
                headers: &headers,
                path: "",
                wire,
            };
            let read =
                crate::elements::Projection::of_query(&query, crate::elements::Interaction::Read)
                    .and_then(|projection| crate::version::store::read(&request, &id, &projection));
            finish(read, wire)
        }

        /// `PUT {id}`: stores the body as `id`.
        pub async fn $update(
            axum::extract::State(state): axum::extract::State<
                std::sync::Arc<crate::state::AppState>,
            >,
            axum::extract::Path(id): axum::extract::Path<String>,
            axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
            headers: http::HeaderMap,
            axum::extract::Query(query): axum::extract::Query<Vec<(String, String)>>,
            body: axum::body::Bytes,
        ) -> axum::response::Response {
            let wire = match Wire::negotiate(&query, &headers) {
                Ok(wire) => wire,
                Err(failure) => return failure.into_response(),
            };
            let path = uri
                .path()
                .rsplit_once('/')
                .map_or_else(|| uri.path().to_owned(), |(head, _)| head.to_owned());
            let request = crate::version::store::Request {
                state: &state,
                surface: surface(),
                resource_type: $kind,
                headers: &headers,
                path: &path,
                wire,
            };
            finish(crate::version::store::update(&request, &id, &body), wire)
        }

        /// `GET {id}/_history/{version}`: one version of the resource.
        pub async fn $version_read(
            axum::extract::State(state): axum::extract::State<
                std::sync::Arc<crate::state::AppState>,
            >,
            axum::extract::Path((id, version)): axum::extract::Path<(String, String)>,
            headers: http::HeaderMap,
            axum::extract::Query(query): axum::extract::Query<Vec<(String, String)>>,
        ) -> axum::response::Response {
            let wire = match Wire::negotiate(&query, &headers) {
                Ok(wire) => wire,
                Err(failure) => return failure.into_response(),
            };
            let request = crate::version::store::Request {
                state: &state,
                surface: surface(),
                resource_type: $kind,
                headers: &headers,
                path: "",
                wire,
            };
            finish(
                crate::version::store::version_read(&request, &id, &version),
                wire,
            )
        }

        /// `DELETE {id}`: removes the current version.
        pub async fn $delete(
            axum::extract::State(state): axum::extract::State<
                std::sync::Arc<crate::state::AppState>,
            >,
            axum::extract::Path(id): axum::extract::Path<String>,
            headers: http::HeaderMap,
            axum::extract::Query(query): axum::extract::Query<Vec<(String, String)>>,
        ) -> axum::response::Response {
            let wire = match Wire::negotiate(&query, &headers) {
                Ok(wire) => wire,
                Err(failure) => return failure.into_response(),
            };
            let request = crate::version::store::Request {
                state: &state,
                surface: surface(),
                resource_type: $kind,
                headers: &headers,
                path: "",
                wire,
            };
            finish(crate::version::store::delete(&request, &id), wire)
        }

        /// `GET ?url=&version=`: a `searchset` of the resources this server
        /// holds, the loaded ones beside the persisted ones.
        pub async fn $search(
            axum::extract::State(state): axum::extract::State<
                std::sync::Arc<crate::state::AppState>,
            >,
            axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
            headers: http::HeaderMap,
            axum::extract::Query(query): axum::extract::Query<Vec<(String, String)>>,
        ) -> axum::response::Response {
            let wire = match Wire::negotiate(&query, &headers) {
                Ok(wire) => wire,
                Err(failure) => return failure.into_response(),
            };
            finish(search(&state, $kind, &query, &headers, &uri, wire), wire)
        }
    };
}

pub(crate) use store_routes;

macro_rules! store {
    ($fhir:ident) => {
        pub mod store {
            //! The persisted-resource routes of one version, over the shared
            //! implementation in `crate::version::store`.

            use axum::response::{IntoResponse, Response};
            use fhir_types::$fhir::bundle::{
                Bundle, BundleEntry, BundleEntryRequest, BundleEntryResponse, BundleEntrySearch,
                BundleLink,
            };
            use fhir_types::$fhir::codeable_concept::CodeableConcept;
            use fhir_types::$fhir::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
            use fhir_types::$fhir::resource::Resource;
            use http::{HeaderMap, StatusCode};

            use crate::outcome::Failure;
            use crate::persistence::{HistoryEntry, Method, ResourceType};
            use crate::state::AppState;
            use crate::version::store::{Request, Surface};
            use crate::wire::Wire;

            use super::metadata::FHIR_VERSION;
            use super::parameters;

            /// The codec of this version, for the shared implementation.
            fn surface() -> Surface {
                Surface {
                    fhir_version: FHIR_VERSION,
                    segment: stringify!($fhir),
                    schemas: &fhir_types::$fhir::schema::SCHEMAS,
                    round_trip: super::resources::round_trip,
                    render_value_set: |model, id| {
                        fhir_types::codec::Json::to_json(&value_set(model, id))
                            .map_err(|error| error.to_string())
                    },
                    render_code_system: |model, id| {
                        fhir_types::codec::Json::to_json(&code_system(model, id))
                            .map_err(|error| error.to_string())
                    },
                    render_concept_map: |model, id| {
                        fhir_types::codec::Json::to_json(&concept_map(model, id))
                            .map_err(|error| error.to_string())
                    },
                }
            }

            /// A loaded code system as this version's `CodeSystem`, under the
            /// instance id the server addresses it by.
            fn code_system(
                model: &fhir_terminology::fhir_codesystem::model::CodeSystemModel,
                id: &str,
            ) -> fhir_types::$fhir::code_system::CodeSystem {
                let mut resource =
                    fhir_terminology::fhir_codesystem::render::$fhir::code_system(model);
                resource.id = Some(id.to_owned());
                resource
            }

            /// A loaded value set as this version's `ValueSet`, under the
            /// instance id the server addresses it by.
            ///
            /// `Resource.id` is the logical id the resource is read by, so a
            /// client can address what a search returned
            /// (<https://hl7.org/fhir/R4B/resource.html#id>).
            fn value_set(
                model: &fhir_terminology::valueset::model::ValueSetModel,
                id: &str,
            ) -> fhir_types::$fhir::value_set::ValueSet {
                let mut resource =
                    fhir_terminology::valueset::render::$fhir::value_set(model, true);
                resource.id = Some(id.to_owned());
                resource
            }

            /// A loaded concept map as this version's `ConceptMap`, under the
            /// instance id the server addresses it by.
            fn concept_map(
                model: &fhir_terminology::conceptmap::model::ConceptMapModel,
                id: &str,
            ) -> fhir_types::$fhir::concept_map::ConceptMap {
                let mut resource = fhir_terminology::conceptmap::render::$fhir::concept_map(model);
                resource.id = Some(id.to_owned());
                resource
            }

            fn finish(handled: Result<Response, Failure>, wire: Wire) -> Response {
                match handled {
                    Ok(response) => response,
                    Err(failure) => failure.respond(wire),
                }
            }

            /// One `match` entry of a `searchset`, at the absolute URL of the
            /// resource when the base this server is reached at is known
            /// (<https://hl7.org/fhir/R4B/bundle.html#bundle-unique>).
            fn found(full_url: Option<String>, resource: Resource) -> BundleEntry {
                BundleEntry {
                    full_url: full_url.map(Into::into),
                    resource: Some(resource),
                    search: Some(BundleEntrySearch {
                        mode: Some("match".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            }

            /// The `outcome` entry naming the resources this server holds and
            /// this version has no representation for.
            ///
            /// A searchset may carry an `OperationOutcome` about the search
            /// beside its matches, with no issue of `fatal` or `error`
            /// severity and `search.mode` of `outcome`
            /// (<https://hl7.org/fhir/R4B/http.html#search>). R5 and the R6
            /// ballot also define `Bundle.issues` for this content, and both
            /// keep the entry form on that same page, so one shape serves
            /// every served version.
            fn outcome(issue: Vec<OperationOutcomeIssue>) -> BundleEntry {
                // NOTE: R5 `bdl-15` requires a `fullUrl` on every searchset entry, and a
                // resource with no persistent identity takes a UUID
                // (<https://hl7.org/fhir/R5/bundle.html>).
                let full_url = format!("urn:uuid:{}", uuid::Uuid::new_v4());
                BundleEntry {
                    full_url: Some(full_url.into()),
                    resource: Some(Resource::OperationOutcome(Box::new(OperationOutcome {
                        issue,
                        ..Default::default()
                    }))),
                    search: Some(BundleEntrySearch {
                        mode: Some("outcome".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            }

            /// The `self` link of a searchset: the URL the search was made at.
            ///
            /// R5 `bdl-18` requires it on every searchset
            /// (<https://hl7.org/fhir/R5/bundle.html>), and `self` is the
            /// relation the search page gives that link
            /// (<https://hl7.org/fhir/R4B/http.html#paging>).
            fn self_link(
                base: Option<&String>,
                resource_type: ResourceType,
                uri: &http::Uri,
            ) -> Vec<BundleLink> {
                let Some(base) = base else {
                    return Vec::new();
                };
                let path = format!("{base}/{}", resource_type.name());
                let url = match uri.query().filter(|query| !query.is_empty()) {
                    Some(query) => format!("{path}?{query}"),
                    None => path,
                };
                vec![BundleLink {
                    relation: "self".into(),
                    url: url.into(),
                    ..Default::default()
                }]
            }

            /// One issue of that entry: the resource left out, and why.
            fn left_out(text: &str) -> OperationOutcomeIssue {
                OperationOutcomeIssue {
                    severity: "warning".into(),
                    code: "not-supported".into(),
                    details: Some(CodeableConcept {
                        text: Some(text.into()),
                        ..Default::default()
                    }),
                    diagnostics: Some(text.into()),
                    ..Default::default()
                }
            }

            /// The `searchset` of the resources of `resource_type` that
            /// `query` matches, the loaded ones before the persisted ones.
            ///
            /// A persisted resource this version has no representation for is
            /// left out of the matches and named in one `outcome` entry, so a
            /// resource written on another base never fails the whole search
            /// (<https://hl7.org/fhir/R4B/http.html#search>); `total` counts
            /// the matches (<https://hl7.org/fhir/R4B/bundle.html#searchset>).
            ///
            /// # Errors
            ///
            /// A search parameter this server does not answer is a 400, and a
            /// loaded resource this version cannot render is a 500.
            pub fn search(
                state: &AppState,
                resource_type: ResourceType,
                query: &[(String, String)],
                headers: &HeaderMap,
                uri: &http::Uri,
                wire: Wire,
            ) -> Result<Response, Failure> {
                let surface = surface();
                let projection = crate::elements::Projection::of_query(
                    query,
                    crate::elements::Interaction::Search,
                )?;
                let base = crate::version::store::base_of(state, surface.segment, headers, uri);
                let at = |id: &str| {
                    base.as_ref()
                        .map(|base| format!("{base}/{}/{id}", resource_type.name()))
                };
                let mut entry = Vec::new();
                if resource_type == ResourceType::CodeSystem {
                    for (id, provider) in crate::version::store::loaded_code_systems(state, query)?
                    {
                        let model = fhir_terminology::fhir_codesystem::model::described(&*provider);
                        entry.push(found(
                            at(&id),
                            Resource::CodeSystem(Box::new(code_system(&model, &id))),
                        ));
                    }
                    for (id, model) in crate::version::store::loaded_supplements(state, query)? {
                        entry.push(found(
                            at(&id),
                            Resource::CodeSystem(Box::new(code_system(&model, &id))),
                        ));
                    }
                }
                if resource_type == ResourceType::ValueSet {
                    for id in crate::version::store::loaded_value_sets(state, query)? {
                        let Some(model) = state.value_set_instance(&id) else {
                            continue;
                        };
                        entry.push(found(
                            at(&id),
                            Resource::ValueSet(Box::new(value_set(&model, &id))),
                        ));
                    }
                }
                if resource_type == ResourceType::ConceptMap {
                    for id in crate::version::store::loaded_concept_maps(state, query)? {
                        let Some(model) = state.concept_map_instance(&id) else {
                            continue;
                        };
                        entry.push(found(
                            at(&id),
                            Resource::ConceptMap(Box::new(concept_map(&model, &id))),
                        ));
                    }
                }
                let mut unreadable = Vec::new();
                for record in crate::version::store::matches(state, resource_type, query)? {
                    let object = match (surface.round_trip)(&record.resource) {
                        Ok(object) => object,
                        Err(reason) => {
                            unreadable.push(left_out(&crate::version::store::unreadable(
                                &record,
                                surface.fhir_version,
                                &reason,
                            )));
                            continue;
                        }
                    };
                    let resource = super::resources::resource_of(&object)
                        .map_err(|reason| crate::version::store::rendering(&reason.to_string()))?;
                    entry.push(found(at(&record.id), resource));
                }
                let total = u32::try_from(entry.len()).map_err(|_| {
                    Failure::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "too-costly",
                        "too many resources to count",
                    )
                })?;
                // NOTE: `_summary=count` returns the count of the matches without the
                // matches (<https://hl7.org/fhir/R4B/search.html#summary>).
                if projection.is_count() {
                    entry.clear();
                }
                if !unreadable.is_empty() {
                    entry.push(outcome(unreadable));
                }
                let bundle = Bundle {
                    r#type: "searchset".into(),
                    link: self_link(base.as_ref(), resource_type, uri),
                    total: Some(total.into()),
                    entry,
                    ..Default::default()
                };
                if projection.is_whole() || projection.is_count() {
                    return parameters::respond_resource(&bundle, wire);
                }
                // A subset of a resource is not a resource, so it has no typed
                // form to build: the projection is over the wire object, which
                // is also what lets one implementation serve all four versions.
                let mut object = fhir_types::codec::Json::to_json(&bundle)
                    .map_err(|reason| crate::version::store::rendering(&reason.to_string()))?;
                projection.apply_bundle(&mut object, surface.schemas);
                wire.object(StatusCode::OK, &object, surface.schemas)
            }

            /// What a history request asked, beside the resource it names.
            struct Asked<'a> {
                /// The query parameters.
                query: &'a [(String, String)],
                /// The request headers, for the base the client used.
                headers: &'a HeaderMap,
                /// The request URI, for the base and the `self` link.
                uri: &'a http::Uri,
            }

            /// The `history` Bundle of the resource `id` names, or of every
            /// resource of `resource_type` when `id` is `None`.
            ///
            /// Each version is one entry, newest first, with `entry.request`
            /// naming the interaction that made it and `entry.response` its
            /// status, `ETag`, and `lastModified`; a delete carries no resource
            /// (<https://hl7.org/fhir/R4B/http.html#history>). The entries,
            /// the absolute `fullUrl`, and the `self` link are the same shape
            /// on every served version.
            ///
            /// # Errors
            ///
            /// The refusals of [`crate::version::store::history_entries`], and
            /// a 500 for a version this version decodes and cannot encode.
            fn history(
                state: &AppState,
                resource_type: ResourceType,
                id: Option<&str>,
                asked: &Asked<'_>,
                wire: Wire,
            ) -> Result<Response, Failure> {
                let surface = surface();
                let request = Request {
                    state,
                    surface,
                    resource_type,
                    headers: asked.headers,
                    path: "",
                    wire,
                };
                let versions = crate::version::store::history_entries(&request, id, asked.query)?;
                let base = crate::version::store::base_of(
                    state,
                    surface.segment,
                    asked.headers,
                    asked.uri,
                );
                let mut entry = Vec::with_capacity(versions.len());
                for version in &versions {
                    entry.push(history_entry(base.as_deref(), resource_type, version)?);
                }
                let total = u32::try_from(entry.len()).map_err(|_| {
                    Failure::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "too-costly",
                        "too many versions to count",
                    )
                })?;
                let bundle = Bundle {
                    r#type: "history".into(),
                    link: history_link(base.as_deref(), resource_type, id, asked.uri),
                    total: Some(total.into()),
                    entry,
                    ..Default::default()
                };
                parameters::respond_resource(&bundle, wire)
            }

            /// One version as a history entry.
            fn history_entry(
                base: Option<&str>,
                resource_type: ResourceType,
                version: &HistoryEntry,
            ) -> Result<BundleEntry, Failure> {
                let surface = surface();
                let name = resource_type.name();
                let status = match (version.method, version.created) {
                    (Method::Delete, _) => "204 No Content",
                    (_, true) => "201 Created",
                    (_, false) => "200 OK",
                };
                let mut response = BundleEntryResponse {
                    status: status.into(),
                    etag: Some(version.etag().into()),
                    last_modified: Some(version.last_modified.as_str().into()),
                    ..Default::default()
                };
                let mut request = Some(BundleEntryRequest {
                    method: version.method.name().into(),
                    url: match version.method {
                        Method::Post => name.to_owned(),
                        Method::Put | Method::Delete => format!("{name}/{}", version.id),
                    }
                    .into(),
                    ..Default::default()
                });
                let mut resource = None;
                if let Some(record) = &version.record {
                    match (surface.round_trip)(&record.resource) {
                        Ok(object) => {
                            resource =
                                Some(super::resources::resource_of(&object).map_err(|reason| {
                                    crate::version::store::rendering(&reason.to_string())
                                })?);
                        }
                        // NOTE: a `PUT` or `POST` entry SHALL carry its resource
                        // (<https://hl7.org/fhir/R4B/http.html#history>), so a version this release cannot read
                        // keeps its `response`, with an `outcome` saying why, and no `request`.
                        Err(reason) => {
                            request = None;
                            response.outcome =
                                Some(Resource::OperationOutcome(Box::new(OperationOutcome {
                                    issue: vec![left_out(&crate::version::store::unreadable(
                                        record,
                                        surface.fhir_version,
                                        &reason,
                                    ))],
                                    ..Default::default()
                                })));
                        }
                    }
                }
                Ok(BundleEntry {
                    full_url: base.map(|base| format!("{base}/{name}/{}", version.id).into()),
                    resource,
                    request,
                    response: Some(response),
                    ..Default::default()
                })
            }

            /// The `self` link of a history Bundle: the URL it was read at
            /// (<https://hl7.org/fhir/R4B/http.html#paging>).
            fn history_link(
                base: Option<&str>,
                resource_type: ResourceType,
                id: Option<&str>,
                uri: &http::Uri,
            ) -> Vec<BundleLink> {
                let Some(base) = base else {
                    return Vec::new();
                };
                let path = match id {
                    Some(id) => format!("{base}/{}/{id}/_history", resource_type.name()),
                    None => format!("{base}/{}/_history", resource_type.name()),
                };
                let url = match uri.query().filter(|query| !query.is_empty()) {
                    Some(query) => format!("{path}?{query}"),
                    None => path,
                };
                vec![BundleLink {
                    relation: "self".into(),
                    url: url.into(),
                    ..Default::default()
                }]
            }

            crate::version::store::store_routes!(
                code_system_create,
                code_system_read,
                code_system_update,
                code_system_version_read,
                code_system_delete,
                code_system_search,
                code_system_history,
                code_system_type_history,
                ResourceType::CodeSystem
            );
            crate::version::store::store_routes!(
                value_set_create,
                value_set_read,
                value_set_update,
                value_set_version_read,
                value_set_delete,
                value_set_search,
                value_set_history,
                value_set_type_history,
                ResourceType::ValueSet
            );
            crate::version::store::store_routes!(
                concept_map_create,
                concept_map_read,
                concept_map_update,
                concept_map_version_read,
                concept_map_delete,
                concept_map_search,
                concept_map_history,
                concept_map_type_history,
                ResourceType::ConceptMap
            );
        }
    };
}

pub(crate) use store;
