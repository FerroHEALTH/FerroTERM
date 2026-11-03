// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resources the deployment loaded rather than persisted, as a search
//! of one version lists them beside the persisted ones.

use std::sync::Arc;

use fhir_terminology::provider::CodeSystemProvider;
use fhir_terminology::provider::code_system::CodeSystemModel;

use http::{HeaderMap, StatusCode};

use crate::outcome::Failure;
use crate::persistence::ResourceType;
use crate::state::AppState;

/// The ids of the loaded resources of `resource_type` among `instances` that
/// `query` matches, the persisted ones left out because a search lists those
/// from their records.
///
/// `ValueSet` and `ConceptMap` search define `url` and `version`
/// (<https://hl7.org/fhir/R4B/valueset.html#search>,
/// <https://hl7.org/fhir/R4B/conceptmap.html#search>).
///
/// # Errors
///
/// A search parameter this server does not answer is a 400.
pub(crate) fn loaded_matching(
    state: &AppState,
    resource_type: ResourceType,
    instances: Vec<(String, String, Option<String>)>,
    query: &[(String, String)],
) -> Result<Vec<String>, Failure> {
    let (url, version) = criteria(query)?;
    Ok(instances
        .into_iter()
        .filter(|(id, served_url, served_version)| {
            state.persisted_record(resource_type, id).is_none()
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
/// TLS or a path prefix declares `fhir.base_url`, because the request
/// carries no scheme and no `Forwarded` header is read here: no FHIR
/// specification governs reverse-proxy trust, and this is our own design.
pub(crate) fn base_of(
    state: &AppState,
    segment: &str,
    headers: &HeaderMap,
    uri: &http::Uri,
) -> Option<String> {
    if let Some(base) = state.base_url() {
        return Some(crate::served::endpoint(base, segment));
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
    Some(crate::served::endpoint(
        &format!("http://{authority}"),
        segment,
    ))
}

/// The `url` and `version` a search names.
///
/// `_elements` and `_summary` name what to return rather than what to match,
/// so they pass through here and are applied to the answer
/// (<https://hl7.org/fhir/R4B/search.html#summary>).
pub(crate) fn criteria(
    query: &[(String, String)],
) -> Result<(Option<&str>, Option<&str>), Failure> {
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
